//! The meeting note: a draft summary a local LLM writes over a finished
//! meeting's transcript, on an explicit click and nothing else. The note has
//! four fields — a short summary, decisions, action items (with an owner only
//! when the transcript names one) and open questions — and is kept beside the
//! transcript, in the entry's metadata; the transcript itself is never
//! touched, and nothing about the note leaves the machine.
//!
//! The transcript of a long meeting does not fit the context of a small local
//! model, so the note is built as map-reduce: the transcript is chunked on
//! sentence boundaries, one structured call extracts a partial note per chunk,
//! and the partial notes are merged in batches until one note remains. The
//! only network traffic of a run is the configured post-processing endpoint,
//! and [`NoteLlm::resolve`] refuses any endpoint that is not loopback, so
//! the note can never leave the machine even if the provider list holds a
//! cloud API.

use crate::actions::{complete_unless_cancelled, first_json_object};
use crate::llm_client;
use crate::managers::history::{HistoryEntry, HistoryManager, HistorySource};
use crate::settings::{get_settings, AppSettings, PostProcessApiFormat, PostProcessProvider};
use anyhow::{anyhow, Result};
use log::{error, info};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Manager};

/// Characters of transcript one extraction call sees. Norwegian runs at
/// roughly three characters per token, so a chunk is about 1200 tokens of
/// context — small enough for the default context of a local model, with room
/// for the prompt and the structured answer.
const CHUNK_TARGET_CHARS: usize = 3500;

/// Partial notes merged per call. Five keeps one merge answer within a small
/// model's output budget while cutting the note tree quickly: a 90-minute
/// meeting is roughly a dozen chunks, which folds to one note in two rounds.
const MERGE_BATCH: usize = 5;

/// One action item from the meeting: what to do, and who owns it. The owner is
/// the name the transcript itself mentions as responsible; an empty string
/// when no name is mentioned.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MeetingActionItem {
    #[serde(default)]
    pub task: String,
    #[serde(default)]
    pub owner: String,
}

/// One headed group of the note: a short topic and the concrete points under
/// it. The reference's note reads like this — a lead paragraph, then sections
/// such as "Pricing" or "Customers and market" — instead of fixed buckets.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MeetingSection {
    #[serde(default)]
    pub heading: String,
    #[serde(default)]
    pub bullets: Vec<String>,
}

/// The generated meeting note: a draft the user must check, not a record of
/// what was decided. `model` names the LLM that wrote it, and `generated_at`
/// when, so the label in the transcript window can say both.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct MeetingNote {
    pub summary: String,
    /// Topic sections, the shape the reference's overview uses. Notes written
    /// before this field existed read as an empty list and fall back to the
    /// fixed buckets below.
    #[serde(default)]
    pub sections: Vec<MeetingSection>,
    pub decisions: Vec<String>,
    pub action_items: Vec<MeetingActionItem>,
    pub open_questions: Vec<String>,
    pub model: String,
    /// Unix seconds when the note was generated.
    pub generated_at: i64,
}

impl MeetingNote {
    fn from_fields(fields: NoteFields, model: String) -> Self {
        Self {
            summary: fields.summary,
            sections: fields.sections,
            decisions: fields.decisions,
            action_items: fields.action_items,
            open_questions: fields.open_questions,
            model,
            generated_at: chrono::Utc::now().timestamp(),
        }
    }
}

/// Why a note could not be generated. The frontend maps each reason to a
/// localized message; `detail` never reaches the user, only the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum MeetingNoteFailure {
    /// The entry is not a meeting session.
    NotAMeeting,
    /// The entry has no transcript to make a note of.
    NoTranscript,
    /// No post-processing provider is selected.
    NoProvider,
    /// The selected provider has no model configured.
    NoModel,
    /// The selected provider cannot write notes: it is not a local
    /// OpenAI-compatible endpoint (a cloud API, or Apple Intelligence, which
    /// has no structured-output API).
    NoLocalLlm,
    /// A generation is already running for this entry.
    AlreadyGenerating,
    /// The local LLM could not be reached, or answered with an error.
    LlmUnreachable,
    /// The local LLM answered, but no note could be read from the answer.
    InvalidResponse,
    /// The user pressed cancel; nothing was saved.
    Cancelled,
}

/// What a click on "generate" comes back with: the note, or why there is none.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum MeetingNoteOutcome {
    /// The note was generated and saved on the entry's metadata.
    Generated { note: MeetingNote },
    /// Nothing was saved; `reason` tells the window what to tell the user.
    Failed {
        reason: MeetingNoteFailure,
        detail: Option<String>,
    },
}

/* ---------- the local LLM the note is allowed to talk to --------------- */

/// The endpoint, model and key one note run uses, resolved once from the
/// post-processing settings. The same LLM the user configured for dictation
/// cleanup writes the notes; there is no separate note configuration.
#[derive(Clone, Debug)]
pub(crate) struct NoteLlm {
    pub(crate) provider: PostProcessProvider,
    pub(crate) model: String,
    pub(crate) api_key: String,
    pub(crate) disable_reasoning: bool,
}

impl NoteLlm {
    /// Resolve the note's LLM from the post-processing settings, refusing
    /// everything that is not a local OpenAI-compatible endpoint. This is the
    /// privacy gate of the feature: a run can only ever reach loopback.
    pub(crate) fn resolve(settings: &AppSettings) -> Result<Self, MeetingNoteFailure> {
        let provider = settings
            .active_post_process_provider()
            .cloned()
            .ok_or(MeetingNoteFailure::NoProvider)?;
        let model = settings
            .post_process_models
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        if model.trim().is_empty() {
            return Err(MeetingNoteFailure::NoModel);
        }
        // Structured output rides on /chat/completions; an Anthropic-format
        // endpoint has no such route, and Apple Intelligence is not an HTTP
        // endpoint at all. Both are refused rather than half-supported.
        if provider.api_format != PostProcessApiFormat::OpenAi
            || !is_local_llm_endpoint(&provider.base_url)
        {
            return Err(MeetingNoteFailure::NoLocalLlm);
        }
        let api_key = settings
            .post_process_api_keys
            .get(&provider.id)
            .cloned()
            .unwrap_or_default();
        let disable_reasoning = matches!(provider.id.as_str(), "custom" | "openrouter");
        Ok(Self {
            provider,
            model,
            api_key,
            disable_reasoning,
        })
    }
}

/// Whether an endpoint base URL points at this machine: `localhost`, a
/// loopback IP, or nothing. The note refuses every other host, so a meeting
/// transcript can never be sent to a cloud API by mistake.
pub fn is_local_llm_endpoint(base_url: &str) -> bool {
    let Some(url) = reqwest::Url::parse(base_url).ok() else {
        return false;
    };
    // Only plain HTTP counts: a native scheme such as
    // `apple-intelligence://local` has no structured-output API to call.
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_lowercase();
    if host == "localhost" {
        return true;
    }
    // An IP is loopback only as an IP: a hostname that merely resolves to
    // loopback is not checked, because DNS is not consulted here.
    host.parse::<std::net::IpAddr>()
        .map(|ip| ip.is_loopback())
        .unwrap_or(false)
}

/* ---------- chunking: the transcript the model sees -------------------- */

/// Split `text` into sentences, keeping the punctuation. A sentence ends at
/// `.`, `!`, `?` or a line break; whitespace around the pieces is trimmed.
pub(crate) fn sentences(text: &str) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        current.push(character);
        if matches!(character, '.' | '!' | '?' | '\n') {
            let piece = current.trim().to_string();
            if !piece.is_empty() {
                pieces.push(piece);
            }
            current.clear();
        }
    }
    let tail = current.trim().to_string();
    if !tail.is_empty() {
        pieces.push(tail);
    }
    pieces
}

/// Hard-split a piece with no sentence breaks into runs of at most `target`
/// characters, on character boundaries so no text is corrupted.
fn hard_runs(text: &str, target: usize) -> Vec<String> {
    let mut runs = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        current.push(character);
        if current.chars().count() >= target {
            runs.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs
}

/// Split one piece longer than `target` into parts of at most `target`
/// characters, cutting at sentence boundaries where the piece has them.
fn split_long_piece(text: &str, target: usize) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for sentence in sentences(text) {
        let sentence_len = sentence.chars().count();
        // A sentence that alone busts the budget is hard-split; no sentence of
        // a transcript is that long, but a hallucinating chunker must not
        // loop forever either.
        if sentence_len > target {
            if !current.is_empty() {
                parts.push(std::mem::take(&mut current));
                current_len = 0;
            }
            parts.extend(hard_runs(&sentence, target));
            continue;
        }
        if current_len + sentence_len > target && !current.is_empty() {
            parts.push(std::mem::take(&mut current));
            current_len = 0;
        }
        if !current.is_empty() {
            current.push(' ');
            current_len += 1;
        }
        current.push_str(&sentence);
        current_len += sentence_len;
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

/// Pack the transcript's ordered pieces into chunks of at most `target`
/// characters. A piece is kept whole when it fits; only a piece longer than
/// the target is cut, and then on sentence boundaries.
pub(crate) fn pack_chunks(pieces: &[String], target: usize) -> Vec<String> {
    let mut sized: Vec<String> = Vec::new();
    for piece in pieces {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if piece.chars().count() > target {
            sized.extend(split_long_piece(piece, target));
        } else {
            sized.push(piece.to_string());
        }
    }

    let mut chunks: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for piece in sized {
        let piece_len = piece.chars().count();
        if current_len + piece_len > target && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
            current_len = 0;
        }
        if !current.is_empty() {
            current.push('\n');
            current_len += 1;
        }
        current.push_str(&piece);
        current_len += piece_len;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// The transcript's ordered pieces: the timed segments when the model
/// produced them, else the plain transcription cut into sentences. Old
/// meetings without segments still get a note.
fn transcript_pieces(entry: &HistoryEntry) -> Vec<String> {
    if let Some(segments) = entry.metadata.as_ref().and_then(|m| m.segments.as_ref()) {
        if !segments.is_empty() {
            return segments
                .iter()
                .map(|segment| segment.text.clone())
                .collect();
        }
    }
    sentences(&entry.transcription_text)
}

/* ---------- prompts and schema ------------------------------------------ */

/// The JSON schema the extraction and merge calls answer within. `owner` is a
/// plain string so a strict schema cannot demand a name the text never gave.
fn note_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "summary": {
                "type": "string",
                "description": "A short summary of this part of the meeting, 2-4 sentences."
            },
            "sections": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "heading": {
                            "type": "string",
                            "description": "A short topic heading in Title case, such as 'Pricing' or 'Customers and market'. Name the subject, never a bucket like 'Decisions' or 'Summary'."
                        },
                        "bullets": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "The concrete points on that topic: names, numbers, dates, status and what was said. One short sentence each. No sub-bullets."
                        }
                    },
                    "required": ["heading", "bullets"],
                    "additionalProperties": false
                },
                "description": "Two to six topic sections covering this part, each with the points that matter."
            },
            "decisions": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Concrete decisions that were made, one short sentence each."
            },
            "action_items": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "task": {
                            "type": "string",
                            "description": "One task someone must do after the meeting, as a short sentence."
                        },
                        "owner": {
                            "type": "string",
                            "description": "The name of the person responsible, exactly as the transcript mentions it. An empty string when no name is mentioned."
                        }
                    },
                    "required": ["task", "owner"],
                    "additionalProperties": false
                }
            },
            "open_questions": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Questions that were left open or need a follow-up, one short sentence each."
            }
        },
        "required": ["summary", "sections", "decisions", "action_items", "open_questions"],
        "additionalProperties": false
    })
}

/// Instructions for extracting a partial note from one chunk.
const EXTRACT_SYSTEM_PROMPT: &str = "You are a meeting assistant. You receive one part of a longer meeting transcript. Write a draft note for this part as JSON with the fields summary, sections, decisions, action_items and open_questions. summary is 2-3 short sentences about what this part covers. sections groups the part's substance by topic: each section has a short heading that names the subject (such as \"Pricing\" or \"Customers and market\", never \"Summary\" or \"Decisions\") and bullets with the concrete points under it — names, numbers, dates, status and what was said, one short sentence each. Two to six sections per part, and a point belongs to exactly one section. decisions lists each concrete decision that was made, one short sentence each. action_items lists each task someone must do after the meeting, as {\"task\": string, \"owner\": string}; owner is the name of the person responsible, exactly as the transcript mentions it, or an empty string when no name is mentioned. open_questions lists each question that was left open or needs a follow-up. Use only what the transcript says: never invent names, dates, numbers or tasks. Keep every item short. Write in the same language as the transcript.";

/// Instructions for merging partial notes into one note.
const MERGE_SYSTEM_PROMPT: &str = "You are a meeting assistant. You receive partial notes taken from consecutive parts of one meeting. Combine them into a single draft note as JSON with the fields summary, sections, decisions, action_items and open_questions. The summary is one paragraph of 3-5 short sentences that says what the meeting was about, who took part when the transcript names them, and where it landed. sections is the body: merge sections that cover the same topic under one heading, keep the headings short and about the subject, and keep every concrete point as a bullet — names, numbers, dates, prices and status. Drop a point that a later part contradicts. Merge duplicate and near-duplicate items into one. Keep the owner of an action item when a name is given. Drop nothing that matters. Write in the same language as the notes.";

/// Added to the prompt when an endpoint rejected structured output and the
/// note falls back to a plain request: the answer must still be pure JSON.
const JSON_ONLY_INSTRUCTIONS: &str = "Respond with only a JSON object with exactly the keys \"summary\" (string), \"sections\" (array of {\"heading\": string, \"bullets\": array of strings}), \"decisions\" (array of strings), \"action_items\" (array of {\"task\": string, \"owner\": string}) and \"open_questions\" (array of strings). No other text.";

/* ---------- reading the model's answer ----------------------------------- */

/// The four note fields, without the facts the entry keeps (model, time).
/// Every field defaults, so a partial or older answer still parses.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct NoteFields {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    sections: Vec<MeetingSection>,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    action_items: Vec<MeetingActionItem>,
    #[serde(default)]
    open_questions: Vec<String>,
}

/// Read a note out of a model answer: JSON when structured output held, and
/// the first JSON object in the text when it did not. Returns `None` when no
/// note can be read without inventing one.
/// Read a note out of a model answer: JSON when structured output held, and
/// the first JSON object in the text when it did not. Returns `None` when no
/// note can be read without inventing one.
fn parse_note_fields(content: &str) -> Option<NoteFields> {
    first_json_object(content).and_then(|object| serde_json::from_str(&object).ok())
}

/* ---------- the run: extract per chunk, merge to one note --------------- */

/// Why a run stopped without a note.
#[derive(Clone, Debug, PartialEq, Eq)]
enum NoteGenerationError {
    /// The user pressed cancel.
    Cancelled,
    /// The endpoint could not be reached, or answered with an HTTP error.
    Unreachable(String),
    /// An answer arrived, but no note could be read from it.
    InvalidResponse(String),
}

/// One structured call to the note's LLM, with a cancellation check that also
/// aborts an in-flight request, and a prompt-only fallback for endpoints that
/// reject structured output (older local servers).
async fn request_note_fields(
    llm: &NoteLlm,
    system_prompt: &str,
    user_content: String,
    cancelled: &AtomicBool,
) -> Result<NoteFields, NoteGenerationError> {
    let is_cancelled = || cancelled.load(Ordering::Relaxed);
    let call = llm_client::send_chat_completion_with_schema(
        &llm.provider,
        llm.api_key.clone(),
        &llm.model,
        user_content.clone(),
        Some(system_prompt.to_string()),
        Some(note_json_schema()),
        llm.disable_reasoning,
    );
    // The poll keeps the request itself droppable, so cancel stops an
    // in-flight call, not just the calls after it.
    let answer = match complete_unless_cancelled(call, is_cancelled).await {
        Some(answer) => answer,
        None => return Err(NoteGenerationError::Cancelled),
    };
    match answer {
        Ok(Some(content)) => parse_note_fields(&content).ok_or_else(|| {
            NoteGenerationError::InvalidResponse(
                "The answer contained no readable note".to_string(),
            )
        }),
        Ok(None) => Err(NoteGenerationError::InvalidResponse(
            "The model returned no content".to_string(),
        )),
        Err(detail) => {
            // An endpoint that rejects the structured request outright (400/422)
            // is almost always one without structured-output support; ask once
            // more without the schema, demanding pure JSON in the prompt.
            if !detail.contains("status 400") && !detail.contains("status 422") {
                return Err(NoteGenerationError::Unreachable(detail));
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
                None => return Err(NoteGenerationError::Cancelled),
            };
            match answer {
                Ok(Some(content)) => parse_note_fields(&content).ok_or_else(|| {
                    NoteGenerationError::InvalidResponse(
                        "The fallback answer contained no readable note".to_string(),
                    )
                }),
                Ok(None) => Err(NoteGenerationError::InvalidResponse(
                    "The model returned no content".to_string(),
                )),
                Err(detail) => Err(NoteGenerationError::Unreachable(detail)),
            }
        }
    }
}

/// Build the note over the transcript's chunks: extract a partial note per
/// chunk, then merge the partials in batches of [`MERGE_BATCH`] until one
/// note remains. Stops with [`NoteGenerationError::Cancelled`] as soon as the
/// cancel flag is set, between calls and inside one.
async fn generate_note_fields(
    llm: &NoteLlm,
    chunks: &[String],
    cancelled: &AtomicBool,
) -> Result<NoteFields, NoteGenerationError> {
    let total = chunks.len();
    let mut partials = Vec::with_capacity(total);
    for (index, chunk) in chunks.iter().enumerate() {
        let user_content = format!(
            "MEETING TRANSCRIPT (part {} of {}):\n\n{chunk}",
            index + 1,
            total
        );
        partials
            .push(request_note_fields(llm, EXTRACT_SYSTEM_PROMPT, user_content, cancelled).await?);
    }

    let mut level = partials;
    while level.len() > 1 {
        let mut merged = Vec::new();
        for batch in level.chunks(MERGE_BATCH) {
            let serialized = serde_json::to_string(batch).unwrap_or_else(|_| "[]".to_string());
            let user_content = format!(
                "PARTIAL MEETING NOTES from {} consecutive parts of the same meeting, in order:\n\n{serialized}",
                batch.len()
            );
            merged.push(
                request_note_fields(llm, MERGE_SYSTEM_PROMPT, user_content, cancelled).await?,
            );
        }
        level = merged;
    }

    Ok(level.pop().unwrap_or_default())
}

/* ---------- the cancel registry ------------------------------------------ */

/// One cancel flag per entry with a run in flight, so the Avbryt button of
/// the window that started a run stops that run alone.
static RUNS: OnceLock<Mutex<HashMap<i64, Arc<AtomicBool>>>> = OnceLock::new();

fn runs() -> &'static Mutex<HashMap<i64, Arc<AtomicBool>>> {
    RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Register a run for `entry_id`, returning its cancel flag; `None` when a
/// run is already in flight for the entry.
fn register_run(entry_id: i64) -> Option<Arc<AtomicBool>> {
    let mut runs = runs().lock().unwrap();
    if runs.contains_key(&entry_id) {
        return None;
    }
    let flag = Arc::new(AtomicBool::new(false));
    runs.insert(entry_id, Arc::clone(&flag));
    Some(flag)
}

/// Ask the run of `entry_id` to stop; `true` when a run was running.
pub fn cancel_run(entry_id: i64) -> bool {
    match runs().lock().unwrap().get(&entry_id) {
        Some(flag) => {
            flag.store(true, Ordering::Relaxed);
            true
        }
        None => false,
    }
}

fn finish_run(entry_id: i64) {
    runs().lock().unwrap().remove(&entry_id);
}

/// Removes the run from the registry when the generation ends, however it
/// ends, so a failed run can never leave its entry stuck as generating.
struct RunGuard(i64);

impl Drop for RunGuard {
    fn drop(&mut self) {
        finish_run(self.0);
    }
}

/* ---------- the entry-facing run ----------------------------------------- */

/// Generate the meeting note for a history entry: resolve the local LLM, read
/// the entry's transcript, chunk it, run the extraction and merge, and save
/// the note on the entry's metadata. The transcript is never modified, and
/// nothing is saved unless a whole note came back.
pub async fn generate_for_entry(app: &AppHandle, entry_id: i64) -> Result<MeetingNoteOutcome> {
    let settings = get_settings(app);
    let llm = match NoteLlm::resolve(&settings) {
        Ok(llm) => llm,
        Err(reason) => {
            info!("Meeting note for entry {entry_id} refused: {reason:?}");
            return Ok(MeetingNoteOutcome::Failed {
                reason,
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
        return Ok(MeetingNoteOutcome::Failed {
            reason: MeetingNoteFailure::NoTranscript,
            detail: Some(format!("History entry {entry_id} was not found")),
        });
    };
    if entry.source != Some(HistorySource::Meeting) {
        return Ok(MeetingNoteOutcome::Failed {
            reason: MeetingNoteFailure::NotAMeeting,
            detail: None,
        });
    }

    let pieces = transcript_pieces(&entry);
    let chunks = pack_chunks(&pieces, CHUNK_TARGET_CHARS);
    if chunks.is_empty() {
        return Ok(MeetingNoteOutcome::Failed {
            reason: MeetingNoteFailure::NoTranscript,
            detail: None,
        });
    }

    let Some(cancelled) = register_run(entry_id) else {
        return Ok(MeetingNoteOutcome::Failed {
            reason: MeetingNoteFailure::AlreadyGenerating,
            detail: None,
        });
    };
    let _run = RunGuard(entry_id);

    info!(
        "Generating the meeting note for entry {entry_id} over {} chunks with model '{}'",
        chunks.len(),
        llm.model
    );
    let result = generate_note_fields(&llm, &chunks, &cancelled).await;

    let fields = match result {
        Ok(fields) => fields,
        Err(NoteGenerationError::Cancelled) => {
            info!("Meeting note generation for entry {entry_id} was cancelled");
            return Ok(MeetingNoteOutcome::Failed {
                reason: MeetingNoteFailure::Cancelled,
                detail: None,
            });
        }
        Err(NoteGenerationError::Unreachable(detail)) => {
            error!("Meeting note generation for entry {entry_id} failed: {detail}");
            return Ok(MeetingNoteOutcome::Failed {
                reason: MeetingNoteFailure::LlmUnreachable,
                detail: Some(detail),
            });
        }
        Err(NoteGenerationError::InvalidResponse(detail)) => {
            error!("Meeting note generation for entry {entry_id} failed: {detail}");
            return Ok(MeetingNoteOutcome::Failed {
                reason: MeetingNoteFailure::InvalidResponse,
                detail: Some(detail),
            });
        }
    };

    let note = MeetingNote::from_fields(fields, llm.model.clone());
    history.save_meeting_note(entry_id, note.clone())?;
    info!(
        "Meeting note for entry {entry_id} saved ({} decisions, {} action items)",
        note.decisions.len(),
        note.action_items.len()
    );
    Ok(MeetingNoteOutcome::Generated { note })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn a_note_reads_topic_sections_and_older_notes_still_parse() {
        let body = r#"{"summary":"Møtet handlet om prising.","sections":[{"heading":"Priser","bullets":["Fast pris per bil.","Volum styrer prisen."]}],"decisions":[],"action_items":[],"open_questions":[]}"#;
        let parsed = parse_note_fields(body).expect("a note with sections parses");
        assert_eq!(parsed.sections.len(), 1);
        assert_eq!(parsed.sections[0].heading, "Priser");
        assert_eq!(parsed.sections[0].bullets.len(), 2);

        // A note written before the field existed has no sections at all.
        let old = r#"{"summary":"Kort.","decisions":["Vi lanserer."],"action_items":[],"open_questions":[]}"#;
        let parsed = parse_note_fields(old).expect("an older note still parses");
        assert!(parsed.sections.is_empty());
        assert_eq!(parsed.decisions.len(), 1);
    }

    /* ---- the endpoint gate ---- */

    #[test]
    fn only_loopback_endpoints_count_as_local() {
        for local in [
            "http://localhost:11434/v1",
            "http://127.0.0.1:11434/v1",
            "http://127.0.0.7/v1",
            "https://localhost:11434/v1/",
            "http://[::1]:11434/v1",
        ] {
            assert!(is_local_llm_endpoint(local), "{local} should be local");
        }
        for remote in [
            "https://api.openai.com/v1",
            "http://192.168.1.50:11434/v1",
            "http://example.com/v1",
            "apple-intelligence://local",
            "not a url",
            "",
        ] {
            assert!(
                !is_local_llm_endpoint(remote),
                "{remote} should not be local"
            );
        }
    }

    /* ---- chunking ---- */

    #[test]
    fn small_pieces_pack_into_chunks_within_the_budget() {
        let pieces: Vec<String> = (0..10).map(|_| "En kort setning.".to_string()).collect();
        let chunks = pack_chunks(&pieces, 40);
        assert!(!chunks.is_empty());
        for chunk in &chunks {
            assert!(chunk.chars().count() <= 40, "{chunk} fits the budget");
        }
        // Every piece's text is kept exactly once.
        let joined = chunks.join("\n");
        for piece in &pieces {
            assert!(joined.contains(piece.as_str()), "no piece is dropped");
        }
    }

    #[test]
    fn one_piece_shorter_than_the_target_stays_whole() {
        let chunks = pack_chunks(&["Hei, dette er møtet.".to_string()], 3500);
        assert_eq!(chunks, vec!["Hei, dette er møtet.".to_string()]);
    }

    #[test]
    fn a_long_piece_is_cut_on_sentence_boundaries() {
        let long = vec!["Vi bestemte at Kari sender rapporten.".repeat(200)];
        let chunks = pack_chunks(&long, 200);
        assert!(chunks.len() > 1, "the long piece is cut");
        for chunk in &chunks {
            assert!(chunk.chars().count() <= 200, "{chunk} fits the budget");
            assert!(chunk.ends_with('.'), "the cut respects sentences: {chunk}");
        }
    }

    #[test]
    fn empty_pieces_make_no_chunks() {
        assert!(pack_chunks(&[], 3500).is_empty());
        assert!(pack_chunks(&["".to_string(), "  ".to_string()], 3500).is_empty());
    }

    #[test]
    fn punctuationless_text_is_hard_split_without_corruption() {
        let chunks = pack_chunks(&["a".repeat(100)], 30);
        assert_eq!(chunks.len(), 4);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 30));
        // No characters lost.
        let total: usize = chunks.iter().map(|c| c.chars().count()).sum();
        assert_eq!(total, 100);
    }

    /* ---- reading the answer ---- */

    #[test]
    fn a_clean_json_answer_is_read_as_note_fields() {
        let content = r#"{"summary":"Møtet handlet om lanseringen.","decisions":["Vi lanserer 1. mai."],"action_items":[{"task":"Kari sender rapporten","owner":"Kari"}],"open_questions":["Hvem betaler?"]}"#;
        let fields = parse_note_fields(content).expect("clean JSON parses");
        assert_eq!(fields.summary, "Møtet handlet om lanseringen.");
        assert_eq!(fields.decisions, vec!["Vi lanserer 1. mai."]);
        assert_eq!(
            fields.action_items,
            vec![MeetingActionItem {
                task: "Kari sender rapporten".to_string(),
                owner: "Kari".to_string()
            }]
        );
        assert_eq!(fields.open_questions, vec!["Hvem betaler?"]);
    }

    #[test]
    fn an_ownerless_action_item_reads_with_an_empty_owner() {
        let content = r#"{"summary":"s","decisions":[],"action_items":[{"task":"Send rapporten","owner":""}],"open_questions":[]}"#;
        let fields = parse_note_fields(content).expect("the answer parses");
        assert_eq!(fields.action_items[0].owner, "");
    }

    #[test]
    fn fenced_and_prose_wrapped_json_is_read() {
        let fenced = "```json\n{\"summary\":\"s\",\"decisions\":[],\"action_items\":[],\"open_questions\":[]}\n```";
        assert!(parse_note_fields(fenced).is_some(), "fenced JSON parses");

        let prose = "Her er notatet:\n{\"summary\":\"Alt ble avgjort.\",\"decisions\":[],\"action_items\":[],\"open_questions\":[]}\nTakk.";
        let fields = parse_note_fields(prose).expect("JSON inside prose parses");
        assert_eq!(fields.summary, "Alt ble avgjort.");
    }

    #[test]
    fn partial_answers_default_the_missing_fields() {
        let fields =
            parse_note_fields(r#"{"summary":"Bare et sammendrag."}"#).expect("partial JSON parses");
        assert_eq!(fields.summary, "Bare et sammendrag.");
        assert!(fields.decisions.is_empty());
        assert!(fields.action_items.is_empty());
        assert!(fields.open_questions.is_empty());
    }

    #[test]
    fn answers_without_json_are_refused() {
        assert!(parse_note_fields("Jeg forstår ikke.").is_none());
        assert!(parse_note_fields("").is_none());
    }

    /* ---- the run registry ---- */

    #[test]
    fn a_run_registers_cancels_and_finishes() {
        let id = 9_001;
        let flag = register_run(id).expect("the first run registers");
        assert!(register_run(id).is_none(), "a second run is refused");
        assert!(!flag.load(Ordering::Relaxed));
        assert!(cancel_run(id), "a registered run is flagged");
        assert!(flag.load(Ordering::Relaxed), "the flag is set");
        assert!(cancel_run(id), "flagging twice is harmless");
        finish_run(id);
        assert!(
            !cancel_run(id),
            "cancel finds nothing once the run finished"
        );
        assert!(register_run(id).is_some(), "the entry is free again");
        finish_run(id);
    }

    /* ---- the pipeline against a local endpoint ---- */

    fn note_llm(base_url: &str) -> NoteLlm {
        NoteLlm {
            provider: PostProcessProvider {
                id: "custom".to_string(),
                label: "Custom".to_string(),
                base_url: base_url.to_string(),
                allow_base_url_edit: true,
                models_endpoint: None,
                supports_structured_output: true,
                api_format: PostProcessApiFormat::OpenAi,
            },
            model: "test-model".to_string(),
            api_key: String::new(),
            disable_reasoning: true,
        }
    }

    /// A canned OpenAI-compatible answer body.
    fn completion_body(content: &str) -> String {
        serde_json::json!({
            "choices": [{ "message": { "content": content }, "finish_reason": "stop" }]
        })
        .to_string()
    }

    /// Whether a received request looks complete: headers plus the whole
    /// Content-Length body.
    fn request_complete(request: &[u8]) -> bool {
        let text = String::from_utf8_lossy(request);
        let Some(header_end) = text.find("\r\n\r\n") else {
            return false;
        };
        let length = text[..header_end]
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|value| value.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        text.len() >= header_end + 4 + length
    }

    /// Serve /chat/completions once per canned `(status, body)` answer, in
    /// order, on the loopback; every request body is copied into `seen`.
    async fn serve_llm_answers(
        answers: Vec<(u16, String)>,
        seen: Arc<Mutex<Vec<String>>>,
    ) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            for (status, body) in answers {
                let Ok((mut stream, _)) = listener.accept().await else {
                    return;
                };
                let mut request = Vec::new();
                let mut buffer = [0_u8; 16_384];
                while !request_complete(&request) {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => break,
                        Ok(n) => request.extend_from_slice(&buffer[..n]),
                    }
                }
                seen.lock()
                    .unwrap()
                    .push(String::from_utf8_lossy(&request).to_string());
                let reason = if status < 400 { "OK" } else { "Bad Request" };
                let response = format!(
                    "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        });
        format!("http://{address}")
    }

    async fn read_all(seen: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        seen.lock().unwrap().clone()
    }

    #[tokio::test]
    async fn a_single_chunk_run_reads_the_note_from_the_endpoint() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let body = completion_body(
            r#"{"summary":"Møtet avgjorde lanseringen.","decisions":["Vi lanserer 1. mai."],"action_items":[{"task":"Kari sender rapporten","owner":"Kari"},{"task":"Bestille kake","owner":""}],"open_questions":["Hvem sender pressemeldingen?"]}"#,
        );
        let base_url = serve_llm_answers(vec![(200, body)], Arc::clone(&seen)).await;
        let llm = note_llm(&base_url);
        let cancelled = AtomicBool::new(false);

        let chunks = vec![
            "Vi bestemte at vi lanserer 1. mai. Kari sender rapporten. Hvem sender pressemeldingen?"
                .to_string(),
        ];
        let fields = generate_note_fields(&llm, &chunks, &cancelled)
            .await
            .expect("the run succeeds");
        assert_eq!(fields.summary, "Møtet avgjorde lanseringen.");
        assert_eq!(fields.decisions.len(), 1);
        assert_eq!(fields.action_items.len(), 2);
        assert_eq!(fields.action_items[0].owner, "Kari");
        assert_eq!(fields.action_items[1].owner, "");
        assert_eq!(fields.open_questions, vec!["Hvem sender pressemeldingen?"]);

        let requests = read_all(&seen).await;
        assert_eq!(requests.len(), 1, "exactly one extraction call");
        assert!(
            requests[0].contains("\"response_format\""),
            "structured output is requested"
        );
        assert!(requests[0].contains("MEETING TRANSCRIPT (part 1 of 1)"));
    }

    #[tokio::test]
    async fn several_chunks_merge_into_one_note() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let first = completion_body(
            r#"{"summary":"Første del handlet om lansering.","decisions":["Vi lanserer 1. mai."],"action_items":[{"task":"Kari sender rapporten","owner":"Kari"}],"open_questions":[]}"#,
        );
        let second = completion_body(
            r#"{"summary":"Andre del handlet om budsjett.","decisions":["Budsjettet settes til 100 000."],"action_items":[],"open_questions":["Hvem betaler?"]}"#,
        );
        let merged = completion_body(
            r#"{"summary":"Møtet avgjorde lanseringen og budsjettet.","decisions":["Vi lanserer 1. mai.","Budsjettet settes til 100 000."],"action_items":[{"task":"Kari sender rapporten","owner":"Kari"}],"open_questions":["Hvem betaler?"]}"#,
        );
        let base_url = serve_llm_answers(
            vec![(200, first), (200, second), (200, merged)],
            Arc::clone(&seen),
        )
        .await;
        let llm = note_llm(&base_url);
        let cancelled = AtomicBool::new(false);

        let fields = generate_note_fields(
            &llm,
            &[
                "Første del av møtet.".to_string(),
                "Andre del av møtet.".to_string(),
            ],
            &cancelled,
        )
        .await
        .expect("the run succeeds");
        assert_eq!(fields.summary, "Møtet avgjorde lanseringen og budsjettet.");
        assert_eq!(fields.decisions.len(), 2, "the merge answer is the note");

        let requests = read_all(&seen).await;
        assert_eq!(requests.len(), 3, "two extractions and one merge");
        assert!(requests[0].contains("MEETING TRANSCRIPT (part 1 of 2)"));
        assert!(requests[1].contains("MEETING TRANSCRIPT (part 2 of 2)"));
        assert!(
            requests[2].contains("PARTIAL MEETING NOTES from 2 consecutive parts"),
            "the merge call carries both partial notes"
        );
    }

    #[tokio::test]
    async fn an_endpoint_without_structured_output_falls_back_to_a_json_prompt() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let rejected = r#"{"error":{"message":"response_format not supported"}}"#.to_string();
        let answer = completion_body(
            "Selvfølgelig! {\"summary\":\"Notatet.\",\"decisions\":[],\"action_items\":[],\"open_questions\":[]}",
        );
        // The endpoint answers 400 twice: once to the schema request with
        // reasoning fields, once to llm_client's own retry without them.
        let base_url = serve_llm_answers(
            vec![(400, rejected.clone()), (400, rejected), (200, answer)],
            Arc::clone(&seen),
        )
        .await;
        let llm = note_llm(&base_url);
        let cancelled = AtomicBool::new(false);

        let fields = generate_note_fields(&llm, &["Møtet.".to_string()], &cancelled)
            .await
            .expect("the fallback run succeeds");
        assert_eq!(fields.summary, "Notatet.");

        let requests = read_all(&seen).await;
        assert_eq!(requests.len(), 3, "two schema tries, then one fallback");
        assert!(
            requests[2].contains("Respond with only a JSON object"),
            "the fallback demands pure JSON"
        );
        assert!(
            !requests[2].contains("\"response_format\""),
            "the fallback asks without the schema"
        );
    }

    #[tokio::test]
    async fn an_unreachable_endpoint_fails_with_the_detail() {
        // Port 1 on loopback is closed on every test machine.
        let llm = note_llm("http://127.0.0.1:1/v1");
        let cancelled = AtomicBool::new(false);
        let error = generate_note_fields(&llm, &["Møtet.".to_string()], &cancelled)
            .await
            .expect_err("the closed port fails");
        assert!(matches!(error, NoteGenerationError::Unreachable(_)));
    }

    #[tokio::test]
    async fn a_cancelled_run_stops_without_a_note() {
        // An endpoint that never answers holds the request open, exactly like
        // a small model thinking through a long chunk.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0_u8; 8192];
            let _ = stream.read(&mut request).await;
            tokio::time::sleep(Duration::from_secs(30)).await;
            let _ = stream.write_all(b"").await;
        });

        let llm = note_llm(&format!("http://{address}"));
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&cancelled);
        let setter = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            flag.store(true, Ordering::Relaxed);
        });

        let started = Instant::now();
        let error = generate_note_fields(&llm, &["Et veldig langt møte.".to_string()], &cancelled)
            .await
            .expect_err("the run is cancelled");
        assert_eq!(error, NoteGenerationError::Cancelled);
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "cancel stops the run promptly"
        );
        setter.join().unwrap();
    }
}
