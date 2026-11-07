//! The "FlowState bundle": one zip the user can hand-carry between machines
//! (AirDrop, USB, a chat with themselves) when there is no shared folder yet.
//!
//! It is *sealed*: `manifest.sig` is the SHA-256 of `manifest.json`, and the
//! manifest carries the SHA-256 of every member. Import verifies the seal
//! and every member hash before touching anything, so a truncated or
//! tampered bundle is refused outright and the local dictionary, snippets
//! and history stay exactly as they were.

use super::{HistoryDocEntry, ItemSet, Timestamp};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

pub const MANIFEST_MEMBER: &str = "manifest.json";
pub const SIGNATURE_MEMBER: &str = "manifest.sig";
pub const EXCERPT_MEMBER: &str = "settings-excerpt.json";
pub const DICTIONARY_MEMBER: &str = "dictionary.json";
pub const SNIPPETS_MEMBER: &str = "snippets.json";
pub const HISTORY_MEMBER: &str = "history.json";
pub const AUDIO_PREFIX: &str = "audio/";
pub const BUNDLE_SCHEMA: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BundleManifest {
    pub schema: u32,
    pub app: String,
    pub format: String,
    pub created_at: Timestamp,
    pub device_name: String,
    pub app_version: String,
    pub members: HashMap<String, String>,
}

/// Everything an export packs into the bundle.
pub struct BundleData {
    /// Settings that are worth carrying over (language, theme, …) as JSON.
    pub excerpt: serde_json::Value,
    pub items: ItemSet,
    pub history: Vec<HistoryDocEntry>,
    /// zip member name -> file on disk to copy in (audio).
    pub audio: HashMap<String, std::path::PathBuf>,
}

/// A verified bundle, ready to merge in. Audio members are extracted to the
/// caller's staging directory and referenced by name.
pub struct BundleImport {
    pub manifest: BundleManifest,
    pub excerpt: serde_json::Value,
    pub items: ItemSet,
    pub history: Vec<HistoryDocEntry>,
    /// zip member name -> extracted file path.
    pub audio: HashMap<String, std::path::PathBuf>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

pub fn export_bundle(
    path: &Path,
    data: &BundleData,
    device_name: &str,
    app_version: &str,
) -> Result<(), String> {
    let mut members: HashMap<String, Vec<u8>> = HashMap::new();

    let excerpt_bytes = serde_json::to_vec_pretty(&data.excerpt).map_err(|e| e.to_string())?;
    members.insert(EXCERPT_MEMBER.to_string(), excerpt_bytes);

    let dictionary: Vec<&super::WordItem> = data.items.words.values().collect();
    let dictionary_bytes = serde_json::to_vec_pretty(&dictionary).map_err(|e| e.to_string())?;
    members.insert(DICTIONARY_MEMBER.to_string(), dictionary_bytes);

    let snippets: Vec<&super::SnippetItem> = data.items.snippets.values().collect();
    let snippets_bytes = serde_json::to_vec_pretty(&snippets).map_err(|e| e.to_string())?;
    members.insert(SNIPPETS_MEMBER.to_string(), snippets_bytes);

    if !data.history.is_empty() {
        let history_bytes = serde_json::to_vec_pretty(&data.history).map_err(|e| e.to_string())?;
        members.insert(HISTORY_MEMBER.to_string(), history_bytes);
    }

    let mut member_hashes = HashMap::new();
    for (name, bytes) in &members {
        member_hashes.insert(name.clone(), sha256_hex(bytes));
    }
    for (name, file) in &data.audio {
        let bytes = std::fs::read(file).map_err(|e| format!("reading {}: {e}", file.display()))?;
        member_hashes.insert(name.clone(), sha256_hex(&bytes));
    }

    let manifest = BundleManifest {
        schema: BUNDLE_SCHEMA,
        app: "FlowState".into(),
        format: "flowstate-bundle".into(),
        created_at: super::now_ms(),
        device_name: device_name.into(),
        app_version: app_version.into(),
        members: member_hashes,
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?;
    let seal = sha256_hex(&manifest_bytes);

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default();

    zip.start_file(MANIFEST_MEMBER, options)
        .map_err(|e| e.to_string())?;
    zip.write_all(&manifest_bytes).map_err(|e| e.to_string())?;
    zip.start_file(SIGNATURE_MEMBER, options)
        .map_err(|e| e.to_string())?;
    zip.write_all(seal.as_bytes()).map_err(|e| e.to_string())?;
    for (name, bytes) in &members {
        zip.start_file(name.as_str(), options)
            .map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())?;
    }
    for (name, file) in &data.audio {
        let bytes = std::fs::read(file).map_err(|e| format!("reading {}: {e}", file.display()))?;
        zip.start_file(name.as_str(), options)
            .map_err(|e| e.to_string())?;
        zip.write_all(&bytes).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

/// Verify + unpack a bundle. Every hash is checked before a single byte is
/// written to `staging_dir`; on any mismatch the bundle is refused and the
/// caller's data is untouched.
pub fn import_bundle(path: &Path, staging_dir: &Path) -> Result<BundleImport, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("opening bundle: {e}"))?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("reading bundle: {e}"))?;

    let mut raw: HashMap<String, Vec<u8>> = HashMap::new();
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|e| format!("reading bundle: {e}"))?;
        let name = entry.name().to_string();
        if name.ends_with('/') {
            continue; // directory entry
        }
        let mut bytes = Vec::new();
        entry
            .read_to_end(&mut bytes)
            .map_err(|e| format!("reading {name}: {e}"))?;
        raw.insert(name, bytes);
    }

    let manifest_bytes = raw
        .get(MANIFEST_MEMBER)
        .ok_or_else(|| "bundle is missing manifest.json".to_string())?;
    let manifest: BundleManifest = serde_json::from_slice(manifest_bytes)
        .map_err(|e| format!("bundle manifest is unreadable: {e}"))?;
    if manifest.app != "FlowState" || manifest.format != "flowstate-bundle" {
        return Err("this is not a FlowState bundle".into());
    }
    if manifest.schema > BUNDLE_SCHEMA {
        return Err("bundle was made by a newer FlowState; update FlowState first".into());
    }

    let seal = String::from_utf8_lossy(
        raw.get(SIGNATURE_MEMBER)
            .ok_or_else(|| "bundle is missing manifest.sig".to_string())?,
    )
    .trim()
    .to_string();
    if !seal.eq_ignore_ascii_case(&sha256_hex(manifest_bytes)) {
        return Err(
            "bundle seal does not match the manifest; it is damaged or tampered with".into(),
        );
    }

    for (name, expected) in &manifest.members {
        let bytes = raw
            .get(name)
            .ok_or_else(|| format!("bundle is missing {name}"))?;
        if sha256_hex(bytes) != *expected {
            return Err(format!(
                "{name} does not match its checksum; refusing to import"
            ));
        }
    }

    let excerpt: serde_json::Value = match raw.get(EXCERPT_MEMBER) {
        Some(bytes) => serde_json::from_slice(bytes).map_err(|e| e.to_string())?,
        None => serde_json::Value::Null,
    };

    let mut items = ItemSet::default();
    if let Some(bytes) = raw.get(DICTIONARY_MEMBER) {
        let words: Vec<super::WordItem> =
            serde_json::from_slice(bytes).map_err(|e| format!("dictionary.json: {e}"))?;
        for word in words {
            items.words.insert(super::word_key(&word.word), word);
        }
    }
    if let Some(bytes) = raw.get(SNIPPETS_MEMBER) {
        let snippets: Vec<super::SnippetItem> =
            serde_json::from_slice(bytes).map_err(|e| format!("snippets.json: {e}"))?;
        for snippet in snippets {
            items
                .snippets
                .insert(super::snippet_key(&snippet.snippet.trigger), snippet);
        }
    }

    let mut history = Vec::new();
    if let Some(bytes) = raw.get(HISTORY_MEMBER) {
        history = serde_json::from_slice(bytes).map_err(|e| format!("history.json: {e}"))?;
    }

    // Extract audio members last: by now the bundle has proven itself.
    let mut audio = HashMap::new();
    let audio_members: Vec<String> = manifest
        .members
        .keys()
        .filter(|name| name.starts_with(AUDIO_PREFIX))
        .cloned()
        .collect();
    if !audio_members.is_empty() {
        std::fs::create_dir_all(staging_dir).map_err(|e| e.to_string())?;
    }
    for name in audio_members {
        let bytes = raw.get(&name).ok_or_else(|| format!("missing {name}"))?;
        // Zip-slip guard: only write inside the staging dir.
        let relative = name.strip_prefix(AUDIO_PREFIX).unwrap_or(&name).to_string();
        let safe = Path::new(&relative);
        if safe.components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir | std::path::Component::RootDir
            )
        }) {
            return Err(format!("bundle has an unsafe path: {name}"));
        }
        let dest = staging_dir.join(safe);
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(&dest, bytes).map_err(|e| e.to_string())?;
        audio.insert(name, dest);
    }

    Ok(BundleImport {
        manifest,
        excerpt,
        items,
        history,
        audio,
    })
}
