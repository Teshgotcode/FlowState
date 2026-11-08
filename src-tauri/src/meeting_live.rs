//! The live transcript window: while a meeting records, a panel on the right
//! edge of the screen shows what is being said, following the same
//! `meeting-segment` events the meeting window gets. It opens with the
//! session and closes with it; nothing it shows is written here — the entry
//! owns the transcript.

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

pub const MEETING_LIVE_LABEL: &str = "meeting-live";

/// The reference's notepad is a tall right-edge panel (500 wide on a 1512
/// point screen), so the live transcript takes the same slot beside the bar.
const WIDTH: f64 = 500.0;
/// The bar's own inset from the screen edge.
const MARGIN: f64 = 16.0;
/// Menu bar and Dock: the panel spans the work area between them.
const TOP_INSET: f64 = 38.0;
const BOTTOM_INSET: f64 = 72.0;

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
    .inner_size(WIDTH, height_for(app))
    .min_inner_size(320.0, 360.0)
    .resizable(true)
    .decorations(false)
    .transparent(true)
    // A normal window, not a floating panel: it goes behind other apps when
    // you switch, and closes like anything else.
    .always_on_top(false)
    .skip_taskbar(true)
    .focused(false)
    .shadow(true)
    .visible(true);

    if let Ok(Some(monitor)) = app.primary_monitor() {
        let scale = monitor.scale_factor();
        let width = monitor.size().width as f64 / scale;
        let x = (width - WIDTH - MARGIN).max(MARGIN);
        builder = builder.position(x, TOP_INSET);
    }

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    if let Err(error) = builder.build() {
        log::warn!("Failed to open the meeting transcript window: {error}");
    }
}

/// The panel's height: the screen minus the menu bar and the Dock.
fn height_for(app: &AppHandle) -> f64 {
    app.primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| {
            (monitor.size().height as f64 / monitor.scale_factor()) - TOP_INSET - BOTTOM_INSET
        })
        .filter(|height| *height > 320.0)
        .unwrap_or(640.0)
}

/// Close the panel: the meeting ended, cancelled, or never started.
pub fn close_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(MEETING_LIVE_LABEL) {
        let _ = window.close();
    }
}
