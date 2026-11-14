//! Tauri commands for the local-first sync. Everything is file copy: the
//! shared folder the user picks (iCloud Drive, Dropbox, OneDrive, USB) plus
//! a hand-carried signed bundle for when there is no folder. No server, no
//! account, no telemetry — and "off" means nothing runs at all.

use crate::managers::history::{HistoryEntry, HistoryManager};
use crate::managers::word_suggestions::WordSuggestionLearner;
use crate::settings::{get_settings, update_settings, write_settings, AppSettings};
use crate::utils::plain_file_name;
use crate::sync::{
    bundle::{export_bundle, import_bundle, BundleData},
    history_key, load_journal, merged_snippets, merged_words, now_ms, read_local_conflicts,
    reconcile, save_journal, save_state, validate_shared_folder, HistoryDocEntry, ItemSet,
    SyncState, CONFLICTS_LOG,
};
use crate::sync::{run_sync, ConflictRecord};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

/// How many history entries one sync publishes (and therefore merges).
/// Keeps the shared-folder document bounded; sync is not a backup service.
const HISTORY_SYNC_CAP: usize = 1000;

#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct SyncStatus {
    pub enabled: bool,
    pub folder: Option<String>,
    pub sync_history: bool,
    pub device_name: String,
    pub last_sync_at: Option<i64>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    pub word_count: usize,
    pub snippet_count: usize,
    pub conflict_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct SyncRunResult {
    pub status: SyncStatus,
    /// Collisions found during this run (also appended to the conflict log).
    pub conflicts: Vec<ConflictRecord>,
    pub history_imported: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct SyncImportResult {
    pub words_added: usize,
    pub snippets_added: usize,
    pub history_imported: usize,
    pub conflicts: Vec<ConflictRecord>,
}

fn state_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::portable::app_data_dir(app).map_err(|e| format!("app data directory: {e}"))?;
    Ok(dir.join("sync"))
}

fn friendly_device_name() -> String {
    if let Ok(name) = std::env::var("FLOWSTATE_DEVICE_NAME") {
        if !name.trim().is_empty() {
            return name.trim().to_string();
        }
    }
    match std::env::var("USER") {
        Ok(user) if !user.is_empty() => format!("{user} sin Mac"),
        _ => "denne Macen".to_string(),
    }
}

fn load_or_init_state(app: &AppHandle) -> Result<SyncState, String> {
    let dir = state_dir(app)?;
    match crate::sync::load_state(&dir)? {
        Some(state) => Ok(state),
        None => {
            let state = SyncState::new(friendly_device_name());
            save_state(&dir, &state)?;
            Ok(state)
        }
    }
}

/// The journal plus the current settings, reconciled: new entries get a
/// timestamp, edits bump it, removals become tombstones.
fn reconciled_items(app: &AppHandle) -> ItemSet {
    let dir = state_dir(app).unwrap_or_else(|_| PathBuf::from(".sync"));
    let journal = load_journal(&dir);
    let settings = get_settings(app);
    reconcile(
        journal,
        &settings.custom_words,
        &settings.snippets,
        now_ms(),
    )
}

/// Write the merged dictionary/snippets back into the settings store.
fn apply_merged(app: &AppHandle, merged: &ItemSet) -> Result<(), String> {
    let words = merged_words(merged);
    let snippets = merged_snippets(merged);
    // One read-modify-write step: the dictionary and snippet lists are
    // replaced wholesale here, and a user edit between read and write must
    // not be reverted by the merge.
    let mut changed = false;
    let settings = update_settings(app, |s| {
        if s.custom_words != words {
            s.custom_words = words.clone();
            changed = true;
        }
        if s.snippets != snippets {
            s.snippets = snippets.clone();
            changed = true;
        }
    });
    if changed {
        if let Some(learner) = app.try_state::<WordSuggestionLearner>() {
            learner.sync_dictionary(&settings.custom_words);
        }
        let _ = app.emit("settings-changed", serde_json::json!({ "setting": "sync" }));
    }
    Ok(())
}

fn status_of(app: &AppHandle, state: &SyncState) -> SyncStatus {
    let settings = get_settings(app);
    let conflict_count = std::fs::read_to_string(
        state_dir(app)
            .map(|d| d.join(CONFLICTS_LOG))
            .unwrap_or_else(|_| PathBuf::from(CONFLICTS_LOG)),
    )
    .map(|text| text.lines().filter(|l| !l.trim().is_empty()).count())
    .unwrap_or(0);
    SyncStatus {
        enabled: state.enabled,
        folder: state.folder.clone(),
        sync_history: state.sync_history,
        device_name: state.device_name.clone(),
        last_sync_at: state.last_sync_at,
        last_result: state.last_result.clone(),
        last_error: state.last_error.clone(),
        word_count: settings.custom_words.len(),
        snippet_count: settings.snippets.len(),
        conflict_count,
    }
}

/// All local history as sync documents, capped, oldest first so the newest
/// entries always make it into the bundle/document.
async fn collect_history(history_manager: &HistoryManager) -> (Vec<HistoryDocEntry>, Vec<String>) {
    let mut entries_out = Vec::new();
    let mut keys = Vec::new();
    let mut cursor: Option<i64> = None;
    loop {
        let Ok(page) = history_manager.get_history_entries(cursor, Some(100)).await else {
            break;
        };
        if page.entries.is_empty() {
            break;
        }
        for entry in &page.entries {
            keys.push(history_key(
                entry.timestamp,
                &entry.title,
                &entry.transcription_text,
            ));
            if entries_out.len() >= HISTORY_SYNC_CAP {
                continue;
            }
            // An empty name (no audio) or one that would leave the
            // recordings dir is skipped by the path helper.
            let has_audio = history_manager
                .get_audio_file_path(&entry.file_name)
                .is_some_and(|path| path.is_file());
            entries_out.push(HistoryDocEntry {
                key: history_key(entry.timestamp, &entry.title, &entry.transcription_text),
                file_name: if has_audio {
                    entry.file_name.clone()
                } else {
                    String::new()
                },
                timestamp: entry.timestamp,
                saved: entry.saved,
                title: entry.title.clone(),
                transcription_text: entry.transcription_text.clone(),
                post_processed_text: entry.post_processed_text.clone(),
                language: entry.language.clone(),
            });
        }
        if !page.has_more {
            break;
        }
        cursor = page.entries.last().map(|e| e.id);
        if cursor.is_none() || entries_out.len() >= HISTORY_SYNC_CAP {
            break;
        }
    }
    (entries_out, keys)
}

/// Copy an audio file arriving from sync into the recordings dir without
/// clobbering an existing file, and return the name it landed under.
fn import_audio_file(
    history_manager: &HistoryManager,
    source: &Path,
    original_name: &str,
) -> Option<String> {
    // The name arrives from a peer or a bundle: only a plain file name may
    // be joined onto the recordings dir, or the copy lands outside it.
    let original_name = plain_file_name(original_name).ok()?;
    if original_name.is_empty() || !source.is_file() {
        return None;
    }
    let recordings = history_manager.recordings_dir().to_path_buf();
    let mut target = recordings.join(&original_name);
    if target.exists() {
        let stem = format!("sync-{}", original_name);
        target = recordings.join(stem);
        if target.exists() {
            return None; // already imported
        }
    }
    std::fs::copy(source, &target).ok()?;
    target.file_name()?.to_str().map(|s| s.to_string())
}

fn insert_synced_history(
    history_manager: &HistoryManager,
    entry: &HistoryDocEntry,
    audio_name: Option<String>,
) -> bool {
    if entry.transcription_text.trim().is_empty() {
        return false;
    }
    let inserted = HistoryEntry {
        id: 0,
        file_name: audio_name.unwrap_or_default(),
        timestamp: entry.timestamp,
        saved: entry.saved,
        title: String::new(),
        transcription_text: entry.transcription_text.clone(),
        post_processed_text: entry.post_processed_text.clone(),
        post_process_prompt: None,
        post_process_requested: false,
        source_name: None,
        language: entry.language.clone(),
        language_source: None,
        source: None,
        metadata: None,
    };
    history_manager.save_synced_entry(inserted).is_ok()
}

#[tauri::command]
#[specta::specta]
pub fn sync_get_status(app: AppHandle) -> Result<SyncStatus, String> {
    let state = load_or_init_state(&app)?;
    Ok(status_of(&app, &state))
}

/// Point sync at a shared folder. A bad folder is refused and changes
/// nothing.
#[tauri::command]
#[specta::specta]
pub fn sync_set_folder(app: AppHandle, folder: Option<String>) -> Result<SyncStatus, String> {
    let dir = state_dir(&app)?;
    let mut state = load_or_init_state(&app)?;
    match folder.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        Some(folder) => {
            validate_shared_folder(Path::new(folder))?;
            state.folder = Some(folder.to_string());
        }
        None => {
            state.folder = None;
        }
    }
    save_state(&dir, &state)?;
    Ok(status_of(&app, &state))
}

/// The master switch. Off stops every sync activity: nothing is published,
/// nothing is merged, nothing is written.
#[tauri::command]
#[specta::specta]
pub fn sync_set_enabled(app: AppHandle, enabled: bool) -> Result<SyncStatus, String> {
    let dir = state_dir(&app)?;
    let mut state = load_or_init_state(&app)?;
    if enabled && state.folder.is_none() {
        return Err("no shared folder selected".into());
    }
    state.enabled = enabled;
    if !enabled {
        state.last_error = None;
    }
    save_state(&dir, &state)?;
    Ok(status_of(&app, &state))
}

#[tauri::command]
#[specta::specta]
pub fn sync_set_sync_history(app: AppHandle, value: bool) -> Result<SyncStatus, String> {
    let dir = state_dir(&app)?;
    let mut state = load_or_init_state(&app)?;
    state.sync_history = value;
    save_state(&dir, &state)?;
    Ok(status_of(&app, &state))
}

/// One sync pass: publish, merge every peer, apply. Works with the network
/// unplugged — it is only file copy. A refused sync (off, no folder,
/// unreadable folder) changes nothing.
#[tauri::command]
#[specta::specta]
pub async fn sync_now(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<SyncRunResult, String> {
    let state = load_or_init_state(&app)?;
    if !state.enabled {
        return Err("sync is off".into());
    }
    let dir = state_dir(&app)?;
    let local = reconciled_items(&app);

    let (history, local_keys) = if state.sync_history {
        collect_history(history_manager.inner()).await
    } else {
        (Vec::new(), Vec::new())
    };

    let audio_dir = history_manager.recordings_dir().to_path_buf();
    let outcome = run_sync(
        &dir,
        &state,
        local,
        if state.sync_history {
            Some(history)
        } else {
            None
        },
        &local_keys,
        Some(&audio_dir),
    )?;

    apply_merged(&app, &outcome.merged)?;
    save_journal(&dir, &outcome.merged)?;

    // History arrives as plain files: copy the audio in, then insert the
    // entry. Sync never deletes history.
    let mut imported = 0;
    if state.sync_history {
        let root = PathBuf::from(state.folder.as_deref().expect("checked in run_sync"))
            .join(crate::sync::SHARED_DIR_NAME);
        for (device_id, entry) in &outcome.imported_history {
            // The peer's device id and file name are both untrusted: only
            // plain names may be joined onto the shared folder (the import
            // re-checks the file name before joining onto the recordings
            // dir).
            let audio_name = plain_file_name(device_id)
                .ok()
                .zip(plain_file_name(&entry.file_name).ok())
                .and_then(|(device_id, name)| {
                    let source = root
                        .join(crate::sync::HISTORY_SUBDIR)
                        .join(&device_id)
                        .join(crate::sync::AUDIO_SUBDIR)
                        .join(&name);
                    import_audio_file(history_manager.inner(), &source, &name)
                });
            if insert_synced_history(history_manager.inner(), entry, audio_name) {
                imported += 1;
            }
        }
    }

    let mut state = state;
    state.last_sync_at = Some(outcome.synced_at);
    state.last_result = Some("ok".into());
    state.last_error = None;
    state.peers = outcome.peers.clone();
    save_state(&dir, &state)?;

    Ok(SyncRunResult {
        status: status_of(&app, &state),
        conflicts: outcome.conflicts,
        history_imported: imported,
    })
}

/// Open the shared folder in Finder/Files — the same window onto "where
/// does this stuff actually live" the app data folder has.
#[tauri::command]
#[specta::specta]
pub fn sync_open_folder(app: AppHandle) -> Result<(), String> {
    let state = load_or_init_state(&app)?;
    let folder = state
        .folder
        .clone()
        .ok_or_else(|| "no shared folder selected".to_string())?;
    app.opener()
        .open_path(folder, None::<String>)
        .map_err(|e| format!("Failed to open the shared folder: {e}"))?;
    Ok(())
}

/// Recent collisions from this machine's own log, newest first.
#[tauri::command]
#[specta::specta]
pub fn sync_get_conflicts(
    app: AppHandle,
    limit: Option<usize>,
) -> Result<Vec<ConflictRecord>, String> {
    let dir = state_dir(&app)?;
    Ok(read_local_conflicts(&dir, limit.unwrap_or(20)))
}

/// Export the signed FlowState bundle (a sealed zip: settings excerpt,
/// dictionary, snippets, optional history with audio).
#[tauri::command]
#[specta::specta]
pub async fn sync_export_bundle(
    app: AppHandle,
    path: String,
    include_history: bool,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<(), String> {
    let state = load_or_init_state(&app)?;
    let settings = get_settings(&app);
    let excerpt = serde_json::json!({
        "selected_language": settings.selected_language,
        "spoken_languages": settings.spoken_languages,
        "app_language": settings.app_language,
        "theme": settings.theme,
    });
    let items = reconciled_items(&app);

    let (history, _keys) = if include_history {
        collect_history(history_manager.inner()).await
    } else {
        (Vec::new(), Vec::new())
    };

    let mut audio = std::collections::HashMap::new();
    for (index, entry) in history.iter().enumerate() {
        if let Some(source) = history_manager.get_audio_file_path(&entry.file_name) {
            if source.is_file() {
                audio.insert(format!("audio/{:04}-{}", index, entry.file_name), source);
            }
        }
    }

    export_bundle(
        Path::new(&path),
        &BundleData {
            excerpt,
            items,
            history,
            audio,
        },
        &state.device_name,
        &app.package_info().version.to_string(),
    )
}

/// Import a bundle. The seal and every member hash are verified first; a
/// damaged or tampered bundle is refused and nothing is touched. Otherwise
/// dictionary, snippets and history merge in — union, with conflicts logged
/// and both versions preserved.
#[tauri::command]
#[specta::specta]
pub async fn sync_import_bundle(
    app: AppHandle,
    path: String,
    history_manager: State<'_, Arc<HistoryManager>>,
) -> Result<SyncImportResult, String> {
    let dir = state_dir(&app)?;
    let staging = dir.join("staging").join(format!("{}", now_ms()));
    let imported = import_bundle(Path::new(&path), &staging)?;

    // Merge the bundle's items into ours. Empty base: everything the bundle
    // carries counts as a change, so pre-existing identical entries stay put
    // and differing ones collide loudly instead of overwriting.
    let local = reconciled_items(&app);
    let now = now_ms();
    let (merged, conflicts) = crate::sync::merge_peer(
        local.clone(),
        &imported.items,
        &ItemSet::default(),
        &imported.manifest.device_name,
        now,
    );
    let words_before = local.words.len();
    let snippets_before = local.snippets.len();

    apply_merged(&app, &merged)?;
    save_journal(&dir, &merged)?;
    if !conflicts.is_empty() {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join(CONFLICTS_LOG))
            .map_err(|e| e.to_string())?;
        for record in &conflicts {
            let _ = writeln!(
                file,
                "{}",
                serde_json::to_string(record).map_err(|e| e.to_string())?
            );
        }
    }

    // History: only entries this machine has never seen, keyed by content.
    let (_docs, local_keys) = collect_history(history_manager.inner()).await;
    let mut history_imported = 0;
    for (index, entry) in imported.history.iter().enumerate() {
        let key = history_key(entry.timestamp, &entry.title, &entry.transcription_text);
        if local_keys.contains(&key) {
            continue;
        }
        // The bundle's file name is untrusted: only a plain name may be
        // joined onto the recordings dir, and the audio member is matched by
        // its exact key (`audio/{index}-{name}`, as exported) so one name
        // that ends with another cannot steal its audio.
        let audio_name = plain_file_name(&entry.file_name)
            .ok()
            .and_then(|name| {
                let member = format!("audio/{:04}-{}", index, name);
                imported
                    .audio
                    .get(&member)
                    .and_then(|path| import_audio_file(history_manager.inner(), path, &name))
            });
        if insert_synced_history(history_manager.inner(), entry, audio_name) {
            history_imported += 1;
        }
    }

    // Settings excerpt: language/theme choices travel with the bundle.
    if imported.excerpt.is_object() {
        let mut settings: AppSettings = get_settings(&app);
        let mut changed = false;
        if let Some(value) = imported
            .excerpt
            .get("selected_language")
            .and_then(|v| v.as_str())
        {
            settings.selected_language = value.to_string();
            changed = true;
        }
        if let Some(value) = imported.excerpt.get("spoken_languages") {
            if let Ok(list) = serde_json::from_value::<Vec<String>>(value.clone()) {
                settings.spoken_languages = list;
                changed = true;
            }
        }
        if let Some(value) = imported
            .excerpt
            .get("app_language")
            .and_then(|v| v.as_str())
        {
            settings.app_language = value.to_string();
            changed = true;
        }
        if let Some(value) = imported.excerpt.get("theme") {
            if let Ok(theme) = serde_json::from_value::<crate::settings::Theme>(value.clone()) {
                settings.theme = theme;
                changed = true;
            }
        }
        if changed {
            write_settings(&app, settings);
            let _ = app.emit("settings-changed", serde_json::json!({ "setting": "sync" }));
        }
    }

    let _ = std::fs::remove_dir_all(&staging);

    Ok(SyncImportResult {
        words_added: merged.words.len().saturating_sub(words_before),
        snippets_added: merged.snippets.len().saturating_sub(snippets_before),
        history_imported,
        conflicts,
    })
}
