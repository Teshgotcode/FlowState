//! macOS click suppression for the mouse push-to-talk trigger.
//!
//! The handy-keys input layer detects middle/side mouse buttons on macOS but
//! never blocks them there, so without this tap the push-to-talk click would
//! still land in whatever app is under the cursor (e.g. a browser side button
//! navigating back). This module appends a tail event tap that swallows the
//! configured button's press and release before they reach other
//! applications.
//!
//! Ordering: handy-keys creates its detection tap with head-insert placement
//! and head-inserted taps always run before tail-appended ones, so detection
//! is unaffected by when each tap appears. Suppression here can then drop the
//! event after handy-keys has seen it.
//!
//! Lifecycle: the tap thread is spawned once, on first use, and then kept for
//! the rest of the session. Disabling the setting only parks the atomic
//! button number at `NO_BUTTON`, which turns the callback into a pass-through;
//! tearing a CFRunLoop down from another thread has a lost-wakeup race, and a
//! dormant pass-through tap costs nothing. Requires the same Accessibility
//! permission handy-keys already needs.
//!
//! Raw FFI (mirrors `crate::input`'s Carbon bindings) instead of new crates:
//! CoreGraphics is always present on macOS.

use std::cell::Cell;
use std::ffi::c_void;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{channel, Sender};

use log::{error, info};

use crate::settings::MousePttButton;

/// No button suppressed (pass-through).
const NO_BUTTON: i64 = -1;

/// kCGEventTapLocation::kCGSessionEventTap
const SESSION_EVENT_TAP: u32 = 1;
/// kCGEventTapPlacement::kCGTailAppendEventTap
const TAIL_APPEND_EVENT_TAP: u32 = 1;
/// kCGEventTapOptions::kCGEventTapOptionDefault (active tap: may block)
const TAP_OPTION_DEFAULT: u32 = 0;
/// CGEventType::OtherMouseDown / OtherMouseUp
const OTHER_MOUSE_DOWN: u32 = 25;
const OTHER_MOUSE_UP: u32 = 26;
/// kCGEventTapDisabledByTimeout / kCGEventTapDisabledByUserInput
const TAP_DISABLED_BY_TIMEOUT: u32 = 0xFFFF_FFFE;
const TAP_DISABLED_BY_USER_INPUT: u32 = 0xFFFF_FFFF;
/// kCGEventField::kCGMouseEventButtonNumber
const MOUSE_EVENT_BUTTON_NUMBER: u32 = 3;

/// The button the tap swallows. 2 = middle, 3 = side "back", 4 = side
/// "forward"; `NO_BUTTON` suppresses nothing.
static SUPPRESSED_BUTTON: AtomicI64 = AtomicI64::new(NO_BUTTON);

/// Whether the suppression tap is running (it never stops once started).
static TAP_STARTED: AtomicI64 = AtomicI64::new(0);

/// The button number macOS reports for each push-to-talk choice.
fn button_number(button: MousePttButton) -> i64 {
    match button {
        MousePttButton::Middle => 2,
        MousePttButton::Back => 3,
        MousePttButton::Forward => 4,
    }
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventTapCreate(
        tap_point: u32,
        placement: u32,
        options: u32,
        event_mask: u64,
        callback: unsafe extern "C" fn(
            proxy: *mut c_void,
            event_type: u32,
            event: *mut c_void,
            user_info: *mut c_void,
        ) -> *mut c_void,
        user_info: *mut c_void,
    ) -> *mut c_void;
    fn CGEventTapEnable(port: *mut c_void, enable: bool);
    fn CGEventGetIntegerValueField(event: *mut c_void, field: u32) -> i64;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFRunLoopCommonModes: *const c_void;
    fn CFMachPortCreateRunLoopSource(
        allocator: *mut c_void,
        port: *mut c_void,
        order: isize,
    ) -> *mut c_void;
    fn CFRunLoopAddSource(run_loop: *mut c_void, source: *mut c_void, mode: *const c_void);
    fn CFRunLoopGetCurrent() -> *mut c_void;
    fn CFRunLoopRun();
    fn CFRelease(cf: *const c_void);
}

/// Callback context: carries the tap port back to the callback so it can
/// re-enable the tap after the system times it out. Touched only on the tap
/// thread once the run loop is running.
struct TapCtx {
    port: Cell<*mut c_void>,
}

/// The tap callback: swallow the configured button's press/release, pass
/// everything else through untouched.
unsafe extern "C" fn suppress_callback(
    _proxy: *mut c_void,
    event_type: u32,
    event: *mut c_void,
    user_info: *mut c_void,
) -> *mut c_void {
    // Pseudo-events must be returned, not swallowed. After a timeout the tap
    // is disabled: re-enable it so suppression survives system hiccups.
    if event_type == TAP_DISABLED_BY_TIMEOUT || event_type == TAP_DISABLED_BY_USER_INPUT {
        if event_type == TAP_DISABLED_BY_TIMEOUT {
            let ctx = unsafe { &*(user_info as *const TapCtx) };
            unsafe { CGEventTapEnable(ctx.port.get(), true) };
        }
        return event;
    }

    // Only mouse-button events reach us; the event type is redundant here but
    // checked defensively in case the mask grows later.
    if event_type != OTHER_MOUSE_DOWN && event_type != OTHER_MOUSE_UP {
        return event;
    }

    let suppressed = SUPPRESSED_BUTTON.load(Ordering::SeqCst);
    if suppressed == NO_BUTTON {
        return event;
    }

    let number = unsafe { CGEventGetIntegerValueField(event, MOUSE_EVENT_BUTTON_NUMBER) };
    if number == suppressed {
        // Swallow: the click never reaches the app under the cursor.
        std::ptr::null_mut()
    } else {
        event
    }
}

/// Thread body: create the tap, report the result, then park in the run loop
/// for the rest of the session.
fn run_suppress_tap(init: Sender<Result<(), String>>) {
    let ctx = Box::into_raw(Box::new(TapCtx {
        port: Cell::new(std::ptr::null_mut()),
    }));

    // SAFETY: all FFI below runs on this dedicated thread. The tap port and
    // run loop live for the whole session; nothing is released because
    // CFRunLoopRun never returns.
    unsafe {
        let mask = (1u64 << OTHER_MOUSE_DOWN) | (1u64 << OTHER_MOUSE_UP);
        let port = CGEventTapCreate(
            SESSION_EVENT_TAP,
            TAIL_APPEND_EVENT_TAP,
            TAP_OPTION_DEFAULT,
            mask,
            suppress_callback,
            ctx as *mut c_void,
        );
        if port.is_null() {
            drop(Box::from_raw(ctx));
            let _ = init.send(Err(
                "event tap creation failed; grant Accessibility in System Settings > Privacy & Security"
                    .to_string(),
            ));
            return;
        }

        // Callbacks fire only on this thread's run loop, which has not
        // started yet, so there is no race publishing the port.
        (*ctx).port.set(port);

        let source = CFMachPortCreateRunLoopSource(std::ptr::null_mut(), port, 0);
        if source.is_null() {
            CFRelease(port);
            drop(Box::from_raw(ctx));
            let _ = init.send(Err("run loop source creation failed".to_string()));
            return;
        }

        let run_loop = CFRunLoopGetCurrent();
        CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);

        if init.send(Ok(())).is_err() {
            // The caller is gone (shutting down); park anyway to keep the
            // port's thread alive rather than unwinding with the tap armed.
        }
        info!("mouse push-to-talk suppression tap running");
        CFRunLoopRun();
    }
}

/// Apply the configured push-to-talk button to the suppression tap: update the
/// swallowed button (or park it at `NO_BUTTON`) and start the tap thread on
/// first use. Called from the shortcut refresh path.
pub fn apply(button: Option<MousePttButton>) {
    SUPPRESSED_BUTTON.store(
        button.map(button_number).unwrap_or(NO_BUTTON),
        Ordering::SeqCst,
    );

    if button.is_none() {
        return;
    }

    // One-shot start; a started tap is never restarted or stopped (see the
    // module docs for why).
    if TAP_STARTED
        .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }

    let (tx, rx) = channel::<Result<(), String>>();
    let spawn_result = std::thread::Builder::new()
        .name("mouse-ptt-suppress".into())
        .spawn(move || run_suppress_tap(tx));

    // On failure, release the one-shot flag so a later settings change can
    // retry; the parked atomic then keeps the tap as a pass-through.
    let result = match spawn_result {
        Ok(_) => match rx.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(e),
            Err(_) => Err("tap thread terminated unexpectedly".to_string()),
        },
        Err(e) => Err(format!("failed to spawn tap thread: {e}")),
    };
    if let Err(e) = result {
        TAP_STARTED.store(0, Ordering::SeqCst);
        error!("Mouse click suppression unavailable: {e}");
    }
}

/// Used by tests to observe the parked button number.
#[cfg(test)]
pub fn suppressed_button() -> i64 {
    SUPPRESSED_BUTTON.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_numbers_match_macos_reporting() {
        assert_eq!(button_number(MousePttButton::Middle), 2);
        assert_eq!(button_number(MousePttButton::Back), 3);
        assert_eq!(button_number(MousePttButton::Forward), 4);
    }

    #[test]
    fn apply_none_parks_the_suppressed_button() {
        apply(None);
        assert_eq!(suppressed_button(), NO_BUTTON);
    }
}
