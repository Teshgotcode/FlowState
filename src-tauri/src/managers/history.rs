use anyhow::{anyhow, Result};
use chrono::{DateTime, Local, Utc};
use log::{debug, error, info, warn};
use rusqlite::{params, Connection, OptionalExtension};
use rusqlite_migration::{MigrationDefinitionError, Migrations, M};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::language_detection::{LanguageDecision, LanguageSource};
use crate::managers::meeting_note::MeetingNote;
use crate::managers::transcription::TranscriptSegment;

/// Database migrations for transcription history.
/// Each migration is applied in order. The library tracks which migrations
/// have been applied using SQLite's user_version pragma.
///
/// Note: For users upgrading from tauri-plugin-sql, migrate_from_tauri_plugin_sql()
/// converts the old _sqlx_migrations table tracking to the user_version pragma,
/// ensuring migrations don't re-run on existing databases.
static MIGRATIONS: &[M] = &[
    M::up(
        "CREATE TABLE IF NOT EXISTS transcription_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            file_name TEXT NOT NULL,
            timestamp INTEGER NOT NULL,
            saved BOOLEAN NOT NULL DEFAULT 0,
            title TEXT NOT NULL,
            transcription_text TEXT NOT NULL
        );",
    ),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_processed_text TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_prompt TEXT;"),
    M::up("ALTER TABLE transcription_history ADD COLUMN post_process_requested BOOLEAN NOT NULL DEFAULT 0;"),
];

/// Nullable columns added outside [`MIGRATIONS`]; see
/// [`HistoryManager::apply_migrations`].
/// Marks a cleaned meeting transcript in `post_process_prompt`, so the
/// windows can tell it apart from a dictation's post-processed text.
pub const MEETING_REFINE_PROMPT: &str = "meeting-refine";

const ADDED_COLUMNS: &[&str] = &[
    "source_name",
    "language",
    "language_source",
    "source",
    "metadata",
];

/// Every column a [`HistoryEntry`] is read from.
const ENTRY_COLUMNS: &str = "id, file_name, timestamp, saved, title, transcription_text, \
    post_processed_text, post_process_prompt, post_process_requested, source_name, language, \
    language_source, source, metadata";

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct PaginatedHistory {
    pub entries: Vec<HistoryEntry>,
    pub has_more: bool,
}

/// One page of `search_history` matches plus the full match count, so the
/// frontend can say how many hits a query found without paging everything.
#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct SearchedHistory {
    pub entries: Vec<HistoryEntry>,
    /// Every entry matching the query, not just this page.
    pub total: i64,
    pub has_more: bool,
}

/// Escape `%`, `_` and `\\` so a user query only matches itself as a
/// `LIKE` pattern instead of acting as a wildcard.
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

#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
#[serde(tag = "action")]
pub enum HistoryUpdatePayload {
    #[serde(rename = "added")]
    Added { entry: HistoryEntry },
    #[serde(rename = "updated")]
    Updated { entry: HistoryEntry },
    #[serde(rename = "deleted")]
    Deleted { id: i64 },
    #[serde(rename = "toggled")]
    Toggled { id: i64 },
}

#[derive(Clone, Debug, Serialize, Deserialize, Type)]
pub struct HistoryEntry {
    pub id: i64,
    pub file_name: String,
    pub timestamp: i64,
    pub saved: bool,
    pub title: String,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
    pub post_process_requested: bool,
    /// The name of the file this entry was transcribed from; `None` for a
    /// dictation.
    pub source_name: Option<String>,
    /// The language it was transcribed in, when known.
    pub language: Option<String>,
    /// How that language was chosen; `None` before it was recorded.
    pub language_source: Option<LanguageSource>,
    /// Where the audio came from, when it wasn't a dictation take; `None` on
    /// old entries and dictations.
    pub source: Option<HistorySource>,
    /// Extra facts kept beside the transcription (a meeting's duration), as
    /// JSON; `None` on old entries.
    pub metadata: Option<HistoryMetadata>,
}

/// Where a history entry's audio came from, beyond a dictation take. Old
/// entries predate the field and read as `None`; a value this build doesn't
/// know also reads as `None`, so a newer database still opens cleanly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "lowercase")]
pub enum HistorySource {
    /// A meeting session: an explicit start/stop recording, not a hotkey take.
    Meeting,
    /// A take dictated into the Scratchpad's note window.
    Note,
}

impl HistorySource {
    pub fn as_str(self) -> &'static str {
        match self {
            HistorySource::Meeting => "meeting",
            HistorySource::Note => "note",
        }
    }

    /// Parse a stored value; unknown values read as `None`.
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "meeting" => Some(HistorySource::Meeting),
            _ => None,
        }
    }
}

/// Extra facts kept beside an entry's transcription, stored as JSON in the
/// `metadata` column. Every field is optional so the shape can grow.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct HistoryMetadata {
    /// Seconds the recording ran (a meeting), when it is known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_secs: Option<u64>,
    /// The transcription's timed segments (t0_ms, t1_ms, text — the model's
    /// own text, not post-processed), when the model produced any. Entries
    /// from before the field existed, and transcriptions whose model produces
    /// none, read as `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub segments: Option<Vec<TranscriptSegment>>,
    /// The draft meeting note a local LLM wrote over the transcript, when the
    /// user asked for one. Kept beside the transcript; the transcript itself
    /// is never changed by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meeting_note: Option<MeetingNote>,
    /// The names the user gave the meeting's speakers, indexed by speaker
    /// number (S1 is index 0); `None` when nobody was named. Names live per
    /// meeting: they survive restarts and re-transcriptions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speaker_names: Option<Vec<Option<String>>>,
    /// What the user typed during the meeting, in their own words. Kept
    /// apart from the transcript and the generated note: neither is ever
    /// rewritten from it, and it survives restarts and re-transcriptions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_notes: Option<String>,
    /// The app the meeting ran in (Zoom, Teams, a browser), read from the
    /// frontmost app when the recording started. Shown beside the date so a
    /// day of meetings reads as more than timestamps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meeting_app: Option<String>,
    /// The meeting's second track: the system audio (what the machine was
    /// playing — a call's far end) captured beside the microphone (S-5),
    /// when one was. Its transcript is merged into the segments by
    /// timestamp; the recording itself is kept like any other.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_audio: Option<SystemAudioTrack>,
}

/// A meeting's captured system audio: where its recording lives and how many
/// milliseconds after the meeting's start its capture began, so the track's
/// own timestamps line up with the mic track's when the two merge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct SystemAudioTrack {
    /// The recording's file name in the recordings directory.
    pub file_name: String,
    /// Milliseconds from the meeting's start to this capture's start.
    pub offset_ms: i64,
}

impl HistoryMetadata {
    /// Whether it holds any fact worth storing.
    fn is_empty(&self) -> bool {
        self.duration_secs.is_none()
            && self.segments.is_none()
            && self.meeting_note.is_none()
            && self.speaker_names.is_none()
            && self.system_audio.is_none()
    }
}

pub struct HistoryManager {
    app_handle: AppHandle,
    recordings_dir: PathBuf,
    db_path: PathBuf,
}

/// Metadata for a fresh entry, keeping only the optional facts it has.
fn fresh_metadata(segments: &[TranscriptSegment]) -> Option<HistoryMetadata> {
    (!segments.is_empty()).then(|| HistoryMetadata {
        duration_secs: None,
        segments: Some(segments.to_vec()),
        system_audio: None,
        meeting_note: None,
        speaker_names: None,
        user_notes: None,
        meeting_app: None,
    })
}

/// A transcription's language as its two columns.
fn split_language(language: Option<LanguageDecision>) -> (Option<String>, Option<LanguageSource>) {
    match language {
        Some(decision) => (decision.language, Some(decision.source)),
        None => (None, None),
    }
}

impl HistoryManager {
    pub fn new(app_handle: &AppHandle) -> Result<Self> {
        // Create recordings directory in app data dir
        let app_data_dir = crate::portable::app_data_dir(app_handle)?;
        let recordings_dir = app_data_dir.join("recordings");
        let db_path = app_data_dir.join("history.db");

        // Ensure recordings directory exists
        if !recordings_dir.exists() {
            fs::create_dir_all(&recordings_dir)?;
            debug!("Created recordings directory: {:?}", recordings_dir);
        }

        let manager = Self {
            app_handle: app_handle.clone(),
            recordings_dir,
            db_path,
        };

        // Initialize database and run migrations synchronously
        manager.init_database()?;

        Ok(manager)
    }

    fn init_database(&self) -> Result<()> {
        info!("Initializing database at {:?}", self.db_path);

        let mut conn = Connection::open(&self.db_path)?;

        // Handle migration from tauri-plugin-sql to rusqlite_migration
        // tauri-plugin-sql used _sqlx_migrations table, rusqlite_migration uses user_version pragma
        self.migrate_from_tauri_plugin_sql(&conn)?;

        // Validate migrations in debug builds
        #[cfg(debug_assertions)]
        Migrations::new(MIGRATIONS.to_vec())
            .validate()
            .expect("Invalid migrations");

        // Get current version before migration
        let version_before: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
        debug!("Database version before migration: {}", version_before);

        Self::apply_migrations(&mut conn)?;

        // Get version after migration
        let version_after: i32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if version_after > version_before {
            info!(
                "Database migrated from version {} to {}",
                version_before, version_after
            );
        } else {
            debug!("Database already at latest version {}", version_after);
        }

        Ok(())
    }

    /// Bring the schema up to date. A newer FlowState's database opens as is:
    /// migrations only add columns, and every query names its columns.
    fn apply_migrations(conn: &mut Connection) -> Result<()> {
        match Migrations::new(MIGRATIONS.to_vec()).to_latest(conn) {
            Err(rusqlite_migration::Error::MigrationDefinition(
                MigrationDefinitionError::DatabaseTooFarAhead,
            )) => {
                warn!(
                    "history.db is from a newer version of FlowState; opening it without migrating"
                )
            }
            other => other?,
        }

        // Not a migration: a higher user_version makes released builds refuse
        // the database, while an extra nullable column is invisible to them.
        for column in ADDED_COLUMNS {
            if !Self::has_column(conn, column)? {
                conn.execute(
                    &format!("ALTER TABLE transcription_history ADD COLUMN {column} TEXT"),
                    [],
                )?;
                info!("Added the {column} column to transcription history");
            }
        }

        Ok(())
    }

    fn has_column(conn: &Connection, name: &str) -> Result<bool> {
        Ok(conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('transcription_history') WHERE name = ?1",
            [name],
            |row| row.get(0),
        )?)
    }

    /// Migrate from tauri-plugin-sql's migration tracking to rusqlite_migration's.
    /// tauri-plugin-sql used a _sqlx_migrations table, while rusqlite_migration uses
    /// SQLite's user_version pragma. This function checks if the old system was in use
    /// and sets the user_version accordingly so migrations don't re-run.
    fn migrate_from_tauri_plugin_sql(&self, conn: &Connection) -> Result<()> {
        // Check if the old _sqlx_migrations table exists
        let has_sqlx_migrations: bool = conn
            .query_row(
                "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='_sqlx_migrations'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(false);

        if !has_sqlx_migrations {
            return Ok(());
        }

        // Check current user_version
        let current_version: i32 =
            conn.pragma_query_value(None, "user_version", |row| row.get(0))?;

        if current_version > 0 {
            // Already migrated to rusqlite_migration system
            return Ok(());
        }

        // Get the highest version from the old migrations table
        let old_version: i32 = conn
            .query_row(
                "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations WHERE success = 1",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        if old_version > 0 {
            info!(
                "Migrating from tauri-plugin-sql (version {}) to rusqlite_migration",
                old_version
            );

            // Set user_version to match the old migration state
            conn.pragma_update(None, "user_version", old_version)?;

            // Optionally drop the old migrations table (keeping it doesn't hurt)
            // conn.execute("DROP TABLE IF EXISTS _sqlx_migrations", [])?;

            info!(
                "Migration tracking converted: user_version set to {}",
                old_version
            );
        }

        Ok(())
    }

    fn get_connection(&self) -> Result<Connection> {
        Ok(Connection::open(&self.db_path)?)
    }

    fn map_history_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<HistoryEntry> {
        Ok(HistoryEntry {
            id: row.get("id")?,
            file_name: row.get("file_name")?,
            timestamp: row.get("timestamp")?,
            saved: row.get("saved")?,
            title: row.get("title")?,
            transcription_text: row.get("transcription_text")?,
            post_processed_text: row.get("post_processed_text")?,
            post_process_prompt: row.get("post_process_prompt")?,
            post_process_requested: row.get("post_process_requested")?,
            source_name: row.get("source_name")?,
            language: row.get("language")?,
            language_source: row
                .get::<_, Option<String>>("language_source")?
                .as_deref()
                .and_then(LanguageSource::parse),
            source: row
                .get::<_, Option<String>>("source")?
                .as_deref()
                .and_then(HistorySource::parse),
            metadata: row
                .get::<_, Option<String>>("metadata")?
                .and_then(|value| serde_json::from_str(&value).ok()),
        })
    }

    pub fn recordings_dir(&self) -> &std::path::Path {
        &self.recordings_dir
    }

    /// Save a new history entry to the database.
    /// The WAV file should already have been written to the recordings directory.
    /// One flat record of one save; a params struct would only move the same
    /// fields around.
    #[allow(clippy::too_many_arguments)]
    pub fn save_entry(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        language: Option<LanguageDecision>,
        segments: Vec<TranscriptSegment>,
    ) -> Result<HistoryEntry> {
        let (language, language_source) = split_language(language);
        let entry = self.insert_entry(HistoryEntry {
            id: 0,
            file_name,
            timestamp: Utc::now().timestamp(),
            saved: false,
            title: String::new(),
            transcription_text,
            post_processed_text,
            post_process_prompt,
            post_process_requested,
            source_name: None,
            language,
            language_source,
            source: None,
            metadata: fresh_metadata(&segments),
        })?;

        self.cleanup_old_entries()?;

        Ok(entry)
    }

    /// Save the entry for a take dictated into the Scratchpad: the same
    /// record a plain hotkey dictation leaves, but marked `source: "note"`
    /// so History can label it. The note's text itself is kept by the
    /// Scratchpad; this entry is the take's record (with its audio), and it
    /// never leaves the machine. Same parameter shape as `save_entry`.
    #[allow(clippy::too_many_arguments)]
    pub fn save_note_entry(
        &self,
        file_name: String,
        transcription_text: String,
        post_process_requested: bool,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        language: Option<LanguageDecision>,
        segments: Vec<TranscriptSegment>,
    ) -> Result<HistoryEntry> {
        let (language, language_source) = split_language(language);
        let entry = self.insert_entry(HistoryEntry {
            id: 0,
            file_name,
            timestamp: Utc::now().timestamp(),
            saved: false,
            title: String::new(),
            transcription_text,
            post_processed_text,
            post_process_prompt,
            post_process_requested,
            source_name: None,
            language,
            language_source,
            source: Some(HistorySource::Note),
            metadata: fresh_metadata(&segments),
        })?;

        self.cleanup_old_entries()?;

        Ok(entry)
    }

    /// Insert a history entry that arrived from sync (shared folder or a
    /// hand-carried bundle): the audio file, if any, must already be in the
    /// recordings directory. Sync never deletes history — this only ever
    /// adds.
    pub fn save_synced_entry(&self, entry: HistoryEntry) -> Result<HistoryEntry> {
        let entry = self.insert_entry(entry)?;
        self.cleanup_old_entries()?;
        Ok(entry)
    }

    /// Save the entry for a file whose audio is already at `file_name`.
    /// Retention never removes it (only deleting it does); never
    /// post-processed.
    pub fn save_file_entry(&self, file_name: String, source_name: String) -> Result<HistoryEntry> {
        self.insert_entry(HistoryEntry {
            id: 0,
            file_name,
            timestamp: Utc::now().timestamp(),
            saved: false,
            title: String::new(),
            transcription_text: String::new(),
            post_processed_text: None,
            post_process_prompt: None,
            post_process_requested: false,
            source_name: Some(source_name),
            language: None,
            language_source: None,
            source: None,
            metadata: None,
        })
    }

    /// Save the entry for a meeting whose audio is already at `file_name`,
    /// renamed from its working file. The entry starts with no transcription —
    /// the background job adds it — and is never post-processed. `timestamp`
    /// is when the meeting started, and the duration is kept in its metadata.
    pub fn save_meeting_entry(
        &self,
        file_name: String,
        started_at: i64,
        duration_secs: u64,
        system_audio: Option<SystemAudioTrack>,
    ) -> Result<HistoryEntry> {
        let entry = self.insert_entry(HistoryEntry {
            id: 0,
            file_name,
            timestamp: started_at,
            saved: false,
            title: String::new(),
            transcription_text: String::new(),
            post_processed_text: None,
            post_process_prompt: None,
            post_process_requested: false,
            source_name: None,
            language: None,
            language_source: None,
            source: Some(HistorySource::Meeting),
            metadata: Some(HistoryMetadata {
                duration_secs: Some(duration_secs),
                segments: None,
                meeting_note: None,
                speaker_names: None,
                user_notes: None,
                meeting_app: None,
                system_audio,
            }),
        })?;

        self.cleanup_old_entries()?;

        Ok(entry)
    }

    /// Insert `entry`, filling in its id and title, and announce it.
    fn insert_entry(&self, mut entry: HistoryEntry) -> Result<HistoryEntry> {
        entry.title = self.format_timestamp_title(entry.timestamp);

        let conn = self.get_connection()?;
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested,
                source_name,
                language,
                language_source,
                source,
                metadata
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                &entry.file_name,
                entry.timestamp,
                entry.saved,
                &entry.title,
                &entry.transcription_text,
                &entry.post_processed_text,
                &entry.post_process_prompt,
                entry.post_process_requested,
                &entry.source_name,
                &entry.language,
                entry.language_source.map(LanguageSource::as_str),
                entry.source.map(HistorySource::as_str),
                entry
                    .metadata
                    .as_ref()
                    .and_then(|metadata| serde_json::to_string(metadata).ok()),
            ],
        )?;
        entry.id = conn.last_insert_rowid();

        debug!("Saved history entry with id {}", entry.id);

        // Emit typed event for real-time frontend updates
        if let Err(e) = (HistoryUpdatePayload::Added {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    /// Update an existing history entry with new transcription results (used by retry).
    pub fn update_transcription(
        &self,
        id: i64,
        transcription_text: String,
        post_processed_text: Option<String>,
        post_process_prompt: Option<String>,
        language: Option<LanguageDecision>,
        segments: Vec<TranscriptSegment>,
    ) -> Result<HistoryEntry> {
        let (language, language_source) = split_language(language);
        let conn = self.get_connection()?;
        // Keep the facts already stored (a meeting's duration); this run's
        // segments replace whatever the previous run left.
        let existing = conn
            .query_row(
                &format!("SELECT {ENTRY_COLUMNS} FROM transcription_history WHERE id = ?1"),
                params![id],
                Self::map_history_entry,
            )
            .optional()?;
        let mut metadata = existing
            .and_then(|entry| entry.metadata)
            .unwrap_or_default();
        metadata.segments = (!segments.is_empty()).then_some(segments);
        let metadata_json = (!metadata.is_empty())
            .then(|| serde_json::to_string(&metadata))
            .and_then(Result::ok);
        let updated = conn.execute(
            "UPDATE transcription_history
             SET transcription_text = ?1,
                 post_processed_text = ?2,
                 post_process_prompt = ?3,
                 language = ?4,
                 language_source = ?5,
                 metadata = ?6
             WHERE id = ?7",
            params![
                transcription_text,
                post_processed_text,
                post_process_prompt,
                language,
                language_source.map(LanguageSource::as_str),
                metadata_json,
                id
            ],
        )?;

        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }

        let entry = conn.query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM transcription_history WHERE id = ?1"),
            params![id],
            Self::map_history_entry,
        )?;

        debug!("Updated transcription for history entry {}", id);

        if let Err(e) = (HistoryUpdatePayload::Updated {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    /// Read the entry back and tell the windows it changed.
    fn entry_and_announce(&self, id: i64) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let entry = conn.query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM transcription_history WHERE id = ?1"),
            params![id],
            Self::map_history_entry,
        )?;
        if let Err(e) = (HistoryUpdatePayload::Updated {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }
        Ok(entry)
    }

    /// Keep the cleaned transcript beside the raw one. The transcription and
    /// the segments are not touched: the window still shows what was heard.
    pub fn save_cleaned_text(&self, id: i64, cleaned: &str) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let updated = conn.execute(
            "UPDATE transcription_history SET post_processed_text = ?1, post_process_prompt = ?2 WHERE id = ?3",
            params![cleaned, MEETING_REFINE_PROMPT, id],
        )?;
        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }
        self.entry_and_announce(id)
    }

    /// Point an entry at the recording's new file name, after the WAV was
    /// transcoded. Nothing else about the entry changes.
    pub fn rename_audio_file(&self, id: i64, file_name: &str) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let updated = conn.execute(
            "UPDATE transcription_history SET file_name = ?1 WHERE id = ?2",
            params![file_name, id],
        )?;
        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }
        self.entry_and_announce(id)
    }

    /// An entry's metadata, empty when it has none. Read-only callers use
    /// this instead of opening the database themselves.
    pub fn metadata_of(&self, id: i64) -> Result<HistoryMetadata> {
        self.read_metadata(id)
    }

    /// Label a meeting with the app it ran in; an empty name writes nothing.
    pub fn save_meeting_app(&self, id: i64, app: String) -> Result<HistoryEntry> {
        let mut metadata = self.read_metadata(id)?;
        let trimmed = app.trim();
        metadata.meeting_app = (!trimmed.is_empty()).then(|| trimmed.to_string());
        let entry = self.write_metadata(id, metadata)?;
        self.announce(&entry);
        Ok(entry)
    }

    /// Save the user's own notes for a meeting. An empty string clears the
    /// field instead of storing a blank, so "no notes" reads the same as
    /// never having written any.
    pub fn save_user_notes(&self, id: i64, notes: String) -> Result<HistoryEntry> {
        let mut metadata = self.read_metadata(id)?;
        let trimmed = notes.trim();
        metadata.user_notes = (!trimmed.is_empty()).then(|| trimmed.to_string());
        let entry = self.write_metadata(id, metadata)?;
        self.announce(&entry);
        Ok(entry)
    }

    /// The "correct this" flow: replaces only the transcription text with
    /// what the user edited it into, leaving the audio, metadata and any
    /// post-processed copy alone. The updated entry is announced like any
    /// other change, so the history window reflects it.
    /// Name a meeting's speakers: the names are stored on the entry's
    /// metadata, indexed by speaker number (S1 is index 0). Whitespace-only
    /// names read back as `None`, so a cleared name disappears rather than
    /// persisting as "". Names survive re-transcriptions — a retry replaces
    /// the segments but keeps what the user wrote.
    pub fn save_speaker_names(&self, id: i64, names: Vec<Option<String>>) -> Result<HistoryEntry> {
        let names: Vec<Option<String>> = names
            .into_iter()
            .map(|name| {
                let trimmed = name.unwrap_or_default().trim().to_string();
                (!trimmed.is_empty()).then_some(trimmed)
            })
            .collect();
        let mut metadata = self.read_metadata(id)?;
        metadata.speaker_names = names.iter().any(Option::is_some).then_some(names);
        let entry = self.write_metadata(id, metadata)?;
        self.announce(&entry);
        Ok(entry)
    }

    /// Replace a meeting's transcript with the merged one its two tracks
    /// produce (mic and system audio, S-5): the text is rebuilt from the
    /// merged segments so the entry reads as one conversation, and the
    /// timestamps decide the order. The recording, the note and the speaker
    /// names are not touched.
    pub fn save_merged_transcript(
        &self,
        id: i64,
        text: String,
        segments: Vec<TranscriptSegment>,
    ) -> Result<HistoryEntry> {
        let mut metadata = self.read_metadata(id)?;
        metadata.segments = (!segments.is_empty()).then_some(segments);
        let entry = self.write_metadata(id, metadata)?;
        let conn = self.get_connection()?;
        conn.execute(
            "UPDATE transcription_history SET transcription_text = ?1 WHERE id = ?2",
            params![text, id],
        )?;
        let entry = conn.query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM transcription_history WHERE id = ?1"),
            params![id],
            Self::map_history_entry,
        )?;
        self.announce(&entry);
        Ok(entry)
    }

    /// Replace the segments a diarization pass produced (the text is the
    /// model's own and stays as it is): the speaker numbers it merged in
    /// ride along on the segments themselves.
    pub fn save_diarized_segments(
        &self,
        id: i64,
        segments: Vec<TranscriptSegment>,
    ) -> Result<HistoryEntry> {
        let mut metadata = self.read_metadata(id)?;
        metadata.segments = (!segments.is_empty()).then_some(segments);
        let entry = self.write_metadata(id, metadata)?;
        self.announce(&entry);
        Ok(entry)
    }

    /// Tell the windows an entry changed, so the history list and the open
    /// transcript window follow along without polling.
    fn announce(&self, entry: &HistoryEntry) {
        if let Err(e) = (HistoryUpdatePayload::Updated {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }
    }

    /// The metadata an entry keeps today, or a fresh one when it has none.
    fn read_metadata(&self, id: i64) -> Result<HistoryMetadata> {
        let conn = self.get_connection()?;
        let existing = conn
            .query_row(
                &format!("SELECT {ENTRY_COLUMNS} FROM transcription_history WHERE id = ?1"),
                params![id],
                Self::map_history_entry,
            )
            .optional()?;
        Ok(existing
            .and_then(|entry| entry.metadata)
            .unwrap_or_default())
    }

    /// Write an entry's metadata back, keeping every other column as it is.
    fn write_metadata(&self, id: i64, metadata: HistoryMetadata) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let metadata_json = (!metadata.is_empty())
            .then(|| serde_json::to_string(&metadata))
            .and_then(Result::ok);
        let updated = conn.execute(
            "UPDATE transcription_history SET metadata = ?1 WHERE id = ?2",
            params![metadata_json, id],
        )?;
        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }
        conn.query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM transcription_history WHERE id = ?1"),
            params![id],
            Self::map_history_entry,
        )
        .map_err(Into::into)
    }

    pub fn update_transcription_text(&self, id: i64, text: &str) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let updated = conn.execute(
            "UPDATE transcription_history SET transcription_text = ?1 WHERE id = ?2",
            params![text, id],
        )?;
        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }
        let entry = conn.query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM transcription_history WHERE id = ?1"),
            params![id],
            Self::map_history_entry,
        )?;
        debug!("Corrected transcription for history entry {}", id);
        if let Err(e) = (HistoryUpdatePayload::Updated {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }
        Ok(entry)
    }

    /// Keep a generated meeting note on the entry's metadata, beside the
    /// transcript it was written from. Only the metadata column changes; the
    /// transcription is never touched. The updated entry is announced so the
    /// window that asked for the note gets it.
    pub fn save_meeting_note(&self, id: i64, note: MeetingNote) -> Result<HistoryEntry> {
        let conn = self.get_connection()?;
        let existing = conn
            .query_row(
                &format!("SELECT {ENTRY_COLUMNS} FROM transcription_history WHERE id = ?1"),
                params![id],
                Self::map_history_entry,
            )
            .optional()?;
        let mut metadata = existing
            .and_then(|entry| entry.metadata)
            .unwrap_or_default();
        metadata.meeting_note = Some(note);
        let metadata_json = serde_json::to_string(&metadata).ok();
        let updated = conn.execute(
            "UPDATE transcription_history SET metadata = ?1 WHERE id = ?2",
            params![metadata_json, id],
        )?;
        if updated == 0 {
            return Err(anyhow!("History entry {} not found", id));
        }

        let entry = conn.query_row(
            &format!("SELECT {ENTRY_COLUMNS} FROM transcription_history WHERE id = ?1"),
            params![id],
            Self::map_history_entry,
        )?;

        debug!("Saved the meeting note on history entry {}", id);

        if let Err(e) = (HistoryUpdatePayload::Updated {
            entry: entry.clone(),
        })
        .emit(&self.app_handle)
        {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(entry)
    }

    pub fn cleanup_old_entries(&self) -> Result<()> {
        let retention_period = crate::settings::get_recording_retention_period(&self.app_handle);

        match retention_period {
            crate::settings::RecordingRetentionPeriod::Never => {
                // Don't delete anything
                Ok(())
            }
            crate::settings::RecordingRetentionPeriod::PreserveLimit => {
                // Use the old count-based logic with history_limit
                let limit = crate::settings::get_history_limit(&self.app_handle);
                self.cleanup_by_count(limit)
            }
            _ => {
                // Use time-based logic
                self.cleanup_by_time(retention_period)
            }
        }
    }

    /// Delete meetings past their retention period: the audio, its
    /// transcript segments and its note are one unit and go together, because
    /// they all live in a single history entry. Dictations are never touched
    /// here; they answer to the dictation history's own setting
    /// ([`crate::settings::RecordingRetentionPeriod`]) and its count/time
    /// cleanup above. Deletion is permanent: nothing is moved anywhere, and
    /// everything stays local to this device.
    pub fn cleanup_old_meetings(&self) -> Result<()> {
        let retention = crate::settings::get_meeting_retention_period(&self.app_handle);
        let Some(secs) = retention.retention_secs() else {
            // "Forever": meetings stay until they are deleted by hand.
            return Ok(());
        };

        // A meeting older than the cutoff, by when it started.
        let cutoff = Utc::now().timestamp() - secs;
        let conn = self.get_connection()?;
        let entries = Self::meeting_entries_before_with_conn(&conn, cutoff)?;

        if entries.is_empty() {
            return Ok(());
        }

        // The row carries the segments and the note in its metadata, so the
        // delete below takes all three; the WAV beside it is the audio.
        self.delete_entries_and_files(&entries)?;

        for (id, _) in &entries {
            if let Err(e) = (HistoryUpdatePayload::Deleted { id: *id }).emit(&self.app_handle) {
                error!("Failed to emit history-updated event: {}", e);
            }
        }

        info!(
            "Deleted {} meeting(s) past their retention period",
            entries.len()
        );

        Ok(())
    }

    /// Run [`HistoryManager::cleanup_old_meetings`] once now — app start —
    /// and then once a day while the app runs, so a session left open for
    /// days still honors the meeting retention period.
    pub fn spawn_meeting_retention_task(self: &Arc<Self>) {
        let manager = Arc::clone(self);
        thread::Builder::new()
            .name("meeting-retention".to_string())
            .spawn(move || loop {
                if let Err(e) = manager.cleanup_old_meetings() {
                    error!("Failed to clean up old meetings: {}", e);
                }
                thread::sleep(Duration::from_secs(24 * 60 * 60));
            })
            .expect("Failed to spawn the meeting retention task");
    }

    /// Every meeting that started before `cutoff`, as (id, file name).
    /// Only meetings match — a dictation take never carries
    /// `source = 'meeting'` — so the dictation history's own cleanup never
    /// runs through here and vice versa. A plain connection, not `&self`,
    /// so the retention test can drive it without an app handle.
    fn meeting_entries_before_with_conn(
        conn: &Connection,
        cutoff: i64,
    ) -> Result<Vec<(i64, String)>> {
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history
             WHERE source = 'meeting' AND timestamp < ?1",
        )?;

        let rows = stmt.query_map(params![cutoff], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries: Vec<(i64, String)> = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        Ok(entries)
    }

    fn delete_entries_and_files(&self, entries: &[(i64, String)]) -> Result<usize> {
        if entries.is_empty() {
            return Ok(0);
        }

        let conn = self.get_connection()?;
        let mut deleted_count = 0;

        for (id, file_name) in entries {
            // Delete database entry
            conn.execute(
                "DELETE FROM transcription_history WHERE id = ?1",
                params![id],
            )?;

            // Delete WAV file
            let file_path = self.recordings_dir.join(file_name);
            if file_path.exists() {
                if let Err(e) = fs::remove_file(&file_path) {
                    error!("Failed to delete WAV file {}: {}", file_name, e);
                } else {
                    debug!("Deleted old WAV file: {}", file_name);
                    deleted_count += 1;
                }
            }
        }

        Ok(deleted_count)
    }

    fn cleanup_by_count(&self, limit: usize) -> Result<()> {
        let conn = self.get_connection()?;

        // Unsaved dictations, newest first. Transcribed files are only ever
        // deleted explicitly, and don't count toward the limit; meetings
        // keep to their own retention and never appear here.
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history
             WHERE saved = 0 AND source_name IS NULL AND source IS NULL
             ORDER BY timestamp DESC",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries: Vec<(i64, String)> = Vec::new();
        for row in rows {
            entries.push(row?);
        }

        if entries.len() > limit {
            let entries_to_delete = &entries[limit..];
            let deleted_count = self.delete_entries_and_files(entries_to_delete)?;

            if deleted_count > 0 {
                debug!("Cleaned up {} old history entries by count", deleted_count);
            }
        }

        Ok(())
    }

    fn cleanup_by_time(
        &self,
        retention_period: crate::settings::RecordingRetentionPeriod,
    ) -> Result<()> {
        let conn = self.get_connection()?;

        // Calculate cutoff timestamp (current time minus retention period)
        let now = Utc::now().timestamp();
        let cutoff_timestamp = match retention_period {
            crate::settings::RecordingRetentionPeriod::Days3 => now - (3 * 24 * 60 * 60), // 3 days in seconds
            crate::settings::RecordingRetentionPeriod::Weeks2 => now - (2 * 7 * 24 * 60 * 60), // 2 weeks in seconds
            crate::settings::RecordingRetentionPeriod::Months3 => now - (3 * 30 * 24 * 60 * 60), // 3 months in seconds (approximate)
            _ => unreachable!("Should not reach here"),
        };

        // Unsaved dictations older than the cutoff; transcribed files are only
        // ever deleted explicitly, and meetings answer to their own retention.
        let mut stmt = conn.prepare(
            "SELECT id, file_name FROM transcription_history
             WHERE saved = 0 AND source_name IS NULL AND source IS NULL AND timestamp < ?1",
        )?;

        let rows = stmt.query_map(params![cutoff_timestamp], |row| {
            Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
        })?;

        let mut entries_to_delete: Vec<(i64, String)> = Vec::new();
        for row in rows {
            entries_to_delete.push(row?);
        }

        let deleted_count = self.delete_entries_and_files(&entries_to_delete)?;

        if deleted_count > 0 {
            debug!(
                "Cleaned up {} old history entries based on retention period",
                deleted_count
            );
        }

        Ok(())
    }

    pub async fn get_history_entries(
        &self,
        cursor: Option<i64>,
        limit: Option<usize>,
    ) -> Result<PaginatedHistory> {
        let conn = self.get_connection()?;
        let limit = limit.map(|l| l.min(100));

        let mut entries: Vec<HistoryEntry> = match (cursor, limit) {
            (Some(cursor_id), Some(lim)) => {
                let fetch_count = (lim + 1) as i64;
                let mut stmt = conn.prepare(&format!(
                    "SELECT {ENTRY_COLUMNS}
                     FROM transcription_history
                     WHERE id < ?1
                     ORDER BY id DESC
                     LIMIT ?2"
                ))?;
                let result = stmt
                    .query_map(params![cursor_id, fetch_count], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                result
            }
            (None, Some(lim)) => {
                let fetch_count = (lim + 1) as i64;
                let mut stmt = conn.prepare(&format!(
                    "SELECT {ENTRY_COLUMNS}
                     FROM transcription_history
                     ORDER BY id DESC
                     LIMIT ?1"
                ))?;
                let result = stmt
                    .query_map(params![fetch_count], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                result
            }
            (_, None) => {
                let mut stmt = conn.prepare(&format!(
                    "SELECT {ENTRY_COLUMNS}
                     FROM transcription_history
                     ORDER BY id DESC"
                ))?;
                let result = stmt
                    .query_map([], Self::map_history_entry)?
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                result
            }
        };

        let has_more = limit.is_some_and(|lim| entries.len() > lim);
        if has_more {
            entries.pop();
        }

        Ok(PaginatedHistory { entries, has_more })
    }

    /// Case-insensitive substring search over `title` and
    /// `transcription_text`, newest first. An empty query matches everything,
    /// so the frontend only calls this with a real query; the plain list
    /// stays `get_history_entries`.
    pub async fn search_history(
        &self,
        query: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<SearchedHistory> {
        let conn = self.get_connection()?;
        Self::search_history_with_conn(&conn, query, limit, offset)
    }

    fn search_history_with_conn(
        conn: &Connection,
        query: &str,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<SearchedHistory> {
        const MATCHES: &str = "title LIKE ?1 ESCAPE '\\' OR transcription_text LIKE ?1 ESCAPE '\\'";
        // LIKE is case-insensitive for ASCII by default; CJK and accented
        // letters match exactly, which is what a dictation search expects.
        let pattern = format!("%{}%", escape_like_pattern(query.trim()));
        let limit = limit.map(|l| l.min(100)).unwrap_or(50) as i64;
        let offset = offset.unwrap_or(0) as i64;

        let total: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM transcription_history WHERE {MATCHES}"),
            params![pattern],
            |row| row.get(0),
        )?;

        let mut stmt = conn.prepare(&format!(
            "SELECT {ENTRY_COLUMNS}
             FROM transcription_history
             WHERE {MATCHES}
             ORDER BY id DESC
             LIMIT ?2 OFFSET ?3"
        ))?;
        let entries = stmt
            .query_map(params![pattern, limit, offset], Self::map_history_entry)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let has_more = offset + (entries.len() as i64) < total;
        Ok(SearchedHistory {
            entries,
            total,
            has_more,
        })
    }

    #[cfg(test)]
    fn get_latest_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let mut stmt = conn.prepare(&format!(
            "SELECT {ENTRY_COLUMNS}
             FROM transcription_history
             ORDER BY timestamp DESC
             LIMIT 1"
        ))?;

        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    /// Get the latest dictation with non-empty transcription text. Files are
    /// left out: a long file's transcript is never the "last transcript".
    pub fn get_latest_completed_entry(&self) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        Self::get_latest_completed_entry_with_conn(&conn)
    }

    fn get_latest_completed_entry_with_conn(conn: &Connection) -> Result<Option<HistoryEntry>> {
        let mut stmt = conn.prepare(&format!(
            "SELECT {ENTRY_COLUMNS}
             FROM transcription_history
             WHERE transcription_text != '' AND source_name IS NULL
             ORDER BY timestamp DESC
             LIMIT 1"
        ))?;

        let entry = stmt.query_row([], Self::map_history_entry).optional()?;
        Ok(entry)
    }

    pub async fn toggle_saved_status(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;

        // Get current saved status
        let current_saved: bool = conn.query_row(
            "SELECT saved FROM transcription_history WHERE id = ?1",
            params![id],
            |row| row.get("saved"),
        )?;

        let new_saved = !current_saved;

        conn.execute(
            "UPDATE transcription_history SET saved = ?1 WHERE id = ?2",
            params![new_saved, id],
        )?;

        debug!("Toggled saved status for entry {}: {}", id, new_saved);

        // Emit history updated event
        if let Err(e) = (HistoryUpdatePayload::Toggled { id }).emit(&self.app_handle) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    pub fn get_audio_file_path(&self, file_name: &str) -> PathBuf {
        self.recordings_dir.join(file_name)
    }

    pub async fn get_entry_by_id(&self, id: i64) -> Result<Option<HistoryEntry>> {
        let conn = self.get_connection()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT {ENTRY_COLUMNS}
             FROM transcription_history
             WHERE id = ?1"
        ))?;

        let entry = stmt.query_row([id], Self::map_history_entry).optional()?;

        Ok(entry)
    }

    pub async fn delete_entry(&self, id: i64) -> Result<()> {
        let conn = self.get_connection()?;

        // Get the entry to find the file name
        if let Some(entry) = self.get_entry_by_id(id).await? {
            // Delete the audio file first
            let file_path = self.get_audio_file_path(&entry.file_name);
            if file_path.exists() {
                if let Err(e) = fs::remove_file(&file_path) {
                    error!("Failed to delete audio file {}: {}", entry.file_name, e);
                    // Continue with database deletion even if file deletion fails
                }
            }
        }

        // Delete from database
        conn.execute(
            "DELETE FROM transcription_history WHERE id = ?1",
            params![id],
        )?;

        debug!("Deleted history entry with id: {}", id);

        // Emit history updated event
        if let Err(e) = (HistoryUpdatePayload::Deleted { id }).emit(&self.app_handle) {
            error!("Failed to emit history-updated event: {}", e);
        }

        Ok(())
    }

    fn format_timestamp_title(&self, timestamp: i64) -> String {
        if let Some(utc_datetime) = DateTime::from_timestamp(timestamp, 0) {
            // Convert UTC to local timezone
            let local_datetime = utc_datetime.with_timezone(&Local);
            local_datetime.format("%B %e, %Y - %l:%M%p").to_string()
        } else {
            format!("Recording {}", timestamp)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};

    fn setup_conn() -> Connection {
        // The real schema comes from the app's migrations, so the test reads
        // and writes through the same columns production does.
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        HistoryManager::apply_migrations(&mut conn).expect("migrate");
        conn
    }

    fn insert_entry(conn: &Connection, timestamp: i64, text: &str, post_processed: Option<&str>) {
        conn.execute(
            "INSERT INTO transcription_history (
                file_name,
                timestamp,
                saved,
                title,
                transcription_text,
                post_processed_text,
                post_process_prompt,
                post_process_requested
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                format!("flowstate-{}.wav", timestamp),
                timestamp,
                false,
                format!("Recording {}", timestamp),
                text,
                post_processed,
                Option::<String>::None,
                false,
            ],
        )
        .expect("insert history entry");
    }

    fn user_version(conn: &Connection) -> i32 {
        conn.pragma_query_value(None, "user_version", |row| row.get(0))
            .expect("read user_version")
    }

    #[test]
    fn user_notes_round_trip_and_old_metadata_still_parses() {
        let with = HistoryMetadata {
            user_notes: Some("budsjett før fredag".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_string(&with).expect("serialize");
        let back: HistoryMetadata = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.user_notes.as_deref(), Some("budsjett før fredag"));

        // Metadata written before the field existed has no key at all.
        let old: HistoryMetadata =
            serde_json::from_str(r#"{"duration_secs":240}"#).expect("parse old metadata");
        assert_eq!(old.user_notes, None);
        assert_eq!(old.meeting_app, None);
    }

    #[test]
    fn migrating_adds_columns_without_raising_the_version() {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        HistoryManager::apply_migrations(&mut conn).expect("migrate");
        for column in ADDED_COLUMNS {
            assert!(HistoryManager::has_column(&conn, column).expect("read table info"));
        }
        assert_eq!(user_version(&conn), MIGRATIONS.len() as i32);

        // Opening it again changes nothing.
        HistoryManager::apply_migrations(&mut conn).expect("migrate again");
        assert_eq!(user_version(&conn), MIGRATIONS.len() as i32);
    }

    #[test]
    fn a_database_from_a_newer_handy_still_opens() {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        HistoryManager::apply_migrations(&mut conn).expect("migrate");
        conn.execute(
            "ALTER TABLE transcription_history ADD COLUMN from_the_future TEXT",
            [],
        )
        .expect("add newer column");
        conn.pragma_update(None, "user_version", MIGRATIONS.len() as i32 + 3)
            .expect("set newer version");

        HistoryManager::apply_migrations(&mut conn).expect("open newer database");
        insert_entry(&conn, 100, "still works", None);
        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");
        assert_eq!(entry.transcription_text, "still works");
    }

    #[test]
    fn reads_the_language_and_ignores_an_unknown_source() {
        let conn = setup_conn();
        let latest = |source: &str| {
            conn.execute(
                "INSERT INTO transcription_history (file_name, timestamp, title, transcription_text, language, language_source)
                 VALUES ('a.wav', (SELECT COUNT(*) FROM transcription_history), 'a', 'hallo', 'de', ?1)",
                [source],
            )
            .expect("insert entry");
            HistoryManager::get_latest_entry_with_conn(&conn)
                .unwrap()
                .unwrap()
        };
        let entry = latest("detected");
        assert_eq!(entry.language.as_deref(), Some("de"));
        assert_eq!(entry.language_source, Some(LanguageSource::Detected));
        assert_eq!(latest("from_the_future").language_source, None);
    }

    #[test]
    fn get_latest_entry_returns_none_when_empty() {
        let conn = setup_conn();
        let entry = HistoryManager::get_latest_entry_with_conn(&conn).expect("fetch latest entry");
        assert!(entry.is_none());
    }

    #[test]
    fn get_latest_entry_returns_newest_entry() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "first", None);
        insert_entry(&conn, 200, "second", Some("processed"));

        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");

        assert_eq!(entry.timestamp, 200);
        assert_eq!(entry.transcription_text, "second");
        assert_eq!(entry.post_processed_text.as_deref(), Some("processed"));
    }

    #[test]
    fn get_latest_completed_entry_skips_empty_entries() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "completed", None);
        insert_entry(&conn, 200, "", None);

        let entry = HistoryManager::get_latest_completed_entry_with_conn(&conn)
            .expect("fetch latest completed entry")
            .expect("completed entry exists");

        assert_eq!(entry.timestamp, 100);
        assert_eq!(entry.transcription_text, "completed");
    }

    #[test]
    fn old_metadata_without_segments_opens_cleanly() {
        // Entries stored before the segments field existed must still open.
        for stored in ["{\"duration_secs\":120}", "{}"] {
            let metadata: HistoryMetadata =
                serde_json::from_str(stored).expect("old metadata opens");
            assert!(metadata.segments.is_none(), "{stored}");
        }
        let with_duration: HistoryMetadata =
            serde_json::from_str("{\"duration_secs\":120}").expect("old metadata opens");
        assert_eq!(with_duration.duration_secs, Some(120));
    }

    #[test]
    fn metadata_round_trips_segments() {
        let metadata = HistoryMetadata {
            duration_secs: Some(90),
            segments: Some(vec![TranscriptSegment {
                t0_ms: 0,
                t1_ms: 1500,
                text: "Hei der.".to_string(),
                speaker_id: None,
            }]),
            meeting_note: None,
            speaker_names: None,
            user_notes: None,
            meeting_app: None,
            system_audio: None,
        };
        let stored = serde_json::to_string(&metadata).expect("store metadata");

        assert_eq!(
            serde_json::from_str::<HistoryMetadata>(&stored).expect("read metadata"),
            metadata
        );
    }

    #[test]
    fn meeting_retention_takes_only_old_meetings() {
        let conn = setup_conn();
        let now = 1_700_000_000;
        let cutoff = now - 7 * 24 * 60 * 60;

        // A meeting past the period, carrying its segments and note in the
        // row's metadata, beside three entries the knife must miss: a
        // meeting inside the period, an old dictation take and an old
        // dictation transcribed from a file.
        conn.execute(
            "INSERT INTO transcription_history (file_name, timestamp, title, transcription_text, source, metadata)
             VALUES ('old-meeting.wav', ?1, 'm', 'Gammelt mote.',
                     'meeting', '{\"duration_secs\":600,\"segments\":[],\"meeting_note\":{\"summary\":\"Sammendrag.\",\"decisions\":[],\"action_items\":[],\"open_questions\":[],\"model\":\"llama\",\"generated_at\":1}}')",
            params![cutoff - 1],
        )
        .expect("insert old meeting");
        conn.execute(
            "INSERT INTO transcription_history (file_name, timestamp, title, transcription_text, source, metadata)
             VALUES ('new-meeting.wav', ?1, 'm', 'Nytt mote.', 'meeting', '{}')",
            params![now],
        )
        .expect("insert new meeting");
        insert_entry(&conn, cutoff - 1, "Gammel diktering", None);
        conn.execute(
            "INSERT INTO transcription_history (file_name, timestamp, title, transcription_text, source_name)
             VALUES ('old-file.wav', ?1, 'f', 'Fra en fil.', 'rapport.wav')",
            params![cutoff - 1],
        )
        .expect("insert file dictation");

        let entries = HistoryManager::meeting_entries_before_with_conn(&conn, cutoff)
            .expect("find meetings past the period");
        assert_eq!(entries.len(), 1, "only the old meeting is selected");
        assert_eq!(entries[0].1, "old-meeting.wav");

        // Deleting the selection — the same rows
        // [`HistoryManager::delete_entries_and_files`] removes — leaves the
        // recent meeting and both dictations, audio files included, alone.
        for (id, _) in &entries {
            conn.execute(
                "DELETE FROM transcription_history WHERE id = ?1",
                params![id],
            )
            .expect("delete old meeting");
        }

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM transcription_history", [], |row| {
                row.get(0)
            })
            .expect("count remaining entries");
        assert_eq!(count, 3, "dictations and recent meetings are untouched");

        let old_meeting_left: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM transcription_history WHERE file_name = 'old-meeting.wav'",
                [],
                |row| row.get(0),
            )
            .expect("count old meetings");
        assert_eq!(old_meeting_left, 0, "the old meeting is gone for good");
    }

    #[test]
    fn dictation_retention_leaves_meetings_alone() {
        let conn = setup_conn();

        // Meetings never count toward the dictation limit, so this is the
        // whole dictation history as far as the cleanup is concerned.
        conn.execute(
            "INSERT INTO transcription_history (file_name, timestamp, title, transcription_text, source)
             VALUES ('meeting.wav', 100, 'm', 'Et mote.', 'meeting')",
            [],
        )
        .expect("insert meeting");
        conn.execute(
            "INSERT INTO transcription_history (file_name, timestamp, title, transcription_text, source, metadata)
             VALUES ('meeting-2.wav', 101, 'm', 'Et mote.', 'meeting', '{}')",
            [],
        )
        .expect("insert meeting");

        // The count-based dictation cleanup's selection, with a limit of 0:
        // every dictation would go, and no meeting may match.
        let mut stmt = conn
            .prepare(
                "SELECT id, file_name FROM transcription_history
                 WHERE saved = 0 AND source_name IS NULL AND source IS NULL
                 ORDER BY timestamp DESC",
            )
            .expect("prepare dictation cleanup query");
        let matches: Vec<(i64, String)> = stmt
            .query_map([], |row| {
                Ok((row.get::<_, i64>("id")?, row.get::<_, String>("file_name")?))
            })
            .expect("query dictation cleanup")
            .collect::<Result<_, _>>()
            .expect("collect dictation cleanup rows");
        assert!(
            matches.is_empty(),
            "meetings stay out of the dictation sweep"
        );
    }

    #[test]
    fn metadata_round_trips_a_meeting_note() {
        let metadata = HistoryMetadata {
            duration_secs: None,
            segments: None,
            user_notes: None,
            meeting_app: None,
            meeting_note: Some(MeetingNote {
                summary: "Møtet avgjorde lanseringen.".to_string(),
                decisions: vec!["Vi lanserer 1. mai.".to_string()],
                action_items: vec![crate::managers::meeting_note::MeetingActionItem {
                    task: "Kari sender rapporten".to_string(),
                    owner: "Kari".to_string(),
                }],
                open_questions: vec!["Hvem sender pressemeldingen?".to_string()],
                model: "llama3.1".to_string(),
                generated_at: 1_700_000_000,
            }),
            speaker_names: None,
            system_audio: None,
        };
        let stored = serde_json::to_string(&metadata).expect("store metadata");
        assert_eq!(
            serde_json::from_str::<HistoryMetadata>(&stored).expect("read metadata"),
            metadata
        );
    }

    #[test]
    fn old_metadata_without_a_note_opens_cleanly() {
        let metadata: HistoryMetadata =
            serde_json::from_str("{\"duration_secs\":120,\"segments\":[]}")
                .expect("old metadata opens");
        assert!(metadata.meeting_note.is_none());
    }

    #[test]
    fn an_entrys_segments_are_read_back_with_its_metadata() {
        let conn = setup_conn();
        conn.execute(
            "INSERT INTO transcription_history (file_name, timestamp, title, transcription_text, metadata)
             VALUES ('a.wav', 100, 'a', 'Hei der.',
                     '{\"segments\":[{\"t0_ms\":0,\"t1_ms\":1500,\"text\":\"Hei der.\"},{\"t0_ms\":1500,\"t1_ms\":3200,\"text\":\"Verden.\"}]}')",
            [],
        )
        .expect("insert entry with segments");

        let entry = HistoryManager::get_latest_entry_with_conn(&conn)
            .expect("fetch latest entry")
            .expect("entry exists");

        let segments = entry
            .metadata
            .expect("metadata stored")
            .segments
            .expect("segments stored");
        assert_eq!(
            segments
                .iter()
                .map(|segment| (segment.t0_ms, segment.t1_ms, segment.text.as_str()))
                .collect::<Vec<_>>(),
            vec![(0, 1500, "Hei der."), (1500, 3200, "Verden.")]
        );
        assert!(
            segments
                .windows(2)
                .all(|pair| pair[0].t0_ms <= pair[1].t0_ms),
            "stored segment start times must be monotone"
        );
    }

    #[test]
    fn get_latest_completed_entry_skips_files() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "dictated", None);
        conn.execute(
            "INSERT INTO transcription_history (file_name, timestamp, saved, title, transcription_text, source_name)
             VALUES ('handy-file-1.wav', 200, 1, 'file', 'from a file', 'talk.mp3')",
            [],
        )
        .expect("insert file entry");

        let entry = HistoryManager::get_latest_completed_entry_with_conn(&conn)
            .expect("fetch latest completed entry")
            .expect("completed entry exists");

        assert_eq!(entry.transcription_text, "dictated");
    }

    #[test]
    fn search_matches_text_case_insensitively_and_newest_first() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "hello world", None);
        insert_entry(&conn, 200, "a HELLO again", None);
        insert_entry(&conn, 300, "nothing here", None);

        let result = HistoryManager::search_history_with_conn(&conn, "hello", Some(10), None)
            .expect("search history");

        assert_eq!(result.total, 2);
        assert!(!result.has_more);
        let ids: Vec<i64> = result.entries.iter().map(|e| e.id).collect();
        // Newest entry with a match first.
        assert_eq!(ids, [2, 1]);
    }

    #[test]
    fn search_matches_titles_and_pages_with_offset() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "alpha", None);
        insert_entry(&conn, 200, "beta", None);
        // A match in the auto-generated title, not the text.
        conn.execute(
            "UPDATE transcription_history SET title = 'Meeting Recap' WHERE id = 2",
            [],
        )
        .expect("set title");

        let by_text = HistoryManager::search_history_with_conn(&conn, "recap", Some(10), None)
            .expect("search by title");
        assert_eq!(by_text.total, 1);
        assert_eq!(by_text.entries[0].id, 2);
        assert!(!by_text.has_more);

        // Both entries match "a"; a one-entry page is followed by the
        // second-newest match.
        let page = HistoryManager::search_history_with_conn(&conn, "a", Some(1), None)
            .expect("first page");
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].id, 2);
        assert!(page.has_more);
        let next = HistoryManager::search_history_with_conn(&conn, "a", Some(1), Some(1))
            .expect("second page");
        assert_eq!(next.entries.len(), 1);
        assert_eq!(next.entries[0].id, 1);
        assert!(!next.has_more);
    }

    #[test]
    fn search_over_many_entries_stays_fast() {
        let conn = setup_conn();
        for i in 0..1500 {
            insert_entry(
                &conn,
                i,
                &format!("dictation number {} with some everyday words", i),
                None,
            );
        }

        let start = std::time::Instant::now();
        for query in ["everyday", "number 700", "zzz", "dictation"] {
            let result = HistoryManager::search_history_with_conn(&conn, query, Some(30), None)
                .expect("search history");
            assert!(result.total > 0 || query == "zzz");
        }
        let elapsed = start.elapsed();
        println!("4 searches over 1500 entries took {:?}", elapsed);
        assert!(
            elapsed.as_millis() < 500,
            "search over 1500 entries took {:?}",
            elapsed
        );
    }

    #[test]
    fn search_without_matches_and_escaped_wildcards() {
        let conn = setup_conn();
        insert_entry(&conn, 100, "50% done", None);
        insert_entry(&conn, 200, "plain text", None);

        let none = HistoryManager::search_history_with_conn(&conn, "missing", Some(10), None)
            .expect("search without matches");
        assert_eq!(none.total, 0);
        assert!(none.entries.is_empty());
        assert!(!none.has_more);

        // A literal % only matches entries containing a %.
        let escaped = HistoryManager::search_history_with_conn(&conn, "50%", Some(10), None)
            .expect("search escaped wildcard");
        assert_eq!(escaped.total, 1);
        assert_eq!(escaped.entries[0].transcription_text, "50% done");

        // The unescaped wildcard would match both; escaping keeps it literal.
        let broad = HistoryManager::search_history_with_conn(&conn, "0% d", Some(10), None)
            .expect("search wildcard as wildcard");
        assert_eq!(broad.total, 1);
    }
}
