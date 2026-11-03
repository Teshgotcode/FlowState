//! Spoken punctuation and emoji commands ("reference-style" normalization).
//!
//! After transcription, dictated phrases like "komma", "ny linje" or
//! "smilefjes" are rewritten into the punctuation, line break or emoji the
//! speaker meant. Everything here is local and deterministic: no network, no
//! model calls.
//!
//! Safety rules — the point of this module is to never corrupt real text:
//! * Commands only match whole words/phrases: "komma" never fires inside
//!   "kommandør", and a hyphen-joined compound ("komma-ordet") is left alone.
//! * Command tables are language-gated: they only run when the output
//!   language is known to be Norwegian or English, and fail closed otherwise.
//! * Noun-named commands ("komma", "kolon") are only replaced when dictation
//!   continues afterwards, so "Jeg liker komma" keeps the plain word.
//! * A command preceded by a "mention" word ("et komma", "a period") is left
//!   alone — the speaker is talking about the character, not dictating it.

use super::text::OutputLanguageEvidence;

/// How careful to be about a command's position in the transcript.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    /// Named like an ordinary noun ("komma", "kolon", "bindestrek").
    /// Replaced only when more dictation follows — a trailing mention such as
    /// "Jeg liker komma" keeps the word.
    Named,
    /// Sentence-final punctuation ("punktum", "spørsmålstegn"). Also valid as
    /// the very last word of the transcript.
    SentenceFinal,
    /// Imperative control phrase ("ny linje") or emoji ("smilefjes") —
    /// unambiguous enough to replace anywhere, including at the end.
    Always,
}

struct VoiceCommand {
    /// Lowercase spoken words of the command, e.g. `["ny", "linje"]`.
    phrase: &'static [&'static str],
    replacement: &'static str,
    kind: CommandKind,
}

const NORWEGIAN_COMMANDS: &[VoiceCommand] = &[
    VoiceCommand {
        phrase: &["nytt", "avsnitt"],
        replacement: "\n\n",
        kind: CommandKind::Always,
    },
    VoiceCommand {
        phrase: &["ny", "linje"],
        replacement: "\n",
        kind: CommandKind::Always,
    },
    VoiceCommand {
        phrase: &["spørsmålstegn"],
        replacement: "?",
        kind: CommandKind::SentenceFinal,
    },
    VoiceCommand {
        phrase: &["utropstegn"],
        replacement: "!",
        kind: CommandKind::SentenceFinal,
    },
    VoiceCommand {
        phrase: &["punktum"],
        replacement: ".",
        kind: CommandKind::SentenceFinal,
    },
    VoiceCommand {
        phrase: &["smilefjes"],
        replacement: "\u{1F642}",
        kind: CommandKind::Always,
    },
    VoiceCommand {
        phrase: &["bindestrek"],
        replacement: "-",
        kind: CommandKind::Named,
    },
    VoiceCommand {
        phrase: &["komma"],
        replacement: ",",
        kind: CommandKind::Named,
    },
    VoiceCommand {
        phrase: &["kolon"],
        replacement: ":",
        kind: CommandKind::Named,
    },
];

const ENGLISH_COMMANDS: &[VoiceCommand] = &[
    VoiceCommand {
        phrase: &["new", "paragraph"],
        replacement: "\n\n",
        kind: CommandKind::Always,
    },
    VoiceCommand {
        phrase: &["new", "line"],
        replacement: "\n",
        kind: CommandKind::Always,
    },
    VoiceCommand {
        phrase: &["question", "mark"],
        replacement: "?",
        kind: CommandKind::SentenceFinal,
    },
    VoiceCommand {
        phrase: &["exclamation", "mark"],
        replacement: "!",
        kind: CommandKind::SentenceFinal,
    },
    VoiceCommand {
        phrase: &["exclamation", "point"],
        replacement: "!",
        kind: CommandKind::SentenceFinal,
    },
    VoiceCommand {
        phrase: &["full", "stop"],
        replacement: ".",
        kind: CommandKind::SentenceFinal,
    },
    VoiceCommand {
        phrase: &["period"],
        replacement: ".",
        kind: CommandKind::SentenceFinal,
    },
    VoiceCommand {
        phrase: &["smiley", "face"],
        replacement: "\u{1F642}",
        kind: CommandKind::Always,
    },
    VoiceCommand {
        phrase: &["hyphen"],
        replacement: "-",
        kind: CommandKind::Named,
    },
    VoiceCommand {
        phrase: &["comma"],
        replacement: ",",
        kind: CommandKind::Named,
    },
    VoiceCommand {
        phrase: &["colon"],
        replacement: ":",
        kind: CommandKind::Named,
    },
];

/// Words that mark the next word as a *mention*, not a dictation command:
/// "et komma", "a period", "liker komma" talk about the character itself.
const NORWEGIAN_MENTION_WORDS: &[&str] = &[
    "et",
    "en",
    "ei",
    "ingen",
    "om",
    "angående",
    "betyr",
    "heter",
    "kalles",
    "kalt",
    "liker",
    "likte",
    "elsker",
    "hater",
    "glemte",
    "savner",
    "bruker",
    "brukte",
    "trenger",
    "mener",
    "skriver",
    "skrev",
    "sier",
    "sa",
    "leser",
    "forstår",
];

const ENGLISH_MENTION_WORDS: &[&str] = &[
    "a", "an", "the", "this", "that", "about", "called", "call", "means", "mean", "like", "likes",
    "liked", "love", "loves", "hate", "hates", "forgot", "use", "uses", "need", "needs",
];

/// Command table plus mention-guard words for a transcribed-output language.
fn table_for_language(lang: &str) -> Option<(&'static [VoiceCommand], &'static [&'static str])> {
    let base = lang.split(&['-', '_'][..]).next().unwrap_or(lang);
    match base {
        "nb" | "nn" | "no" => Some((NORWEGIAN_COMMANDS, NORWEGIAN_MENTION_WORDS)),
        "en" => Some((ENGLISH_COMMANDS, ENGLISH_MENTION_WORDS)),
        _ => None,
    }
}

/// A slice of the transcription: either a maximal run of alphanumeric
/// characters (a word, possibly with digits) or the separator text around it.
struct Token {
    text: String,
    is_word: bool,
}

fn tokenize(text: &str) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut current = String::new();
    let mut current_is_word: Option<bool> = None;

    for c in text.chars() {
        let is_word = c.is_alphanumeric();
        match current_is_word {
            Some(kind) if kind == is_word => current.push(c),
            Some(kind) => {
                tokens.push(Token {
                    text: std::mem::take(&mut current),
                    is_word: kind,
                });
                current.push(c);
                current_is_word = Some(is_word);
            }
            None => {
                current.push(c);
                current_is_word = Some(is_word);
            }
        }
    }
    if !current.is_empty() {
        tokens.push(Token {
            text: current,
            is_word: current_is_word.unwrap_or(false),
        });
    }
    tokens
}

/// Appends `segment`, uppercasing its first alphanumeric character while
/// `capital_owed` is set (mirrors the filler-removal capital handling).
fn push_restoring_capital(out: &mut String, segment: &str, capital_owed: &mut bool) {
    if *capital_owed {
        if let Some((index, first)) = segment.char_indices().find(|(_, c)| c.is_alphanumeric()) {
            *capital_owed = false;
            out.push_str(&segment[..index]);
            out.extend(first.to_uppercase());
            out.push_str(&segment[index + first.len_utf8()..]);
            return;
        }
    }
    out.push_str(segment);
}

/// Removes trailing spaces and tabs (never newlines — the user may have
/// dictated line breaks that must survive).
fn trim_trailing_spaces(out: &mut String) {
    while out
        .chars()
        .next_back()
        .is_some_and(|c| c == ' ' || c == '\t')
    {
        out.pop();
    }
}

/// The token index of the next word token after `token_index`, plus whether
/// the separator in between is plain whitespace. `None` when no word follows.
fn next_word_after(tokens: &[Token], token_index: usize) -> Option<(usize, bool)> {
    let mut gap_whitespace_only = true;
    for (index, token) in tokens.iter().enumerate().skip(token_index + 1) {
        if token.is_word {
            return Some((index, gap_whitespace_only));
        }
        if !token.text.chars().all(char::is_whitespace) {
            gap_whitespace_only = false;
        }
    }
    None
}

/// The separator token directly before `token_index`, if any.
fn separator_before(tokens: &[Token], token_index: usize) -> Option<&Token> {
    token_index
        .checked_sub(1)
        .and_then(|index| tokens.get(index))
        .filter(|token| !token.is_word)
}

/// True when a hyphen glues the command word to its neighbours, as in
/// "komma-ordet": the token is part of a compound word, not a command.
fn hyphen_attached(tokens: &[Token], first: usize, last: usize) -> bool {
    separator_before(tokens, first).is_some_and(|gap| gap.text.ends_with('-'))
        || tokens
            .get(last + 1)
            .is_some_and(|token| !token.is_word && token.text.starts_with('-'))
}

/// The command starting at word `word_index`, if any, plus the token index of
/// its last spoken word.
fn command_at(
    tokens: &[Token],
    words: &[(usize, String)],
    word_index: usize,
    commands: &[&'static VoiceCommand],
    mention_words: &'static [&'static str],
) -> Option<(&'static VoiceCommand, usize)> {
    // A "mention" word right before the command means the speaker refers to
    // the character itself: "et komma", "a period".
    if word_index > 0 && mention_words.contains(&words[word_index - 1].1.as_str()) {
        return None;
    }

    for command in commands {
        let phrase_len = command.phrase.len();
        if word_index + phrase_len > words.len() {
            continue;
        }
        if words[word_index].1 != command.phrase[0] {
            continue;
        }
        // Walk the remaining phrase words; they must be separated by plain
        // whitespace so "ny, linje" (a punctuated sentence) never matches.
        let mut last_token = words[word_index].0;
        let mut matched = true;
        for spoken in command.phrase.iter().skip(1) {
            match next_word_after(tokens, last_token) {
                Some((index, true)) if words_at(words, index).is_some_and(|w| w == *spoken) => {
                    last_token = index;
                }
                _ => {
                    matched = false;
                    break;
                }
            }
        }
        if !matched || hyphen_attached(tokens, words[word_index].0, last_token) {
            continue;
        }
        // A trailing noun-like command is a mention of the word, not a
        // dictation command ("Jeg liker komma" — without other context).
        if command.kind == CommandKind::Named && next_word_after(tokens, last_token).is_none() {
            continue;
        }
        return Some((command, last_token));
    }
    None
}

/// The lowercase word at a given token index, if that token is a word.
fn words_at(words: &[(usize, String)], token_index: usize) -> Option<&str> {
    words
        .iter()
        .find(|(index, _)| *index == token_index)
        .map(|(_, word)| word.as_str())
}

/// Rewrites spoken punctuation/emoji commands in `text` into their characters.
///
/// `enabled` is the master toggle; without language evidence the text is
/// returned unchanged (fail closed, like gated filler removal).
pub fn apply_voice_commands(
    text: &str,
    language: &OutputLanguageEvidence,
    enabled: bool,
) -> String {
    if !enabled || text.trim().is_empty() {
        return text.to_string();
    }
    let Some((commands, mention_words)) = language.language().and_then(table_for_language) else {
        return text.to_string();
    };

    let tokens = tokenize(text);
    let words: Vec<(usize, String)> = tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| token.is_word)
        .map(|(index, token)| (index, token.text.to_lowercase()))
        .collect();
    // Longest phrase first so "nytt avsnitt" wins over a single word.
    let mut ordered: Vec<&'static VoiceCommand> = commands.iter().collect();
    ordered.sort_by_key(|command| std::cmp::Reverse(command.phrase.len()));

    let mut out = String::with_capacity(text.len());
    let mut capital_owed = false;
    // Set after a punctuation insert so a model-written duplicate of the same
    // character ("spørsmålstegn?") is not kept alongside the inserted one.
    let mut dedupe: Option<char> = None;
    let mut word_index = 0usize;
    let mut token_index = 0usize;

    while token_index < tokens.len() {
        let token = &tokens[token_index];

        if !token.is_word {
            let mut text = token.text.as_str();
            if let Some(needle) = dedupe.filter(|_| !token.is_word) {
                if let Some(rest) = text.strip_prefix(needle) {
                    text = rest;
                }
            }
            push_restoring_capital(&mut out, text, &mut capital_owed);
            dedupe = None;
            token_index += 1;
            continue;
        }

        match command_at(&tokens, &words, word_index, &ordered, mention_words) {
            Some((command, last_token)) => {
                emit_command(&mut out, &mut capital_owed, &mut dedupe, command);
                let consumed_words = command.phrase.len();
                // Line breaks and hyphens swallow the whitespace after them;
                // other commands keep it as separation.
                if command.replacement.contains('\n') || command.replacement == "-" {
                    if let Some((next, true)) = next_word_after(&tokens, last_token) {
                        token_index = next;
                    } else {
                        token_index = tokens.len();
                    }
                } else {
                    token_index = last_token + 1;
                }
                word_index += consumed_words;
            }
            None => {
                push_restoring_capital(&mut out, &token.text, &mut capital_owed);
                dedupe = None;
                token_index += 1;
                word_index += 1;
            }
        }
    }

    out
}

/// Writes a matched command's replacement into `out`, fixing up spacing and
/// the capitalization of whatever follows.
fn emit_command(
    out: &mut String,
    capital_owed: &mut bool,
    dedupe: &mut Option<char>,
    command: &VoiceCommand,
) {
    let replacement = command.replacement;
    let single_char = replacement.chars().count() == 1;

    match command.kind {
        CommandKind::Always if replacement.contains('\n') => {
            // Line breaks: attach to the previous line, swallow the space
            // after them and capitalize the new line's first word.
            trim_trailing_spaces(out);
            out.push_str(replacement);
            *capital_owed = true;
        }
        CommandKind::Always => {
            // Emoji: keep a space on both sides.
            out.push_str(replacement);
        }
        CommandKind::SentenceFinal | CommandKind::Named => {
            trim_trailing_spaces(out);
            let already_there = single_char
                && !replacement.chars().next().is_some_and(char::is_whitespace)
                && out.ends_with(replacement);
            if already_there {
                // The model already punctuated here ("Hei, komma").
                *dedupe = replacement.chars().next();
            } else {
                out.push_str(replacement);
                if command.kind == CommandKind::SentenceFinal {
                    *capital_owed = true;
                }
                if single_char {
                    *dedupe = replacement.chars().next();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no(text: &str) -> String {
        apply_voice_commands(
            text,
            &OutputLanguageEvidence::UserSelected("nb".into()),
            true,
        )
    }

    fn en(text: &str) -> String {
        apply_voice_commands(
            text,
            &OutputLanguageEvidence::UserSelected("en".into()),
            true,
        )
    }

    #[test]
    fn norwegian_acceptance_phrase() {
        assert_eq!(
            no("Hei komma ny linje Hvordan går det spørsmålstegn"),
            "Hei,\nHvordan går det?"
        );
    }

    #[test]
    fn trailing_noun_command_is_kept() {
        assert_eq!(no("Jeg liker komma"), "Jeg liker komma");
        assert_eq!(no("Hei komma"), "Hei komma");
    }

    #[test]
    fn mention_word_blocks_replacement() {
        assert_eq!(no("Jeg glemte et komma i går"), "Jeg glemte et komma i går");
        assert_eq!(en("I forgot a comma"), "I forgot a comma");
        assert_eq!(en("a period of time"), "a period of time");
    }

    #[test]
    fn never_matches_inside_longer_words() {
        assert_eq!(
            no("Kommandoen starter med komma-ordet"),
            "Kommandoen starter med komma-ordet"
        );
        assert_eq!(no("kommandocentralen"), "kommandocentralen");
    }

    #[test]
    fn disabled_and_unknown_language_are_noops() {
        let text = "Hei komma dette";
        assert_eq!(
            apply_voice_commands(
                text,
                &OutputLanguageEvidence::UserSelected("nb".into()),
                false
            ),
            text
        );
        assert_eq!(
            apply_voice_commands(text, &OutputLanguageEvidence::Unknown, true),
            text
        );
        assert_eq!(
            apply_voice_commands(
                text,
                &OutputLanguageEvidence::UserSelected("de".into()),
                true
            ),
            text
        );
        assert_eq!(
            apply_voice_commands(text, &OutputLanguageEvidence::TranslatedToEnglish, true),
            text
        );
    }

    #[test]
    fn english_commands() {
        assert_eq!(
            en("Hello comma new line How are you question mark"),
            "Hello,\nHow are you?"
        );
        assert_eq!(en("smiley face to you"), "\u{1F642} to you");
    }

    #[test]
    fn paragraph_break_and_emoji() {
        assert_eq!(no("Slutt nytt avsnitt Start"), "Slutt\n\nStart");
        assert_eq!(no("Takk smilefjes"), "Takk \u{1F642}");
        assert_eq!(no("Jeg liker smilefjes"), "Jeg liker smilefjes");
    }

    #[test]
    fn model_punctuation_is_not_duplicated() {
        assert_eq!(no("Hei, komma dette"), "Hei, dette");
        assert_eq!(no("Hvordan går det spørsmålstegn?"), "Hvordan går det?");
    }

    #[test]
    fn sentence_capitalization_after_command() {
        assert_eq!(no("hei punktum dette er en test"), "hei. Dette er en test");
    }

    #[test]
    fn hyphen_joins_words() {
        assert_eq!(no("tjue bindestrek fem"), "tjue-fem");
    }

    #[test]
    fn colon_and_mid_sentence_comma() {
        assert_eq!(no("Listen kolon eple og banan"), "Listen: eple og banan");
        assert_eq!(no("Hei komma dette er en test"), "Hei, dette er en test");
    }
}
