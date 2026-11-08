use crate::input;
use crate::managers::meeting::MeetingManager;
use crate::settings;
use crate::settings::{AppSettings, OverlayPosition, OverlayStyle};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::menu::{ContextMenu, Menu, MenuItem};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

#[cfg(not(target_os = "macos"))]
use log::debug;

#[cfg(not(target_os = "macos"))]
use tauri::WebviewWindowBuilder;

#[cfg(target_os = "macos")]
use tauri::WebviewUrl;

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel, StyleMask};

#[cfg(target_os = "linux")]
use crate::utils;

#[cfg(target_os = "linux")]
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

// Native overlay window sizes (logical points). One window is reused for every
// state and resized in `show_overlay_state`; each size need only be at least as
// large as the card it hosts (the `--ov-*` vars in RecordingOverlay.css). The
// card is CSS-anchored flush to the screen edge, so window height doesn't move
// where the card sits — only OVERLAY_TOP_OFFSET / OVERLAY_BOTTOM_OFFSET do. Keep
// these in sync with the CSS card geometry.
//
// On Windows these sizes are additionally multiplied by the accessibility text
// scale (see windows_text_scale_factor), which WebView2 applies as a zoom.
//
// Compact overlay (Minimal / transcribing / processing): the 40h pill animates
// width from 172 (--ov-rest-w) to 216 (--ov-work-w) and expands from center, so
// the window must fit the widest state plus a little slack.
const OVERLAY_WIDTH: f64 = 256.0;
const OVERLAY_HEIGHT: f64 = 50.0;

// Actual is 394x118, just a little extra
const OVERLAY_STREAM_WIDTH: f64 = 400.0;
const OVERLAY_STREAM_HEIGHT: f64 = 120.0;

// Meeting bar (the Flow-style floating bar): idle is a 9x30 translucent
// sliver at the screen edge, and the recording pill is the capsule with the
// running clock and Stop. Both windows are EXACT fits for their visible
// content: a transparent margin would swallow clicks meant for the app
// underneath the bar (Sjekkis' S-4c remark), so the slack S-4c carried is gone
// and the CSS hover no longer scales the disc past the window. Keep in sync
// with .mdisc / .mcard in RecordingOverlay.css (both border-box). The
// recording pill shares its footprint with the frontend's short "Saving" pill.
const MEETING_DISC_WIDTH: f64 = 8.0;
const MEETING_DISC_HEIGHT: f64 = 40.0;
const MEETING_PILL_WIDTH: f64 = 176.0;
const MEETING_PILL_HEIGHT: f64 = 40.0;
/// Width the recording pill's window grows by while the "still going?"
/// prompt shows beside it (S-24): the prompt card plus the gap to the pill.
/// The window stays an EXACT fit — no transparent margin to swallow clicks
/// meant for the app underneath. Keep in sync with `.mnu` in
/// RecordingOverlay.css.
const MEETING_NUDGE_WIDTH: f64 = 226.0;
/// The idle disc while hovered: three 40px actions with 6px gaps. Exact fit,
/// same rule as the disc: no transparent slack to swallow clicks.
const MEETING_DISC_WIDE: f64 = 132.0;

/// The meeting bar's own `show-overlay` states. The dictation states
/// ("recording" | "streaming" | "transcribing" | "processing") are unchanged;
/// these are the only additions to the one shared overlay window.
pub const MEETING_BAR_IDLE: &str = "meeting-idle";
pub const MEETING_BAR_RECORDING: &str = "meeting-recording";
/// The recording pill with the "still going?" prompt beside it: the same
/// view as `MEETING_BAR_RECORDING`, one prompt-card wider. Frontend-driven —
/// the webview knows the session's clock and asks for it via
/// `set_meeting_bar_nudge` — so the backend only owns the footprint.
pub const MEETING_BAR_NUDGE: &str = "meeting-recording-nudge";

/// Overlay window size (logical) for a given UI state.
fn overlay_dimensions(state: &str) -> (f64, f64) {
    match state {
        "streaming" => (OVERLAY_STREAM_WIDTH, OVERLAY_STREAM_HEIGHT),
        MEETING_BAR_IDLE => (MEETING_DISC_WIDTH, MEETING_DISC_HEIGHT),
        // The recording pill and the frontend's "Saving" pill share the
        // meeting bar's own exact footprint.
        MEETING_BAR_RECORDING => (MEETING_PILL_WIDTH, MEETING_PILL_HEIGHT),
        // The "still going?" prompt widens the recording pill's window by its
        // own card; every other axis is the pill's.
        MEETING_BAR_NUDGE => (
            MEETING_PILL_WIDTH + MEETING_NUDGE_WIDTH,
            MEETING_PILL_HEIGHT,
        ),
        _ => (OVERLAY_WIDTH, OVERLAY_HEIGHT),
    }
}

/// Whether `state` is one of the dictation overlay's own states. The meeting
/// bar's states are everything else.
fn is_dictation_state(state: &str) -> bool {
    matches!(
        state,
        "recording" | "streaming" | "transcribing" | "processing" | "command"
    )
}

static LAST_MIC_LEVEL_EMIT: AtomicU64 = AtomicU64::new(0);
const EMIT_THROTTLE_MS: u64 = 33; // ~30 FPS

// macOS measures every offset from `NSScreen.visibleFrame` (Tauri's
// work_area), so the bar clears the Dock and the menu bar without hardcoding
// their heights. The other platforms keep full monitor bounds with fixed
// offsets (work_area is unreliable on Wayland; Windows' bottom offset clears
// the taskbar).
#[cfg(target_os = "macos")]
const OVERLAY_TOP_OFFSET: f64 = 12.0;
#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_TOP_OFFSET: f64 = 4.0;

#[cfg(target_os = "macos")]
const OVERLAY_BOTTOM_OFFSET: f64 = 15.0;

#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_BOTTOM_OFFSET: f64 = 40.0;

// Left / Right placement: the gap from the chosen side's edge.
const OVERLAY_SIDE_OFFSET: f64 = 12.0;

/// Configures the edge and offset of a GTK layer surface. gtk-layer-shell
/// commits anchor and margin changes itself, including while the surface is
/// mapped, so changing position does not require a manual hide/show cycle.
#[cfg(target_os = "linux")]
fn configure_layer_shell_position(gtk_window: &gtk::ApplicationWindow, position: OverlayPosition) {
    let (edge, opposite_edge, margin) = match position {
        OverlayPosition::Top => (Edge::Top, Edge::Bottom, OVERLAY_TOP_OFFSET),
        OverlayPosition::Bottom => (Edge::Bottom, Edge::Top, OVERLAY_BOTTOM_OFFSET),
        // Anchoring one side edge centers the bar on the other axis for free.
        OverlayPosition::Left => (Edge::Left, Edge::Right, OVERLAY_SIDE_OFFSET),
        OverlayPosition::Right => (Edge::Right, Edge::Left, OVERLAY_SIDE_OFFSET),
    };

    gtk_window.set_anchor(edge, true);
    gtk_window.set_anchor(opposite_edge, false);
    gtk_window.set_layer_shell_margin(edge, margin.round() as i32);
    gtk_window.set_layer_shell_margin(opposite_edge, 0);
}

/// Configures a GTK layer surface before it is shown.
///
/// Tauri's normal `set_size` path calls `gtk_window_resize`, but layer surfaces
/// derive their dimensions from GTK's size request. gtk-layer-shell documents
/// the `set_size_request` + `resize(1, 1)` sequence for forcing a new size.
#[cfg(target_os = "linux")]
fn configure_layer_shell_surface(
    gtk_window: &gtk::ApplicationWindow,
    position: OverlayPosition,
    width: f64,
    height: f64,
) {
    use gtk::prelude::{GtkWindowExt, WidgetExt};

    configure_layer_shell_position(gtk_window, position);

    gtk_window.set_size_request(
        width.round().max(1.0) as i32,
        height.round().max(1.0) as i32,
    );
    gtk_window.resize(1, 1);
}

/// Initializes GTK layer shell for Linux overlay window
/// Returns true if layer shell was successfully initialized, false otherwise
#[cfg(target_os = "linux")]
fn init_gtk_layer_shell(overlay_window: &tauri::webview::WebviewWindow) -> bool {
    if utils::flowstate_env_flag_enabled("FLOWSTATE_NO_GTK_LAYER_SHELL") {
        debug!("Skipping GTK layer shell init (FLOWSTATE_NO_GTK_LAYER_SHELL is enabled)");
        return false;
    }

    if !gtk_layer_shell::is_supported() {
        return false;
    }

    // Try to get the GTK window from the Tauri webview
    if let Ok(gtk_window) = overlay_window.gtk_window() {
        gtk_window.init_layer_shell();
        gtk_window.set_layer(Layer::Overlay);
        gtk_window.set_keyboard_mode(KeyboardMode::None);
        gtk_window.set_exclusive_zone(0);

        let overlay_position = settings::get_settings(overlay_window.app_handle()).overlay_position;
        configure_layer_shell_surface(&gtk_window, overlay_position, OVERLAY_WIDTH, OVERLAY_HEIGHT);

        let initialized = gtk_window.is_layer_window();
        LAYER_SHELL_ACTIVE.store(initialized, Ordering::SeqCst);
        return initialized;
    }
    false
}

/// Forces a window to be topmost using Win32 API (Windows only)
/// This is more reliable than Tauri's set_always_on_top which can be overridden
#[cfg(target_os = "windows")]
fn force_overlay_topmost(overlay_window: &tauri::webview::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    // Clone because run_on_main_thread takes 'static
    let overlay_clone = overlay_window.clone();

    // Make sure the Win32 call happens on the UI thread
    let _ = overlay_clone.clone().run_on_main_thread(move || {
        if let Ok(hwnd) = overlay_clone.hwnd() {
            unsafe {
                // Force Z-order: make this window topmost without changing size/pos or stealing focus
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    });
}

fn get_monitor_with_cursor(app_handle: &AppHandle) -> Option<tauri::Monitor> {
    if let Some(mouse_location) = input::get_cursor_position(app_handle) {
        if let Ok(monitors) = app_handle.available_monitors() {
            for monitor in monitors {
                // On Windows both the cursor (enigo -> GetCursorPos) and the
                // monitor bounds are physical pixels, so compare them directly.
                #[cfg(target_os = "windows")]
                if is_mouse_within_monitor(mouse_location, monitor.position(), monitor.size()) {
                    return Some(monitor);
                }

                // macOS/Linux: enigo returns logical coords, so scale the bounds down.
                #[cfg(not(target_os = "windows"))]
                {
                    let scale = monitor.scale_factor();
                    let pos = PhysicalPosition::new(
                        (monitor.position().x as f64 / scale) as i32,
                        (monitor.position().y as f64 / scale) as i32,
                    );
                    let size = PhysicalSize::new(
                        (monitor.size().width as f64 / scale) as u32,
                        (monitor.size().height as f64 / scale) as u32,
                    );
                    if is_mouse_within_monitor(mouse_location, &pos, &size) {
                        return Some(monitor);
                    }
                }
            }
        }
    }

    app_handle.primary_monitor().ok().flatten()
}

fn is_mouse_within_monitor(
    mouse_pos: (i32, i32),
    monitor_pos: &PhysicalPosition<i32>,
    monitor_size: &PhysicalSize<u32>,
) -> bool {
    let (mouse_x, mouse_y) = mouse_pos;
    let PhysicalPosition {
        x: monitor_x,
        y: monitor_y,
    } = *monitor_pos;
    let PhysicalSize {
        width: monitor_width,
        height: monitor_height,
    } = *monitor_size;

    mouse_x >= monitor_x
        && mouse_x < (monitor_x + monitor_width as i32)
        && mouse_y >= monitor_y
        && mouse_y < (monitor_y + monitor_height as i32)
}

/// Returns overlay position in logical coordinates (points on macOS).
///
/// The Bottom anchor uses the macOS work area (visibleFrame) so the overlay
/// tracks the Dock — above it when shown, at the screen edge when hidden.
/// This relies on tauri 2.11's work_area.position.y fix (#14655), the same
/// bug that led PR #969 to abandon work_area for full monitor bounds. Top and
/// the other platforms keep full monitor bounds plus the fixed offsets
/// (work_area is unreliable on Wayland; Windows' offset clears the taskbar).
///
/// We must use LogicalPosition (not PhysicalPosition) because Tauri/tao
/// converts PhysicalPosition using the scale factor of the monitor the window
/// is *currently* on, which is wrong when moving cross-monitor. Windows uses
/// `place_windows_overlay` instead (no single logical space across mixed DPI).
fn calculate_overlay_position(
    app_handle: &AppHandle,
    width: f64,
    height: f64,
    state: &str,
) -> Option<(f64, f64)> {
    let monitor = get_monitor_with_cursor(app_handle)?;
    let position = overlay_position_for(&settings::get_settings(app_handle), state);
    Some(overlay_position_on_monitor(
        &monitor, width, height, position,
    ))
}

/// Which screen edge a view docks on. The dictation overlay follows the
/// user's `overlay_position` setting; the meeting bar has its own dock edge —
/// the reference's `statusDockEdge` — once it has been dragged to one, and follows the
/// dictation overlay's edge until then (the S-4c behaviour).
fn overlay_position_for(settings: &AppSettings, state: &str) -> OverlayPosition {
    if is_dictation_state(state) {
        settings.overlay_position
    } else {
        settings
            .meeting_bar_position
            // The reference docks its bar on the right; the dictation card's
            // edge says nothing about where the meeting bar belongs.
            .unwrap_or(OverlayPosition::Right)
    }
}

/// The monitor's dockable region in global logical coordinates. macOS anchors
/// every edge to the work area (NSScreen.visibleFrame — the Dock- and
/// menu-bar-aware region), so Top/Left/Right/Bottom all keep the bar out of
/// the Dock and the menu bar without hardcoded pixel guesses. The other
/// platforms keep full monitor bounds: work_area is unreliable on Wayland, and
/// Windows' fixed offsets already clear the taskbar.
fn monitor_dock_area(monitor: &tauri::Monitor) -> (f64, f64, f64, f64) {
    let scale = monitor.scale_factor();
    #[cfg(target_os = "macos")]
    let (x, y, w, h) = {
        let wa = monitor.work_area();
        (
            wa.position.x as f64 / scale,
            wa.position.y as f64 / scale,
            wa.size.width as f64 / scale,
            wa.size.height as f64 / scale,
        )
    };
    #[cfg(not(target_os = "macos"))]
    let (x, y, w, h) = (
        monitor.position().x as f64 / scale,
        monitor.position().y as f64 / scale,
        monitor.size().width as f64 / scale,
        monitor.size().height as f64 / scale,
    );
    (x, y, w, h)
}

/// The docked origin (logical) for `width`x`height` on one monitor.
fn overlay_position_on_monitor(
    monitor: &tauri::Monitor,
    width: f64,
    height: f64,
    position: OverlayPosition,
) -> (f64, f64) {
    let (area_x, area_y, area_w, area_h) = monitor_dock_area(monitor);
    match position {
        OverlayPosition::Top => (area_x + (area_w - width) / 2.0, area_y + OVERLAY_TOP_OFFSET),
        OverlayPosition::Bottom => (
            area_x + (area_w - width) / 2.0,
            area_y + area_h - height - OVERLAY_BOTTOM_OFFSET,
        ),
        OverlayPosition::Left => (
            area_x + OVERLAY_SIDE_OFFSET,
            area_y + (area_h - height) / 2.0,
        ),
        OverlayPosition::Right => (
            area_x + area_w - width - OVERLAY_SIDE_OFFSET,
            area_y + (area_h - height) / 2.0,
        ),
    }
}

/// The monitor whose bounds contain a global logical point — where a dragged
/// bar was dropped — or the nearest one when it sits between monitors. Same
/// coordinate convention as `get_monitor_with_cursor`: monitor bounds are
/// physical, the point is logical, so the bounds are scaled down.
fn monitor_for_point(app_handle: &AppHandle, x: f64, y: f64) -> Option<tauri::Monitor> {
    let monitors = app_handle.available_monitors().ok()?;
    let mut nearest: Option<(f64, tauri::Monitor)> = None;
    for monitor in monitors {
        let scale = monitor.scale_factor();
        // Containment uses the full monitor bounds, not the dock area — a bar
        // dropped into the Dock's strip or under the menu bar still docks to
        // the screen it was dropped on.
        let (bounds_x, bounds_y, bounds_w, bounds_h) = (
            monitor.position().x as f64 / scale,
            monitor.position().y as f64 / scale,
            monitor.size().width as f64 / scale,
            monitor.size().height as f64 / scale,
        );
        if x >= bounds_x && x < bounds_x + bounds_w && y >= bounds_y && y < bounds_y + bounds_h {
            return Some(monitor);
        }
        let center = (bounds_x + bounds_w / 2.0, bounds_y + bounds_h / 2.0);
        let distance = (center.0 - x).hypot(center.1 - y);
        if nearest.as_ref().is_none_or(|(best, _)| distance < *best) {
            nearest = Some((distance, monitor));
        }
    }
    nearest.map(|(_, monitor)| monitor)
}

/// The edge of `area` a dropped bar belongs to: the one it was dropped
/// closest to (the reference statusDockEdge snapping).
fn nearest_overlay_edge(area: (f64, f64, f64, f64), x: f64, y: f64) -> OverlayPosition {
    let (area_x, area_y, area_w, area_h) = area;
    [
        (OverlayPosition::Bottom, area_y + area_h - y),
        (OverlayPosition::Top, y - area_y),
        (OverlayPosition::Left, x - area_x),
        (OverlayPosition::Right, area_x + area_w - x),
    ]
    .into_iter()
    .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    .map(|(position, _)| position)
    .unwrap_or(OverlayPosition::Bottom)
}

/// Windows accessibility text size (Settings > Accessibility > Text size), a
/// separate axis from display scaling that WebView2 applies as a document zoom.
#[cfg(target_os = "windows")]
fn windows_text_scale_factor() -> f64 {
    // Absent until the user moves the slider off 100%; stored as a percentage.
    winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
        .open_subkey(r"Software\Microsoft\Accessibility")
        .and_then(|key| key.get_value::<u32, _>("TextScaleFactor"))
        .map(|percent| (percent as f64 / 100.0).clamp(1.0, 2.25))
        .unwrap_or(1.0)
}

/// Overlay rectangle in the destination monitor's physical pixels, so nothing
/// is converted through the window's previous-monitor DPI.
#[cfg(target_os = "windows")]
fn windows_overlay_bounds(
    monitor_position: PhysicalPosition<i32>,
    monitor_size: PhysicalSize<u32>,
    scale: f64,
    text_scale: f64,
    logical_width: f64,
    logical_height: f64,
    overlay_position: OverlayPosition,
) -> (i32, i32, i32, i32) {
    // Grow the window with the text scale; offsets stay DPI-only since the
    // card sits flush against the window's screen-edge side.
    let content_scale = scale * text_scale;
    let width = (logical_width * content_scale).round().max(1.0) as i32;
    let height = (logical_height * content_scale).round().max(1.0) as i32;
    let center_x = (monitor_position.x as f64 + (monitor_size.width as f64 - width as f64) / 2.0)
        .round() as i32;
    let center_y = (monitor_position.y as f64 + (monitor_size.height as f64 - height as f64) / 2.0)
        .round() as i32;
    let (x, y) = match overlay_position {
        OverlayPosition::Top => (
            center_x,
            (monitor_position.y as f64 + OVERLAY_TOP_OFFSET * scale).round() as i32,
        ),
        OverlayPosition::Bottom => (
            center_x,
            (monitor_position.y as f64 + monitor_size.height as f64
                - height as f64
                - OVERLAY_BOTTOM_OFFSET * scale)
                .round() as i32,
        ),
        OverlayPosition::Left => (
            (monitor_position.x as f64 + OVERLAY_SIDE_OFFSET * scale).round() as i32,
            center_y,
        ),
        OverlayPosition::Right => (
            (monitor_position.x as f64 + monitor_size.width as f64
                - width as f64
                - OVERLAY_SIDE_OFFSET * scale)
                .round() as i32,
            center_y,
        ),
    };

    (x, y, width, height)
}

/// Moves and sizes the overlay in one native SetWindowPos, bypassing tao's
/// current-DPI logical conversion that mislands cross-monitor moves.
#[cfg(target_os = "windows")]
fn place_windows_overlay(
    app_handle: &AppHandle,
    overlay_window: &tauri::webview::WebviewWindow,
    logical_width: f64,
    logical_height: f64,
    position: OverlayPosition,
) -> Result<(), String> {
    use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER};

    let monitor = get_monitor_with_cursor(app_handle)
        .ok_or_else(|| "failed to determine the monitor containing the cursor".to_string())?;
    let text_scale = windows_text_scale_factor();
    let (x, y, width, height) = windows_overlay_bounds(
        *monitor.position(),
        *monitor.size(),
        monitor.scale_factor(),
        text_scale,
        logical_width,
        logical_height,
        position,
    );
    let hwnd = overlay_window
        .hwnd()
        .map_err(|error| format!("failed to get overlay window handle: {error}"))?;

    unsafe {
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width,
            height,
            SWP_NOACTIVATE | SWP_NOZORDER,
        )
        .map_err(|error| format!("failed to set overlay bounds: {error}"))?;
    }

    log::debug!(
        "windows overlay bounds: x={} y={} width={} height={} scale={} text_scale={}",
        x,
        y,
        width,
        height,
        monitor.scale_factor(),
        text_scale
    );
    Ok(())
}

/// Creates the recording overlay window and keeps it hidden by default
#[cfg(not(target_os = "macos"))]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    // On Linux (Wayland), monitor detection often fails, but we don't need exact coordinates
    // for Layer Shell as we use anchors. On other platforms, we require a monitor.
    #[cfg(not(target_os = "linux"))]
    {
        let position =
            calculate_overlay_position(app_handle, OVERLAY_WIDTH, OVERLAY_HEIGHT, "recording");
        if position.is_none() {
            debug!("Failed to determine overlay position, not creating overlay window");
            return;
        }
    }

    // Position starts unset — update_overlay_position() sets the correct
    // LogicalPosition before the overlay is shown.
    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        "recording_overlay",
        tauri::WebviewUrl::App("src/overlay/index.html".into()),
    )
    .title("Recording")
    .resizable(false)
    .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
    .shadow(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .accept_first_mouse(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focusable(false)
    .focused(false)
    .visible(false);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    #[allow(unused_variables)]
    match builder.build() {
        Ok(window) => {
            #[cfg(target_os = "linux")]
            {
                // Try to initialize GTK layer shell, ignore errors if compositor doesn't support it
                if init_gtk_layer_shell(&window) {
                    debug!("GTK layer shell initialized for overlay window");
                } else {
                    debug!("GTK layer shell not available, falling back to regular window");
                }
            }

            debug!("Recording overlay window created successfully (hidden)");
        }
        Err(e) => {
            debug!("Failed to create recording overlay window: {}", e);
        }
    }
}

/// Creates the recording overlay panel and keeps it hidden by default (macOS)
#[cfg(target_os = "macos")]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    if let Some((x, y)) =
        calculate_overlay_position(app_handle, OVERLAY_WIDTH, OVERLAY_HEIGHT, "recording")
    {
        // PanelBuilder creates a Tauri window then converts it to NSPanel.
        // The window remains registered, so get_webview_window() still works.
        match PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, "recording_overlay")
            .url(WebviewUrl::App("src/overlay/index.html".into()))
            .title("Recording")
            .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
            .level(PanelLevel::Status)
            .size(tauri::Size::Logical(tauri::LogicalSize {
                width: OVERLAY_WIDTH,
                height: OVERLAY_HEIGHT,
            }))
            .has_shadow(false)
            .transparent(true)
            .no_activate(true)
            .corner_radius(0.0)
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .with_window(|w| w.decorations(false).transparent(true).focusable(false))
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary(),
            )
            .build()
        {
            Ok(panel) => {
                panel.hide();
            }
            Err(e) => {
                log::error!("Failed to create recording overlay panel: {}", e);
            }
        }
    }
}

fn show_overlay_state(app_handle: &AppHandle, state: &str) {
    // Whether the dictation overlay shows at all is governed by overlay_style;
    // position only chooses which edge it sits on. Checked here (off the main
    // thread) so the common overlay-disabled case never pays for a
    // main-thread hop.
    let settings = settings::get_settings(app_handle);
    if settings.overlay_style == OverlayStyle::None {
        return;
    }

    queue_overlay_state(app_handle, state);
}

/// Queue a meeting-bar view (the idle disc or the recording pill) into the one
/// shared overlay window. The bar is the meeting feature's own UI, so it shows
/// regardless of `overlay_style`, which governs only the dictation overlay.
pub fn show_meeting_bar_state(app_handle: &AppHandle, state: &str) {
    queue_overlay_state(app_handle, state);
}

fn queue_overlay_state(app_handle: &AppHandle, state: &str) {
    // The rest queries monitors and the cursor and mutates window geometry. On
    // Linux the monitor/cursor lookups hit GDK/Xlib on the process's shared X11
    // connection, which is only safe from the GTK main thread — running them on
    // a background thread corrupts the connection and hard-crashes the app
    // (issue #227). Hop to the main thread on every platform to keep the
    // geometry path uniform (a no-op cost on Windows, and it also keeps macOS's
    // NSScreen access main-thread-correct). run_on_main_thread runs the closure
    // inline when already on the main thread, so this never deadlocks.
    let handle = app_handle.clone();
    let state = state.to_string();
    let _ = app_handle.run_on_main_thread(move || show_overlay_state_on_main(&handle, &state));
}

/// Size and place the overlay window for `state` (no show, no event). Shared
/// by the show path and the meeting bar's dock. Runs on the main thread.
/// Layer-shell surfaces (Linux) never come through here: their geometry is
/// committed by `configure_layer_shell_surface` instead, and dragging a
/// compositor-anchored surface is not ours to do.
fn apply_overlay_geometry_on_main(
    app_handle: &AppHandle,
    overlay_window: &tauri::webview::WebviewWindow,
    state: &str,
) {
    apply_overlay_geometry_sized(app_handle, overlay_window, state, overlay_dimensions(state));
}

/// Same placement as `apply_overlay_geometry_on_main`, with an explicit size:
/// the idle disc grows while it is hovered without becoming a different state.
fn apply_overlay_geometry_sized(
    app_handle: &AppHandle,
    overlay_window: &tauri::webview::WebviewWindow,
    state: &str,
    size: (f64, f64),
) {
    let (width, height) = size;
    #[cfg(not(target_os = "windows"))]
    {
        let _ = overlay_window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        if let Some((x, y)) = calculate_overlay_position(app_handle, width, height, state) {
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
    }
    #[cfg(target_os = "windows")]
    {
        let position = overlay_position_for(&settings::get_settings(app_handle), state);
        if let Err(error) =
            place_windows_overlay(app_handle, overlay_window, width, height, position)
        {
            log::error!("Failed to place recording overlay: {error}");
        }
    }
}

fn show_overlay_state_on_main(app_handle: &AppHandle, state: &str) {
    // Remember which view owns the window: the position updater re-applies the
    // right geometry per state, and the meeting refresh defers while a
    // dictation view is showing.
    *CURRENT_OVERLAY_STATE.lock().unwrap() = state.to_string();
    DICTATION_OVERLAY_ACTIVE.store(is_dictation_state(state), Ordering::SeqCst);

    // Size the overlay for this state (compact vs. streaming), then position it.
    // Only the layer-shell branch reads the dimensions here directly; every
    // other path is sized inside apply_overlay_geometry_on_main.
    #[cfg(target_os = "linux")]
    let (width, height) = overlay_dimensions(state);
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // Invalidate any delayed hide still in flight from a previous session
        // (see `hide_recording_overlay`).
        OVERLAY_SHOW_GENERATION.fetch_add(1, Ordering::SeqCst);

        #[cfg(target_os = "linux")]
        let shown_with_layer_shell = if LAYER_SHELL_ACTIVE.load(Ordering::SeqCst) {
            let position = overlay_position_for(&settings::get_settings(app_handle), state);
            match overlay_window.gtk_window() {
                Ok(gtk_window) => {
                    configure_layer_shell_surface(&gtk_window, position, width, height)
                }
                Err(error) => log::error!("Failed to access GTK overlay window: {error}"),
            }
            let _ = overlay_window.show();
            true
        } else {
            false
        };
        #[cfg(not(target_os = "linux"))]
        let shown_with_layer_shell = false;

        if !shown_with_layer_shell {
            apply_overlay_geometry_on_main(app_handle, &overlay_window, state);

            let show_started = std::time::Instant::now();
            let _ = overlay_window.show();
            let show_elapsed = show_started.elapsed();

            // On Windows, aggressively re-assert "topmost" in the native Z-order after showing
            #[cfg(target_os = "windows")]
            force_overlay_topmost(&overlay_window);

            // Re-assert bounds after show(): the pre-show move crosses the DPI
            // boundary, and tao's WM_DPICHANGED reflow clobbers the first placement.
            #[cfg(target_os = "windows")]
            apply_overlay_geometry_on_main(app_handle, &overlay_window, state);

            log::debug!("overlay '{}': show={:?}", state, show_elapsed);
        }

        // macOS polish for the meeting bar (the reference meeting bar direction): the bar
        // rides on the window server's own soft shadow, drawn from the window's
        // content shape — it can never intercept a click the way a transparent
        // margin would. The dictation overlay keeps its flat, shadow-less look.
        #[cfg(target_os = "macos")]
        let _ = overlay_window.set_shadow(!is_dictation_state(state));

        // Never carry a click-through state across views: only the idle disc's
        // hit-shape poll (below) sets `ignoresMouseEvents`, and every other view
        // must own a fully interactive window.
        #[cfg(target_os = "macos")]
        let _ = overlay_window.set_ignore_cursor_events(false);

        // The idle disc's window is the disc's bounding square, and the disc is
        // a circle: the transparent corners would swallow clicks meant for the
        // app underneath. Keep them click-through while the disc is up.
        #[cfg(target_os = "macos")]
        if state == MEETING_BAR_IDLE {
            start_disc_hit_shape_poll(app_handle);
        }

        let _ = overlay_window.emit("show-overlay", state);
    }
}

/// Notify the visible recording overlay that the input stream has delivered its
/// first sample chunk. Audio feedback uses the same backend readiness signal,
/// but this targeted event is skipped when overlays are disabled.
pub fn emit_recording_ready(app_handle: &AppHandle) {
    if !OVERLAY_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    // Showing the overlay is also queued onto the main thread. Queue readiness
    // there as well so a very fast always-on stream cannot overtake show-overlay
    // and then get reset back to the arming state by the frontend.
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        let _ = handle.emit_to("recording_overlay", "recording-ready", ());
    });
}

/// Shows the recording overlay window with fade-in animation
pub fn show_recording_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "recording");
}

/// Shows the larger streaming overlay that displays live transcription text
pub fn show_streaming_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "streaming");
}

/// Shows the transcribing overlay window
/// Shows the command-mode overlay window: the user is speaking an
/// instruction that will edit their selected text.
pub fn show_command_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "command");
}

pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "transcribing");
}

/// Shows the processing overlay window
pub fn show_processing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "processing");
}

/// Updates the overlay window position based on current settings
pub fn update_overlay_position(app_handle: &AppHandle) {
    // Positioning queries monitors/cursor (GDK/Xlib on Linux) and moves the
    // window, so it must run on the main thread — see show_overlay_state.
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || update_overlay_position_on_main(&handle));
}

fn update_overlay_position_on_main(app_handle: &AppHandle) {
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        #[cfg(target_os = "linux")]
        if LAYER_SHELL_ACTIVE.load(Ordering::SeqCst) {
            // Under layer shell only the anchor changes; the compositor commits
            // it. (The meeting bar is never dragged on layer-shell Wayland —
            // see `begin_meeting_bar_drag` — but honour a pinned edge anyway.)
            let state = CURRENT_OVERLAY_STATE.lock().unwrap().clone();
            let position = overlay_position_for(&settings::get_settings(app_handle), &state);
            match overlay_window.gtk_window() {
                Ok(gtk_window) => configure_layer_shell_position(&gtk_window, position),
                Err(error) => log::error!("Failed to access GTK overlay window: {error}"),
            }
            let _ = overlay_window.emit("overlay-position-changed", ());
            return;
        }

        // Re-dock whatever view owns the window right now: the dictation card
        // follows `overlay_position`, the meeting bar its own pinned edge.
        let state = CURRENT_OVERLAY_STATE.lock().unwrap().clone();
        apply_overlay_geometry_on_main(app_handle, &overlay_window, &state);

        // Tell the webview to re-read its placement: the meeting bar is a
        // persistent view and must re-anchor to the new edge without waiting
        // for the next state change.
        let _ = overlay_window.emit("overlay-position-changed", ());
    }
}

/// Generation counter bumped every time the overlay is shown. The delayed
/// `hide()` below only unmaps the window if no show happened after it was
/// scheduled, so a hide left over from a finished transcription can never
/// take down the overlay of a session that started in the meantime — e.g. a
/// press the coordinator remembered while the pipeline was busy and started
/// the instant it drained, well inside the 300 ms hide delay.
static OVERLAY_SHOW_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Whether a dictation view (recording/streaming/transcribing/processing)
/// currently owns the overlay window. The meeting bar yields to dictation:
/// its refresh is skipped while this is set, and the dictation hide path
/// refreshes again once the window is free, so the bar always comes back.
static DICTATION_OVERLAY_ACTIVE: AtomicBool = AtomicBool::new(false);

/// The state `show-overlay` last showed ("meeting-idle", "recording", ...).
/// The position updater re-applies the geometry that belongs to this state —
/// the meeting disc is a different footprint than the dictation pill.
static CURRENT_OVERLAY_STATE: Mutex<String> = Mutex::new(String::new());

/// Hides the recording overlay window with fade-out animation
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    // Always hide the overlay regardless of settings - if setting was changed while recording,
    // we still want to hide it properly
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // Snapshot before doing anything observable, so any show that lands
        // after this point invalidates the delayed hide below.
        let scheduled_at = OVERLAY_SHOW_GENERATION.load(Ordering::SeqCst);
        // Emit event to trigger fade-out animation
        let _ = overlay_window.emit("hide-overlay", ());
        // After the fade delay the dictation view is done. The meeting bar is a
        // persistent fixture of this window, so hand the window back to it —
        // the recording pill while a meeting records, the idle disc otherwise —
        // instead of unmapping the window. A newer dictation session that
        // showed the overlay in the meantime still wins (generation check).
        let app_for_thread = app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            if OVERLAY_SHOW_GENERATION.load(Ordering::SeqCst) != scheduled_at {
                log::debug!("Skipping stale overlay hide: a newer session is showing the overlay");
                return;
            }
            DICTATION_OVERLAY_ACTIVE.store(false, Ordering::SeqCst);
            refresh_meeting_bar(&app_for_thread);
        });
    }
}

/// Show the meeting-bar view that matches the meeting session right now: the
/// recording pill while a meeting records, the idle disc otherwise. Called when
/// the app starts, whenever the session's stage changes, and when a dictation
/// view gives the window back — so the bar is effectively always present.
/// Skipped while a dictation view owns the window: the dictation hide path
/// calls this again once it is done.
pub fn refresh_meeting_bar(app_handle: &AppHandle) {
    if DICTATION_OVERLAY_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    if is_meeting_recording(app_handle) {
        show_meeting_bar_state(app_handle, MEETING_BAR_RECORDING);
        return;
    }

    // The disc can be dismissed for a while (right-click > "Hide for 1 hour"):
    // while it is, the window itself goes away — there is nothing else to see.
    if meeting_bar_hidden() {
        hide_meeting_bar_window(app_handle);
        return;
    }

    show_meeting_bar_state(app_handle, MEETING_BAR_IDLE);
}

/// Whether a meeting records right now.
fn is_meeting_recording(app_handle: &AppHandle) -> bool {
    app_handle
        .try_state::<Arc<MeetingManager>>()
        .map(|meetings| meetings.is_recording())
        .unwrap_or(false)
}

// ============================================================
// Meeting bar: drag-to-edge, right-click hide, hit-shape (S-4d)
//
// the reference meeting bar parity, round 2. The bar (idle disc and recording pill
// alike) can be dragged to another screen edge like the reference statusDockEdge;
// the disc offers "Hide for 1 hour" on right-click; and the window no longer
// swallows clicks outside its visible content.
// ============================================================

/// How long the bar must sit still after a move before it docks. A native
/// window drag ends with a mouse-up we never see (the modal drag loop consumes
/// it), so docking is driven by move-quiescence instead: the last move of a
/// drag arms the dock, and it fires once the window has been still this long.
/// Programmatic placements arm it too — and then no-op, because the window
/// already sits at its docked spot.
const MEETING_BAR_DOCK_QUIESCE: Duration = Duration::from_millis(150);

/// Move signals for the dock watcher above. Created once, on the first move.
static OVERLAY_MOVE_SIGNAL: OnceLock<mpsc::Sender<()>> = OnceLock::new();

/// The idle disc can be dismissed for a while: the bar stays gone until the
/// hour passes, the app restarts, or a meeting is started from Settings.
/// Deliberately in-memory — a restart always brings the disc back.
static MEETING_BAR_HIDDEN_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

/// While the disc's native context menu is up. The menu runs its own event
/// loop; the disc's hit-shape poll leaves the window's input state alone
/// until it closes.
static MEETING_BAR_MENU_OPEN: AtomicBool = AtomicBool::new(false);

/// The right-click menu's "hide for an hour" item, handled by the app-wide
/// menu-event handler in lib.rs.
pub const MEETING_BAR_HIDE_FOR_HOUR_ID: &str = "meeting_bar_hide_for_hour";

/// The idle disc's menu covers what the bead can start: a meeting (also its
/// left click), a note, the scratchpad, and hiding the bar for an hour.
pub const MEETING_BAR_START_ID: &str = "meeting_bar_start";
pub const MEETING_BAR_NOTE_ID: &str = "meeting_bar_note";
/// The menu item that opens the Scratchpad (quick note) window.
pub const MEETING_BAR_SCRATCHPAD_ID: &str = "meeting_bar_scratchpad";

/// The window moved. Called from the app's window-event handler; every move of
/// the overlay window — a drag or a programmatic placement — arms the dock
/// watcher.
pub fn on_overlay_window_moved(app_handle: &AppHandle) {
    let sender = OVERLAY_MOVE_SIGNAL.get_or_init(|| {
        let (tx, rx) = mpsc::channel();
        let handle = app_handle.clone();
        std::thread::spawn(move || loop {
            // Park until the first move of a burst, then wait for quiet.
            if rx.recv().is_err() {
                return;
            }
            loop {
                match rx.recv_timeout(MEETING_BAR_DOCK_QUIESCE) {
                    Ok(()) => {}
                    Err(_) => {
                        dock_meeting_bar_if_moved(&handle);
                        break;
                    }
                }
            }
        });
        tx
    });
    let _ = sender.send(());
}

/// Begin a native drag of the meeting bar. The window-server drag carries the
/// window with the pointer across the whole screen — the bar's own exact-fit
/// window could never track a cross-screen drag by itself — and the dock
/// watcher snaps it to the nearest edge once it settles. A drag never touches
/// the session: the frontend only hands over once the pointer has crossed its
/// drag threshold, so a click still clicks and a drag drags.
pub fn begin_meeting_bar_drag(app_handle: &AppHandle) {
    let state = CURRENT_OVERLAY_STATE.lock().unwrap().clone();
    if is_dictation_state(&state) {
        return;
    }
    // A compositor-anchored layer-shell surface is not ours to move.
    #[cfg(target_os = "linux")]
    if LAYER_SHELL_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("recording_overlay") {
            if let Err(error) = window.start_dragging() {
                log::warn!("Failed to start the meeting bar drag: {error}");
            }
        }
    });
}

/// Dock the meeting bar after its window has been still: snap it to the
/// nearest edge of the monitor it was dropped on, save that edge (the reference
/// `statusDockEdge` choice, resumed on restart), and tell the webview to
/// re-anchor. No-op for dictation views and for a bar that already sits at
/// its docked spot — which is what every programmatic placement does.
fn dock_meeting_bar_if_moved(app_handle: &AppHandle) {
    let state = CURRENT_OVERLAY_STATE.lock().unwrap().clone();
    if is_dictation_state(&state) {
        return;
    }
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        dock_meeting_bar_on_main(&handle, &state);
    });
}

fn dock_meeting_bar_on_main(app_handle: &AppHandle, state: &str) {
    #[cfg(target_os = "linux")]
    if LAYER_SHELL_ACTIVE.load(Ordering::SeqCst) {
        return;
    }

    let Some(window) = app_handle.get_webview_window("recording_overlay") else {
        return;
    };
    let (Ok(position), Ok(size), Ok(scale)) = (
        window.outer_position(),
        window.outer_size(),
        window.scale_factor(),
    ) else {
        return;
    };

    // Where the bar was dropped, in global logical coordinates.
    let center_x = (position.x as f64 + size.width as f64 / 2.0) / scale;
    let center_y = (position.y as f64 + size.height as f64 / 2.0) / scale;
    let Some(monitor) = monitor_for_point(app_handle, center_x, center_y) else {
        return;
    };
    let area = monitor_dock_area(&monitor);
    let edge = nearest_overlay_edge(area, center_x, center_y);

    // Already docked here — a programmatic placement, or a drag that ended
    // where it began: nothing to move, and nothing to pin. The docked spot is
    // computed for the footprint the window has right now: the hovered disc
    // is wider than its resting form, and docking it back to the resting
    // footprint's spot would collapse it 150 ms after hover opened it.
    let dropped_x = position.x as f64 / scale;
    let dropped_y = position.y as f64 / scale;
    let (x, y) = overlay_position_on_monitor(
        &monitor,
        size.width as f64 / scale,
        size.height as f64 / scale,
        edge,
    );
    if (dropped_x - x).abs() <= 3.0 && (dropped_y - y).abs() <= 3.0 {
        return;
    }

    let mut settings = settings::get_settings(app_handle);
    settings.meeting_bar_position = Some(edge);
    settings::write_settings(app_handle, settings);
    log::debug!("meeting bar docked to the {edge:?} edge");

    apply_overlay_geometry_on_main(app_handle, &window, state);
    let _ = window.emit("overlay-position-changed", ());
}

/// Grow or shrink the meeting bar's window for the "still going?" prompt
/// (S-24): the webview owns the timing — it knows the session's clock — and
/// asks for the wider footprint while the prompt is up. Resizes in place,
/// never `show()`s or focuses: the window is already on screen and must not
/// steal focus from the app the person is working in. No-op for any other
/// view: a dictation card, the disc or a wrap owns the geometry right now,
/// and re-applies its own footprint on its own transitions anyway.
pub fn set_meeting_bar_nudge(app_handle: &AppHandle, show: bool) {
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        let current = CURRENT_OVERLAY_STATE.lock().unwrap().clone();
        if current != MEETING_BAR_RECORDING && current != MEETING_BAR_NUDGE {
            return;
        }
        let target = if show {
            MEETING_BAR_NUDGE
        } else {
            MEETING_BAR_RECORDING
        };
        *CURRENT_OVERLAY_STATE.lock().unwrap() = target.to_string();
        let Some(window) = handle.get_webview_window("recording_overlay") else {
            return;
        };

        // A layer-shell surface's geometry is the compositor's to commit.
        #[cfg(target_os = "linux")]
        if LAYER_SHELL_ACTIVE.load(Ordering::SeqCst) {
            let position = overlay_position_for(&settings::get_settings(&handle), target);
            let (width, height) = overlay_dimensions(target);
            match window.gtk_window() {
                Ok(gtk_window) => {
                    configure_layer_shell_surface(&gtk_window, position, width, height)
                }
                Err(error) => log::error!("Failed to access GTK overlay window: {error}"),
            }
            return;
        }

        apply_overlay_geometry_on_main(&handle, &window, target);
    });
}

/// Grow or shrink the idle disc into its action row. Hover-driven, and only
/// while the bar is idle: a dictation or a recording owns the window.
pub fn set_meeting_bar_expanded(app_handle: &AppHandle, expanded: bool) {
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        if *CURRENT_OVERLAY_STATE.lock().unwrap() != MEETING_BAR_IDLE {
            return;
        }
        let Some(window) = handle.get_webview_window("recording_overlay") else {
            return;
        };
        let size = if expanded {
            (MEETING_DISC_WIDE, MEETING_DISC_HEIGHT)
        } else {
            (MEETING_DISC_WIDTH, MEETING_DISC_HEIGHT)
        };
        apply_overlay_geometry_sized(&handle, &window, MEETING_BAR_IDLE, size);
    });
}

/// The idle disc's right-click menu (the reference's escape hatch). Native, not HTML:
/// the disc's window is exactly the disc, so a menu could never render inside
/// it. Pops at the cursor; the item is dispatched by the app-wide menu-event
/// handler.
pub fn show_meeting_bar_menu(app_handle: &AppHandle) {
    // Only the idle disc offers the menu — the recording pill is a live
    // session's UI and can never be hidden.
    if *CURRENT_OVERLAY_STATE.lock().unwrap() != MEETING_BAR_IDLE
        || is_meeting_recording(app_handle)
    {
        return;
    }

    let strings = crate::tray_i18n::get_tray_translations(Some(
        settings::get_settings(app_handle).app_language,
    ));
    // Locales that haven't translated a key yet fall back to English, the
    // same contract the tray's secure-input item has.
    let english = crate::tray_i18n::get_tray_translations(Some("en".to_string()));
    let label = if strings.meeting_bar_hide_for_hour.is_empty() {
        english.meeting_bar_hide_for_hour
    } else {
        strings.meeting_bar_hide_for_hour
    };
    let scratchpad_label = if strings.scratchpad_open.is_empty() {
        english.scratchpad_open
    } else {
        strings.scratchpad_open
    };

    let start_label = if strings.meeting_bar_start.is_empty() {
        english.meeting_bar_start
    } else {
        strings.meeting_bar_start
    };
    let note_label = if strings.meeting_bar_note.is_empty() {
        english.meeting_bar_note
    } else {
        strings.meeting_bar_note
    };

    let build = || -> Result<Menu<tauri::Wry>, tauri::Error> {
        let start = MenuItem::with_id(
            app_handle,
            MEETING_BAR_START_ID,
            &start_label,
            true,
            None::<&str>,
        )?;
        let note = MenuItem::with_id(
            app_handle,
            MEETING_BAR_NOTE_ID,
            &note_label,
            true,
            None::<&str>,
        )?;
        let scratchpad = MenuItem::with_id(
            app_handle,
            MEETING_BAR_SCRATCHPAD_ID,
            &scratchpad_label,
            true,
            None::<&str>,
        )?;
        let hide = MenuItem::with_id(
            app_handle,
            MEETING_BAR_HIDE_FOR_HOUR_ID,
            &label,
            true,
            None::<&str>,
        )?;
        Menu::with_items(app_handle, &[&start, &note, &scratchpad, &hide])
    };
    let menu = match build() {
        Ok(menu) => menu,
        Err(error) => {
            log::warn!("Failed to build the meeting bar menu: {error}");
            return;
        }
    };
    let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") else {
        return;
    };
    // The popup anchors to the hosting window (the panel); the manager's
    // `get_window` is unstable-only, so reach it through the webview window.
    let window = overlay_window.as_ref().window();

    MEETING_BAR_MENU_OPEN.store(true, Ordering::SeqCst);
    if let Err(error) = menu.popup(window) {
        log::warn!("Failed to open the meeting bar menu: {error}");
    }
    MEETING_BAR_MENU_OPEN.store(false, Ordering::SeqCst);
}

/// Hide the idle disc for a while (right-click > "Hide for 1 hour"). It comes
/// back when the hour passes, when the app restarts, or when a meeting is
/// started from Settings. The recording pill is never hideable.
pub fn hide_meeting_bar_for(app_handle: &AppHandle, secs: u64) {
    if is_meeting_recording(app_handle) {
        return;
    }
    let Some(deadline) = Instant::now().checked_add(Duration::from_secs(secs)) else {
        return;
    };
    *MEETING_BAR_HIDDEN_UNTIL.lock().unwrap() = Some(deadline);

    // Hide the window now if the idle disc is showing; a dictation view keeps
    // it — the flag is re-checked when the dictation hands the window back.
    refresh_meeting_bar(app_handle);

    // The hour itself: one sleeping thread, no polling, no persistence.
    let handle = app_handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(secs));
        let mut hidden = MEETING_BAR_HIDDEN_UNTIL.lock().unwrap();
        // Only this hide's own deadline may expire it; anything else that
        // changed the flag since owns the decision.
        if *hidden == Some(deadline) {
            *hidden = None;
            drop(hidden);
            refresh_meeting_bar(&handle);
        }
    });
}

/// Bring the bar back early: starting a meeting from Settings dismisses the
/// hour-long hide. Callers refresh the bar themselves (the pill shows).
pub fn unhide_meeting_bar() {
    *MEETING_BAR_HIDDEN_UNTIL.lock().unwrap() = None;
}

fn meeting_bar_hidden() -> bool {
    MEETING_BAR_HIDDEN_UNTIL
        .lock()
        .unwrap()
        .is_some_and(|until| until > Instant::now())
}

/// Take the overlay window off screen while the bar is hidden. The window is
/// the bar's whole UI; hiding it hides the bar (and frees the space it sat in).
fn hide_meeting_bar_window(app_handle: &AppHandle) {
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window("recording_overlay") {
            #[cfg(target_os = "macos")]
            let _ = window.set_ignore_cursor_events(false);
            let _ = window.hide();
        }
    });
}

/// Slack added to the disc's radius when deciding whether the cursor is "on"
/// the disc: the whole visible circle must stay clickable even if the
/// global-cursor-to-window-frame math lands a point off on a scaled display.
#[cfg(target_os = "macos")]
const DISC_HIT_MARGIN: f64 = 3.0;

#[cfg(target_os = "macos")]
const DISC_HIT_POLL_INTERVAL: Duration = Duration::from_millis(40);

/// Freshness token for the disc's hit-shape poll: each show of the idle disc
/// starts a poll and invalidates the previous one.
#[cfg(target_os = "macos")]
static DISC_HIT_POLL_GENERATION: AtomicU64 = AtomicU64::new(0);

/// The idle disc's window is the disc's bounding square, and the disc is a
/// circle: the square's corners are transparent yet still ours, so a click
/// aimed just past the disc's edge would die in our window instead of reaching
/// the app underneath. While the disc owns the window, keep the window's
/// `ignoresMouseEvents` in sync with the cursor: click-through everywhere
/// except on the disc itself (plus a small margin). Fails safe — on any error
/// the window stays fully clickable.
#[cfg(target_os = "macos")]
fn start_disc_hit_shape_poll(app_handle: &AppHandle) {
    let generation = DISC_HIT_POLL_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let handle = app_handle.clone();
    std::thread::spawn(move || {
        let mut ignored = false;
        loop {
            std::thread::sleep(DISC_HIT_POLL_INTERVAL);
            if generation != DISC_HIT_POLL_GENERATION.load(Ordering::SeqCst) {
                break;
            }
            if *CURRENT_OVERLAY_STATE.lock().unwrap() != MEETING_BAR_IDLE {
                break;
            }
            if MEETING_BAR_MENU_OPEN.load(Ordering::SeqCst) {
                continue;
            }
            let Some(window) = handle.get_webview_window("recording_overlay") else {
                break;
            };
            if !window.is_visible().unwrap_or(false) {
                break;
            }

            // None (no cursor, no frame) means "can't tell" — stay clickable.
            let hits_disc = disc_cursor_hits(&handle, &window).unwrap_or(true);
            let want_ignore = !hits_disc;
            if want_ignore != ignored {
                ignored = want_ignore;
                let poll_handle = handle.clone();
                let _ = handle.run_on_main_thread(move || {
                    if let Some(window) = poll_handle.get_webview_window("recording_overlay") {
                        let _ = window.set_ignore_cursor_events(want_ignore);
                    }
                });
            }
        }

        // Leaving the disc: whatever comes next owns a fully clickable window.
        let exit_handle = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if let Some(window) = exit_handle.get_webview_window("recording_overlay") {
                let _ = window.set_ignore_cursor_events(false);
            }
        });
    });
}

/// Whether the global cursor sits on the disc's circle. Both the cursor
/// (enigo reports points on macOS) and the window frame (physical, scaled
/// down) land in the same global logical space.
#[cfg(target_os = "macos")]
fn disc_cursor_hits(
    app_handle: &AppHandle,
    window: &tauri::webview::WebviewWindow,
) -> Option<bool> {
    let (cursor_x, cursor_y) = input::get_cursor_position(app_handle)?;
    let position = window.outer_position().ok()?;
    let size = window.outer_size().ok()?;
    let scale = window.scale_factor().ok()?;

    let center_x = (position.x as f64 + size.width as f64 / 2.0) / scale;
    let center_y = (position.y as f64 + size.height as f64 / 2.0) / scale;
    let radius = size.width as f64 / scale / 2.0 + DISC_HIT_MARGIN;
    Some((cursor_x as f64 - center_x).hypot(cursor_y as f64 - center_y) <= radius)
}

// Cached "overlay is enabled" flag, kept in sync with overlay_style. Avoids
// reading the Tauri store on every audio callback (~24 Hz during recording).
// Defaults to false so the audio path doesn't emit until lib.rs::setup
// populates the cache from initial settings.
static OVERLAY_ENABLED: AtomicBool = AtomicBool::new(false);

/// Tracks whether gtk-layer-shell was successfully initialized (Linux only).
/// Used to skip layer-shell calls when the window is a regular fallback.
#[cfg(target_os = "linux")]
static LAYER_SHELL_ACTIVE: AtomicBool = AtomicBool::new(false);

/// Update the cached overlay-enabled flag. Called from `lib.rs` at
/// startup after settings load, and from `change_overlay_style_setting`
/// whenever the user changes whether the overlay is shown.
pub fn update_overlay_enabled_cache(enabled: bool) {
    OVERLAY_ENABLED.store(enabled, Ordering::Relaxed);
}

pub fn emit_levels(app_handle: &AppHandle, levels: &[f32]) {
    // Skip emission when the overlay is disabled. The recording_overlay
    // window is created at boot regardless of overlay_style, so without this
    // guard a hidden overlay's WebKit subprocess still
    // processes every event. Each event drives some kind of WebKit
    // C++ allocation that accumulates without bound (mechanism not
    // directly characterized; see issue #1279 for the investigation).
    // For users with `overlay_style: none` (the Linux default) this skip
    // eliminates the upstream driver of that accumulation.
    if !OVERLAY_ENABLED.load(Ordering::Relaxed) {
        return;
    }

    // Throttle to ~30 FPS. Even with the overlay enabled, the raw audio
    // callback fires far faster than the UI needs; capping emission rate
    // cuts the per-frame `eval_script`/IPC volume that drives the wry
    // memory growth in issue #1279 (upstream tauri-apps/wry#1489).
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let last = LAST_MIC_LEVEL_EMIT.load(Ordering::Relaxed);
    if now.saturating_sub(last) < EMIT_THROTTLE_MS {
        return;
    }
    LAST_MIC_LEVEL_EMIT.store(now, Ordering::Relaxed);

    // Target only the overlay window. In Tauri 2 both `AppHandle::emit`
    // and `WebviewWindow::emit` broadcast to all webviews; Tauri's
    // listener filter then skips webviews with no registered listener
    // for the event, so the settings webview never received `mic-level`.
    // But the previous dual-call pattern still produced two `eval_script`
    // calls to the overlay per audio callback (one from each .emit()).
    // `emit_to` with the overlay's window label produces a single
    // eval_script call per callback, cutting the per-callback WebKit
    // dispatch work in half.
    let _ = app_handle.emit_to("recording_overlay", "mic-level", levels);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meeting_bar_views_have_their_own_footprints() {
        // The idle disc's window is exactly the 40x40 circle: no transparent
        // margin to swallow clicks around it.
        assert_eq!(
            overlay_dimensions(MEETING_BAR_IDLE),
            (MEETING_DISC_WIDTH, MEETING_DISC_HEIGHT)
        );
        // The recording pill (and the frontend's "Saving" pill) is the
        // capsule's own exact footprint, not the dictation pill's.
        assert_eq!(
            overlay_dimensions(MEETING_BAR_RECORDING),
            (MEETING_PILL_WIDTH, MEETING_PILL_HEIGHT)
        );
        // The dictation states keep their own footprints untouched.
        assert_eq!(
            overlay_dimensions("recording"),
            (OVERLAY_WIDTH, OVERLAY_HEIGHT)
        );
        // The "still going?" prompt adds exactly its card's width to the
        // pill's footprint — the window is never wider than what is visible.
        assert_eq!(
            overlay_dimensions(MEETING_BAR_NUDGE),
            (
                MEETING_PILL_WIDTH + MEETING_NUDGE_WIDTH,
                MEETING_PILL_HEIGHT
            )
        );
    }

    #[test]
    fn meeting_bar_edge_follows_the_dictation_overlay_until_dragged() {
        let mut settings = crate::settings::get_default_settings();
        settings.overlay_position = OverlayPosition::Top;
        // Unpinned (the default): the bar sits on the right edge, where the
        // reference's Flow Bar lives, whatever the dictation card does.
        assert_eq!(
            overlay_position_for(&settings, MEETING_BAR_IDLE),
            OverlayPosition::Right
        );
        // Pinned by a drag: the bar keeps its own edge, the dictation card
        // keeps the setting's.
        settings.meeting_bar_position = Some(OverlayPosition::Left);
        assert_eq!(
            overlay_position_for(&settings, MEETING_BAR_IDLE),
            OverlayPosition::Left
        );
        assert_eq!(
            overlay_position_for(&settings, "recording"),
            OverlayPosition::Top
        );
    }

    #[test]
    fn dropped_bar_docks_to_the_nearest_edge() {
        let area = (0.0, 0.0, 1000.0, 800.0);
        assert_eq!(
            nearest_overlay_edge(area, 500.0, 790.0),
            OverlayPosition::Bottom
        );
        assert_eq!(
            nearest_overlay_edge(area, 500.0, 12.0),
            OverlayPosition::Top
        );
        assert_eq!(
            nearest_overlay_edge(area, 9.0, 400.0),
            OverlayPosition::Left
        );
        assert_eq!(
            nearest_overlay_edge(area, 990.0, 400.0),
            OverlayPosition::Right
        );
        // Ties prefer the resting dock (bottom first), like the bar's default.
        assert_eq!(
            nearest_overlay_edge(area, 500.0, 400.0),
            OverlayPosition::Bottom
        );
    }

    #[test]
    fn monitor_hit_test_uses_half_open_physical_bounds() {
        let position = PhysicalPosition::new(-2560, -200);
        let size = PhysicalSize::new(2560, 1440);

        assert!(is_mouse_within_monitor((-2560, -200), &position, &size));
        assert!(is_mouse_within_monitor((-1, 1239), &position, &size));
        assert!(!is_mouse_within_monitor((0, 0), &position, &size));
        assert!(!is_mouse_within_monitor((-1, 1240), &position, &size));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_cursor_hit_test_does_not_scale_physical_monitor_bounds() {
        let position = PhysicalPosition::new(1920, 0);
        let size = PhysicalSize::new(3840, 2160);
        let cursor = (5000, 1000);

        assert!(is_mouse_within_monitor(cursor, &position, &size));

        // This is the old mixed-coordinate comparison. It excludes a cursor
        // that is visibly inside a secondary display running at 150%.
        let scale = 1.5;
        let logical_position = PhysicalPosition::new(
            (position.x as f64 / scale) as i32,
            (position.y as f64 / scale) as i32,
        );
        let logical_size = PhysicalSize::new(
            (size.width as f64 / scale) as u32,
            (size.height as f64 / scale) as u32,
        );
        assert!(!is_mouse_within_monitor(
            cursor,
            &logical_position,
            &logical_size
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_overlay_bounds_use_destination_monitor_scale() {
        let monitor_position = PhysicalPosition::new(1920, 0);
        let monitor_size = PhysicalSize::new(3840, 2160);

        assert_eq!(
            windows_overlay_bounds(
                monitor_position,
                monitor_size,
                1.5,
                1.0,
                OVERLAY_WIDTH,
                OVERLAY_HEIGHT,
                OverlayPosition::Bottom,
            ),
            (3648, 2025, 384, 75)
        );
        assert_eq!(
            windows_overlay_bounds(
                monitor_position,
                monitor_size,
                1.5,
                1.0,
                OVERLAY_WIDTH,
                OVERLAY_HEIGHT,
                OverlayPosition::Top,
            ),
            (3648, 6, 384, 75)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_overlay_bounds_support_negative_monitor_origins() {
        assert_eq!(
            windows_overlay_bounds(
                PhysicalPosition::new(-2560, -200),
                PhysicalSize::new(2560, 1440),
                1.25,
                1.0,
                OVERLAY_STREAM_WIDTH,
                OVERLAY_STREAM_HEIGHT,
                OverlayPosition::Bottom,
            ),
            (-1530, 1040, 500, 150)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_overlay_bounds_anchor_left_and_right_at_their_edges() {
        // Same monitor as the negative-origin case: 2560x1440 at (-2560, -200),
        // 1.25 DPI, streaming footprint 400x120 logical (500x150 physical).
        assert_eq!(
            windows_overlay_bounds(
                PhysicalPosition::new(-2560, -200),
                PhysicalSize::new(2560, 1440),
                1.25,
                1.0,
                OVERLAY_STREAM_WIDTH,
                OVERLAY_STREAM_HEIGHT,
                OverlayPosition::Left,
            ),
            (-2545, 445, 500, 150)
        );
        assert_eq!(
            windows_overlay_bounds(
                PhysicalPosition::new(-2560, -200),
                PhysicalSize::new(2560, 1440),
                1.25,
                1.0,
                OVERLAY_STREAM_WIDTH,
                OVERLAY_STREAM_HEIGHT,
                OverlayPosition::Right,
            ),
            (-515, 445, 500, 150)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_overlay_bounds_grow_with_text_scale_without_moving_the_anchored_edge() {
        let monitor_position = PhysicalPosition::new(-2560, -200);
        let monitor_size = PhysicalSize::new(2560, 1440);

        let (x, y, width, height) = windows_overlay_bounds(
            monitor_position,
            monitor_size,
            1.25,
            1.1,
            OVERLAY_STREAM_WIDTH,
            OVERLAY_STREAM_HEIGHT,
            OverlayPosition::Bottom,
        );
        // 400x120 logical at 1.25 DPI x 1.1 text, still centered horizontally.
        assert_eq!((x, y, width, height), (-1555, 1025, 550, 165));
        // Bottom edge unchanged from the 1.0 case above (1040 + 150).
        assert_eq!(y + height, 1190);

        let (_, top_y, _, _) = windows_overlay_bounds(
            monitor_position,
            monitor_size,
            1.25,
            1.1,
            OVERLAY_STREAM_WIDTH,
            OVERLAY_STREAM_HEIGHT,
            OverlayPosition::Top,
        );
        // Top offset rides the DPI scale alone, so the top edge doesn't move.
        assert_eq!(top_y, -195);
    }
}
