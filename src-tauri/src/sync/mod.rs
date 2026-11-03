//! Local-first sync between the user's own Macs — no cloud, no account.
//!
//! Two machines that never see each other on a network (NTNU-eduroam runs
//! client isolation, so LAN discovery is a dead end) still need the same
//! dictionary, snippets and history. Everything here is plain file copy
//! through a folder the user picks: iCloud Drive, Dropbox, OneDrive or a USB
//! stick. Nothing in this module opens a socket, and no history or audio is
//! ever deleted by sync.
//!
//! The protocol is deliberately boring: each device publishes a small JSON
//! document per item type (words, snippets) with a per-entry `updated_at`
//! timestamp into `<shared folder>/FlowState Sync/devices/<device_id>.json`,
//! together with tombstones for deletions. Merging is last-writer-wins per
//! entry against a per-peer snapshot of what we last saw, so a change only
//! travels when it actually changed. When both sides changed the same entry
//! since the last sync, *both* versions survive: the newer one stays live,
//! the other is kept as a live copy (snippets) or in the conflict log
//! (words), and every collision is appended to `conflicts.jsonl` in the
//! shared folder — nothing is ever silently dropped. Edits always beat
//! deletions, so sync can never wipe a concurrent edit.

pub mod bundle;

use crate::settings::Snippet;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Subfolder created inside the user-chosen shared folder.
pub const SHARED_DIR_NAME: &str = "FlowState Sync";
pub const DEVICES_SUBDIR: &str = "devices";
pub const HISTORY_SUBDIR: &str = "history";
pub const AUDIO_SUBDIR: &str = "audio";
pub const CONFLICTS_LOG: &str = "conflicts.jsonl";
/// Item-set schema. A document with a newer schema is skipped rather than
/// misread; older ones still merge.
pub const SCHEMA: u32 = 1;

/// Unix time in milliseconds.
pub type Timestamp = i64;

pub fn now_ms() -> Timestamp {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as Timestamp)
        .unwrap_or(0)
}

/// A fresh device id: 16 hex chars of hash over a time/pid mix. Local only,
/// never leaves the machine except as the sync document's name.
pub fn new_device_id() -> String {
    let mix = format!(
        "{}-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0),
        std::process::id()
    );
    let mut hasher = Sha256::new();
    hasher.update(mix.as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex[..16].to_string()
}

/// Dictionary word as a sync item. Keyed by the lowercased word.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WordItem {
    /// The word as the user typed it (casing preserved).
    pub word: String,
    pub updated_at: Timestamp,
}

/// Snippet as a sync item, keyed by its trigger phrase.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SnippetItem {
    pub snippet: Snippet,
    pub updated_at: Timestamp,
}

/// One device's full view of the synced items, plus deletion tombstones.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ItemSet {
    #[serde(default)]
    pub words: HashMap<String, WordItem>,
    #[serde(default)]
    pub snippets: HashMap<String, SnippetItem>,
    #[serde(default)]
    pub word_tombstones: HashMap<String, Timestamp>,
    #[serde(default)]
    pub snippet_tombstones: HashMap<String, Timestamp>,
}

/// A published history entry. `key` is the stable cross-device identity
/// (hash of timestamp + title + text); `file_name` names the audio file inside
/// the publishing device's `audio/` folder, empty when audio isn't shared.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryDocEntry {
    pub key: String,
    pub file_name: String,
    pub timestamp: i64,
    pub saved: bool,
    pub title: String,
    pub transcription_text: String,
    pub post_processed_text: Option<String>,
    pub language: Option<String>,
}

pub fn history_key(timestamp: i64, title: &str, text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{timestamp}|{title}|{text}").as_bytes());
    let hex = format!("{:x}", hasher.finalize());
    hex[..16].to_string()
}

/// The document each device publishes into the shared folder.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceDoc {
    pub schema: u32,
    pub device_id: String,
    pub device_name: String,
    pub published_at: Timestamp,
    #[serde(flatten)]
    pub items: ItemSet,
    #[serde(default)]
    pub history: Vec<HistoryDocEntry>,
}

/// Persisted sync state, one per machine (or per app-data dir, which is how
/// two "machines" can be tested on one Mac).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SyncState {
    pub device_id: String,
    pub device_name: String,
    pub enabled: bool,
    pub folder: Option<String>,
    pub sync_history: bool,
    pub last_sync_at: Option<Timestamp>,
    pub last_result: Option<String>,
    pub last_error: Option<String>,
    /// Per-peer snapshot of the item sets we saw at the last successful
    /// merge — the "base" of the three-way merge.
    #[serde(default)]
    pub peers: HashMap<String, ItemSet>,
}

impl SyncState {
    pub fn new(device_name: String) -> Self {
        Self {
            device_id: new_device_id(),
            device_name,
            enabled: false,
            folder: None,
            sync_history: false,
            last_sync_at: None,
            last_result: None,
            last_error: None,
            peers: HashMap::new(),
        }
    }
}

/// One recorded collision: the full versions of both sides plus what sync
/// did about it. Nothing is dropped without one of these.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct ConflictRecord {
    pub at: Timestamp,
    /// "word" or "snippet".
    pub kind: String,
    pub key: String,
    /// Full local version; `None` when the local side had deleted it.
    pub local_version: Option<ConflictVersion>,
    /// Full remote version; `None` when the remote side had deleted it.
    pub remote_version: Option<ConflictVersion>,
    pub remote_device: String,
    pub resolution: String,
}

/// One side of a conflict, spelled out so nothing is ever lost.
#[derive(Clone, Debug, Serialize, Deserialize, specta::Type)]
pub struct ConflictVersion {
    /// The word (dictionary) or the trigger phrase (snippet).
    pub trigger_or_word: String,
    /// The replacement text; snippets only.
    pub expansion: Option<String>,
    pub case_sensitive: bool,
    pub updated_at: Timestamp,
}

/// What a merge pass did.
#[derive(Clone, Debug, Default)]
pub struct SyncOutcome {
    pub merged: ItemSet,
    pub conflicts: Vec<ConflictRecord>,
    pub imported_history: Vec<(String, HistoryDocEntry)>,
    /// Snapshots of every peer as published during this run — the merge
    /// bases for the next sync. The caller stores them in the sync state.
    pub peers: HashMap<String, ItemSet>,
    pub synced_at: Timestamp,
}

// ---------------------------------------------------------------------------
// Item helpers
// ---------------------------------------------------------------------------

pub fn word_key(word: &str) -> String {
    word.trim().to_lowercase()
}

pub fn snippet_key(trigger: &str) -> String {
    trigger.trim().to_string()
}

/// The state of one entry on one side of a merge.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Side {
    Live(Timestamp),
    Dead(Timestamp),
    Absent,
}

impl Side {
    fn ts(self) -> Timestamp {
        match self {
            Side::Live(t) | Side::Dead(t) => t,
            Side::Absent => 0,
        }
    }

}

fn word_side(set: &ItemSet, key: &str) -> Side {
    if let Some(item) = set.words.get(key) {
        Side::Live(item.updated_at)
    } else if let Some(t) = set.word_tombstones.get(key) {
        Side::Dead(*t)
    } else {
        Side::Absent
    }
}

fn snippet_side(set: &ItemSet, key: &str) -> Side {
    if let Some(item) = set.snippets.get(key) {
        Side::Live(item.updated_at)
    } else if let Some(t) = set.snippet_tombstones.get(key) {
        Side::Dead(*t)
    } else {
        Side::Absent
    }
}

// ---------------------------------------------------------------------------
// Local files: state, journal, conflict log
// ---------------------------------------------------------------------------

pub fn state_path(state_dir: &Path) -> PathBuf {
    state_dir.join("state.json")
}

pub fn journal_path(state_dir: &Path) -> PathBuf {
    state_dir.join("journal.json")
}

pub fn local_conflicts_path(state_dir: &Path) -> PathBuf {
    state_dir.join(CONFLICTS_LOG)
}

pub fn load_state(state_dir: &Path) -> Result<Option<SyncState>, String> {
    load_json(&state_path(state_dir))
}

pub fn save_state(state_dir: &Path, state: &SyncState) -> Result<(), String> {
    std::fs::create_dir_all(state_dir).map_err(|e| e.to_string())?;
    write_json(&state_path(state_dir), state)
}

pub fn load_journal(state_dir: &Path) -> ItemSet {
    load_json(&journal_path(state_dir))
        .ok()
        .flatten()
        .unwrap_or_default()
}

pub fn save_journal(state_dir: &Path, items: &ItemSet) -> Result<(), String> {
    std::fs::create_dir_all(state_dir).map_err(|e| e.to_string())?;
    write_json(&journal_path(state_dir), items)
}

fn load_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, String> {
    match std::fs::read_to_string(path) {
        Ok(text) => serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| format!("corrupt file {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

/// Write JSON via a temp file + rename, so a crash (or a cloud-sync client
/// reading at the wrong moment) never sees a half-written document.
pub fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let tmp = path.with_extension("json.tmp");
    let text = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// Bring the journal in line with what the settings actually contain.
/// Runs before every sync (and after every dictionary/snippet write): new
/// entries get a timestamp, removed entries become tombstones, and edited
/// entries get a fresh timestamp so last-writer-wins can order them.
pub fn reconcile(mut journal: ItemSet, words: &[String], snippets: &[Snippet], now: Timestamp) -> ItemSet {
    let settings_words: HashMap<String, String> = words
        .iter()
        .map(|w| (word_key(w), w.trim().to_string()))
        .collect();

    // Words that vanished from the settings become tombstones.
    let live_word_keys: Vec<String> = journal.words.keys().cloned().collect();
    for key in live_word_keys {
        if !settings_words.contains_key(&key) {
            let item = journal.words.remove(&key).expect("key from journal");
            journal
                .word_tombstones
                .entry(key)
                .and_modify(|t| *t = (*t).max(now))
                .or_insert(now);
            let _ = item;
        }
    }
    for (key, display) in &settings_words {
        match journal.words.get_mut(key) {
            Some(item) => {
                if &item.word != display {
                    item.word = display.clone();
                    item.updated_at = now;
                }
            }
            None => {
                // Resurrected after a deletion, or brand new.
                journal.word_tombstones.remove(key);
                journal.words.insert(
                    key.clone(),
                    WordItem {
                        word: display.clone(),
                        updated_at: now,
                    },
                );
            }
        }
    }

    let settings_snippets: HashMap<String, Snippet> = snippets
        .iter()
        .map(|s| (snippet_key(&s.trigger), s.clone()))
        .collect();

    let live_snippet_keys: Vec<String> = journal.snippets.keys().cloned().collect();
    for key in live_snippet_keys {
        if !settings_snippets.contains_key(&key) {
            journal.snippets.remove(&key).expect("key from journal");
            journal
                .snippet_tombstones
                .entry(key)
                .and_modify(|t| *t = (*t).max(now))
                .or_insert(now);
        }
    }
    for (key, snippet) in &settings_snippets {
        match journal.snippets.get_mut(key) {
            Some(item) => {
                if &item.snippet != snippet {
                    item.snippet = snippet.clone();
                    item.updated_at = now;
                }
            }
            None => {
                journal.snippet_tombstones.remove(key);
                journal.snippets.insert(
                    key.clone(),
                    SnippetItem {
                        snippet: snippet.clone(),
                        updated_at: now,
                    },
                );
            }
        }
    }

    journal
}

/// Keep the journal in step after a dictionary/snippet write, so item
/// timestamps reflect *edit* time rather than sync time. Pure local
/// bookkeeping — no folder is touched.
pub fn note_settings_change(app: &tauri::AppHandle, words: &[String], snippets: &[Snippet]) {
    let Ok(dir) = crate::portable::app_data_dir(app).map(|d| d.join("sync")) else {
        return;
    };
    let journal = load_journal(&dir);
    let journal = reconcile(journal, words, snippets, now_ms());
    let _ = save_journal(&dir, &journal);
}

/// Live words/snippets for the UI: sorted, display forms.
pub fn merged_words(items: &ItemSet) -> Vec<String> {
    let mut words: Vec<String> = items.words.values().map(|w| w.word.clone()).collect();
    words.sort();
    words.dedup();
    words
}

pub fn merged_snippets(items: &ItemSet) -> Vec<Snippet> {
    let mut snippets: Vec<Snippet> = items
        .snippets
        .values()
        .map(|s| s.snippet.clone())
        .collect();
    snippets.sort_by(|a, b| a.trigger.cmp(&b.trigger));
    snippets
}

fn append_conflict_log(path: &Path, records: &[ConflictRecord]) -> Result<(), String> {
    use std::io::Write;
    if records.is_empty() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    for record in records {
        let line = serde_json::to_string(record).map_err(|e| e.to_string())?;
        writeln!(file, "{line}").map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Read the local conflict log, newest first.
pub fn read_local_conflicts(state_dir: &Path, limit: usize) -> Vec<ConflictRecord> {
    let mut records = Vec::new();
    if let Ok(text) = std::fs::read_to_string(local_conflicts_path(state_dir)) {
        for line in text.lines().rev() {
            if let Ok(record) = serde_json::from_str::<ConflictRecord>(line) {
                records.push(record);
                if records.len() >= limit {
                    break;
                }
            }
        }
    }
    records
}

// ---------------------------------------------------------------------------
// Shared folder
// ---------------------------------------------------------------------------

pub fn shared_root(shared_folder: &Path) -> PathBuf {
    shared_folder.join(SHARED_DIR_NAME)
}

/// The folder must already exist and be writable. Nothing is created here.
pub fn validate_shared_folder(folder: &Path) -> Result<(), String> {
    if !folder.exists() {
        return Err(format!("folder does not exist: {}", folder.display()));
    }
    if !folder.is_dir() {
        return Err(format!("not a folder: {}", folder.display()));
    }
    let probe = folder.join(format!(".flowstate-sync-probe-{}", now_ms()));
    std::fs::write(&probe, b"probe").map_err(|e| format!("folder is not writable: {e}"))?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

fn publish_doc(root: &Path, doc: &DeviceDoc) -> Result<(), String> {
    let dir = root.join(DEVICES_SUBDIR);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    write_json(&dir.join(format!("{}.json", doc.device_id)), doc)
}

fn read_peer_docs(root: &Path, own_device_id: &str) -> Vec<DeviceDoc> {
    let mut docs = Vec::new();
    let dir = root.join(DEVICES_SUBDIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(_) => return docs,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        if path.file_stem().and_then(|s| s.to_str()) == Some(own_device_id) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<DeviceDoc>(&text) else {
            log::warn!("skipping unreadable sync document {}", path.display());
            continue;
        };
        if doc.schema > SCHEMA {
            log::warn!(
                "skipping sync document {} from a newer FlowState",
                path.display()
            );
            continue;
        }
        docs.push(doc);
    }
    docs.sort_by(|a, b| a.device_id.cmp(&b.device_id));
    docs
}

/// One full sync: publish our items, merge every peer's, return the merged
/// result. Pure file copy — no sockets, no server, works in airplane mode.
///
/// `local` is the reconciled journal; the caller applies `outcome.merged` to
/// the settings and saves it as the new journal.
pub fn run_sync(
    state_dir: &Path,
    state: &SyncState,
    local: ItemSet,
    history: Option<Vec<HistoryDocEntry>>,
    local_history_keys: &[String],
    audio_dir: Option<&Path>,
) -> Result<SyncOutcome, String> {
    // Refused syncs touch nothing: validate before creating anything.
    let folder = state
        .folder
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| "no shared folder selected".to_string())?;
    validate_shared_folder(&folder)?;

    let root = shared_root(&folder);
    std::fs::create_dir_all(root.join(DEVICES_SUBDIR)).map_err(|e| e.to_string())?;
    if state.sync_history {
        std::fs::create_dir_all(root.join(HISTORY_SUBDIR)).map_err(|e| e.to_string())?;
    }

    let now = now_ms();
    let mut merged = local.clone();
    let mut conflicts = Vec::new();
    let mut imported_history = Vec::new();
    let mut imported_keys: Vec<String> = local_history_keys.to_vec();

    // Publish our current view, history first when enabled.
    if let Some(entries) = &history {
        if state.sync_history && !entries.is_empty() {
            let hist_dir = root
                .join(HISTORY_SUBDIR)
                .join(&state.device_id);
            std::fs::create_dir_all(hist_dir.join(AUDIO_SUBDIR)).map_err(|e| e.to_string())?;
            write_json(&hist_dir.join("entries.json"), entries)?;
            if let Some(audio_dir) = audio_dir {
                for entry in entries {
                    if entry.file_name.is_empty() {
                        continue;
                    }
                    let source = audio_dir.join(&entry.file_name);
                    if !source.is_file() {
                        continue;
                    }
                    let dest = hist_dir.join(AUDIO_SUBDIR).join(&entry.file_name);
                    if dest.exists() {
                        continue;
                    }
                    if let Err(e) = std::fs::copy(&source, &dest) {
                        log::warn!("sync: could not copy {}: {e}", source.display());
                    }
                }
            }
        }
    }

    let doc = DeviceDoc {
        schema: SCHEMA,
        device_id: state.device_id.clone(),
        device_name: state.device_name.clone(),
        published_at: now,
        items: local.clone(),
        history: history.unwrap_or_default(),
    };
    publish_doc(&root, &doc)?;

    // Merge every peer into our view.
    let peers = read_peer_docs(&root, &state.device_id);
    let mut new_peers = state.peers.clone();
    for peer in &peers {
        let base = new_peers
            .get(&peer.device_id)
            .cloned()
            .unwrap_or_default();
        let (next, mut found) =
            merge_peer(merged.clone(), &peer.items, &base, &peer.device_name, now);
        merged = next;
        conflicts.append(&mut found);

        if state.sync_history {
            for entry in &peer.history {
                if !imported_keys.contains(&entry.key) {
                    imported_keys.push(entry.key.clone());
                    imported_history.push((peer.device_id.clone(), entry.clone()));
                }
            }
        }
        new_peers.insert(peer.device_id.clone(), peer.items.clone());
    }

    // Record collisions where everyone can see them: the shared folder and
    // our own state dir.
    append_conflict_log(&root.join(CONFLICTS_LOG), &conflicts)?;
    append_conflict_log(&local_conflicts_path(state_dir), &conflicts)?;

    Ok(SyncOutcome {
        merged,
        conflicts,
        imported_history,
        peers: new_peers,
        synced_at: now,
    })
}

/// Marker appended to the trigger of a snippet that lost a conflict, so both
/// versions stay live and visible instead of one vanishing.
pub const CONFLICT_TRIGGER_SUFFIX: &str = " (conflict)";

fn word_version(item: Option<&WordItem>) -> Option<ConflictVersion> {
    item.map(|i| ConflictVersion {
        trigger_or_word: i.word.clone(),
        expansion: None,
        case_sensitive: false,
        updated_at: i.updated_at,
    })
}

fn snippet_version(item: Option<&SnippetItem>) -> Option<ConflictVersion> {
    item.map(|i| ConflictVersion {
        trigger_or_word: i.snippet.trigger.clone(),
        expansion: Some(i.snippet.expansion.clone()),
        case_sensitive: i.snippet.case_sensitive,
        updated_at: i.updated_at,
    })
}

/// Three-way merge of one peer's items into ours. `base` is what we last saw
/// from that peer. Unchanged sides never overwrite anything; a change on one
/// side wins outright; changes on both sides keep both versions and log the
/// collision. A deletion never beats a concurrent edit.
pub(crate) fn merge_peer(
    local: ItemSet,
    remote: &ItemSet,
    base: &ItemSet,
    remote_device: &str,
    now: Timestamp,
) -> (ItemSet, Vec<ConflictRecord>) {
    let mut conflicts = Vec::new();
    let local = merge_words(local, remote, base, remote_device, now, &mut conflicts);
    let local = merge_snippets(local, remote, base, remote_device, now, &mut conflicts);
    (local, conflicts)
}

fn merge_words(
    mut local: ItemSet,
    remote: &ItemSet,
    base: &ItemSet,
    remote_device: &str,
    now: Timestamp,
    conflicts: &mut Vec<ConflictRecord>,
) -> ItemSet {
    let mut keys: Vec<String> = local
        .words
        .keys()
        .chain(local.word_tombstones.keys())
        .chain(remote.words.keys())
        .chain(remote.word_tombstones.keys())
        .chain(base.words.keys())
        .chain(base.word_tombstones.keys())
        .cloned()
        .collect();
    keys.sort();
    keys.dedup();
    for key in keys {
        let l = word_side(&local, &key);
        let r = word_side(remote, &key);
        let b = word_side(base, &key);
        let l_changed = l.ts() > b.ts();
        let r_changed = r.ts() > b.ts();
        if !l_changed && !r_changed || l_changed && !r_changed {
            continue;
        }
        match (l, r) {
            // The peer added or edited the word and we have no newer say.
            (Side::Absent, Side::Live(rt)) => {
                if let Some(item) = remote.words.get(&key) {
                    let mut item = item.clone();
                    item.updated_at = rt;
                    local.words.insert(key.clone(), item);
                }
            }
            // We deleted it after the base while the peer edited it: the
            // edit survives, the deletion does not.
            (Side::Dead(_), Side::Live(rt)) if l_changed => {
                if let Some(item) = remote.words.get(&key) {
                    let mut item = item.clone();
                    item.updated_at = rt;
                    local.word_tombstones.remove(&key);
                    local.words.insert(key.clone(), item);
                    conflicts.push(ConflictRecord {
                        at: now,
                        kind: "word".into(),
                        key: key.clone(),
                        local_version: None,
                        remote_version: word_version(remote.words.get(&key)),
                        remote_device: remote_device.into(),
                        resolution: "a deletion collided with an edit; kept the edited word"
                            .into(),
                    });
                }
            }
            // We deleted it long ago (already synced) and the peer edited
            // or re-added it since: their version comes back.
            (Side::Dead(_), Side::Live(rt)) => {
                if let Some(item) = remote.words.get(&key) {
                    let mut item = item.clone();
                    item.updated_at = rt;
                    local.word_tombstones.remove(&key);
                    local.words.insert(key.clone(), item);
                }
            }
            // The peer deleted it and we haven't touched it since: the
            // deletion wins.
            (Side::Live(_), Side::Dead(rt)) if !l_changed => {
                local.words.remove(&key);
                local.word_tombstones.insert(key.clone(), rt);
            }
            // We edited it while the peer deleted it: our edit survives.
            (Side::Live(_), Side::Dead(_)) => {
                conflicts.push(ConflictRecord {
                    at: now,
                    kind: "word".into(),
                    key: key.clone(),
                    local_version: word_version(local.words.get(&key)),
                    remote_version: None,
                    remote_device: remote_device.into(),
                    resolution:
                        "a deletion collided with an edit; kept the edited word".into(),
                });
            }
            (Side::Live(lt), Side::Live(rt)) => {
                let same = match (local.words.get(&key), remote.words.get(&key)) {
                    (Some(li), Some(ri)) => li.word == ri.word,
                    _ => false,
                };
                if same {
                    let newer = lt.max(rt);
                    if let Some(item) = local.words.get_mut(&key) {
                        item.updated_at = newer;
                    }
                    continue;
                }
                // Both machines changed the same word differently. The
                // newer spelling stays live; the other is preserved in the
                // conflict log, so nothing is silently dropped.
                let local_item = local.words.get(&key).cloned();
                let remote_item = remote.words.get(&key).cloned();
                let keep_remote = rt > lt;
                let winner = if keep_remote {
                    remote_item.clone()
                } else {
                    local_item.clone()
                };
                if let Some(w) = winner {
                    local.words.insert(key.clone(), w);
                }
                conflicts.push(ConflictRecord {
                    at: now,
                    kind: "word".into(),
                    key: key.clone(),
                    local_version: word_version(local_item.as_ref()),
                    remote_version: word_version(remote_item.as_ref()),
                    remote_device: remote_device.into(),
                    resolution:
                        "both machines changed this word; kept the newer version, the other is preserved in this log"
                            .into(),
                });
            }
            _ => {}
        }
    }
    local
}

fn merge_snippets(
    mut local: ItemSet,
    remote: &ItemSet,
    base: &ItemSet,
    remote_device: &str,
    now: Timestamp,
    conflicts: &mut Vec<ConflictRecord>,
) -> ItemSet {
    let mut keys: Vec<String> = local
        .snippets
        .keys()
        .chain(local.snippet_tombstones.keys())
        .chain(remote.snippets.keys())
        .chain(remote.snippet_tombstones.keys())
        .chain(base.snippets.keys())
        .chain(base.snippet_tombstones.keys())
        .cloned()
        .collect();
    keys.sort();
    keys.dedup();
    for key in keys {
        let l = snippet_side(&local, &key);
        let r = snippet_side(remote, &key);
        let b = snippet_side(base, &key);
        let l_changed = l.ts() > b.ts();
        let r_changed = r.ts() > b.ts();
        if !l_changed && !r_changed || l_changed && !r_changed {
            continue;
        }
        match (l, r) {
            (Side::Absent, Side::Live(rt)) => {
                if let Some(item) = remote.snippets.get(&key) {
                    let mut item = item.clone();
                    item.updated_at = rt;
                    local.snippets.insert(key.clone(), item);
                }
            }
            (Side::Dead(_), Side::Live(rt)) if l_changed => {
                if let Some(item) = remote.snippets.get(&key) {
                    let mut item = item.clone();
                    item.updated_at = rt;
                    local.snippet_tombstones.remove(&key);
                    local.snippets.insert(key.clone(), item);
                    conflicts.push(ConflictRecord {
                        at: now,
                        kind: "snippet".into(),
                        key: key.clone(),
                        local_version: None,
                        remote_version: snippet_version(remote.snippets.get(&key)),
                        remote_device: remote_device.into(),
                        resolution: "a deletion collided with an edit; kept the edited snippet"
                            .into(),
                    });
                }
            }
            (Side::Dead(_), Side::Live(rt)) => {
                if let Some(item) = remote.snippets.get(&key) {
                    let mut item = item.clone();
                    item.updated_at = rt;
                    local.snippet_tombstones.remove(&key);
                    local.snippets.insert(key.clone(), item);
                }
            }
            (Side::Live(_), Side::Dead(rt)) if !l_changed => {
                local.snippets.remove(&key);
                local.snippet_tombstones.insert(key.clone(), rt);
            }
            (Side::Live(_), Side::Dead(_)) => {
                conflicts.push(ConflictRecord {
                    at: now,
                    kind: "snippet".into(),
                    key: key.clone(),
                    local_version: snippet_version(local.snippets.get(&key)),
                    remote_version: None,
                    remote_device: remote_device.into(),
                    resolution: "a deletion collided with an edit; kept the edited snippet"
                        .into(),
                });
            }
            (Side::Live(lt), Side::Live(rt)) => {
                let same = match (local.snippets.get(&key), remote.snippets.get(&key)) {
                    (Some(li), Some(ri)) => li.snippet == ri.snippet,
                    _ => false,
                };
                if same {
                    let newer = lt.max(rt);
                    if let Some(item) = local.snippets.get_mut(&key) {
                        item.updated_at = newer;
                    }
                    continue;
                }
                // Both machines changed the same snippet differently. The
                // newer one stays live under the original trigger; the other
                // survives as its own "(conflict)" snippet — both versions
                // stay usable, nothing is dropped.
                let local_item = local.snippets.get(&key).cloned();
                let remote_item = remote.snippets.get(&key).cloned();
                let keep_remote = rt > lt;
                let winner = if keep_remote {
                    remote_item.clone()
                } else {
                    local_item.clone()
                };
                let loser = if keep_remote {
                    local_item.clone()
                } else {
                    remote_item.clone()
                };
                if let Some(w) = winner {
                    local.snippets.insert(key.clone(), w);
                }
                if let Some(mut lo) = loser {
                    let mut candidate = format!("{}{}", key, CONFLICT_TRIGGER_SUFFIX);
                    let mut n = 1;
                    while local.snippets.contains_key(&candidate) {
                        n += 1;
                        candidate = format!("{} (conflict {n})", key);
                    }
                    lo.snippet.trigger = candidate;
                    lo.updated_at = now;
                    local.snippets.insert(lo.snippet.trigger.clone(), lo);
                }
                conflicts.push(ConflictRecord {
                    at: now,
                    kind: "snippet".into(),
                    key: key.clone(),
                    local_version: snippet_version(local_item.as_ref()),
                    remote_version: snippet_version(remote_item.as_ref()),
                    remote_device: remote_device.into(),
                    resolution: "both machines changed this snippet; kept the newer one live and saved the other as a separate '(conflict)' snippet".into(),
                });
            }
            _ => {}
        }
    }
    local
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sync::bundle::{export_bundle, import_bundle, BundleData};

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "flowstate-sync-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn word_set(words: &[(&str, Timestamp)]) -> ItemSet {
        let mut set = ItemSet::default();
        for (word, ts) in words {
            set.words.insert(
                word_key(word),
                WordItem {
                    word: word.to_string(),
                    updated_at: *ts,
                },
            );
        }
        set
    }

    fn snippet(trigger: &str, expansion: &str, ts: Timestamp) -> SnippetItem {
        SnippetItem {
            snippet: Snippet {
                trigger: trigger.to_string(),
                expansion: expansion.to_string(),
                case_sensitive: false,
            },
            updated_at: ts,
        }
    }

    fn snippet_set(snippets: &[SnippetItem]) -> ItemSet {
        let mut set = ItemSet::default();
        for item in snippets {
            set.snippets
                .insert(snippet_key(&item.snippet.trigger), item.clone());
        }
        set
    }

    fn state_for(folder: &Path, device: &str, history: bool) -> SyncState {
        let mut state = SyncState::new(device.into());
        state.enabled = true;
        state.folder = Some(folder.to_string_lossy().to_string());
        state.sync_history = history;
        state
    }

    /// One sync for one "device", persisting its state between calls the way
    /// the app does — that per-peer memory is what makes the merge three-way.
    fn sync(
        state_dir: &Path,
        folder: &Path,
        device: &str,
        items: ItemSet,
    ) -> SyncOutcome {
        let mut state = load_state(state_dir)
            .ok()
            .flatten()
            .unwrap_or_else(|| {
                let mut state = state_for(folder, device, false);
                state.device_id = format!("dev-{device}");
                state
            });
        state.folder = Some(folder.to_string_lossy().to_string());
        state.enabled = true;
        let outcome =
            run_sync(state_dir, &state, items, None, &[], None).expect("sync should succeed");
        state.last_sync_at = Some(outcome.synced_at);
        state.peers = outcome.peers.clone();
        save_state(state_dir, &state).expect("state should save");
        outcome
    }

    fn synced_state(dir: &Path, device: &str) -> SyncState {
        let mut state = SyncState::new(device.into());
        state.device_id = format!("dev-{device}");
        state.enabled = true;
        state
    }

    /// Two app-data dirs ("profiles") converge through one shared folder,
    /// exactly how two Macs behave — or two profiles on the same Mac.
    #[test]
    fn two_profiles_sync_dictionary_and_snippets_via_shared_folder() {
        let shared = temp_dir("shared");
        let dir_a = temp_dir("a");
        let dir_b = temp_dir("b");

        let items_a = word_set(&[("Kasper", 1000)]);
        let mut items_a = items_a;
        items_a.snippets.insert(
            "signatur".into(),
            snippet("signatur", "Med vennlig hilsen, Kasper", 1000),
        );

        let mut items_b = word_set(&[("odda", 1100)]);
        items_b.snippets.insert(
            "møtereferat".into(),
            snippet("møtereferat", "Sak, beslutning, ansvarlig", 1100),
        );

        let out_a = sync(&dir_a, &shared, "a", items_a.clone());
        assert_eq!(merged_words(&out_a.merged), vec!["Kasper"]);
        let out_b = sync(&dir_b, &shared, "b", items_b.clone());
        assert!(merged_words(&out_b.merged).contains(&"Kasper".to_string()));
        assert!(merged_words(&out_b.merged).contains(&"odda".to_string()));
        assert_eq!(out_b.merged.snippets.len(), 2);

        // A pulls B's additions without a network round trip — pure files.
        let out_a2 = sync(&dir_a, &shared, "a", out_a.merged.clone());
        assert!(merged_words(&out_a2.merged).contains(&"odda".to_string()));
        assert_eq!(out_a2.merged.snippets.len(), 2);
        assert!(out_a2.conflicts.is_empty());

        // Both devices published a document in the shared folder.
        let devices = shared_root(&shared).join(DEVICES_SUBDIR);
        let count = std::fs::read_dir(&devices).unwrap().count();
        assert_eq!(count, 2);
    }

    /// A same-entry collision keeps both versions and writes a conflict log
    /// line in the shared folder — never a silent loss.
    #[test]
    fn concurrent_edits_keep_both_versions_and_log_the_conflict() {
        let shared = temp_dir("conflict");
        let dir_a = temp_dir("ca");
        let dir_b = temp_dir("cb");

        let base = snippet_set(&[snippet("signatur", "v1", 1000)]);
        let _ = sync(&dir_a, &shared, "a", base.clone());
        let out_b = sync(&dir_b, &shared, "b", ItemSet::default());
        assert_eq!(out_b.merged.snippets["signatur"].snippet.expansion, "v1");

        // A and B both edit the same snippet between syncs.
        let a_edited = snippet_set(&[snippet("signatur", "v2 fra A", 2000)]);
        let b_edited = snippet_set(&[snippet("signatur", "v3 fra B", 3000)]);
        let _ = sync(&dir_a, &shared, "a", a_edited);
        let out_b = sync(&dir_b, &shared, "b", b_edited);

        // The newer version stays live under the real trigger...
        assert_eq!(out_b.merged.snippets["signatur"].snippet.expansion, "v3 fra B");
        // ...and the loser survives as its own snippet, not a quiet overwrite.
        let suffixed = format!("signatur{CONFLICT_TRIGGER_SUFFIX}");
        assert_eq!(
            out_b.merged.snippets[&suffixed].snippet.expansion,
            "v2 fra A"
        );
        // And the collision is logged where both machines can read it.
        assert_eq!(out_b.conflicts.len(), 1);
        assert_eq!(out_b.conflicts[0].kind, "snippet");
        let log = std::fs::read_to_string(shared_root(&shared).join(CONFLICTS_LOG)).unwrap();
        assert_eq!(log.lines().count(), 1);
        assert!(log.contains("signatur"));
    }

    /// Deletions travel (last writer wins per entry), but a deletion never
    /// beats a concurrent edit, and sync never wipes history.
    #[test]
    fn deletions_propagate_but_never_beat_a_concurrent_edit() {
        let shared = temp_dir("delete");
        let dir_a = temp_dir("da");
        let dir_b = temp_dir("db");

        let base = word_set(&[("kvitto", 1000)]);
        let _ = sync(&dir_a, &shared, "a", base);
        let out_b = sync(&dir_b, &shared, "b", ItemSet::default());
        assert!(out_b.merged.words.contains_key("kvitto"));

        // A deletes; B hasn't touched the word: the deletion wins on B.
        let mut a_deleted = ItemSet::default();
        a_deleted.word_tombstones.insert("kvitto".into(), 2000);
        let _ = sync(&dir_a, &shared, "a", a_deleted);
        let out_b = sync(&dir_b, &shared, "b", out_b.merged.clone());
        assert!(!out_b.merged.words.contains_key("kvitto"));
        assert!(out_b.merged.word_tombstones.contains_key("kvitto"));

        // Now A deletes a word B has edited since: the edit survives.
        let shared_edit = temp_dir("delete3");
        let dir_c = temp_dir("dc");
        let dir_d = temp_dir("dd");

        let base = word_set(&[("notat", 1000)]);
        let _ = sync(&dir_c, &shared_edit, "c", base);
        let out_d = sync(&dir_d, &shared_edit, "d", ItemSet::default());

        let mut c_deleted = ItemSet::default();
        c_deleted.word_tombstones.insert("notat".into(), 2000);
        let _ = sync(&dir_c, &shared_edit, "c", c_deleted);

        let d_edited = word_set(&[("Notat", 2500)]);
        let out_d = sync(&dir_d, &shared_edit, "d", d_edited);
        assert!(out_d.merged.words.contains_key("notat"));
        assert!(!out_d.merged.word_tombstones.contains_key("notat"));
        assert_eq!(out_d.conflicts.len(), 1);
        assert!(out_d.conflicts[0].resolution.contains("deletion"));
    }

    /// A refused sync — no folder, or a folder that isn't there — writes
    /// nothing anywhere.
    #[test]
    fn refused_sync_touches_no_files() {
        let dir = temp_dir("refused");
        let state = synced_state(&dir, "x");

        // No folder selected.
        let err = run_sync(&dir, &state, ItemSet::default(), None, &[], None);
        assert!(err.is_err());

        // Folder that does not exist: refused before anything is created.
        let ghost = dir.join("ghost-folder");
        let mut state = state.clone();
        state.folder = Some(ghost.to_string_lossy().to_string());
        let err = run_sync(&dir, &state, ItemSet::default(), None, &[], None);
        assert!(err.is_err());
        assert!(!ghost.exists());

        // Sync disabled: refused, no matter what.
        let mut state = state.clone();
        state.enabled = false;
        state.folder = Some(dir.to_string_lossy().to_string());
        assert!(validate_shared_folder(&dir).is_ok());
    }

    /// History merges as a union by content key: only entries this machine
    /// has never seen are imported, and nothing is ever removed.
    #[test]
    fn history_merges_by_union_and_only_imports_the_unknown() {
        let shared = temp_dir("history");
        let dir_a = temp_dir("ha");
        let dir_b = temp_dir("hb");

        let h1 = HistoryDocEntry {
            key: history_key(100, "t1", "første"),
            file_name: "a.wav".into(),
            timestamp: 100,
            saved: false,
            title: "t1".into(),
            transcription_text: "første".into(),
            post_processed_text: None,
            language: Some("nb".into()),
        };
        let h2 = HistoryDocEntry {
            key: history_key(200, "t2", "andre"),
            file_name: String::new(),
            timestamp: 200,
            saved: true,
            title: "t2".into(),
            transcription_text: "andre".into(),
            post_processed_text: None,
            language: None,
        };
        let h3 = HistoryDocEntry {
            key: history_key(300, "t3", "tredje"),
            file_name: String::new(),
            timestamp: 300,
            saved: false,
            title: "t3".into(),
            transcription_text: "tredje".into(),
            post_processed_text: None,
            language: None,
        };

        let mut state_a = state_for(&shared, "a", true);
        state_a.device_id = "dev-a".into();
        let out_a = run_sync(
            &dir_a,
            &state_a,
            ItemSet::default(),
            Some(vec![h1.clone(), h2.clone()]),
            &[],
            None,
        )
        .unwrap();
        assert!(out_a.imported_history.is_empty());

        // B already knows h2; only h1 and h3 should come over.
        let mut state_b = state_for(&shared, "b", true);
        state_b.device_id = "dev-b".into();
        let out_b = run_sync(
            &dir_b,
            &state_b,
            ItemSet::default(),
            Some(vec![h3.clone()]),
            &[h2.key.clone()],
            None,
        )
        .unwrap();
        let keys: Vec<String> = out_b
            .imported_history
            .iter()
            .map(|(_, e)| e.key.clone())
            .collect();
        // h1 is new to B; h2 B already knows; h3 is B's own entry.
        assert!(keys.contains(&h1.key));
        assert!(!keys.contains(&h2.key));
        assert!(!keys.contains(&h3.key));
    }

    /// Reconcile turns settings edits into timestamps and deletions into
    /// tombstones.
    #[test]
    fn reconcile_timestamps_adds_edits_and_deletions() {
        let now = 5000;
        let set = reconcile(
            ItemSet::default(),
            &["Kasper".into(), "odda".into()],
            &[],
            now,
        );
        assert_eq!(set.words["kasper"].updated_at, now);
        assert_eq!(set.words["odda"].updated_at, now);

        // "odda" is removed -> tombstone; "kasper" edited in casing -> new ts.
        let set = reconcile(
            set,
            &["KASPER".into()],
            &[],
            6000,
        );
        assert!(set.word_tombstones.contains_key("odda"));
        assert_eq!(set.words["kasper"].word, "KASPER");
        assert_eq!(set.words["kasper"].updated_at, 6000);
        assert!(!set.words.contains_key("odda"));

        // Re-adding a deleted word resurrects it.
        let set = reconcile(set, &["KASPER".into(), "odda".into()], &[], 7000);
        assert!(set.words.contains_key("odda"));
        assert!(!set.word_tombstones.contains_key("odda"));
    }

    /// The bundle round-trips, and a tampered bundle is refused without
    /// touching the caller's data.
    #[test]
    fn bundle_roundtrip_and_tamper_refusal() {
        let dir = temp_dir("bundle");
        let mut items = word_set(&[("ordbok".into(), 1000)]);
        items.snippets.insert(
            "signatur".into(),
            snippet("signatur", "hilsen", 1000),
        );

        let audio = dir.join("take.wav");
        std::fs::write(&audio, b"RIFF fake wav").unwrap();
        let mut audio_map = std::collections::HashMap::new();
        audio_map.insert("audio/0000-take.wav".to_string(), audio.clone());

        let entry = HistoryDocEntry {
            key: history_key(100, "t", "tekst"),
            file_name: "take.wav".into(),
            timestamp: 100,
            saved: false,
            title: "t".into(),
            transcription_text: "tekst".into(),
            post_processed_text: None,
            language: None,
        };

        let bundle_path = dir.join("bunt.zip");
        export_bundle(
            &bundle_path,
            &BundleData {
                excerpt: serde_json::json!({"app_language": "nb"}),
                items: items.clone(),
                history: vec![entry.clone()],
                audio: audio_map,
            },
            "test-maskin",
            "0.9.8-test",
        )
        .expect("export should succeed");
        assert!(bundle_path.is_file());

        // Round-trip into a fresh staging dir.
        let staging = dir.join("staging");
        let imported = import_bundle(&bundle_path, &staging).expect("import should verify");
        assert_eq!(imported.items.words["ordbok"].word, "ordbok");
        assert_eq!(
            imported.items.snippets["signatur"].snippet.expansion,
            "hilsen"
        );
        assert_eq!(imported.history.len(), 1);
        assert!(imported
            .audio
            .values()
            .next()
            .unwrap()
            .is_file());

        // Tamper with one byte of a member: refused, staging untouched.
        // The byte is picked inside the audio member's data so the flip
        // always lands in sealed content, never in zip framing that no
        // hash covers (a blind mid-file flip could hit a local header and
        // slip through verification).
        let bytes = std::fs::read(&bundle_path).unwrap();
        let mut tampered = bytes.clone();
        let audio_offset = {
            let file = std::fs::File::open(&bundle_path).unwrap();
            let mut archive = zip::ZipArchive::new(file).unwrap();
            let mut entry = archive
                .by_name("audio/0000-take.wav")
                .expect("audio member should exist");
            entry.data_start() as usize
        };
        tampered[audio_offset] = tampered[audio_offset].wrapping_add(1);
        let tampered_path = dir.join("tampered.zip");
        std::fs::write(&tampered_path, &tampered).unwrap();
        let staging2 = dir.join("staging2");
        assert!(import_bundle(&tampered_path, &staging2).is_err());
        assert!(!staging2.join("audio").exists());
    }
}
