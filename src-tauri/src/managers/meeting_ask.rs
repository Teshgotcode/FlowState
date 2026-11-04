//! The meeting "Ask" tab: a chat with a local LLM about one finished meeting,
//! answered from the meeting's own transcript. Each answer carries the
//! segments it builds on, so the transcript window can jump to the exact
//! timestamp the answer came from; the answer itself is marked as generated
//! and must be checked against the transcript.
//!
//! The context of a short meeting is the whole transcript. A long one does
//! not fit the context window of a small local model, so the parts the answer
//! may need are picked with a plain keyword ranking — BM25 over the
//! segments' words, plus a substring (`LIKE`) bonus so a Norwegian compound
//! such as "planleggingsmøtet" still matches the question "planlegging". No
//! vector database, no embeddings, nothing leaves the machine.
//!
//! Nothing an ask produces is saved: the transcript stands untouched, the
//! conversation lives only in the window that asked. The only network traffic
//! is the configured post-processing endpoint, and the note's
//! [`NoteLlm::resolve`](crate::managers::meeting_note::NoteLlm::resolve)
//! refuses any endpoint that is not loopback, so a question about a meeting
//! can never reach a cloud API by mistake.

use crate::actions::{complete_unless_cancelled, first_json_object};
use crate::llm_client;
use crate::managers::history::{HistoryEntry, HistoryManager, HistorySource};
use crate::managers::meeting_note::{pack_chunks, sentences, MeetingNoteFailure, NoteLlm};
use crate::settings::get_settings;
use anyhow::{anyhow, Result};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Manager};

/// Characters of transcript one ask call sees. Norwegian runs at roughly
/// three characters per token, so the whole-transcript path stays inside a
/// small model's default context with room for the prompt and the answer.
const CONTEXT_CHAR_BUDGET: usize = 9000;

/// At most this many transcript parts ride on one ask, whatever the budget:
/// every part costs a numbered line the model must read.
const MAX_CONTEXT_UNITS: usize = 80;

/// Characters per part when an entry has no timed segments and the plain text
/// is grouped into parts: whole sentences, packed to a readable size.
const ASK_UNIT_TARGET_CHARS: usize = 600;

/// BM25 parameters: the classic Robertson defaults.
const BM25_K1: f32 = 1.5;
const BM25_B: f32 = 0.75;
/// Score added for a query word found as a substring inside a part, the
/// `LIKE` half of the ranking: word-split BM25 alone misses compounds and
/// inflections that share a stem as a substring.
const LIKE_BONUS: f32 = 1.0;

/// One transcript part the model may cite. `id` is the 1-based number shown
/// in the prompt; the timestamps are `None` for parts grouped out of a plain
/// transcript with no timed segments.
#[derive(Debug)]
struct AskUnit {
    id: i64,
    t0_ms: Option<i64>,
    t1_ms: Option<i64>,
    text: String,
}

/// One quote an answer cites: the verbatim transcript part it came from and
/// the part's place on the recording's timeline, so the window can jump to
/// the right timestamp. Timestamps are `None` when the entry had no segments.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MeetingQuote {
    pub t0_ms: Option<i64>,
    pub t1_ms: Option<i64>,
    pub text: String,
}

/// The answer to one question: the text the model wrote, whether the
/// transcript actually answers the question, and the parts the answer builds
/// on. `model` names the LLM that wrote it, for the "generated locally" label.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MeetingAnswer {
    /// Whether the transcript parts answer the question. `false` means the
    /// answer says so, and no quote was kept.
    pub found: bool,
    /// The model's answer, in the transcript's language.
    pub answer: String,
    /// The verbatim transcript parts the answer builds on.
    pub quotes: Vec<MeetingQuote>,
    pub model: String,
}

/// Why a question could not be answered. The frontend maps each reason to a
/// localized message; `detail` never reaches the user, only the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MeetingAskFailure {
    /// The entry is not a meeting session.
    NotAMeeting,
    /// The entry has no transcript to ask about.
    NoTranscript,
    /// No post-processing provider is selected.
    NoProvider,
    /// The selected provider has no model configured.
    NoModel,
    /// The selected provider cannot answer: it is not a local
    /// OpenAI-compatible endpoint.
    NoLocalLlm,
    /// The question was empty.
    EmptyQuestion,
    /// Another question is already being answered for this entry.
    AlreadyAsking,
    /// The local LLM could not be reached, or answered with an error.
    LlmUnreachable,
    /// The local LLM answered, but no answer could be read from it.
    InvalidResponse,
    /// The user pressed cancel; nothing was shown.
    Cancelled,
}

/// What one question comes back with: the answer, or why there is none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum MeetingAskOutcome {
    Answered {
        answer: MeetingAnswer,
    },
    Failed {
        reason: MeetingAskFailure,
        detail: Option<String>,
    },
}

/* ---------- the transcript parts one ask sees ----------------------------- */

/// Split the entry into the parts the model cites: its timed segments when it
/// has them, otherwise its text grouped to whole sentences. Empty when there
/// is nothing to ask about.
fn ask_units(entry: &HistoryEntry) -> Vec<AskUnit> {
    if let Some(segments) = entry.metadata.as_ref().and_then(|m| m.segments.as_ref()) {
        if !segments.is_empty() {
            return segments
                .iter()
                .enumerate()
                .map(|(index, segment)| AskUnit {
                    id: index as i64 + 1,
                    t0_ms: Some(segment.t0_ms),
                    t1_ms: Some(segment.t1_ms),
                    text: segment.text.clone(),
                })
                .collect();
        }
    }
    pack_chunks(&sentences(&entry.transcription_text), ASK_UNIT_TARGET_CHARS)
        .into_iter()
        .enumerate()
        .map(|(index, text)| AskUnit {
            id: index as i64 + 1,
            t0_ms: None,
            t1_ms: None,
            text,
        })
        .collect()
}

/// Words that carry no meaning for retrieval, in the languages a transcript
/// in this app is likely to be in. A question made only of these falls back
/// to the transcript's opening, the way a plain prefix scan would.
const STOPWORDS: &[&str] = &[
    // Norwegian
    "og", "eller", "men", "som", "på", "til", "fra", "med", "uten", "for", "av", "om", "at", "det",
    "den", "de", "dem", "deres", "vi", "oss", "vår", "vårt", "være", "er", "var", "blir", "ble",
    "har", "hadde", "kan", "skal", "vil", "må", "se", "snakker", "sier", "hva", "hvem", "hvor",
    "når", "hvordan", "hvorfor", "alle", "noe", "noen", "ingen", "meg", "deg", "du", "han", "hun",
    "hans", "hennes", "dette", "disse", "her", "der", "da", "så", "også", "ikke", "en", "et", "ei",
    // English
    "the", "a", "an", "of", "to", "in", "on", "at", "by", "for", "and", "or", "but", "is", "are",
    "was", "were", "be", "been", "am", "do", "does", "did", "have", "has", "had", "will", "would",
    "can", "could", "should", "shall", "may", "might", "must", "what", "who", "when", "where",
    "why", "how", "which", "that", "this", "these", "those", "there", "here", "it", "its", "he",
    "she", "they", "them", "their", "we", "us", "our", "you", "your", "i", "me", "my", "not", "no",
    "yes", "about", "into", "from", "with", "without", "all", "any", "some",
];

/// The content words of `text`, lowercased: what retrieval matches on.
fn content_tokens(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|token| token.len() >= 2 && !STOPWORDS.contains(token))
        .map(str::to_string)
        .collect()
}

/// Rank every part against the question: BM25 over the parts' words, plus a
/// substring bonus per distinct query word found in the part's raw text.
/// Parts that share no content word with the question score zero, and the
/// zero-scored prefix of the transcript wins the budget, the way a plain
/// `LIKE` scan of the first rows would.
fn retrieval_scores(units: &[AskUnit], question: &str) -> Vec<f32> {
    let mut query: Vec<String> = Vec::new();
    for token in content_tokens(question) {
        if !query.contains(&token) {
            query.push(token);
        }
    }
    let documents: Vec<Vec<String>> = units
        .iter()
        .map(|unit| content_tokens(&unit.text))
        .collect();
    let empty_query = query.is_empty();
    let total = units.len() as f32;
    let average_length = documents.iter().map(Vec::len).sum::<usize>() as f32 / total.max(1.0);
    units
        .iter()
        .enumerate()
        .map(|(index, unit)| {
            if empty_query {
                return 0.0;
            }
            let document = &documents[index];
            let length = document.len() as f32;
            let lowered = unit.text.to_lowercase();
            let mut score = 0.0;
            for token in &query {
                let frequency = document.iter().filter(|word| *word == token).count() as f32;
                let containing = documents
                    .iter()
                    .filter(|words| words.contains(token))
                    .count() as f32;
                // BM25: rarer words and denser hits weigh more.
                let idf = ((total - containing + 0.5) / (containing + 0.5) + 1.0).ln();
                if frequency > 0.0 {
                    score += idf * (frequency * (BM25_K1 + 1.0))
                        / (frequency
                            + BM25_K1 * (1.0 - BM25_B + BM25_B * length / average_length.max(1.0)));
                }
                // The LIKE half: a hit inside a longer word still counts.
                if lowered.contains(token.as_str()) {
                    score += LIKE_BONUS;
                }
            }
            score
        })
        .collect()
}

/// The parts one ask sees: everything when the transcript fits the budget,
/// otherwise the parts the question ranks highest, in transcript order.
fn select_context<'a>(units: &'a [AskUnit], question: &str) -> Vec<&'a AskUnit> {
    let total: usize = units.iter().map(|unit| unit.text.len()).sum();
    if total <= CONTEXT_CHAR_BUDGET {
        return units.iter().collect();
    }
    let mut ranked: Vec<usize> = (0..units.len()).collect();
    let scores = retrieval_scores(units, question);
    ranked.sort_by(|a, b| {
        scores[*b]
            .partial_cmp(&scores[*a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(b))
    });
    let mut chosen: Vec<usize> = Vec::new();
    let mut used = 0;
    for index in ranked {
        let length = units[index].text.len();
        if !chosen.is_empty() && used + length > CONTEXT_CHAR_BUDGET {
            break;
        }
        used += length;
        chosen.push(index);
        if chosen.len() >= MAX_CONTEXT_UNITS {
            break;
        }
    }
    // Back on the timeline, so the model reads the parts in order.
    chosen.sort_unstable();
    chosen.into_iter().map(|index| &units[index]).collect()
}

/* ---------- prompts and schema ------------------------------------------- */

/// The JSON schema the ask call answers within.
fn ask_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "found": {
                "type": "boolean",
                "description": "Whether the transcript parts answer the question."
            },
            "answer": {
                "type": "string",
                "description": "The answer in the transcript's language, at most four short sentences. When found is false, one short sentence saying the transcript does not say."
            },
            "quotes": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "segment": {
                            "type": "integer",
                            "description": "The number in square brackets of the transcript part the answer builds on."
                        },
                        "text": {
                            "type": "string",
                            "description": "A short exact quote from that part."
                        }
                    },
                    "required": ["segment", "text"],
                    "additionalProperties": false
                },
                "description": "The transcript parts the answer builds on. Empty when found is false."
            }
        },
        "required": ["found", "answer", "quotes"],
        "additionalProperties": false
    })
}

/// Instructions for answering from the transcript parts.
const ASK_SYSTEM_PROMPT: &str = "You are a meeting assistant. You receive one question and numbered parts of a meeting transcript. Answer the question using only what those parts say. If the parts do not answer the question, set \"found\" to false, leave \"quotes\" empty, and make \"answer\" one short sentence saying that the transcript does not say — never invent names, numbers, dates, decisions or facts. When the parts do answer it, set \"found\" to true, write the answer in the same language as the transcript in at most four short sentences, and list in \"quotes\" every part the answer builds on, as {\"segment\": number, \"text\": a short exact quote from that part}. The segment numbers are the ones in square brackets before each part.";

/// Said in the prompt when retrieval picked parts out of a longer
/// transcript, so the model knows more of it exists.
const SELECTION_NOTE: &str =
    "The parts below are the parts of the transcript that look most relevant to the question; the transcript was longer.";

/// Added to the prompt when an endpoint rejected structured output and the
/// ask falls back to a plain request: the answer must still be pure JSON.
const JSON_ONLY_INSTRUCTIONS: &str = "Respond with only a JSON object with exactly the keys \"found\" (boolean), \"answer\" (string) and \"quotes\" (array of {\"segment\": number, \"text\": string}). No other text.";

/// The user message: the question, then the numbered transcript parts.
fn build_user_content(question: &str, context: &[&AskUnit], selected: bool) -> String {
    let mut content = String::new();
    content.push_str("Question: ");
    content.push_str(question);
    content.push_str("\n\n");
    if selected {
        content.push_str(SELECTION_NOTE);
        content.push_str("\n\n");
    }
    content.push_str("Transcript parts:\n");
    for unit in context {
        content.push_str(&format!("[{}] {}\n", unit.id, unit.text));
    }
    content
}

/* ---------- reading the model's answer ------------------------------------ */

/// The answer fields, without the facts the outcome adds (model). Every
/// field defaults, so a partial answer still parses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
struct AskFields {
    #[serde(default)]
    found: bool,
    #[serde(default)]
    answer: String,
    #[serde(default)]
    quotes: Vec<AskQuoteFields>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
struct AskQuoteFields {
    #[serde(default)]
    segment: i64,
    #[serde(default)]
    text: String,
}

/// Read the answer out of a model reply: JSON when structured output held,
/// and the first JSON object in the text when it did not. Returns `None`
/// when no answer can be read without inventing one.
fn parse_ask_fields(content: &str) -> Option<AskFields> {
    first_json_object(content).and_then(|object| serde_json::from_str(&object).ok())
}

/// The parts the answer may cite: one quote per cited part, each kept as the
/// part's verbatim text, so a quote can never be anything but a real line of
/// the transcript. Citations of numbers the prompt never showed are dropped,
/// and an answer the transcript could not give keeps no quotes at all.
fn verified_quotes(fields: &AskFields, context: &[&AskUnit]) -> Vec<MeetingQuote> {
    if !fields.found {
        return Vec::new();
    }
    let by_id: HashMap<i64, &AskUnit> = context.iter().map(|unit| (unit.id, *unit)).collect();
    let mut seen: HashSet<i64> = HashSet::new();
    let mut quotes = Vec::new();
    for cited in &fields.quotes {
        let Some(unit) = by_id.get(&cited.segment) else {
            continue;
        };
        if !seen.insert(cited.segment) {
            continue;
        }
        if cited.text.trim() != unit.text.trim() {
            debug!(
                "The model's quote for part {} paraphrases it; keeping the verbatim text",
                unit.id
            );
        }
        quotes.push(MeetingQuote {
            t0_ms: unit.t0_ms,
            t1_ms: unit.t1_ms,
            // The verbatim part, not the model's paraphrase of it.
            text: unit.text.clone(),
        });
    }
    quotes
}

/* ---------- one ask, cancellable like the note ----------------------------- */

/// Why an ask stopped without an answer.
#[derive(Clone, Debug, PartialEq, Eq)]
enum AskError {
    Cancelled,
    Unreachable(String),
    InvalidResponse(String),
}

/// The asks in flight, one per entry, so cancel can drop an in-flight
/// request and a second question for the same entry is refused.
fn asks() -> &'static Mutex<HashMap<i64, Arc<AtomicBool>>> {
    static RUNS: OnceLock<Mutex<HashMap<i64, Arc<AtomicBool>>>> = OnceLock::new();
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_ask(entry_id: i64) -> Option<Arc<AtomicBool>> {
    let mut runs = asks().lock().unwrap();
    if runs.contains_key(&entry_id) {
        return None;
    }
    let flag = Arc::new(AtomicBool::new(false));
    runs.insert(entry_id, Arc::clone(&flag));
    Some(flag)
}

/// Ask the ask of `entry_id` to stop; `true` when one was running.
pub fn cancel_ask(entry_id: i64) -> bool {
    match asks().lock().unwrap().get(&entry_id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

fn finish_ask(entry_id: i64) {
    asks().lock().unwrap().remove(&entry_id);
}

/// Removes the ask from the registry when it ends, however it ends, so a
/// failed ask can never leave its entry stuck as asking.
struct AskGuard(i64);

impl Drop for AskGuard {
    fn drop(&mut self) {
        finish_ask(self.0);
    }
}

/// One structured ask call, with a cancellation check that also aborts an
/// in-flight request, and a prompt-only fallback for endpoints that reject
/// structured output (older local servers).
async fn request_answer(
    llm: &NoteLlm,
    question: &str,
    context: &[&AskUnit],
    selected: bool,
    cancelled: &AtomicBool,
) -> Result<AskFields, AskError> {
    let is_cancelled = || cancelled.load(Ordering::Relaxed);
    let user_content = build_user_content(question, context, selected);
    let call = llm_client::send_chat_completion_with_schema(
        &llm.provider,
        llm.api_key.clone(),
        &llm.model,
        user_content.clone(),
        Some(ASK_SYSTEM_PROMPT.to_string()),
        Some(ask_json_schema()),
        llm.disable_reasoning,
    );
    let answer = match complete_unless_cancelled(call, is_cancelled).await {
        Some(answer) => answer,
        None => return Err(AskError::Cancelled),
    };
    match answer {
        Ok(Some(content)) => parse_ask_fields(&content).ok_or_else(|| {
            AskError::InvalidResponse("The answer contained no readable object".to_string())
        }),
        Ok(None) => Err(AskError::InvalidResponse(
            "The model returned no content".to_string(),
        )),
        Err(detail) => {
            // An endpoint that rejects the structured request outright (400/422)
            // is almost always one without structured-output support; ask once
            // more without the schema, demanding pure JSON in the prompt.
            if !detail.contains("status 400") && !detail.contains("status 422") {
                return Err(AskError::Unreachable(detail));
            }
            info!("Endpoint rejected structured output; retrying with a JSON-only prompt");
            let fallback_prompt = format!("{user_content}\n\n{JSON_ONLY_INSTRUCTIONS}");
            let call = llm_client::send_chat_completion(
                &llm.provider,
                llm.api_key.clone(),
                &llm.model,
                fallback_prompt,
                llm.disable_reasoning,
            );
            let answer = match complete_unless_cancelled(call, is_cancelled).await {
                Some(answer) => answer,
                None => return Err(AskError::Cancelled),
            };
            match answer {
                Ok(Some(content)) => parse_ask_fields(&content).ok_or_else(|| {
                    AskError::InvalidResponse(
                        "The fallback answer contained no readable object".to_string(),
                    )
                }),
                Ok(None) => Err(AskError::InvalidResponse(
                    "The model returned no content".to_string(),
                )),
                Err(detail) => Err(AskError::Unreachable(detail)),
            }
        }
    }
}

/// Ask one question about a finished meeting's transcript and return the
/// local LLM's answer with the transcript parts it cites. Nothing is saved
/// on the entry: the transcript is never touched, and the conversation lives
/// only where it was asked.
pub async fn ask_about_entry(
    app: &AppHandle,
    entry_id: i64,
    question: &str,
) -> Result<MeetingAskOutcome> {
    let question = question.trim();
    if question.is_empty() {
        return Ok(MeetingAskOutcome::Failed {
            reason: MeetingAskFailure::EmptyQuestion,
            detail: None,
        });
    }

    let settings = get_settings(app);
    let llm = match NoteLlm::resolve(&settings) {
        Ok(llm) => llm,
        Err(reason) => {
            info!("Ask for entry {entry_id} refused: {reason:?}");
            return Ok(MeetingAskOutcome::Failed {
                reason: match reason {
                    MeetingNoteFailure::NoProvider => MeetingAskFailure::NoProvider,
                    MeetingNoteFailure::NoModel => MeetingAskFailure::NoModel,
                    // `resolve` only refuses for these reasons.
                    _ => MeetingAskFailure::NoLocalLlm,
                },
                detail: None,
            });
        }
    };

    let history = app.state::<Arc<HistoryManager>>();
    let entry = history
        .get_entry_by_id(entry_id)
        .await
        .map_err(|e| anyhow!("Failed to read the history entry {entry_id}: {e}"))?;
    let Some(entry) = entry else {
        return Ok(MeetingAskOutcome::Failed {
            reason: MeetingAskFailure::NoTranscript,
            detail: Some(format!("History entry {entry_id} was not found")),
        });
    };
    if entry.source != Some(HistorySource::Meeting) {
        return Ok(MeetingAskOutcome::Failed {
            reason: MeetingAskFailure::NotAMeeting,
            detail: None,
        });
    }

    let units = ask_units(&entry);
    if units.is_empty() {
        return Ok(MeetingAskOutcome::Failed {
            reason: MeetingAskFailure::NoTranscript,
            detail: None,
        });
    }

    let Some(cancelled) = register_ask(entry_id) else {
        return Ok(MeetingAskOutcome::Failed {
            reason: MeetingAskFailure::AlreadyAsking,
            detail: None,
        });
    };
    let _ask = AskGuard(entry_id);

    let context = select_context(&units, question);
    let selected = context.len() < units.len();
    info!(
        "Asking about entry {entry_id} over {} of {} transcript parts with model '{}'",
        context.len(),
        units.len(),
        llm.model
    );
    let fields = match request_answer(&llm, question, &context, selected, &cancelled).await {
        Ok(fields) => fields,
        Err(AskError::Cancelled) => {
            info!("The ask about entry {entry_id} was cancelled");
            return Ok(MeetingAskOutcome::Failed {
                reason: MeetingAskFailure::Cancelled,
                detail: None,
            });
        }
        Err(AskError::Unreachable(detail)) => {
            error!("The ask about entry {entry_id} failed: {detail}");
            return Ok(MeetingAskOutcome::Failed {
                reason: MeetingAskFailure::LlmUnreachable,
                detail: Some(detail),
            });
        }
        Err(AskError::InvalidResponse(detail)) => {
            error!("The ask about entry {entry_id} failed: {detail}");
            return Ok(MeetingAskOutcome::Failed {
                reason: MeetingAskFailure::InvalidResponse,
                detail: Some(detail),
            });
        }
    };

    let answer = MeetingAnswer {
        found: fields.found,
        answer: fields.answer.trim().to_string(),
        quotes: verified_quotes(&fields, &context),
        model: llm.model.clone(),
    };
    Ok(MeetingAskOutcome::Answered { answer })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::transcription::TranscriptSegment;

    fn unit(id: i64, text: &str) -> AskUnit {
        AskUnit {
            id,
            t0_ms: Some(id * 1000),
            t1_ms: Some(id * 1000 + 900),
            text: text.to_string(),
        }
    }

    fn entry_with(segments: &[(i64, i64, &str)]) -> HistoryEntry {
        HistoryEntry {
            id: 1,
            file_name: "meeting.wav".to_string(),
            timestamp: 1_735_689_600,
            saved: false,
            title: "Møte".to_string(),
            transcription_text: String::new(),
            post_processed_text: None,
            post_process_prompt: None,
            post_process_requested: false,
            source_name: None,
            language: None,
            language_source: None,
            source: Some(HistorySource::Meeting),
            metadata: Some(crate::managers::history::HistoryMetadata {
                duration_secs: Some(60),
                segments: Some(
                    segments
                        .iter()
                        .map(|(t0, t1, text)| TranscriptSegment {
                            t0_ms: *t0,
                            t1_ms: *t1,
                            text: text.to_string(),
                            speaker_id: None,
                        })
                        .collect(),
                ),
                meeting_note: None,
                speaker_names: None,
                system_audio: None,
            }),
        }
    }

    #[test]
    fn a_short_transcript_is_passed_whole() {
        let units: Vec<AskUnit> = (1..=5)
            .map(|id| unit(id, &format!("Del {id} av møtet om budsjettet.")))
            .collect();
        let context = select_context(&units, "Hva ble sagt om budsjettet?");
        assert_eq!(context.len(), 5, "{context:?}");
    }

    #[test]
    fn a_long_transcript_ranks_the_matching_parts_first() {
        let mut units = Vec::new();
        for id in 1..=400 {
            let text = if id == 7 || id == 250 {
                format!("Vi bestemte at budsjettet settes ned med ti prosent i dag, del {id}.")
            } else {
                format!("Møtet fortsatte om generelle temaer uten konkrete beslutninger, del {id}.")
            };
            units.push(unit(id, &text));
        }
        let context = select_context(&units, "Hva ble bestemt om budsjettet?");
        assert!(context.len() < units.len(), "{context:?}");
        let ids: Vec<i64> = context.iter().map(|unit| unit.id).collect();
        assert!(ids.contains(&7), "{ids:?}");
        assert!(ids.contains(&250), "{ids:?}");
        // The parts ride on the prompt in transcript order.
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted);
        // And the budget holds.
        let total: usize = context.iter().map(|unit| unit.text.len()).sum();
        assert!(total <= CONTEXT_CHAR_BUDGET, "{total}");
    }

    #[test]
    fn a_question_of_stopwords_alone_falls_back_to_the_opening() {
        let units: Vec<AskUnit> = (1..=200)
            .map(|id| unit(id, &format!("Sak {id} om lanseringen av produktet.")))
            .collect();
        let context = select_context(&units, "Hva om det?");
        assert!(!context.is_empty());
        assert_eq!(context[0].id, 1, "{context:?}");
        let total: usize = context.iter().map(|unit| unit.text.len()).sum();
        assert!(total <= CONTEXT_CHAR_BUDGET, "{total}");
    }

    #[test]
    fn the_compound_like_bonus_ranks_a_compound_part() {
        let units = vec![
            unit(1, "Vi snakket lenge om generelle ting."),
            unit(
                2,
                "Deretter tok vi opp planleggingsmøtet for neste kvartal.",
            ),
        ];
        let scores = retrieval_scores(&units, "planlegging");
        assert!(scores[1] > scores[0], "{scores:?}");
    }

    #[test]
    fn segments_become_numbered_parts_and_plain_text_groups() {
        let entry = entry_with(&[(0, 1000, "Hei."), (1000, 2000, "På gjensyn.")]);
        let units = ask_units(&entry);
        assert_eq!(units.len(), 2);
        assert_eq!(units[0].id, 1);
        assert_eq!(units[0].t0_ms, Some(0));
        assert_eq!(units[1].text, "På gjensyn.");

        let mut plain = entry_with(&[]);
        plain.metadata = None;
        plain.transcription_text = "Første setning. Andre setning.".to_string();
        let units = ask_units(&plain);
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].t0_ms, None);
        assert_eq!(units[0].text, "Første setning.\nAndre setning.");
    }

    #[test]
    fn the_prompt_names_the_question_and_the_parts() {
        let units = vec![unit(1, "Vi ble enige."), unit(2, "Møtet tok ti minutter.")];
        let context: Vec<&AskUnit> = units.iter().collect();
        let content = build_user_content("Hvor lenge varte møtet?", &context, false);
        assert!(
            content.contains("Question: Hvor lenge varte møtet?"),
            "{content}"
        );
        assert!(content.contains("[1] Vi ble enige."), "{content}");
        assert!(content.contains("[2] Møtet tok ti minutter."), "{content}");
        let selected = build_user_content("Hvor lenge?", &context, true);
        assert!(selected.contains(SELECTION_NOTE), "{selected}");
    }

    #[test]
    fn a_clean_json_answer_is_read_as_fields() {
        let fields =
            parse_ask_fields("{\"found\":true,\"answer\":\"Ti minutter.\",\"quotes\":[{\"segment\":2,\"text\":\"Møtet tok ti minutter.\"}]}");
        let fields = fields.expect("a clean object parses");
        assert!(fields.found);
        assert_eq!(fields.answer, "Ti minutter.");
        assert_eq!(fields.quotes.len(), 1);
        assert_eq!(fields.quotes[0].segment, 2);
    }

    #[test]
    fn fenced_and_prose_wrapped_answers_are_read() {
        let fenced = parse_ask_fields(
            "```json\n{\"found\":false,\"answer\":\"Står ikke der.\",\"quotes\":[]}\n```",
        );
        assert!(fenced.expect("fenced JSON parses").quotes.is_empty());
        let prose =
            parse_ask_fields("Selvfølgelig! {\"found\":true,\"answer\":\"Ja.\",\"quotes\":[]}");
        assert!(prose.expect("prose-wrapped JSON parses").found);
    }

    #[test]
    fn answers_without_json_are_refused() {
        assert!(parse_ask_fields("Møtet varte ti minutter.").is_none());
        assert!(parse_ask_fields("").is_none());
    }

    #[test]
    fn quotes_keep_only_real_verbatim_parts() {
        let units = vec![unit(1, "Vi ble enige."), unit(2, "Møtet tok ti minutter.")];
        let context: Vec<&AskUnit> = units.iter().collect();
        let fields = AskFields {
            found: true,
            answer: "Ti minutter.".to_string(),
            quotes: vec![
                AskQuoteFields {
                    segment: 2,
                    text: "ti minutter (paraphrase)".to_string(),
                },
                AskQuoteFields {
                    segment: 2,
                    text: "duplicate".to_string(),
                },
                AskQuoteFields {
                    segment: 99,
                    text: "never shown".to_string(),
                },
                AskQuoteFields {
                    segment: 1,
                    text: "enige".to_string(),
                },
            ],
        };
        let quotes = verified_quotes(&fields, &context);
        assert_eq!(quotes.len(), 2, "{quotes:?}");
        // The verbatim part text, not the model's paraphrase of it.
        assert_eq!(quotes[0].text, "Møtet tok ti minutter.");
        assert_eq!(quotes[0].t0_ms, Some(2000));
        assert_eq!(quotes[1].text, "Vi ble enige.");
    }

    #[test]
    fn an_answer_the_transcript_could_not_give_has_no_quotes() {
        let units = vec![unit(1, "Vi ble enige.")];
        let context: Vec<&AskUnit> = units.iter().collect();
        let fields = AskFields {
            found: false,
            answer: "Transkriptet sier ikke.".to_string(),
            quotes: vec![AskQuoteFields {
                segment: 1,
                text: "invented".to_string(),
            }],
        };
        assert!(verified_quotes(&fields, &context).is_empty());
    }

    #[test]
    fn an_ask_registers_cancels_and_finishes() {
        assert!(register_ask(42).is_some());
        assert!(register_ask(42).is_none(), "a second ask is refused");
        assert!(cancel_ask(42));
        finish_ask(42);
        assert!(register_ask(42).is_some(), "the entry asks again");
        finish_ask(42);
    }
}
