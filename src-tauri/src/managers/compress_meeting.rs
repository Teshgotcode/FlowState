//! Shrinking a finished meeting's recording.
//!
//! A meeting records to WAV, 16 kHz mono: about 115 MB an hour, which adds up
//! fast for someone who records lectures. Once the transcript exists the audio
//! only has to be playable and re-readable, not raw, so it is transcoded to AAC
//! and the WAV is deleted. Forty-eight kbps mono keeps speech clear at about
//! a fifth of the size, measured on a real recording: 532 KB became 105 KB.
//!
//! The transcode is only kept when the result decodes back to the same length:
//! a failed or truncated encode leaves the WAV in place, because losing a
//! recording to save disk is not a trade this code makes.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Result};
use tauri::AppHandle;

use crate::managers::history::HistoryManager;

/// Speech at 16 kHz is clear at this rate, and it is about a fifth of the WAV
/// it replaces.
const AAC_BITRATE: &str = "48000";

/// How far the decoded length may differ before the encode is treated as
/// broken. Encoders pad by a frame or two; they must not drop seconds.
const LENGTH_TOLERANCE: f64 = 0.02;

/// Transcode a meeting's recording and point the entry at the smaller file.
/// Returns the new file name. On any doubt the WAV stays and the call fails.
pub fn compress_meeting_recording(
    _app: &AppHandle,
    history: &HistoryManager,
    entry_id: i64,
    file_name: &str,
    wav_samples: usize,
) -> Result<String> {
    let source: PathBuf = history.get_audio_file_path(file_name);
    if !source.exists() {
        return Err(anyhow!("the recording is already gone"));
    }
    let target = source.with_extension("m4a");
    // A stale half-written file from an earlier attempt must not survive.
    if target.exists() {
        let _ = std::fs::remove_file(&target);
    }

    let status = Command::new("/usr/bin/afconvert")
        .args([
            "-f",
            "m4af",
            "-d",
            "aac",
            "-b",
            AAC_BITRATE,
            "-c",
            "1",
        ])
        .arg(&source)
        .arg(&target)
        .status()
        .map_err(|e| anyhow!("afconvert did not run: {e}"))?;
    if !status.success() {
        let _ = std::fs::remove_file(&target);
        return Err(anyhow!("afconvert exited with {status}"));
    }

    // Decode what was written: same length or the encode is not trusted.
    let decoded = crate::audio_toolkit::audio::decode_audio_file(&target)?;
    if !length_matches(wav_samples, decoded.len()) {
        let _ = std::fs::remove_file(&target);
        return Err(anyhow!(
            "the encode decoded to {} samples, not {}",
            decoded.len(),
            wav_samples
        ));
    }

    std::fs::remove_file(&source).map_err(|e| anyhow!("could not remove the WAV: {e}"))?;
    let new_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| anyhow!("the new recording has no name"))?
        .to_string();
    history.rename_audio_file(entry_id, &new_name)?;
    Ok(new_name)
}

/// Whether an encode kept the recording's length. Empty output never matches.
pub fn length_matches(original: usize, decoded: usize) -> bool {
    if original == 0 || decoded == 0 {
        return false;
    }
    let original = original as f64;
    ((decoded as f64) - original).abs() / original <= LENGTH_TOLERANCE
}

/// A short label for the log line: megabytes saved.
pub fn size_note(path: &Path) -> String {
    std::fs::metadata(path)
        .map(|meta| format!("{} KB", meta.len() / 1024))
        .unwrap_or_else(|_| "missing".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_encode_must_keep_the_length() {
        assert!(length_matches(16_000 * 60, 16_000 * 60));
        // Encoders pad a frame or two; that is fine.
        assert!(length_matches(16_000 * 60, 16_000 * 60 + 2_000));
        // A truncated encode is not.
        assert!(!length_matches(16_000 * 60, 16_000 * 30));
        assert!(!length_matches(16_000 * 60, 0));
        assert!(!length_matches(0, 0));
    }
}
