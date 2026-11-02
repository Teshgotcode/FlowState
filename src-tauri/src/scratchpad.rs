//! The Scratchpad: a small note window opened from the meeting bar.
//!
//! Wispr-style quick capture: one menu click opens a tiny note with the caret
//! ready. A dictation taken while the window holds the focus lands in the
//! note instead of the focused app, and the note's text is kept locally in
//! `scratchpad.txt` under the app data directory — it is never uploaded
//! anywhere, and reopening the window shows the note as it was left.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The Scratchpad window's label — also the event target a focused take is
/// delivered to.
pub const SCRATCHPAD_WINDOW_LABEL: &str = "scratchpad";

/// Where the note's text lives, inside the app data directory.
const NOTE_FILE: &str = "scratchpad.txt";

const SCRATCHPAD_WIDTH: f64 = 360.0;
const SCRATCHPAD_HEIGHT: f64 = 300.0;

/// True while the Scratchpad window exists and holds the keyboard focus: a
/// dictation taken now belongs in the note, not in another app.
pub fn is_focused(app: &AppHandle) -> bool {
    app.get_webview_window(SCRATCHPAD_WINDOW_LABEL)
        .map(|window| window.is_focused().unwrap_or(false))
        .unwrap_or(false)
}

/// Open the Scratchpad: show and focus the window, or create it on first
/// open. Created programmatically so portable mode can redirect the
/// webview's data directory, exactly like the main window.
pub fn open_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SCRATCHPAD_WINDOW_LABEL) {
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }

    let mut builder = WebviewWindowBuilder::new(
        app,
        SCRATCHPAD_WINDOW_LABEL,
        WebviewUrl::App("src/scratchpad/index.html".into()),
    )
    .title("FlowState")
    .inner_size(SCRATCHPAD_WIDTH, SCRATCHPAD_HEIGHT)
    .min_inner_size(220.0, 140.0)
    .resizable(true)
    .visible(true);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    if let Err(error) = builder.build() {
        log::warn!("Failed to open the Scratchpad window: {error}");
    }
}

fn note_path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let app_data_dir = crate::portable::app_data_dir(app)
        .map_err(|e| format!("Failed to resolve the app data directory: {e}"))?;
    Ok(app_data_dir.join(NOTE_FILE))
}

/// The note's current text; an empty string when nothing was saved yet.
pub fn read_note(app: &AppHandle) -> Result<String, String> {
    match std::fs::read_to_string(note_path(app)?) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(format!("Failed to read the Scratchpad note: {error}")),
    }
}

/// Persist the note's text locally. Nothing else happens to it: no upload,
/// no sync, no share.
pub fn write_note(app: &AppHandle, content: &str) -> Result<(), String> {
    let path = note_path(app)?;
    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            return Err(format!("Failed to create the app data directory: {error}"));
        }
    }
    std::fs::write(&path, content).map_err(|e| format!("Failed to save the Scratchpad note: {e}"))
}
