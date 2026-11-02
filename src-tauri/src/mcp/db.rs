//! Read-only access to FlowState's `history.db` for the MCP tools.
//!
//! The connection is opened with `SQLITE_OPEN_READ_ONLY`, so the server
//! physically cannot write to the app's database — every tool is a query.

use chrono::{DateTime, Local};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// The fields `list_meetings` and `get_meeting` read from every meeting row.
const MEETING_COLUMNS: &str = "id, title, timestamp, saved, language, metadata";

/// A meeting as `list_meetings` returns it: enough to pick one out, without
/// the transcript itself.
#[derive(Clone, Debug, Serialize)]
pub struct MeetingSummary {
    pub id: i64,
    pub title: String,
    /// Unix seconds when the meeting finished.
    pub timestamp: i64,
    /// The same moment as ISO 8601 local time, so an assistant can show it.
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub saved: bool,
    /// Whether a draft meeting note has been generated for this meeting.
    pub has_note: bool,
}

/// One action item from the meeting note stored on a meeting's metadata.
#[derive(Clone, Debug, Serialize)]
pub struct NoteActionItem {
    pub task: String,
    pub owner: String,
}

/// The draft meeting note, as stored on the entry's metadata.
#[derive(Clone, Debug, Serialize)]
pub struct MeetingNote {
    pub summary: String,
    pub decisions: Vec<String>,
    pub action_items: Vec<NoteActionItem>,
    pub open_questions: Vec<String>,
    pub model: String,
    /// Unix seconds when the note was generated.
    pub generated_at: i64,
}

/// A full meeting: the summary fields plus its transcript and note.
#[derive(Clone, Debug, Serialize)]
pub struct Meeting {
    pub id: i64,
    pub title: String,
    pub timestamp: i64,
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    pub saved: bool,
    /// The model's own transcript, verbatim — post-processing never rewrites
    /// what this returns.
    pub transcript: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<MeetingNote>,
    /// Timed segments when the model produced them, oldest first.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<TranscriptSegment>>,
}

/// One timed piece of a transcript, on the recording's timeline.
#[derive(Clone, Debug, Serialize)]
pub struct TranscriptSegment {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

/// The verbatim transcript of any history entry (meeting or dictation).
#[derive(Clone, Debug, Serialize)]
pub struct Transcript {
    pub id: i64,
    pub title: String,
    pub timestamp: i64,
    pub date: String,
    /// "meeting" or "dictation".
    pub source: String,
    /// The model's own text, exactly as it was transcribed.
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<TranscriptSegment>>,
}

/// One `search_history` hit: where it is, plus a snippet around the match.
#[derive(Clone, Debug, Serialize)]
pub struct SearchHit {
    pub id: i64,
    pub title: String,
    pub timestamp: i64,
    pub date: String,
    /// "meeting" or "dictation".
    pub source: String,
    /// First 200 characters of the transcription, so the user can tell the
    /// hits apart without fetching each transcript.
    pub snippet: String,
}

/// A page of `search_history` hits plus the total match count.
#[derive(Clone, Debug, Serialize)]
pub struct SearchResults {
    pub total: i64,
    pub hits: Vec<SearchHit>,
}

/// The metadata JSON stored beside an entry, as far as the MCP tools read it.
#[derive(Clone, Debug, Default, Deserialize)]
struct EntryMetadata {
    #[serde(default)]
    duration_secs: Option<u64>,
    #[serde(default)]
    segments: Option<Vec<RawSegment>>,
    #[serde(default)]
    meeting_note: Option<RawNote>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawSegment {
    t0_ms: i64,
    t1_ms: i64,
    text: String,
}

#[derive(Clone, Debug, Deserialize)]
struct RawNote {
    #[serde(default)]
    summary: String,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    action_items: Vec<Value>,
    #[serde(default)]
    open_questions: Vec<String>,
    #[serde(default)]
    model: String,
    #[serde(default)]
    generated_at: i64,
}

impl RawSegment {
    fn into_segment(self) -> TranscriptSegment {
        TranscriptSegment {
            start_ms: self.t0_ms,
            end_ms: self.t1_ms,
            text: self.text,
        }
    }
}

impl RawNote {
    fn into_note(self) -> MeetingNote {
        let action_items = self
            .action_items
            .into_iter()
            .filter_map(|item| match item {
                Value::Object(map) => Some(NoteActionItem {
                    task: map
                        .get("task")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    owner: map
                        .get("owner")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                }),
                _ => None,
            })
            .collect();
        MeetingNote {
            summary: self.summary,
            decisions: self.decisions,
            action_items,
            open_questions: self.open_questions,
            model: self.model,
            generated_at: self.generated_at,
        }
    }
}

/// ISO 8601 local time for a Unix-seconds timestamp.
fn iso_date(timestamp: i64) -> String {
    DateTime::from_timestamp(timestamp, 0)
        .map(|dt| dt.with_timezone(&Local).to_rfc3339())
        .unwrap_or_else(|| timestamp.to_string())
}

/// Parse the metadata JSON column, tolerating NULL and malformed JSON.
fn parse_metadata(raw: Option<String>) -> EntryMetadata {
    raw.and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

/// Escape `%`, `_` and `\` so a user query only matches itself as a `LIKE`
/// pattern instead of acting as a wildcard.
fn escape_like_pattern(query: &str) -> String {
    let mut escaped = String::with_capacity(query.len());
    for ch in query.chars() {
        if ch == '%' || ch == '_' || ch == '\\' {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

/// "meeting" when the row's source column says so, "dictation" otherwise:
/// dictations store NULL in `source`.
fn source_name(raw: Option<String>) -> String {
    match raw.as_deref() {
        Some("meeting") => "meeting".to_string(),
        _ => "dictation".to_string(),
    }
}

pub struct McpDb {
    conn: Connection,
}

impl McpDb {
    /// Open `history.db` read-only. The file must already exist — the MCP
    /// server never creates or migrates the app's database.
    pub fn open(path: &Path) -> Result<Self, String> {
        if !path.exists() {
            return Err(format!(
                "FlowState's history database was not found at {}",
                path.display()
            ));
        }
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("Failed to open history database: {e}"))?;
        Ok(Self { conn })
    }

    /// Meeting sessions, newest first. `limit`/`offset` page through them.
    pub fn list_meetings(&self, limit: u64, offset: u64) -> Result<Vec<MeetingSummary>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {MEETING_COLUMNS}
                 FROM transcription_history
                 WHERE source = 'meeting'
                 ORDER BY timestamp DESC
                 LIMIT ?1 OFFSET ?2"
            ))
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([limit as i64, offset as i64], |row| {
                let metadata = parse_metadata(row.get("metadata")?);
                Ok(MeetingSummary {
                    id: row.get("id")?,
                    title: row.get("title")?,
                    timestamp: row.get("timestamp")?,
                    date: iso_date(row.get("timestamp")?),
                    duration_secs: metadata.duration_secs,
                    language: row.get("language")?,
                    saved: row.get("saved")?,
                    has_note: metadata.meeting_note.is_some(),
                })
            })
            .map_err(|e| e.to_string())?;

        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())
    }

    /// One meeting by id. `None` when the id is unknown or the entry is not
    /// a meeting session.
    pub fn get_meeting(&self, id: i64) -> Result<Option<Meeting>, String> {
        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT {MEETING_COLUMNS}, transcription_text
                 FROM transcription_history
                 WHERE id = ?1 AND source = 'meeting'"
            ))
            .map_err(|e| e.to_string())?;

        let mut rows = stmt
            .query_map([id], |row| {
                let metadata = parse_metadata(row.get("metadata")?);
                Ok(Meeting {
                    id: row.get("id")?,
                    title: row.get("title")?,
                    timestamp: row.get("timestamp")?,
                    date: iso_date(row.get("timestamp")?),
                    duration_secs: metadata.duration_secs,
                    language: row.get("language")?,
                    saved: row.get("saved")?,
                    transcript: row.get("transcription_text")?,
                    note: metadata.meeting_note.map(RawNote::into_note),
                    segments: metadata.segments.map(|segments| {
                        segments.into_iter().map(RawSegment::into_segment).collect()
                    }),
                })
            })
            .map_err(|e| e.to_string())?;

        match rows.next() {
            Some(Ok(meeting)) => Ok(Some(meeting)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    /// The verbatim transcript of any entry, meeting or dictation. `None`
    /// when the id is unknown.
    pub fn get_transcript(&self, id: i64) -> Result<Option<Transcript>, String> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, title, timestamp, source, transcription_text, metadata
                 FROM transcription_history
                 WHERE id = ?1",
            )
            .map_err(|e| e.to_string())?;

        let mut rows = stmt
            .query_map([id], |row| {
                let metadata = parse_metadata(row.get("metadata")?);
                Ok(Transcript {
                    id: row.get("id")?,
                    title: row.get("title")?,
                    timestamp: row.get("timestamp")?,
                    date: iso_date(row.get("timestamp")?),
                    source: source_name(row.get("source")?),
                    text: row.get("transcription_text")?,
                    segments: metadata.segments.map(|segments| {
                        segments.into_iter().map(RawSegment::into_segment).collect()
                    }),
                })
            })
            .map_err(|e| e.to_string())?;

        match rows.next() {
            Some(Ok(transcript)) => Ok(Some(transcript)),
            Some(Err(e)) => Err(e.to_string()),
            None => Ok(None),
        }
    }

    /// Case-insensitive substring search over every entry's title and
    /// transcription, newest first.
    pub fn search_history(
        &self,
        query: &str,
        limit: u64,
        offset: u64,
    ) -> Result<SearchResults, String> {
        const MATCHES: &str = "title LIKE ?1 ESCAPE '\\' OR transcription_text LIKE ?1 ESCAPE '\\'";
        let pattern = format!("%{}%", escape_like_pattern(query.trim()));

        let total: i64 = self
            .conn
            .query_row(
                &format!("SELECT COUNT(*) FROM transcription_history WHERE {MATCHES}"),
                [&pattern],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;

        let mut stmt = self
            .conn
            .prepare(&format!(
                "SELECT id, title, timestamp, source, transcription_text
                 FROM transcription_history
                 WHERE {MATCHES}
                 ORDER BY id DESC
                 LIMIT ?2 OFFSET ?3"
            ))
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(
                rusqlite::params![pattern, limit as i64, offset as i64],
                |row| {
                    let text: String = row.get("transcription_text")?;
                    Ok(SearchHit {
                        id: row.get("id")?,
                        title: row.get("title")?,
                        timestamp: row.get("timestamp")?,
                        date: iso_date(row.get("timestamp")?),
                        source: source_name(row.get("source")?),
                        snippet: text.chars().take(200).collect(),
                    })
                },
            )
            .map_err(|e| e.to_string())?;

        let hits = rows
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        Ok(SearchResults { total, hits })
    }
}
