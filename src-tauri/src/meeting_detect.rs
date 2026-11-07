//! Notices that another app has the microphone open, which is what a meeting
//! looks like from the outside.
//!
//! macOS exposes this through CoreAudio: the default input device answers
//! `kAudioDevicePropertyDeviceIsRunningSomewhere` while any process is
//! recording from it. FlowState's own capture also sets it, so the watcher
//! stays quiet while we are the ones recording.
//!
//! The probe is thin and unsafe; the decision to prompt is a pure function so
//! it can be tested without audio hardware.

use std::time::{Duration, Instant};

/// How often the probe runs. Coarse on purpose: a meeting does not start in
/// the 200 ms between two ticks, and polling less keeps the CPU idle.
const POLL_INTERVAL: Duration = Duration::from_secs(4);
/// The device must stay busy this long before we say anything, so a short
/// voice memo in another app does not trigger the prompt.
const CONFIRM_AFTER: Duration = Duration::from_secs(12);
/// After a prompt is dismissed, stay quiet for this long.
const QUIET_AFTER_DISMISS: Duration = Duration::from_secs(30 * 60);

/// What the watcher knows at one tick.
#[derive(Clone, Copy, Debug, Default)]
pub struct Watch {
    /// The input device was busy at this tick.
    pub busy: bool,
    /// FlowState itself is capturing (meeting or dictation).
    pub own_capture: bool,
    /// The first tick the device looked busy, if it still does.
    pub busy_since: Option<Instant>,
    /// When the last prompt was dismissed, if ever.
    pub quiet_since: Option<Instant>,
    /// A prompt is on screen right now.
    pub prompted: bool,
}

/// The outcome of one tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// Nothing to do.
    Idle,
    /// Start tracking the busy spell from now.
    Arm(Instant),
    /// Stay silent and keep the current arming time.
    Hold,
    /// Show the prompt, arming nothing new.
    Prompt,
    /// The device went quiet: forget the spell.
    Clear,
}

/// Decide what one tick means. `busy` is the probe's answer this tick.
pub fn step(watch: &Watch, busy: bool, now: Instant) -> Step {
    if !busy || watch.own_capture {
        return if watch.busy_since.is_some() {
            Step::Clear
        } else {
            Step::Idle
        };
    }
    if watch.prompted {
        return Step::Hold;
    }
    if let Some(quiet) = watch.quiet_since {
        if now.duration_since(quiet) < QUIET_AFTER_DISMISS {
            return Step::Hold;
        }
    }
    match watch.busy_since {
        None => Step::Arm(now),
        Some(since) if now.duration_since(since) >= CONFIRM_AFTER => Step::Prompt,
        Some(_) => Step::Hold,
    }
}

/// Whether the default input device is in use by any process right now.
/// Returns false when the property cannot be read, so a broken probe is quiet
/// rather than noisy.
#[cfg(target_os = "macos")]
pub fn input_device_is_busy() -> bool {
    use std::ptr::NonNull;

    use objc2_core_audio::{
        kAudioDevicePropertyDeviceIsRunningSomewhere, kAudioHardwarePropertyDefaultInputDevice,
        kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal as Scope,
        AudioObjectGetPropertyData, AudioObjectID, AudioObjectPropertyAddress,
        AudioObjectPropertyScope,
    };

    // kAudioObjectSystemObject: the system-wide object every property hangs off.
    const SYSTEM_OBJECT: AudioObjectID = 1;

    let mut device: AudioObjectID = 0;
    let mut size = std::mem::size_of::<AudioObjectID>() as u32;
    let mut device_address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyDefaultInputDevice,
        mScope: Scope,
        mElement: kAudioObjectPropertyElementMain,
    };
    // SAFETY: address and out-pointer are valid for one call; the sizes match.
    let status = unsafe {
        AudioObjectGetPropertyData(
            SYSTEM_OBJECT,
            NonNull::from(&mut device_address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut device).cast(),
        )
    };
    if status != 0 || device == 0 {
        return false;
    }

    let mut running: u32 = 0;
    let mut running_size = std::mem::size_of::<u32>() as u32;
    let mut running_address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
        mScope: Scope,
        mElement: kAudioObjectPropertyElementMain,
    };
    // SAFETY: same contract as above; the device answers a u32 for this selector.
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            NonNull::from(&mut running_address),
            0,
            std::ptr::null(),
            NonNull::from(&mut running_size),
            NonNull::from(&mut running).cast(),
        )
    };
    status == 0 && running != 0
}

#[cfg(not(target_os = "macos"))]
pub fn input_device_is_busy() -> bool {
    false
}

/// True while FlowState is recording a meeting or a dictation of its own.
pub fn own_capture_active(app: &tauri::AppHandle) -> bool {
    use std::sync::Arc;
    use tauri::Manager;
    let dictating = app
        .try_state::<Arc<crate::managers::audio::AudioRecordingManager>>()
        .map(|manager| manager.is_recording())
        .unwrap_or(false);
    let meeting = app
        .try_state::<crate::managers::meeting::MeetingManager>()
        .map(|manager| manager.is_recording())
        .unwrap_or(false);
    dictating || meeting
}

/// Poll the probe forever, call `on_prompt` once per confirmed spell, and ask
/// `own_capture` each tick whether we are the ones using the microphone.
pub fn watch_forever<C, F>(app: tauri::AppHandle, mut own_capture: C, mut on_prompt: F)
where
    C: FnMut(&tauri::AppHandle) -> bool,
    F: FnMut(),
{
    let mut watch = Watch::default();
    loop {
        let now = Instant::now();
        let busy = input_device_is_busy();
        watch.own_capture = own_capture(&app);
        match step(&watch, busy, now) {
            Step::Idle => {}
            Step::Arm(at) => watch.busy_since = Some(at),
            Step::Hold => {}
            Step::Clear => watch.busy_since = None,
            Step::Prompt => {
                watch.prompted = true;
                on_prompt();
            }
        }
        watch.busy = busy;
        if !busy {
            watch.busy_since = None;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Instant {
        Instant::now() + Duration::from_millis(ms)
    }

    #[test]
    fn the_probe_answers_without_panicking() {
        // Runs the real CoreAudio query on this machine: no assertion on the
        // value, only that the unsafe block is sound and returns.
        let _ = input_device_is_busy();
    }

    #[test]
    fn a_short_blip_in_another_app_never_prompts() {
        let start = Instant::now();
        let mut watch = Watch::default();
        let arm = step(&watch, true, start);
        assert!(matches!(arm, Step::Arm(_)));
        watch.busy_since = Some(start);
        assert_eq!(step(&watch, true, start + Duration::from_secs(4)), Step::Hold);
        assert_eq!(step(&watch, false, start + Duration::from_secs(8)), Step::Clear);
    }

    #[test]
    fn a_confirmed_busy_spell_prompts_once() {
        let start = Instant::now();
        let mut watch = Watch {
            busy_since: Some(start),
            ..Default::default()
        };
        assert_eq!(
            step(&watch, true, start + CONFIRM_AFTER),
            Step::Prompt
        );
        watch.prompted = true;
        assert_eq!(step(&watch, true, at(60_000)), Step::Hold);
    }

    #[test]
    fn our_own_recording_keeps_the_watcher_quiet() {
        let start = Instant::now();
        let watch = Watch {
            busy: true,
            own_capture: true,
            busy_since: Some(start),
            ..Default::default()
        };
        assert_eq!(step(&watch, true, start + CONFIRM_AFTER), Step::Clear);
    }

    #[test]
    fn a_dismissal_keeps_it_quiet_for_an_earlier_spell() {
        let start = Instant::now();
        let watch = Watch {
            busy_since: Some(start),
            quiet_since: Some(start),
            ..Default::default()
        };
        assert_eq!(step(&watch, true, start + CONFIRM_AFTER), Step::Hold);
    }
}
