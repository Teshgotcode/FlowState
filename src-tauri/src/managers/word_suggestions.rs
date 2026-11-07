//! The dictionary's study partner: keeps a local tally of the words the user
//! corrects in their transcriptions.
//!
//! Corrections come from two places only — the explicit "correct this" flow
//! in the history panel, and the diff between a raw transcription and its
//! LLM post-processed text. Editing a pasted text in another app is not
//! observable, so it is never guessed at.
//!
//! Nothing leaves the device and nothing is added to the dictionary
//! automatically: the counts only become suggestions the user sees (and
//! accepts or ignores) in Settings → Dictionary. The whole feature can be
//! switched off with `learn_from_corrections`.

use log::warn;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;

/// A word must be corrected this many times before it is suggested. One
/// sighting is a fluke; two is a pattern.
pub const MIN_SIGHTINGS: u32 = 2;
/// Matches the 50-character cap the custom words input already enforces.
const MAX_WORD_LEN: usize = 50;
/// How many recent correction events are remembered, so saving the same edit
/// twice — or an LLM re-run diffing the same pair — never counts twice.
const RECENT_EVENTS: usize = 64;
/// How many suggestions are handed to the UI at once.
const MAX_SUGGESTIONS: usize = 20;

/// One word's correction tally. The map key is the lowercased word;
/// `display` keeps a casing worth showing, preferring a capitalized
/// correction — names are the classic case.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
struct Sighting {
    display: String,
    count: u32,
}

/// Everything the learner remembers, persisted as one JSON file.
#[derive(Debug, Serialize, Deserialize, Default)]
struct LearnerState {
    /// Lowercased word → how many distinct corrections introduced it.
    counts: BTreeMap<String, Sighting>,
    /// Suggestions the user ignored; they only come back with new
    /// corrections, and only after those reach the threshold again.
    ignored: BTreeSet<String>,
    /// Words already in (or accepted into) the dictionary, never suggested.
    learned: BTreeSet<String>,
    /// FNV-1a hashes of recent correction events, to skip duplicate saves.
    recent: VecDeque<u64>,
}

/// One suggestion for the Dictionary settings page.
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct WordSuggestion {
    pub word: String,
    pub count: u32,
}

/// Common function words, so a lowercase correction like "the" or "og"
/// doesn't get offered as dictionary material. Far from a real dictionary —
/// just a noise floor. Checked case-insensitively.
const STOPWORDS: &[&str] = &[
    // English
    "the", "and", "for", "are", "but", "not", "you", "all", "can", "was", "one", "our", "out",
    "get", "has", "him", "his", "how", "its", "new", "now", "see", "two", "who", "did", "put",
    "say", "she", "too", "use", "that", "with", "have", "this", "will", "your", "from", "they",
    "know", "want", "been", "good", "much", "some", "time", "very", "when", "come", "here", "just",
    "like", "long", "make", "many", "over", "such", "take", "than", "them", "well", "were", "what",
    "then", "also", "about", "after", "into", "more", "only", "other", "same",
    // Norwegian/Danish/Swedish
    "og", "er", "det", "som", "en", "den", "til", "at", "av", "har", "ikke", "med", "på", "for",
    "han", "hun", "vi", "de", "var", "meg", "deg", "kan", "skal", "ble", "får", "eller", "men",
    "så", "da", "nå", "her", "där", "och", "att", "ett", "bara", "väl", // German/Dutch
    "der", "die", "das", "und", "ist", "nicht", "mit", "auf", "für", "von", "dem", "den", "een",
    "het", "van", "dat", "niet", "met", "voor", // French/Spanish/Italian/Portuguese
    "les", "des", "une", "est", "que", "qui", "pour", "avec", "dans", "sur", "son", "ses", "del",
    "los", "las", "por", "con", "para", "una", "como", "che", "gli", "non", "per",
];

/// Counts word corrections and hands them to the UI as suggestions. All state
/// lives in one JSON file in the app data directory.
pub struct WordSuggestionLearner {
    path: PathBuf,
    state: Mutex<LearnerState>,
}

impl WordSuggestionLearner {
    /// Loads the tally from `path`, or starts fresh when there is none yet.
    pub fn new(path: PathBuf) -> Self {
        let state = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            path,
            state: Mutex::new(state),
        }
    }

    /// Learns from one correction: `original` is what the recognizer
    /// produced, `corrected` what it should have been. `dictionary` is the
    /// current custom words; words already in it are never counted.
    pub fn record_correction(&self, original: &str, corrected: &str, dictionary: &[String]) {
        let event = fnv1a(original, corrected);
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        // The same pair saved twice — or an LLM re-run producing the same
        // rewrite — is one correction, not two.
        if state.recent.contains(&event) {
            return;
        }
        state.recent.push_back(event);
        if state.recent.len() > RECENT_EVENTS {
            state.recent.pop_front();
        }

        for candidate in correction_candidates(original, corrected) {
            let key = candidate.to_lowercase();
            // Already in the dictionary: remember that, and never count it.
            if dictionary_contains(dictionary, &key) {
                state.learned.insert(key.clone());
                state.counts.remove(&key);
                state.ignored.remove(&key);
                continue;
            }
            if state.learned.contains(&key) {
                continue;
            }
            // An ignored word with fresh evidence starts a new tally; it
            // only reappears once that evidence reaches the threshold.
            state.ignored.remove(&key);
            let sighting = state.counts.entry(key).or_default();
            sighting.count = sighting.count.saturating_add(1);
            if candidate
                .chars()
                .next()
                .is_some_and(|first| first.is_uppercase())
                || sighting.display.is_empty()
            {
                sighting.display = candidate;
            }
        }

        self.save(&state);
    }

    /// The words worth offering: corrected at least [`MIN_SIGHTINGS`]
    /// times, and still missing from the dictionary.
    pub fn suggestions(&self, dictionary: &[String]) -> Vec<WordSuggestion> {
        let state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let mut suggestions: Vec<WordSuggestion> = state
            .counts
            .iter()
            .filter(|(key, sighting)| {
                sighting.count >= MIN_SIGHTINGS
                    && !state.learned.contains(*key)
                    && !state.ignored.contains(*key)
                    && !dictionary_contains(dictionary, key)
                    && !sighting.display.is_empty()
            })
            .map(|(_, sighting)| WordSuggestion {
                word: sighting.display.clone(),
                count: sighting.count,
            })
            .collect();
        // Most-corrected first, ties alphabetically for a stable list.
        suggestions.sort_by(|a, b| b.count.cmp(&a.count).then(a.word.cmp(&b.word)));
        suggestions.truncate(MAX_SUGGESTIONS);
        suggestions
    }

    /// The user accepted a suggestion: never suggest it again, even if it is
    /// later removed from the dictionary — that removal is its own decision.
    pub fn accept(&self, word: &str) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let key = word.to_lowercase();
        state.counts.remove(&key);
        state.ignored.remove(&key);
        state.learned.insert(key);
        self.save(&state);
    }

    /// The user dismissed a suggestion: it stays away until fresh
    /// corrections reach the threshold again.
    pub fn ignore(&self, word: &str) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let key = word.to_lowercase();
        state.counts.remove(&key);
        state.ignored.insert(key);
        self.save(&state);
    }

    /// Called whenever the custom words list changes, so the learner's idea
    /// of "already known" tracks the real dictionary: words that left it
    /// may be learned about again.
    pub fn sync_dictionary(&self, dictionary: &[String]) {
        let mut state = match self.state.lock() {
            Ok(state) => state,
            Err(poisoned) => poisoned.into_inner(),
        };
        let before = state.learned.len();
        state
            .learned
            .retain(|word| dictionary_contains(dictionary, word));
        if state.learned.len() != before {
            self.save(&state);
        }
    }

    /// Persists the tally; a failed write is logged, never fatal — the
    /// worst case is relearning a few corrections.
    fn save(&self, state: &LearnerState) {
        match serde_json::to_vec_pretty(state) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(&self.path, bytes) {
                    warn!(
                        "Failed to save word suggestions to {}: {}",
                        self.path.display(),
                        e
                    );
                }
            }
            Err(e) => warn!("Failed to serialize word suggestions: {}", e),
        }
    }
}

/// Whether `word` (lowercased) is in the dictionary, case-insensitively.
fn dictionary_contains(dictionary: &[String], word: &str) -> bool {
    dictionary.iter().any(|entry| entry.to_lowercase() == word)
}

/// FNV-1a over the pair, so identical corrections are recognized without
/// keeping the texts themselves around.
fn fnv1a(original: &str, corrected: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut fold = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    };
    original.as_bytes().iter().for_each(|byte| fold(*byte));
    fold(0);
    corrected.as_bytes().iter().for_each(|byte| fold(*byte));
    hash
}

/// Splits text into word tokens: runs of ASCII letters and digits. This
/// matches the A–Z-and-numbers limitation the custom words matcher already
/// documents, and keeps CJK text from becoming one enormous "word".
fn ascii_tokens(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut start = None;
    for (index, byte) in text.as_bytes().iter().enumerate() {
        if byte.is_ascii_alphanumeric() {
            if start.is_none() {
                start = Some(index);
            }
        } else if let Some(begin) = start.take() {
            tokens.push(&text[begin..index]);
        }
    }
    if let Some(begin) = start {
        tokens.push(&text[begin..]);
    }
    tokens
}

/// The words a correction introduced: tokens in the corrected text that the
/// original doesn't already contain. Noun-like, per the feature brief — a
/// capitalized word, or an unknown one — which here means: at least three
/// characters, at least one letter, and not a common function word. The
/// candidate is "unknown" by construction, since anything the recognizer
/// already got right is filtered out.
pub fn correction_candidates(original: &str, corrected: &str) -> Vec<String> {
    let original_words: BTreeSet<&str> = ascii_tokens(original).into_iter().collect();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut candidates = Vec::new();
    for token in ascii_tokens(corrected) {
        let has_letter = token.chars().any(|c| c.is_ascii_alphabetic());
        let lower = token.to_lowercase();
        if !has_letter
            || token.len() < 3
            || token.len() > MAX_WORD_LEN
            || original_words.contains(token)
            || STOPWORDS.contains(&lower.as_str())
            || !seen.insert(lower)
        {
            continue;
        }
        candidates.push(token.to_string());
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each test gets its own throwaway tally file, so parallel tests never
    /// share state.
    fn learner(name: &str) -> WordSuggestionLearner {
        let path = std::env::temp_dir().join(format!(
            "flowstate-word-suggestions-{}-{name}.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        WordSuggestionLearner::new(path)
    }

    fn suggestions(l: &WordSuggestionLearner) -> Vec<(String, u32)> {
        l.suggestions(&[])
            .into_iter()
            .map(|s| (s.word, s.count))
            .collect()
    }

    #[test]
    fn correcting_the_same_name_twice_yields_one_suggestion() {
        let l = learner("twice");
        l.record_correction("hei ola nordmann", "hei Ola Nordmann", &[]);
        // Below the threshold: one correction is not yet a pattern.
        assert_eq!(suggestions(&l), Vec::<(String, u32)>::new());
        l.record_correction("ring ola igjen", "ring Ola igjen", &[]);
        // One entry for Ola — with both sightings behind it — not two.
        assert_eq!(suggestions(&l), vec![("Ola".into(), 2)]);
    }

    #[test]
    fn the_same_correction_saved_twice_counts_once() {
        let l = learner("dedupe");
        l.record_correction("mote med ola", "møte med Ola", &[]);
        l.record_correction("mote med ola", "møte med Ola", &[]);
        assert_eq!(suggestions(&l), Vec::<(String, u32)>::new());
    }

    #[test]
    fn accepted_words_are_never_suggested_again() {
        let l = learner("accept");
        l.record_correction("a", "Ola", &[]);
        l.record_correction("b", "Ola", &[]);
        assert_eq!(suggestions(&l), vec![("Ola".into(), 2)]);
        l.accept("Ola");
        assert_eq!(suggestions(&l), Vec::<(String, u32)>::new());
        // New corrections of an accepted word stay silent.
        l.record_correction("c", "Ola", &[]);
        l.record_correction("d", "Ola", &[]);
        assert_eq!(suggestions(&l), Vec::<(String, u32)>::new());
    }

    #[test]
    fn ignored_words_stay_away_until_new_evidence() {
        let l = learner("ignore");
        l.record_correction("a", "Ola", &[]);
        l.record_correction("b", "Ola", &[]);
        l.ignore("Ola");
        assert_eq!(suggestions(&l), Vec::<(String, u32)>::new());
        // One new correction is not enough to bring it back.
        l.record_correction("c", "Ola", &[]);
        assert_eq!(suggestions(&l), Vec::<(String, u32)>::new());
        // A second one is: fresh evidence, fresh suggestion.
        l.record_correction("d", "Ola", &[]);
        assert_eq!(suggestions(&l), vec![("Ola".into(), 2)]);
    }

    #[test]
    fn dictionary_words_are_not_counted_or_suggested() {
        let l = learner("dictionary");
        l.record_correction("a", "FlowState", &["flowstate".into()]);
        l.record_correction("b", "FlowState", &["flowstate".into()]);
        assert_eq!(suggestions(&l), Vec::<(String, u32)>::new());
        // Removing the word from the dictionary allows learning it again.
        l.sync_dictionary(&[]);
        assert_eq!(suggestions(&l), Vec::<(String, u32)>::new());
        l.record_correction("c", "FlowState", &[]);
        l.record_correction("d", "FlowState", &[]);
        assert_eq!(suggestions(&l), vec![("FlowState".into(), 2)]);
    }

    #[test]
    fn suggestions_exclude_words_already_in_the_dictionary() {
        let l = learner("exclude");
        l.record_correction("a", "Ola", &[]);
        l.record_correction("b", "Ola", &[]);
        assert_eq!(suggestions(&l), vec![("Ola".into(), 2)]);
        assert!(l.suggestions(&["ola".into()]).is_empty());
    }

    #[test]
    fn function_words_and_noise_are_filtered() {
        let candidates = correction_candidates("the and of", "The AND og 42 x ab Ola");
        assert_eq!(candidates, vec!["Ola".to_string()]);
    }

    #[test]
    fn a_casing_fix_of_a_name_is_a_candidate() {
        // The recognizer wrote "ola" lowercase; the user fixed the name.
        let candidates = correction_candidates("hei ola der", "hei Ola der");
        assert_eq!(candidates, vec!["Ola".to_string()]);
    }

    #[test]
    fn numbers_only_and_overlong_tokens_are_skipped() {
        let long = "a".repeat(51);
        let candidates = correction_candidates(" gammel ", &format!(" 123 {long} WhisperCPP"));
        assert_eq!(candidates, vec!["WhisperCPP".to_string()]);
    }

    #[test]
    fn the_tally_survives_a_restart() {
        let path = std::env::temp_dir().join(format!(
            "flowstate-word-suggestions-{}-reload.json",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&path);
        {
            let l = WordSuggestionLearner::new(path.clone());
            l.record_correction("a", "Ola", &[]);
            l.record_correction("b", "Ola", &[]);
            assert_eq!(l.suggestions(&[]).len(), 1);
        }
        let reloaded = WordSuggestionLearner::new(path.clone());
        assert_eq!(
            reloaded
                .suggestions(&[])
                .into_iter()
                .map(|s| (s.word, s.count))
                .collect::<Vec<_>>(),
            vec![("Ola".into(), 2)]
        );
        let _ = std::fs::remove_file(&path);
    }
}
