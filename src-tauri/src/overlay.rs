use crate::input;
use crate::managers::meeting::MeetingManager;
use crate::settings;
use crate::settings::{AppSettings, OverlayPosition, OverlayStyle};
use crate::transcription_coordinator::RecordingPhase;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::menu::{CheckMenuItem, ContextMenu, Menu, MenuItem, PredefinedMenuItem, Submenu};
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

// The overlay window (logical points; 1 CSS px = 1 pt in the webview).
//
// macOS follows Wispr's model: one fixed, transparent window per edge that is
// never resized between states. The webview CSS-anchors the pill inside it
// (14 px from the edge) and reports the rects that take clicks
// (`set_bar_hit_rects`); a cursor poll keeps the rest click-through. That is
// what lets the pill morph in 100 ms, grow side buttons and show tooltips and
// cards without the window resizing under the cursor.
//
// Windows and Linux have no such poll (and layer-shell surfaces cannot use
// one), so their window stays an exact fit for each state's visible content.
// On Windows those sizes are additionally multiplied by the accessibility text
// scale (see windows_text_scale_factor), which WebView2 applies as a zoom.
const FIXED_BAR_WINDOW: bool = cfg!(target_os = "macos");
/// The fixed window at the bottom/top edge (Wispr's status window).
const BAR_WINDOW_HORIZONTAL: (f64, f64) = (440.0, 320.0);
/// The fixed window at the left/right edge.
const BAR_WINDOW_VERTICAL: (f64, f64) = (490.0, 440.0);

// Exact-fit sizes (Windows/Linux). Each must hold the largest pill of its
// state (src/overlay/pillGeometry.ts): 102.5x30 hands-free, 98x30 processing.
const OVERLAY_WIDTH: f64 = 256.0;
const OVERLAY_HEIGHT: f64 = 50.0;
/// Live: the text card above the pill.
const OVERLAY_STREAM_WIDTH: f64 = 400.0;
const OVERLAY_STREAM_HEIGHT: f64 = 120.0;
/// The 91x30 failure pill with its notification card above it.
const OVERLAY_ERROR_WIDTH: f64 = 360.0;
const OVERLAY_ERROR_HEIGHT: f64 = 130.0;
/// Resting bar: 40x8 at the bottom/top, 8x40 at the sides.
const BAR_REST_LONG: f64 = 40.0;
const BAR_REST_SHORT: f64 = 8.0;
/// Hovered bar ("ready"): the 50x30 pill centred between up to two 30 px
/// circles per side, 2 px apart.
const BAR_READY_LONG: f64 = 180.0;
const BAR_READY_SHORT: f64 = 30.0;
/// Meeting pill: a fit-content 30 px capsule (wave + stop, ~69 px) plus slack.
const MEETING_PILL_WIDTH: f64 = 120.0;
const MEETING_PILL_HEIGHT: f64 = 30.0;
/// Width the meeting pill's window grows by while the "still going?" prompt
/// shows beside it (S-24): the prompt card plus the gap to the pill.
const MEETING_NUDGE_WIDTH: f64 = 226.0;

/// The resting Flow Bar: the bar between dictations, which a hover turns into
/// the "ready" pill. It is the same view whatever the meeting state.
pub const BAR_RESTING: &str = "bar-resting";
/// The old name of the resting view; the frontend accepts both for one
/// release.
pub const MEETING_BAR_IDLE: &str = "meeting-idle";
/// The bar hidden at rest (Hide for 1 hour, or "Show at all times" off). Only
/// shown while the hide's undo card is up; otherwise the window is hidden.
pub const BAR_HIDDEN: &str = "bar-hidden";
pub const MEETING_BAR_RECORDING: &str = "meeting-recording";
/// The recording pill with the "still going?" prompt beside it: the same
/// view as `MEETING_BAR_RECORDING`, one prompt-card wider. Frontend-driven —
/// the webview knows the session's clock and asks for it via
/// `set_meeting_bar_nudge` — so the backend only owns the footprint.
pub const MEETING_BAR_NUDGE: &str = "meeting-recording-nudge";

fn is_bar_resting(state: &str) -> bool {
    state == BAR_RESTING || state == MEETING_BAR_IDLE
}

fn is_side_edge(position: OverlayPosition) -> bool {
    matches!(position, OverlayPosition::Left | OverlayPosition::Right)
}

/// The fixed window's size at `position`.
fn bar_window_size(position: OverlayPosition) -> (f64, f64) {
    if is_side_edge(position) {
        BAR_WINDOW_VERTICAL
    } else {
        BAR_WINDOW_HORIZONTAL
    }
}

/// Overlay window size (logical) for a given UI state at an edge.
fn overlay_dimensions(state: &str, position: OverlayPosition) -> (f64, f64) {
    if FIXED_BAR_WINDOW {
        return bar_window_size(position);
    }
    exact_fit_dimensions(state, position)
}

/// The exact-fit window (Windows/Linux) for `state` at `position`.
fn exact_fit_dimensions(state: &str, position: OverlayPosition) -> (f64, f64) {
    let side = is_side_edge(position);
    let along_edge = |long: f64, short: f64| if side { (short, long) } else { (long, short) };
    match state {
        "streaming" if side => (OVERLAY_STREAM_WIDTH + 40.0, OVERLAY_STREAM_HEIGHT + 20.0),
        "streaming" => (OVERLAY_STREAM_WIDTH, OVERLAY_STREAM_HEIGHT),
        "error" => (OVERLAY_ERROR_WIDTH, OVERLAY_ERROR_HEIGHT),
        BAR_HIDDEN => along_edge(BAR_REST_LONG, BAR_REST_SHORT),
        s if is_bar_resting(s) => along_edge(BAR_REST_LONG, BAR_REST_SHORT),
        // The recording pill and the frontend's "Saving" pill share the
        // meeting bar's own exact footprint.
        MEETING_BAR_RECORDING => (MEETING_PILL_WIDTH, MEETING_PILL_HEIGHT),
        MEETING_BAR_NUDGE => (
            MEETING_PILL_WIDTH + MEETING_NUDGE_WIDTH,
            MEETING_PILL_HEIGHT,
        ),
        _ if side => (OVERLAY_HEIGHT, 140.0),
        _ => (OVERLAY_WIDTH, OVERLAY_HEIGHT),
    }
}

/// The hovered ("ready") bar's exact-fit window (Windows/Linux).
fn bar_ready_dimensions(position: OverlayPosition) -> (f64, f64) {
    if is_side_edge(position) {
        (BAR_READY_SHORT, BAR_READY_LONG)
    } else {
        (BAR_READY_LONG, BAR_READY_SHORT)
    }
}

/// Whether `state` is one of the dictation overlay's own states. The meeting
/// bar's states are everything else.
fn is_dictation_state(state: &str) -> bool {
    matches!(
        state,
        "recording" | "streaming" | "transcribing" | "processing" | "command" | "error"
    )
}

/// Keep the overlay out of screenshots and screen shares only while a meeting
/// records (Wispr: "excluded only while a meeting is recording").
fn capture_protected(state: &str, meeting_recording: bool) -> bool {
    meeting_recording || state == MEETING_BAR_RECORDING || state == MEETING_BAR_NUDGE
}

static LAST_MIC_LEVEL_EMIT: AtomicU64 = AtomicU64::new(0);
const EMIT_THROTTLE_MS: u64 = 33; // ~30 FPS

// macOS measures every offset from `NSScreen.visibleFrame` (Tauri's
// work_area), so the bar clears the Dock and the menu bar without hardcoding
// their heights. The other platforms keep full monitor bounds with fixed
// offsets (work_area is unreliable on Wayland; Windows' bottom offset clears
// the taskbar).
//
// On macOS the fixed window sits flush with the work area; the pill's own
// distance from the edge (14 px, 12 px at the top) lives in the overlay CSS.
#[cfg(target_os = "macos")]
const OVERLAY_TOP_OFFSET: f64 = 0.0;
#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_TOP_OFFSET: f64 = 4.0;

#[cfg(target_os = "macos")]
const OVERLAY_BOTTOM_OFFSET: f64 = 0.0;

#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_BOTTOM_OFFSET: f64 = 40.0;

// Left / Right placement: the gap from the chosen side's edge.
#[cfg(target_os = "macos")]
const OVERLAY_SIDE_OFFSET: f64 = 0.0;
#[cfg(any(target_os = "windows", target_os = "linux"))]
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
/// user's `overlay_position` setting; the resting bar follows it too unless
/// a drag pinned the bar's own edge (`meeting_bar_position`, Wispr's
/// `statusDockEdge`). Wispr has one bar on one edge, so both default to the
/// bottom, and a drag now writes both settings (see `dock_meeting_bar_on_main`);
/// a store that pinned them apart before keeps working.
fn overlay_position_for(settings: &AppSettings, state: &str) -> OverlayPosition {
    if is_dictation_state(state) {
        settings.overlay_position
    } else {
        settings
            .meeting_bar_position
            .unwrap_or(settings.overlay_position)
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
    if FIXED_BAR_WINDOW && (width, height) == bar_window_size(position) {
        let (x, y, _, _) = bar_window_frame(monitor_dock_area(monitor), position);
        return (x, y);
    }
    origin_in_area(
        monitor_dock_area(monitor),
        (width, height),
        position,
        (
            OVERLAY_TOP_OFFSET,
            OVERLAY_BOTTOM_OFFSET,
            OVERLAY_SIDE_OFFSET,
        ),
    )
}

/// Where a `size` window docks in `area` (x, y, w, h) at `position`, with
/// (top, bottom, side) offsets from the area's edges. Centred on the other
/// axis.
fn origin_in_area(
    area: (f64, f64, f64, f64),
    size: (f64, f64),
    position: OverlayPosition,
    offsets: (f64, f64, f64),
) -> (f64, f64) {
    let (area_x, area_y, area_w, area_h) = area;
    let (width, height) = size;
    let (top, bottom, side) = offsets;
    match position {
        OverlayPosition::Top => (area_x + (area_w - width) / 2.0, area_y + top),
        OverlayPosition::Bottom => (
            area_x + (area_w - width) / 2.0,
            area_y + area_h - height - bottom,
        ),
        OverlayPosition::Left => (area_x + side, area_y + (area_h - height) / 2.0),
        OverlayPosition::Right => (
            area_x + area_w - width - side,
            area_y + (area_h - height) / 2.0,
        ),
    }
}

/// The fixed bar window's frame (x, y, w, h) in a work area: 440x320 centred
/// with its bottom on the work-area bottom, or 490x440 vertically centred and
/// flush with a side edge. The pill's 14 px margin lives in the CSS.
fn bar_window_frame(area: (f64, f64, f64, f64), edge: OverlayPosition) -> (f64, f64, f64, f64) {
    let size = bar_window_size(edge);
    let (x, y) = origin_in_area(area, size, edge, (0.0, 0.0, 0.0));
    (x, y, size.0, size.1)
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
/// closest to. Like Wispr's drop zones, a drag offers bottom, left and right;
/// the top edge stays a FlowState extra reachable from Settings.
fn nearest_overlay_edge(area: (f64, f64, f64, f64), x: f64, y: f64) -> OverlayPosition {
    let (area_x, area_y, area_w, area_h) = area;
    [
        (OverlayPosition::Bottom, area_y + area_h - y),
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
    let position = overlay_position_for(&settings::get_settings(app_handle), state);
    apply_overlay_geometry_sized(
        app_handle,
        overlay_window,
        state,
        overlay_dimensions(state, position),
    );
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
            // The hit poll maps the webview's rects to the screen through
            // this origin; it is recorded here, on the thread that may talk
            // to the window server.
            remember_window_origin(app_handle, x, y);
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
    let (width, height) = overlay_dimensions(
        state,
        overlay_position_for(&settings::get_settings(app_handle), state),
    );
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

        // No window shadow in any state: Wispr's bar is flat (the panel is
        // created with `has_shadow(false)`).

        // Out of screenshots and screen shares only while a meeting records.
        let _ = overlay_window
            .set_content_protected(capture_protected(state, is_meeting_recording(app_handle)));

        // The fixed window is click-through except where the webview reports
        // a hit rect; the poll flips that as the cursor moves. Start every
        // view click-through so the transparent window never swallows a
        // click before the poll has looked.
        #[cfg(target_os = "macos")]
        {
            let _ = overlay_window.set_ignore_cursor_events(true);
            start_bar_hit_poll(app_handle);
        }

        let _ = overlay_window.emit("show-overlay", state);

        // The failure pill dismisses itself after four seconds — long enough
        // to read one short line. The generation is captured at this show, so
        // a dictation that starts meanwhile bumps it and wins the window.
        if state == "error" {
            let shown_at = OVERLAY_SHOW_GENERATION.load(Ordering::SeqCst);
            let handle = app_handle.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(4200));
                if OVERLAY_SHOW_GENERATION.load(Ordering::SeqCst) == shown_at {
                    hide_recording_overlay(&handle);
                }
            });
        }
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

/// Which listening pill a recording phase asks for: hold-to-talk while the
/// key is held, hands-free once the session outlives its key (a tap, toggle
/// mode, or a click on the bar). Other phases leave the pill as it is.
fn listening_mode_for_phase(phase: RecordingPhase) -> Option<&'static str> {
    match phase {
        RecordingPhase::Started => Some("ptt"),
        RecordingPhase::Locked => Some("handsfree"),
        RecordingPhase::Stopped | RecordingPhase::Cancelled => None,
    }
}

/// Tell the overlay whether this take is hold-to-talk or hands-free
/// (`overlay-mode`). Queued on the main thread behind the `show-overlay`
/// that started the take, so the show's reset can never overwrite it.
pub fn emit_listening_mode(app_handle: &AppHandle, phase: RecordingPhase) {
    let Some(mode) = listening_mode_for_phase(phase) else {
        return;
    };
    if !OVERLAY_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        let _ = handle.emit_to("recording_overlay", "overlay-mode", mode);
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

/// Failures the dictation overlay is allowed to speak out loud. Cosmetic ones
/// stay silent (the reference collapses a failed polish back to resting), but a
/// take that never started has to say so: otherwise pressing the shortcut with
/// no model selected looks like the app ignoring you.
pub const ERROR_NO_MODEL: &str = "no_model";
pub const ERROR_MODEL_FAILED: &str = "model_failed";
pub const ERROR_START_FAILED: &str = "start_failed";
/// Command Mode: applying the spoken instruction to the selected text failed
/// (no provider, LLM error, empty answer). Nothing is pasted.
pub const ERROR_COMMAND_FAILED: &str = "command_failed";

/// Show a short failure pill in the dictation overlay and fade it out again.
/// The self-dismiss timer lives in `show_overlay_state_on_main`, where the
/// generation of this show is known, so it can never hide a dictation that
/// started meanwhile.
pub fn show_error_overlay(app_handle: &AppHandle, code: &str) {
    show_overlay_state(app_handle, "error");
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        let _ = overlay_window.emit("overlay-error", code);
    }
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

/// Show the bar view that matches the session right now: the meeting pill
/// while a meeting records, the resting bar otherwise. Called when the app
/// starts, whenever the session's stage changes, and when a dictation view
/// gives the window back — so the bar is effectively always present.
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

    // Hidden at rest: right-click > "Hide for 1 hour", or "Show the bar at
    // all times" turned off. Dictation still shows. While the hide's undo
    // card is up the window stays (macOS, where the window has room for it);
    // after that the window itself goes away — there is nothing else to see.
    let show_at_all_times = settings::get_settings(app_handle).show_bar_at_all_times;
    if !resting_bar_visible(meeting_bar_hidden(), show_at_all_times) {
        if FIXED_BAR_WINDOW && hide_toast_pending() {
            show_meeting_bar_state(app_handle, BAR_HIDDEN);
        } else {
            hide_meeting_bar_window(app_handle);
        }
        return;
    }

    show_meeting_bar_state(app_handle, BAR_RESTING);
}

/// Whether the resting bar shows: not while hidden for the hour, and not
/// when the user turned "show at all times" off.
fn resting_bar_visible(hidden_for_hour: bool, show_at_all_times: bool) -> bool {
    show_at_all_times && !hidden_for_hour
}

/// Whether a meeting records right now.
fn is_meeting_recording(app_handle: &AppHandle) -> bool {
    app_handle
        .try_state::<Arc<MeetingManager>>()
        .map(|meetings| meetings.is_recording())
        .unwrap_or(false)
}

// ============================================================
// The bar: drag-to-edge, right-click menu, hide for an hour, hit shape
//
// The bar (resting or meeting pill) can be dragged to another screen edge
// like Wispr's statusDockEdge; the resting bar offers its menu on
// right-click; and the window never swallows clicks outside the pill.
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

/// The resting bar can be dismissed for a while: it stays gone until the
/// hour passes, the app restarts, the hide is undone, or a meeting is started
/// from Settings. Deliberately in-memory — a restart always brings it back.
static MEETING_BAR_HIDDEN_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);

/// While the "hidden for an hour" card with its Undo is on screen.
static HIDE_TOAST_UNTIL: Mutex<Option<Instant>> = Mutex::new(None);
/// How long the hide's undo card stays up (Wispr: a few seconds).
const HIDE_TOAST_DURATION: Duration = Duration::from_secs(5);

/// While the bar's native context menu is up. The menu runs its own event
/// loop; the hit-shape poll leaves the window's input state alone until it
/// closes.
static MEETING_BAR_MENU_OPEN: AtomicBool = AtomicBool::new(false);

/// The right-click menu's "hide for an hour" item, handled by the app-wide
/// menu-event handler in lib.rs.
pub const MEETING_BAR_HIDE_FOR_HOUR_ID: &str = "meeting_bar_hide_for_hour";

/// FlowState extras in the bar's menu: start a meeting, a new note, the
/// scratchpad.
pub const MEETING_BAR_START_ID: &str = "meeting_bar_start";
pub const MEETING_BAR_NOTE_ID: &str = "meeting_bar_note";
/// The menu item that opens the Scratchpad (quick note) window.
pub const MEETING_BAR_SCRATCHPAD_ID: &str = "meeting_bar_scratchpad";
/// Wispr's menu items. Languages, Microphone and Style are submenus whose
/// entries carry the prefixes below; lib.rs handles History and Settings
/// (they open the main window), `handle_bar_menu_event` the rest.
pub const BAR_MENU_LANGUAGES_ID: &str = "bar_languages";
pub const BAR_MENU_MICROPHONE_ID: &str = "bar_microphone";
pub const BAR_MENU_PASTE_LAST_ID: &str = "bar_paste_last";
pub const BAR_MENU_HISTORY_ID: &str = "bar_history";
pub const BAR_MENU_SETTINGS_ID: &str = "bar_settings";
pub const BAR_MENU_STYLE_ID: &str = "bar_style";
const BAR_LANGUAGE_PREFIX: &str = "bar_lang:";
const BAR_MICROPHONE_PREFIX: &str = "bar_mic:";
const BAR_STYLE_PREFIX: &str = "bar_style:";

/// The bar's right-click menu, top to bottom: Wispr's items first, then the
/// FlowState extras. Empty outside the resting bar: never during dictation,
/// and the meeting pill is a live session's UI.
fn bar_menu_items(state: &str, meeting_recording: bool) -> Vec<&'static str> {
    if !is_bar_resting(state) || meeting_recording {
        return Vec::new();
    }
    vec![
        MEETING_BAR_HIDE_FOR_HOUR_ID,
        BAR_MENU_LANGUAGES_ID,
        BAR_MENU_MICROPHONE_ID,
        BAR_MENU_PASTE_LAST_ID,
        BAR_MENU_HISTORY_ID,
        BAR_MENU_SETTINGS_ID,
        MEETING_BAR_START_ID,
        MEETING_BAR_NOTE_ID,
        MEETING_BAR_SCRATCHPAD_ID,
        BAR_MENU_STYLE_ID,
    ]
}

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

/// Begin a native drag of the bar. The window-server drag carries the window
/// with the pointer across the whole screen, and the dock watcher snaps it to
/// the nearest edge once it settles. A drag never touches the session: the
/// frontend only hands over once the pointer has crossed its drag threshold,
/// so a click still clicks and a drag drags.
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

/// Dock the bar after its window has been still: snap it to the nearest edge
/// of the monitor it was dropped on, save that edge, and tell the webview to
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
    // where it began: nothing to move, and nothing to pin. A settings edge
    // the drag does not offer (Top) counts as docked when the window sits
    // there.
    let dropped_x = position.x as f64 / scale;
    let dropped_y = position.y as f64 / scale;
    let width = size.width as f64 / scale;
    let height = size.height as f64 / scale;
    let current = overlay_position_for(&settings::get_settings(app_handle), state);
    let docked_at = |edge: OverlayPosition| {
        let (x, y) = overlay_position_on_monitor(&monitor, width, height, edge);
        (dropped_x - x).abs() <= 3.0 && (dropped_y - y).abs() <= 3.0
    };
    if docked_at(edge) || docked_at(current) {
        return;
    }

    // One bar, one edge (Wispr): the drag moves the dictation pill too.
    let mut settings = settings::get_settings(app_handle);
    settings.meeting_bar_position = Some(edge);
    settings.overlay_position = edge;
    settings::write_settings(app_handle, settings);
    log::debug!("bar docked to the {edge:?} edge");

    apply_overlay_geometry_on_main(app_handle, &window, state);
    let _ = window.emit("overlay-position-changed", ());
}

/// Grow or shrink the meeting bar's window for the "still going?" prompt
/// (S-24): the webview owns the timing — it knows the session's clock — and
/// asks for the wider footprint while the prompt is up. Resizes in place,
/// never `show()`s or focuses. On macOS the window is fixed and already has
/// room, so only the state is recorded.
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
        if FIXED_BAR_WINDOW {
            return;
        }
        let Some(window) = handle.get_webview_window("recording_overlay") else {
            return;
        };

        // A layer-shell surface's geometry is the compositor's to commit.
        #[cfg(target_os = "linux")]
        if LAYER_SHELL_ACTIVE.load(Ordering::SeqCst) {
            let position = overlay_position_for(&settings::get_settings(&handle), target);
            let (width, height) = overlay_dimensions(target, position);
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

/// Grow or shrink the resting bar's exact-fit window into the hovered
/// ("ready") pill with its side buttons (Windows/Linux). macOS has the fixed
/// window and a backend hover poll, so this is a no-op there.
pub fn set_meeting_bar_expanded(app_handle: &AppHandle, expanded: bool) {
    if FIXED_BAR_WINDOW {
        return;
    }
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        let state = CURRENT_OVERLAY_STATE.lock().unwrap().clone();
        if !is_bar_resting(&state) {
            return;
        }
        let Some(window) = handle.get_webview_window("recording_overlay") else {
            return;
        };
        let position = overlay_position_for(&settings::get_settings(&handle), &state);
        let size = if expanded {
            bar_ready_dimensions(position)
        } else {
            overlay_dimensions(&state, position)
        };
        apply_overlay_geometry_sized(&handle, &window, &state, size);
    });
}

/// The microphones the bar's menu offers, by name. Device enumeration can
/// stall, so callers run this off the main thread.
pub fn list_bar_microphones() -> Vec<String> {
    match handy_recorder::list_input_devices() {
        Ok(devices) => devices
            .into_iter()
            .filter(|device| !device.is_monitor)
            .map(|device| device.name)
            .collect(),
        Err(error) => {
            log::warn!("Failed to list microphones for the bar menu: {error}");
            Vec::new()
        }
    }
}

/// A tray string, falling back to English where a locale has not translated
/// it yet (the same contract the tray's secure-input item has).
fn tray_label(
    strings: &crate::tray_i18n::TrayStrings,
    english: &crate::tray_i18n::TrayStrings,
    pick: fn(&crate::tray_i18n::TrayStrings) -> &String,
) -> String {
    let value = pick(strings);
    if value.is_empty() {
        pick(english).clone()
    } else {
        value.clone()
    }
}

/// The resting bar's right-click menu (Wispr's escape hatch plus the
/// FlowState extras). Native, so it works in the exact-fit windows too. Pops
/// at the cursor on the main thread; items are dispatched by the app-wide
/// menu-event handler. `language_labels` are the webview's localized names
/// for the language codes; `microphones` comes from `list_bar_microphones`.
pub fn show_meeting_bar_menu(
    app_handle: &AppHandle,
    language_labels: Vec<(String, String)>,
    microphones: Vec<String>,
) {
    let state = CURRENT_OVERLAY_STATE.lock().unwrap().clone();
    let items = bar_menu_items(&state, is_meeting_recording(app_handle));
    if items.is_empty() {
        return;
    }

    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        let settings = settings::get_settings(&handle);
        let strings = crate::tray_i18n::get_tray_translations(Some(settings.app_language.clone()));
        let english = crate::tray_i18n::get_tray_translations(Some("en".to_string()));
        let label = |pick: fn(&crate::tray_i18n::TrayStrings) -> &String| {
            tray_label(&strings, &english, pick)
        };

        let build = || -> Result<Menu<tauri::Wry>, tauri::Error> {
            let menu = Menu::new(&handle)?;
            for (index, id) in items.iter().enumerate() {
                // The FlowState extras sit under a separator.
                if *id == MEETING_BAR_START_ID && index > 0 {
                    menu.append(&PredefinedMenuItem::separator(&handle)?)?;
                }
                match *id {
                    BAR_MENU_LANGUAGES_ID => {
                        let submenu = Submenu::with_id(
                            &handle,
                            BAR_MENU_LANGUAGES_ID,
                            label(|s| &s.bar_languages),
                            true,
                        )?;
                        let mut codes = vec!["auto".to_string()];
                        for code in &settings.spoken_languages {
                            if code != "auto" && !codes.contains(code) {
                                codes.push(code.clone());
                            }
                        }
                        if !codes.contains(&settings.selected_language) {
                            codes.push(settings.selected_language.clone());
                        }
                        for code in codes {
                            let text = if code == "auto" {
                                label(|s| &s.bar_language_auto)
                            } else {
                                language_labels
                                    .iter()
                                    .find(|(known, _)| *known == code)
                                    .map(|(_, name)| name.clone())
                                    .unwrap_or_else(|| code.to_uppercase())
                            };
                            submenu.append(&CheckMenuItem::with_id(
                                &handle,
                                format!("{BAR_LANGUAGE_PREFIX}{code}"),
                                text,
                                true,
                                settings.selected_language == code,
                                None::<&str>,
                            )?)?;
                        }
                        menu.append(&submenu)?;
                    }
                    BAR_MENU_MICROPHONE_ID => {
                        let submenu = Submenu::with_id(
                            &handle,
                            BAR_MENU_MICROPHONE_ID,
                            label(|s| &s.bar_microphone),
                            true,
                        )?;
                        let selected = settings.selected_microphone.clone();
                        submenu.append(&CheckMenuItem::with_id(
                            &handle,
                            format!("{BAR_MICROPHONE_PREFIX}default"),
                            label(|s| &s.bar_microphone_default),
                            true,
                            selected.is_none(),
                            None::<&str>,
                        )?)?;
                        for name in &microphones {
                            submenu.append(&CheckMenuItem::with_id(
                                &handle,
                                format!("{BAR_MICROPHONE_PREFIX}{name}"),
                                name,
                                true,
                                selected.as_deref() == Some(name.as_str()),
                                None::<&str>,
                            )?)?;
                        }
                        menu.append(&submenu)?;
                    }
                    BAR_MENU_STYLE_ID => {
                        // Style switching moved here from the dictation-time
                        // menu; only offered when post-processing runs.
                        if !settings.post_process_enabled
                            || settings.post_process_prompts.is_empty()
                        {
                            continue;
                        }
                        let submenu = Submenu::with_id(
                            &handle,
                            BAR_MENU_STYLE_ID,
                            label(|s| &s.bar_style),
                            true,
                        )?;
                        for prompt in &settings.post_process_prompts {
                            submenu.append(&CheckMenuItem::with_id(
                                &handle,
                                format!("{BAR_STYLE_PREFIX}{}", prompt.id),
                                &prompt.name,
                                true,
                                settings.post_process_selected_prompt_id.as_deref()
                                    == Some(prompt.id.as_str()),
                                None::<&str>,
                            )?)?;
                        }
                        menu.append(&submenu)?;
                    }
                    other => {
                        let text = match other {
                            MEETING_BAR_HIDE_FOR_HOUR_ID => label(|s| &s.meeting_bar_hide_for_hour),
                            BAR_MENU_PASTE_LAST_ID => label(|s| &s.paste_last_transcript),
                            BAR_MENU_HISTORY_ID => label(|s| &s.bar_history),
                            BAR_MENU_SETTINGS_ID => label(|s| &s.settings),
                            MEETING_BAR_START_ID => label(|s| &s.meeting_bar_start),
                            MEETING_BAR_NOTE_ID => label(|s| &s.meeting_bar_note),
                            _ => label(|s| &s.scratchpad_open),
                        };
                        menu.append(&MenuItem::with_id(
                            &handle,
                            other,
                            text,
                            true,
                            None::<&str>,
                        )?)?;
                    }
                }
            }
            Ok(menu)
        };
        let menu = match build() {
            Ok(menu) => menu,
            Err(error) => {
                log::warn!("Failed to build the bar menu: {error}");
                return;
            }
        };
        let Some(overlay_window) = handle.get_webview_window("recording_overlay") else {
            return;
        };
        // The popup anchors to the hosting window (the panel); the manager's
        // `get_window` is unstable-only, so reach it through the webview window.
        let window = overlay_window.as_ref().window();

        MEETING_BAR_MENU_OPEN.store(true, Ordering::SeqCst);
        // The menu takes the clicks it needs; the fixed window must accept
        // them while it is up.
        #[cfg(target_os = "macos")]
        let _ = overlay_window.set_ignore_cursor_events(false);
        if let Err(error) = menu.popup(window) {
            log::warn!("Failed to open the bar menu: {error}");
        }
        MEETING_BAR_MENU_OPEN.store(false, Ordering::SeqCst);
    });
}

/// Handle the bar menu's own items (language, microphone, style, paste
/// last). Returns whether `id` was one of them; History and Settings open the
/// main window and are handled in lib.rs.
pub fn handle_bar_menu_event(app_handle: &AppHandle, id: &str) -> bool {
    if let Some(code) = id.strip_prefix(BAR_LANGUAGE_PREFIX) {
        if let Err(error) =
            crate::shortcut::change_selected_language_setting(app_handle.clone(), code.to_string())
        {
            log::warn!("Failed to switch the dictation language: {error}");
        }
        let _ = app_handle.emit(
            "settings-changed",
            serde_json::json!({ "setting": "selected_language", "value": code }),
        );
        return true;
    }
    if let Some(name) = id.strip_prefix(BAR_MICROPHONE_PREFIX) {
        let mut settings = settings::get_settings(app_handle);
        settings.selected_microphone = (name != "default").then(|| name.to_string());
        settings::write_settings(app_handle, settings);
        // Restarting the stream can block on CoreAudio: never on the main
        // thread.
        if let Some(recorder) =
            app_handle.try_state::<Arc<crate::managers::audio::AudioRecordingManager>>()
        {
            let recorder = recorder.inner().clone();
            std::thread::spawn(move || {
                if let Err(error) = recorder.update_selected_device() {
                    log::warn!("Failed to switch the microphone: {error}");
                }
            });
        }
        let _ = app_handle.emit(
            "settings-changed",
            serde_json::json!({ "setting": "selected_microphone", "value": name }),
        );
        return true;
    }
    if let Some(style_id) = id.strip_prefix(BAR_STYLE_PREFIX) {
        if let Err(error) = crate::shortcut::set_post_process_selected_prompt(
            app_handle.clone(),
            style_id.to_string(),
        ) {
            log::warn!("Failed to switch the dictation style: {error}");
        }
        let _ = app_handle.emit(
            "settings-changed",
            serde_json::json!({ "setting": "post_process_selected_prompt_id", "value": style_id }),
        );
        return true;
    }
    if id == BAR_MENU_PASTE_LAST_ID {
        crate::tray::paste_last_transcript(app_handle);
        return true;
    }
    false
}

/// Hide the resting bar for a while (right-click > "Hide for 1 hour"). It
/// comes back when the hour passes, when the app restarts, when the hide is
/// undone, or when a meeting is started from Settings. The meeting pill is
/// never hideable. On macOS an undo card shows for a few seconds first.
pub fn hide_meeting_bar_for(app_handle: &AppHandle, secs: u64) {
    if is_meeting_recording(app_handle) {
        return;
    }
    let Some(deadline) = Instant::now().checked_add(Duration::from_secs(secs)) else {
        return;
    };
    *MEETING_BAR_HIDDEN_UNTIL.lock().unwrap() = Some(deadline);
    *HIDE_TOAST_UNTIL.lock().unwrap() = Instant::now().checked_add(HIDE_TOAST_DURATION);

    // Show the undo card (or hide the window) now if the bar is resting; a
    // dictation view keeps the window — the flag is re-checked when the
    // dictation hands the window back.
    refresh_meeting_bar(app_handle);

    // The card's end: take the window away unless the hide was undone.
    let toast_handle = app_handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(HIDE_TOAST_DURATION + Duration::from_millis(200));
        *HIDE_TOAST_UNTIL.lock().unwrap() = None;
        if *CURRENT_OVERLAY_STATE.lock().unwrap() == BAR_HIDDEN {
            refresh_meeting_bar(&toast_handle);
        }
    });

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

/// Bring the bar back early: starting a meeting from Settings, undoing the
/// hide, or turning "show at all times" back on ends the hour-long hide.
/// Callers refresh the bar themselves.
pub fn unhide_meeting_bar() {
    *MEETING_BAR_HIDDEN_UNTIL.lock().unwrap() = None;
    *HIDE_TOAST_UNTIL.lock().unwrap() = None;
}

/// The undo card's action: end the hide and bring the resting bar back.
pub fn undo_hide_meeting_bar(app_handle: &AppHandle) {
    unhide_meeting_bar();
    refresh_meeting_bar(app_handle);
}

fn meeting_bar_hidden() -> bool {
    MEETING_BAR_HIDDEN_UNTIL
        .lock()
        .unwrap()
        .is_some_and(|until| until > Instant::now())
}

fn hide_toast_pending() -> bool {
    HIDE_TOAST_UNTIL
        .lock()
        .unwrap()
        .is_some_and(|until| until > Instant::now())
}

/// Take the overlay window off screen while the bar is hidden. The window is
/// the bar's whole UI; hiding it hides the bar (and frees the space it sat in).
fn hide_meeting_bar_window(app_handle: &AppHandle) {
    let handle = app_handle.clone();
    let _ = app_handle.run_on_main_thread(move || {
        *CURRENT_OVERLAY_STATE.lock().unwrap() = BAR_HIDDEN.to_string();
        if let Some(window) = handle.get_webview_window("recording_overlay") {
            #[cfg(target_os = "macos")]
            let _ = window.set_ignore_cursor_events(false);
            let _ = window.hide();
        }
    });
}

// ---- Hit shape and hover (the fixed window) ----

/// A rect of the webview that takes clicks (the pill, its side buttons, a
/// card, an open menu), in window-local logical points.
#[derive(Clone, Copy, Debug, PartialEq, serde::Deserialize, specta::Type)]
pub struct BarHitRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The rects the webview last reported (`set_bar_hit_rects`).
static BAR_HIT_RECTS: Mutex<Vec<BarHitRect>> = Mutex::new(Vec::new());

/// Record the webview's hit rects; the poll reads them on its next tick.
pub fn set_bar_hit_rects(rects: Vec<BarHitRect>) {
    *BAR_HIT_RECTS.lock().unwrap() = rects
        .into_iter()
        .filter(|rect| {
            rect.width.is_finite()
                && rect.height.is_finite()
                && rect.width > 0.0
                && rect.height > 0.0
        })
        .collect();
}

/// How close the pointer must come to the resting bar before it opens into
/// the "ready" pill, and how far past the ready pill and its side buttons it
/// may go before it closes (Wispr: hot-area padding 6/10 px plus 12 px).
const BAR_HOVER_ZONE: f64 = 12.0;
/// Slack on every hit rect, so a scaled display's rounding never leaves the
/// pill's own edge unclickable.
#[cfg(target_os = "macos")]
const BAR_HIT_MARGIN: f64 = 1.0;

/// Distance from a point to a rect (0 inside).
fn rect_distance(rect: &BarHitRect, point: (f64, f64)) -> f64 {
    let (x, y) = point;
    let dx = (rect.x - x).max(x - (rect.x + rect.width)).max(0.0);
    let dy = (rect.y - y).max(y - (rect.y + rect.height)).max(0.0);
    dx.hypot(dy)
}

/// Whether `point` lies within `margin` of any rect.
fn rect_hit(rects: &[BarHitRect], point: (f64, f64), margin: f64) -> bool {
    rects
        .iter()
        .any(|rect| rect_distance(rect, point) <= margin)
}

/// Whether the pointer is in the bar's hover zone: within 12 px of the
/// resting bar (to open) or of the ready pill and its side buttons (to stay
/// open). The webview reports whichever is showing, so one rule does both.
fn bar_hover_zone(rects: &[BarHitRect], point: (f64, f64)) -> bool {
    rect_hit(rects, point, BAR_HOVER_ZONE)
}

#[cfg(target_os = "macos")]
const BAR_HIT_POLL_INTERVAL: Duration = Duration::from_millis(40);

/// Freshness token for the hit poll: each show starts a poll and
/// invalidates the previous one.
#[cfg(target_os = "macos")]
static BAR_HIT_POLL_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Keep the fixed window click-through everywhere except the webview's hit
/// rects, and drive the resting bar's hover from the cursor's distance to
/// them (Wispr's `setIgnoreMouseEvents(true, {forward: true})` with an input
/// shape). Runs while the window is visible; each show restarts it. When
/// the cursor or the window's origin is unknown, the window stays
/// click-through: a lost click on the pill beats a swallowed one elsewhere.
#[cfg(target_os = "macos")]
fn start_bar_hit_poll(app_handle: &AppHandle) {
    let generation = BAR_HIT_POLL_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    let handle = app_handle.clone();
    std::thread::spawn(move || {
        let mut ignored = true;
        let mut hovered = false;
        loop {
            std::thread::sleep(BAR_HIT_POLL_INTERVAL);
            if generation != BAR_HIT_POLL_GENERATION.load(Ordering::SeqCst) {
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

            let state = CURRENT_OVERLAY_STATE.lock().unwrap().clone();
            let rects = BAR_HIT_RECTS.lock().unwrap().clone();
            let local = window_local_cursor(&handle);

            let inside = local.is_some_and(|point| rect_hit(&rects, point, BAR_HIT_MARGIN));
            // Only the resting bar opens on approach; every other view keeps
            // its own shape.
            let near =
                is_bar_resting(&state) && local.is_some_and(|point| bar_hover_zone(&rects, point));
            if near != hovered {
                hovered = near;
                log::debug!("bar hover: {near}");
                let _ = handle.emit_to("recording_overlay", "meeting-bar-hover", near);
            }

            let want_ignore = !inside;
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
    });
}

/// Where the pointer is, in the same logical, top-left space the window's
/// origin is computed in. enigo answers this too, but its reading has come
/// back stale or missing after a relaunch; AppKit's is the one the window
/// server itself uses.
#[cfg(target_os = "macos")]
fn cursor_location(screen_height: f64) -> Option<(f64, f64)> {
    use objc2_app_kit::NSEvent;
    if screen_height <= 0.0 {
        return None;
    }
    // AppKit counts up from the bottom of the main screen; the window's
    // origin counts down from the top.
    let point = NSEvent::mouseLocation();
    Some((point.x, screen_height - point.y))
}

/// The cursor in the overlay window's own coordinates (the webview's CSS px).
#[cfg(target_os = "macos")]
fn window_local_cursor(_app_handle: &AppHandle) -> Option<(f64, f64)> {
    let origin = (*BAR_WINDOW_ORIGIN.lock().unwrap())?;
    let (x, y) = cursor_location(origin.screen_height)?;
    Some((x - origin.x, y - origin.y))
}

/// The overlay window's logical origin, cached on the main thread: the
/// window server cannot be queried from the hit poll's own thread.
static BAR_WINDOW_ORIGIN: Mutex<Option<WindowOrigin>> = Mutex::new(None);

#[derive(Clone, Copy)]
struct WindowOrigin {
    x: f64,
    y: f64,
    /// The main screen's logical height, for flipping AppKit's bottom-up
    /// cursor into the top-down space the origin uses.
    screen_height: f64,
}

fn remember_window_origin(app_handle: &AppHandle, x: f64, y: f64) {
    let screen_height = app_handle
        .primary_monitor()
        .ok()
        .flatten()
        .map(|monitor| monitor.size().height as f64 / monitor.scale_factor())
        .unwrap_or(0.0);
    *BAR_WINDOW_ORIGIN.lock().unwrap() = Some(WindowOrigin {
        x,
        y,
        screen_height,
    });
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
    fn exact_fit_windows_hold_each_state() {
        use OverlayPosition::*;
        // Resting bar: 40x8 at the bottom, 8x40 at the sides (Wispr).
        assert_eq!(exact_fit_dimensions(BAR_RESTING, Bottom), (40.0, 8.0));
        assert_eq!(exact_fit_dimensions(MEETING_BAR_IDLE, Right), (8.0, 40.0));
        // Hovered: room for the 50x30 pill and two 30px circles per side.
        assert_eq!(bar_ready_dimensions(Bottom), (180.0, 30.0));
        assert_eq!(bar_ready_dimensions(Left), (30.0, 180.0));
        // Dictation: the widest pill (102.5x30 hands-free) fits.
        let (w, h) = exact_fit_dimensions("recording", Bottom);
        assert!(w >= 102.5 && h >= 30.0);
        let (w, h) = exact_fit_dimensions("recording", Left);
        assert!(w >= 30.0 && h >= 102.5);
        // The prompt adds exactly its card's width to the meeting pill.
        assert_eq!(
            exact_fit_dimensions(MEETING_BAR_NUDGE, Bottom),
            (
                MEETING_PILL_WIDTH + MEETING_NUDGE_WIDTH,
                MEETING_PILL_HEIGHT
            )
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_window_is_fixed_for_every_state() {
        use OverlayPosition::*;
        for state in [
            BAR_RESTING,
            "recording",
            "streaming",
            "error",
            MEETING_BAR_RECORDING,
        ] {
            assert_eq!(overlay_dimensions(state, Bottom), (440.0, 320.0));
            assert_eq!(overlay_dimensions(state, Right), (490.0, 440.0));
        }
    }

    #[test]
    fn bar_window_frame_docks_flush_with_the_work_area() {
        // A 1512x944 work area below a 38pt menu bar.
        let area = (0.0, 38.0, 1512.0, 944.0);
        let (x, y, w, h) = bar_window_frame(area, OverlayPosition::Bottom);
        assert_eq!((w, h), (440.0, 320.0));
        assert_eq!(x, (1512.0 - 440.0) / 2.0);
        assert_eq!(y + h, 38.0 + 944.0);
        let (x, y, w, h) = bar_window_frame(area, OverlayPosition::Right);
        assert_eq!((w, h), (490.0, 440.0));
        assert_eq!(x + w, 1512.0);
        assert_eq!(y, 38.0 + (944.0 - 440.0) / 2.0);
        let (x, _, _, _) = bar_window_frame(area, OverlayPosition::Left);
        assert_eq!(x, 0.0);
    }

    #[test]
    fn hit_rects_and_hover_zone() {
        // The resting bar, centred at the bottom of the 440x320 window with
        // its bottom 14px above the window's bottom.
        let resting = [BarHitRect {
            x: 200.0,
            y: 298.0,
            width: 40.0,
            height: 8.0,
        }];
        assert!(rect_hit(&resting, (220.0, 302.0), 0.0));
        assert!(!rect_hit(&resting, (220.0, 290.0), 0.0));
        // 11px above the bar: in the hover zone; 13px: not.
        assert!(bar_hover_zone(&resting, (220.0, 298.0 - 11.0)));
        assert!(!bar_hover_zone(&resting, (220.0, 298.0 - 13.0)));
        // Sideways too, measured from the bar's edge.
        assert!(bar_hover_zone(&resting, (240.0 + 11.0, 302.0)));
        assert!(!bar_hover_zone(&resting, (240.0 + 13.0, 302.0)));
        // No rects: nothing is hit and nothing hovers.
        assert!(!rect_hit(&[], (0.0, 0.0), 100.0));
        assert!(!bar_hover_zone(&[], (0.0, 0.0)));
    }

    #[test]
    fn bar_menu_is_only_offered_at_rest() {
        let items = bar_menu_items(BAR_RESTING, false);
        // Wispr's items first, in order, then the FlowState extras.
        assert_eq!(
            &items[..6],
            &[
                MEETING_BAR_HIDE_FOR_HOUR_ID,
                BAR_MENU_LANGUAGES_ID,
                BAR_MENU_MICROPHONE_ID,
                BAR_MENU_PASTE_LAST_ID,
                BAR_MENU_HISTORY_ID,
                BAR_MENU_SETTINGS_ID,
            ]
        );
        assert!(items.contains(&MEETING_BAR_START_ID));
        assert!(items.contains(&MEETING_BAR_NOTE_ID));
        assert!(items.contains(&MEETING_BAR_SCRATCHPAD_ID));
        assert_eq!(bar_menu_items(MEETING_BAR_IDLE, false), items);
        for state in [
            "recording",
            "streaming",
            "transcribing",
            "processing",
            "command",
            "error",
        ] {
            assert!(bar_menu_items(state, false).is_empty(), "{state}");
        }
        assert!(bar_menu_items(MEETING_BAR_RECORDING, true).is_empty());
        assert!(bar_menu_items(BAR_RESTING, true).is_empty());
    }

    #[test]
    fn screen_capture_is_blocked_only_while_a_meeting_records() {
        assert!(capture_protected(MEETING_BAR_RECORDING, true));
        assert!(capture_protected(MEETING_BAR_NUDGE, true));
        // A dictation during a meeting stays protected.
        assert!(capture_protected("recording", true));
        assert!(!capture_protected("recording", false));
        assert!(!capture_protected(BAR_RESTING, false));
    }

    #[test]
    fn resting_bar_hides_for_the_hour_or_when_not_shown_at_all_times() {
        assert!(resting_bar_visible(false, true));
        assert!(!resting_bar_visible(true, true));
        assert!(!resting_bar_visible(false, false));
        assert!(!resting_bar_visible(true, false));
    }

    #[test]
    fn bar_and_dictation_share_the_bottom_edge_by_default() {
        let mut settings = crate::settings::get_default_settings();
        // Default settings: one bar on the bottom edge (Wispr).
        assert_eq!(
            overlay_position_for(&settings, BAR_RESTING),
            OverlayPosition::Bottom
        );
        assert_eq!(
            overlay_position_for(&settings, "recording"),
            OverlayPosition::Bottom
        );
        // Unpinned, the bar follows the dictation edge.
        settings.overlay_position = OverlayPosition::Top;
        assert_eq!(
            overlay_position_for(&settings, BAR_RESTING),
            OverlayPosition::Top
        );
        // A store that pinned the bar apart keeps working.
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
    fn recording_phases_map_to_the_listening_pill() {
        assert_eq!(
            listening_mode_for_phase(RecordingPhase::Started),
            Some("ptt")
        );
        assert_eq!(
            listening_mode_for_phase(RecordingPhase::Locked),
            Some("handsfree")
        );
        assert_eq!(listening_mode_for_phase(RecordingPhase::Stopped), None);
        assert_eq!(listening_mode_for_phase(RecordingPhase::Cancelled), None);
    }

    #[test]
    fn dropped_bar_docks_to_the_nearest_edge() {
        let area = (0.0, 0.0, 1000.0, 800.0);
        assert_eq!(
            nearest_overlay_edge(area, 500.0, 790.0),
            OverlayPosition::Bottom
        );
        // Wispr offers no top drop zone: near the top, the nearest of the
        // other three wins.
        assert_eq!(
            nearest_overlay_edge(area, 120.0, 12.0),
            OverlayPosition::Left
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
    #[test]
    fn a_failed_take_shows_as_a_dictation_pill() {
        // The failure pill lives in the dictation window, sized for the
        // pill plus its notification card.
        assert!(is_dictation_state("error"));
        assert_eq!(
            exact_fit_dimensions("error", OverlayPosition::Bottom),
            (OVERLAY_ERROR_WIDTH, OVERLAY_ERROR_HEIGHT)
        );
    }

}
