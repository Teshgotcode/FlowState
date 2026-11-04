//! System audio on macOS: what the machine itself is playing — the far end
//! of a Zoom or Teams call, a video, a voice message — captured through
//! the CoreAudio process tap API that macOS 14.4 introduced (S-5).
//!
//! Why a process tap and not ScreenCaptureKit: the S-5 TCC spike
//! (`research/2026-10-27-systemlyd-tcc.md`) measured both routes on this
//! machine. The tap — from a bare test process and from an ad-hoc signed
//! bundle — captures with no permission dialog at all, because a CoreAudio
//! tap is a HAL feature, not a TCC-protected resource; ScreenCaptureKit
//! was refused outright without a Screen Recording grant, which costs a
//! dialog plus a trip to System Settings. The tap is the route.
//!
//! Runtime gate first: the app's deployment target stays 10.15, while
//! `CATapDescription` and `AudioHardwareCreateProcessTap` only exist from
//! 14.4. [`supported`] is checked before any of those classes is so much as
//! named, so an older Mac never links into them, and a meeting there is
//! simply mic-only.
//!
//! The shape of a capture: a mono global tap that excludes FlowState
//! itself (its own feedback sounds would otherwise land in the track),
//! wrapped in a *private* aggregate device — invisible in the system's
//! device list, no driver installed, nothing another process can open —
//! read on CoreAudio's realtime IO thread into a plain buffer a writer
//! thread drains to the caller. Stopping stops the IO, destroys the
//! aggregate and the tap, and leaves only the captured audio behind. No
//! sidecar process, no virtual device that outlives the recording, no
//! network anything.

use anyhow::{anyhow, Context, Result};

/// The frames the tap captured, handed over as mono f32 at the machine's
/// output sample rate. Delivered on a writer thread, never on the
/// realtime IO thread, so the receiver may lock, write files and log.
#[cfg(target_os = "macos")]
pub type FramesSink =
    std::sync::Arc<dyn Fn(&[f32]) + Send + Sync + 'static>;

#[cfg(target_os = "macos")]
mod imp {
    use super::*;
    use cidre::{
        arc,
        cat,
        cf,
        core_audio::{
            self as ca, aggregate_device_keys as agg, device_start, hardware::StartedDevice,
            AggregateDevice, Process, System, TapDesc,
        },
        ns, os,
    };
    use log::{debug, warn};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Condvar, Mutex};

    /// Mono frames buffered up front, so the IO callback's pushes almost
    /// never allocate while it runs (it drains continuously).
    const RESERVE_FRAMES: usize = 96_000;

    /// What the IO callback and the writer thread share: samples tapped but
    /// not yet handed to the sink, and the flag that ends the writer.
    struct Shared {
        buffer: Mutex<Vec<f32>>,
        drained: Condvar,
        done: AtomicBool,
    }

    /// The realtime callback CoreAudio calls with the tap's audio. Only
    /// touches the preallocated buffer; the sink never runs here.
    extern "C" fn io_proc(
        _device: cidre::core_audio::Device,
        _now: &cat::AudioTimeStamp,
        input_data: &cat::AudioBufList<2>,
        _input_time: &cat::AudioTimeStamp,
        _output_data: &mut cat::AudioBufList<1>,
        _output_time: &cat::AudioTimeStamp,
        client_data: Option<&mut *mut Shared>,
    ) -> os::Status {
        let Some(ptr) = client_data else {
            return os::Status::NO_ERR;
        };
        // The Arc in the recorder keeps this alive for as long as the device
        // runs, and the callback only reads through it.
        let shared: &Shared = unsafe { &*(*ptr) };

        let buffer_count = (input_data.number_buffers as usize).min(2);
        // Slice per buffer, so non-interleaved input (one mono buffer per
        // channel) can be summed frame by frame instead of appended.
        let mut slices: [Option<&[f32]>; 2] = [None, None];
        for (i, slot) in slices.iter_mut().enumerate().take(buffer_count) {
            let buf = &input_data.buffers[i];
            if buf.data_bytes_size == 0 {
                continue;
            }
            let samples = unsafe {
                std::slice::from_raw_parts(
                    buf.data as *const f32,
                    (buf.data_bytes_size as usize) / 4,
                )
            };
            *slot = Some(samples);
        }
        if slices[0].is_none() {
            return os::Status::NO_ERR; // no input at all this cycle
        }

        let mut guard = shared.buffer.lock().unwrap();
        match slices {
            // The usual case: one mono (or one interleaved) buffer.
            [Some(a), None] => guard.extend_from_slice(a),
            // Several mono buffers: sum them per frame.
            [Some(a), Some(b)] => {
                let frames = a.len().min(b.len());
                let (a, b) = (&a[..frames], &b[..frames]);
                let spare = guard.capacity() - guard.len();
                guard.reserve(frames.saturating_sub(spare));
                for (a, b) in a.iter().zip(b) {
                    guard.push((a + b) * 0.5);
                }
            }
            _ => return os::Status::NO_ERR,
        }
        drop(guard);
        shared.drained.notify_all();
        os::Status::NO_ERR
    }

    /// A running system-audio capture: the private aggregate device (with
    /// the tap in it), the tap itself, and the thread that drains it.
    pub struct SystemAudioRecorder {
        /// The running aggregate. Dropping it stops the IO and destroys the
        /// device; it must go before the tap it reads.
        started: Option<StartedDevice<AggregateDevice>>,
        /// The process tap, alive exactly while the recording runs.
        tap: Option<ca::hardware_tapping::TapGuard>,
        shared: Arc<Shared>,
        writer: Option<std::thread::JoinHandle<()>>,
        sample_rate: u32,
        frames: Arc<AtomicUsize>,
    }

    /// A prepared process tap: created (the permission-free step the TCC
    /// spike measures) but not yet running. Split from `start` so the
    /// caller can size its files to the tap's sample rate before the
    /// capture begins.
    pub struct SystemTap {
        tap: ca::hardware_tapping::TapGuard,
        uid: arc::R<cf::String>,
        sample_rate: u32,
    }

    impl SystemTap {
        /// Create the process tap: everything the machine plays except
        /// FlowState itself (its own feedback sounds would otherwise land in
        /// the track). This is the step any permission would gate — and the
        /// spike found none does.
        pub fn prepare() -> Result<Self> {
            if !super::supported() {
                return Err(anyhow!(
                    "System audio needs macOS 14.4 or newer; this Mac is older"
                ));
            }

            let mut exclude = ns::ArrayMut::<ns::Number>::with_capacity(1);
            if let Ok(me) = Process::with_pid(std::process::id() as _) {
                exclude.push(&ns::Number::with_u32(me.0 .0));
            }
            let exclude = exclude.freeze();
            let tap_desc = TapDesc::with_mono_global_tap_excluding_processes(&exclude);
            let tap = tap_desc
                .create_process_tap()
                .context("CoreAudio refused to create the process tap")?;
            let tap_uid = tap.uid().context("The process tap has no UID")?;
            let asbd = tap
                .asbd()
                .context("The process tap reports no audio format")?;
            let sample_rate = asbd.sample_rate as u32;
            if sample_rate == 0 {
                return Err(anyhow!("The process tap reports a zero sample rate"));
            }
            if !asbd.is_common_f32() {
                return Err(anyhow!(
                    "The process tap's format is not 32-bit float, which this capture does not read"
                ));
            }
            debug!(
                "System audio tap prepared: {} Hz, tap {}",
                sample_rate,
                tap_uid.as_ref()
            );
            Ok(Self {
                tap,
                uid: tap_uid,
                sample_rate,
            })
        }

        /// The tap's sample rate, for the WAV header the caller writes.
        pub fn sample_rate(&self) -> u32 {
            self.sample_rate
        }

        /// Begin capturing: wraps the prepared tap in a private aggregate
        /// device and hands its mono frames to `sink`. Fails — and the
        /// caller falls back to mic-only — when CoreAudio refuses one of
        /// the objects.
        pub fn start(self, sink: FramesSink) -> Result<SystemAudioRecorder> {
            let tap = self.tap;
            let tap_uid = self.uid;
            let sample_rate = self.sample_rate;
            let shared = Arc::new(Shared {
                buffer: Mutex::new(Vec::with_capacity(RESERVE_FRAMES)),
                drained: Condvar::new(),
                done: AtomicBool::new(false),
            });
            let frames = Arc::new(AtomicUsize::new(0));

            // The aggregate needs a device to clock it, and a machine with
            // no output device has no system audio to tap anyway.
            let output_uid = System::default_output_device()
                .ok()
                .and_then(|d| d.uid().ok())
                .ok_or_else(|| anyhow!("This Mac has no output device, so there is no system audio to capture"))?;

            let uuid = cf::Uuid::new().to_cf_string();
            let name = cf::str!(c"FlowState System Audio");
            let dict = cf::DictionaryOf::<cf::String, cf::Type>::with_keys_values(
                &[
                    agg::is_private(),
                    agg::is_stacked(),
                    agg::tap_auto_start(),
                    agg::name(),
                    agg::uid(),
                ],
                &[
                    cf::Boolean::value_true().as_type_ref(),
                    cf::Boolean::value_false(),
                    cf::Boolean::value_true(),
                    name,
                    uuid.as_ref(),
                ],
            );
            let mut agg = AggregateDevice::with_desc(&dict).context(
                "CoreAudio refused to create the system audio aggregate device",
            )?;

            let mut taps = cf::ArrayOf::<cf::String>::new()
                .copy_mut()
                .ok_or_else(|| anyhow!("CoreAudio could not allocate the tap list"))?;
            taps.push(tap_uid.as_ref());
            agg.set_tap_list(&taps)
                .context("The aggregate device refused the process tap")?;
            // The output device joins as the aggregate's clock (its main sub
            // device); being in a private aggregate does not mute or tie it
            // up, everything keeps playing exactly as before.
            let mut sub_devices = cf::ArrayOf::<cf::String>::new()
                .copy_mut()
                .ok_or_else(|| anyhow!("CoreAudio could not allocate the sub device list"))?;
            sub_devices.push(output_uid.as_ref());
            agg.set_full_sub_device_list(&sub_devices)
                .context("The aggregate device refused its clock device")?;
            agg.set_main_sub_device(output_uid.as_ref())
                .context("The aggregate device refused its clock device")?;

            let mut raw = Arc::as_ptr(&shared) as *mut Shared;
            let proc_id = agg
                .create_io_proc_id::<2, 1, *mut Shared>(io_proc, Some(&mut raw))
                .context("The aggregate device refused the capture callback")?;
            let started =
                device_start(agg, Some(proc_id)).context("The system audio capture did not start")?;

            // Draining thread: the only place the sink runs.
            let writer = {
                let shared = Arc::clone(&shared);
                let frames = Arc::clone(&frames);
                std::thread::Builder::new()
                    .name("flowstate-system-audio".into())
                    .spawn(move || {
                        loop {
                            let chunk = {
                                let mut buffer = shared.buffer.lock().unwrap();
                                loop {
                                    if !buffer.is_empty() {
                                        break;
                                    }
                                    if shared.done.load(Ordering::Acquire) {
                                        return;
                                    }
                                    buffer = shared.drained.wait(buffer).unwrap();
                                }
                                std::mem::take(&mut *buffer)
                            };
                            frames.fetch_add(chunk.len(), Ordering::Relaxed);
                            sink(&chunk);
                        }
                    })
                    .context("Failed to start the system audio writer thread")?
            };

            debug!("System audio tap running: {} Hz", sample_rate);
            Ok(SystemAudioRecorder {
                started: Some(started),
                tap: Some(tap),
                shared,
                writer: Some(writer),
                sample_rate,
                frames,
            })
        }
    }

    impl SystemAudioRecorder {
        /// The capture's sample rate, for the WAV header the caller wrote.
        pub fn sample_rate(&self) -> u32 {
            self.sample_rate
        }

        /// Frames captured so far (mono samples).
        pub fn frames(&self) -> usize {
            self.frames.load(Ordering::Relaxed)
        }

        /// End the capture: stop the IO, destroy the aggregate and the tap,
        /// and wait for every buffered frame to be handed over.
        pub fn stop(mut self) {
            if let Some(started) = self.started.take() {
                if let Err(e) = started.stop() {
                    warn!("Failed to stop the system audio device: {e}");
                }
            }
            self.shared.done.store(true, Ordering::Release);
            self.shared.drained.notify_all();
            if let Some(writer) = self.writer.take() {
                let _ = writer.join();
            }
            self.tap.take();
            debug!("System audio tap stopped");
        }
    }

    impl Drop for SystemAudioRecorder {
        fn drop(&mut self) {
            // The explicit stop() has normally run; a dropped recorder
            // (a failed meeting start) still tears the tap down.
            if let Some(started) = self.started.take() {
                drop(started);
            }
            self.shared.done.store(true, Ordering::Release);
            self.shared.drained.notify_all();
            if let Some(writer) = self.writer.take() {
                let _ = writer.join();
            }
            self.tap.take();
        }
    }
}

#[cfg(target_os = "macos")]
pub use imp::{SystemAudioRecorder, SystemTap};

/// Whether this Mac can capture system audio: the CoreAudio process tap
/// API exists from macOS 14.4, and FlowState's deployment target stays
/// 10.15, so the check is made at runtime — never by touching a class
/// that an older OS does not have.
#[cfg(target_os = "macos")]
pub fn supported() -> bool {
    let version = cidre::ns::ProcessInfo::current().os_version();
    version.major > 14 || (version.major == 14 && version.minor >= 4)
}

/// Everywhere else there is no system audio: meetings stay mic-only and
/// the setting never shows.
#[cfg(not(target_os = "macos"))]
pub fn supported() -> bool {
    false
}

/// The S-5 TCC spike, step 0: measures which route to system audio costs
/// the least permission friction, on the machine it runs on.
///
/// * The CoreAudio process tap: creates the tap, captures three seconds,
///   reports frames and peak. No dialog may appear — a tap is not a
///   TCC-protected resource.
/// * ScreenCaptureKit, for contrast: `CGPreflightScreenCaptureAccess`
///   reports whether the Screen Recording grant exists — *without*
///   showing a dialog. `false` means ScreenCaptureKit would cost a TCC
///   dialog and a visit to System Settings.
///
/// Ignored in the normal gate (it needs the machine's audio stack and a
/// few seconds of wall time); run it with
/// `cargo test --ignored system_audio_tcc_spike -- --nocapture`.
#[cfg(all(test, target_os = "macos"))]
mod spike_tests {
    use super::*;
    use crate::system_audio::{SystemAudioRecorder, SystemTap};
    use std::sync::Arc;

    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
    }

    #[test]
    #[ignore]
    fn system_audio_tcc_spike() {
        println!("macOS: {}", cidre::ns::ProcessInfo::current().os_version_string());
        let sck_granted = unsafe { CGPreflightScreenCaptureAccess() };
        println!("ScreenCaptureKit preflight (Screen Recording granted, no dialog): {sck_granted}");

        // The TCC question itself: creating the tap is the step any
        // permission would gate. No dialog may appear, and it must succeed.
        let tap = SystemTap::prepare()
            .expect("the process tap must be creatable with no permission dialog");
        println!("process tap created with no permission dialog at {} Hz", tap.sample_rate());
        drop(tap);

        // The capture pipeline end to end, when the machine can run it.
        let (tx, rx) = std::sync::mpsc::channel::<Vec<f32>>();
        let tx = Arc::new(std::sync::Mutex::new(tx));
        let capture = SystemTap::prepare().and_then(|tap| {
            let tx = Arc::clone(&tx);
            tap.start(Arc::new(move |frames: &[f32]| {
                let _ = tx.lock().unwrap().send(frames.to_vec());
            }))
        });
        let recorder = match capture {
            Ok(recorder) => recorder,
            Err(e) => {
                // A machine with no output device (a headless VM) has
                // nothing to tap; the tap-creation measurement above is the
                // spike's permission answer either way.
                println!("capture skipped on this machine: {e:#}");
                return;
            }
        };
        println!("tap running at {} Hz", recorder.sample_rate());
        let _ = std::process::Command::new("afplay")
            .arg("/System/Library/Sounds/Glass.aiff")
            .spawn();
        std::thread::sleep(std::time::Duration::from_secs(3));
        recorder.stop();
        drop(tx);
        let mut captured = Vec::new();
        while let Ok(chunk) = rx.try_recv() {
            captured.extend(chunk);
        }
        let peak = captured.iter().fold(0f32, |m, s| m.max(s.abs()));
        println!("process tap captured {} frames in 3s, peak {peak:.4}", captured.len());
        assert!(!captured.is_empty(), "the tap's IO cycle must deliver frames");
    }
}
