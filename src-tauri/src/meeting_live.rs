//! The live transcript window: while a meeting records, a panel on the right
//! edge of the screen shows what is being said, following the same
//! `meeting-segment` events the meeting window gets. It opens with the
//! session and closes with it; nothing it shows is written here — the entry
//! owns the transcript.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const MEETING_LIVE_LABEL: &str = "meeting-live";

const WIDTH: f64 = 380.0;
const HEIGHT: f64 = 540.0;
/// Gap from the screen edge, and the minimum from the top.
const MARGIN: f64 = 24.0;

/// Show the panel, creating it the first time. Positioned on the right of the
/// primary monitor, vertically centred, above other windows.
pub fn open_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MEETING_LIVE_LABEL) {
        let _ = window.show();
        return;
    }

    let mut builder = WebviewWindowBuilder::new(
        app,
        MEETING_LIVE_LABEL,
        WebviewUrl::App("src/meeting-live/index.html".into()),
    )
    .title("FlowState")
    .inner_size(WIDTH, HEIGHT)
    .min_inner_size(280.0, 320.0)
    .resizable(true)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .shadow(true)
    .visible(true);

    if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        let width = monitor.size().width as f64 / scale;
        let height = monitor.size().height as f64 / scale;
        let x = (width - WIDTH - MARGIN).max(MARGIN);
        let y = ((height - HEIGHT) / 2.0).max(MARGIN);
        builder = builder.position(x, y);
    }

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    if let Err(error) = builder.build() {
        log::warn!("Failed to open the meeting transcript window: {error}");
    }
}

/// Close the panel: the meeting ended, cancelled, or never started.
pub fn close_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MEETING_LIVE_LABEL) {
        let _ = window.close();
    }
}
