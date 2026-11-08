//! The meeting session: a recording that starts and stops on command, not on
//! a hotkey, and runs for the length of a meeting (30-90 minutes) rather than
//! a dictation take. Audio is written to a working file while it is captured
//! so a long session never lives only in memory, and the same live
//! chunking/VAD loop a dictation's preview uses keeps a running transcript.
//! The finished recording becomes a history entry with `source: "meeting"`;
//! a cancelled one leaves nothing behind.
//!
//! A dictation pressed while a meeting records finds the microphone held and
//! takes the same failed-start path it takes today when another dictation
//! holds it: its cleanup can end the meeting's live preview early, but the
//! meeting's capture and working file are untouched, and its transcript
//! arrives from the background job regardless.

use crate::audio_toolkit::{constants, VadPolicy};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::policy::{live_decode, LiveDecode, ModelTraits};
use crate::managers::transcription::TranscriptionManager;
use crate::managers::transcription_jobs::TranscriptionJobManager;
use crate::settings::{get_settings, AppSettings};
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

/// The audio manager's recording identity of a meeting. Dictations record
/// under their binding ids; the meeting records under its own, so a stray
/// dictation stop can never collect a meeting's audio.
const MEETING_BINDING_ID: &str = "meeting";

/// Suffix of a working file; finished recordings have no suffix.
const WORKING_SUFFIX: &str = "partial";

/// The header is refreshed at least once per second of audio, so a crash or
/// a kill leaves a playable WAV of the meeting so far.
const FLUSH_SAMPLES: usize = constants::WHISPER_SAMPLE_RATE as usize;

/// The mic track is the 16 kHz mono 16-bit WAV every recording is kept as;
/// the system track is written at whatever rate the machine plays at, so
/// the spec is built per track.
const WAV_BITS_PER_SAMPLE: u16 = 16;

/// The meeting session's stage, for the frontend.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum MeetingStatus {
    /// No meeting is recording.
    Idle,
    /// A meeting is recording, started at `started_at` (unix seconds);
    /// `duration_secs` have passed so far.
    Recording { started_at: i64, duration_secs: u64 },
}

/// What a finished meeting reports: the history entry it saved (when it
/// captured audio) and how long it ran.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type)]
pub struct StoppedMeeting {
    /// The new history entry's id; `None` when nothing was captured, in which
    /// case no entry exists.
    pub entry_id: Option<i64>,
    /// Unix seconds the meeting started.
    pub started_at: i64,
    /// Seconds from start to stop.
    pub duration_secs: u64,
}

/// Emitted whenever the meeting session's stage changes, so a window can
/// follow along without polling.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct MeetingStateEvent {
    pub status: MeetingStatus,
}

/// One finished piece of a meeting's live transcript, emitted as the
/// `meeting-segment` event while the meeting records, so its window can show
/// the transcript as it is spoken instead of only after the stop. The segment
/// comes from the live decode the recording already runs — the same chunks a
/// dictation's preview decodes — so no second transcription happens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct MeetingSegment {
    /// Arrival order within the session, from 0: the window keeps the list in
    /// the order the segments were spoken.
    pub index: u32,
    /// When the segment starts, in milliseconds from the meeting's start.
    pub t0_ms: i64,
    /// When the segment ends, in milliseconds from the meeting's start.
    pub t1_ms: i64,
    /// What the model heard in it.
    pub text: String,
}

/// Why a session transition was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MeetingError {
    /// A meeting is already recording.
    AlreadyRunning,
    /// No meeting is recording.
    NotRunning,
    /// A dictation holds the microphone.
    MicrophoneBusy,
}

impl std::fmt::Display for MeetingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MeetingError::AlreadyRunning => write!(f, "A meeting is already recording"),
            MeetingError::NotRunning => write!(f, "No meeting is recording"),
            MeetingError::MicrophoneBusy => {
                write!(f, "A dictation is using the microphone")
            }
        }
    }
}

impl std::error::Error for MeetingError {}

fn emit_meeting_state(app: &AppHandle, status: &MeetingStatus) {
    if let Err(e) = (MeetingStateEvent {
        status: status.clone(),
    })
    .emit(app)
    {
        warn!("Failed to emit meeting state: {e}");
    }
}

/* ---------- the session's decisions ---------------------------------------- */

/// What a stopped session hands its executor: the working file to finalize
/// and the facts its history entry keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StoppedDecision {
    working_path: PathBuf,
    started_at_unix: i64,
    duration_secs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum MeetingStage {
    #[default]
    Idle,
    Recording {
        started_at: Instant,
    },
}

/// Pure bookkeeping for one meeting session: whether it records, when it
/// started, and the working file its audio goes to. It decides transitions
/// only — the caller does the microphone, file and history work — so unit
/// tests drive the real production decisions without a Tauri app.
#[derive(Debug, Default)]
struct MeetingState {
    stage: MeetingStage,
    working_path: Option<PathBuf>,
    started_at_unix: Option<i64>,
}

impl MeetingState {
    /// Begin a session that writes to `working_path`. Refused while another
    /// meeting records or a dictation holds the microphone.
    fn start(
        &mut self,
        now: Instant,
        started_at_unix: i64,
        working_path: PathBuf,
        mic_busy: bool,
    ) -> Result<(), MeetingError> {
        match self.stage {
            MeetingStage::Recording { .. } => Err(MeetingError::AlreadyRunning),
            MeetingStage::Idle if mic_busy => Err(MeetingError::MicrophoneBusy),
            MeetingStage::Idle => {
                self.stage = MeetingStage::Recording { started_at: now };
                self.working_path = Some(working_path);
                self.started_at_unix = Some(started_at_unix);
                Ok(())
            }
        }
    }

    /// End the session, keeping its audio: the working file to finalize as a
    /// recording, with the start time and duration its entry keeps.
    fn stop(&mut self, now: Instant) -> Result<StoppedDecision, MeetingError> {
        let MeetingStage::Recording { started_at } = self.stage else {
            return Err(MeetingError::NotRunning);
        };
        let decision = StoppedDecision {
            working_path: self
                .working_path
                .take()
                .expect("a recording session has a working file"),
            started_at_unix: self
                .started_at_unix
                .take()
                .expect("a recording session has a start time"),
            duration_secs: now.saturating_duration_since(started_at).as_secs(),
        };
        self.stage = MeetingStage::Idle;
        Ok(decision)
    }

    /// End the session, discarding its audio: the working file to remove.
    /// No duration, no history entry.
    fn cancel(&mut self) -> Result<PathBuf, MeetingError> {
        let decision = self.stop(Instant::now())?;
        Ok(decision.working_path)
    }

    /// Roll back to idle after the start's side effects failed (microphone
    /// denied, working file unwritable), so the session doesn't wedge as
    /// recording.
    fn abort(&mut self) {
        self.stage = MeetingStage::Idle;
        self.working_path = None;
        self.started_at_unix = None;
    }

    fn is_recording(&self) -> bool {
        matches!(self.stage, MeetingStage::Recording { .. })
    }

    fn status(&self, now: Instant) -> MeetingStatus {
        match self.stage {
            MeetingStage::Idle => MeetingStatus::Idle,
            MeetingStage::Recording { started_at } => MeetingStatus::Recording {
                started_at: self.started_at_unix.unwrap_or_default(),
                duration_secs: now.saturating_duration_since(started_at).as_secs(),
            },
        }
    }
}

/* ---------- the working file ------------------------------------------------ */

/// A meeting's audio as it is captured: a WAV written frame by frame, so a
/// long recording reaches the disk while it runs, not only when it ends.
/// Frames arrive on the recorder's delivery thread, so writes stay cheap and
/// the first error is remembered for the stop to report.
struct WorkingFile {
    /// The WAV writer, present until the file is finished or discarded.
    writer: Option<hound::WavWriter<std::io::BufWriter<std::fs::File>>>,
    /// The working path (`...wav.partial`); finished at its name without the suffix.
    path: PathBuf,
    frames: usize,
    since_flush: usize,
    write_error: Option<hound::Error>,
}

impl WorkingFile {
    fn create(path: &Path, sample_rate: u32) -> Result<Self> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: WAV_BITS_PER_SAMPLE,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::create(path, spec)?;
        Ok(Self {
            writer: Some(writer),
            path: path.to_path_buf(),
            frames: 0,
            since_flush: 0,
            write_error: None,
        })
    }

    /// Append one captured frame (16 kHz mono, exactly what the sink keeps).
    fn write_frame(&mut self, frame: &[f32]) {
        if self.write_error.is_some() {
            return;
        }
        let Some(writer) = self.writer.as_mut() else {
            return;
        };
        let mut failure = None;
        for sample in frame {
            // The same conversion as `audio_toolkit::save_wav_file`, so a
            // meeting's WAV is byte-compatible with a dictation's.
            let i16_sample = (sample * i16::MAX as f32) as i16;
            if let Err(e) = writer.write_sample(i16_sample) {
                failure = Some(e);
                break;
            }
        }
        // Checkpoint once per second of audio: the header then describes
        // everything written so far, so a crash leaves a playable WAV of the
        // meeting up to the last second.
        if failure.is_none() && self.since_flush + frame.len() >= FLUSH_SAMPLES {
            if let Err(e) = writer.flush() {
                failure = Some(e);
            }
        }
        match failure {
            Some(e) => {
                error!("Meeting working file write failed: {e}");
                self.write_error = Some(e);
            }
            None => {
                self.frames += frame.len();
                self.since_flush = (self.since_flush + frame.len()) % FLUSH_SAMPLES;
            }
        }
    }

    /// Samples written so far.
    fn frames(&self) -> usize {
        self.frames
    }

    /// Patch the header and move the file to its finished name, returning the
    /// recording's file name for the history entry. On failure the file is
    /// left in place: a long meeting's audio must not be deleted because a
    /// rename failed.
    fn finish(&mut self) -> Result<String> {
        if let Some(e) = self.write_error.take() {
            return Err(anyhow!("The meeting's working file failed to write: {e}"));
        }
        let writer = self
            .writer
            .take()
            .ok_or_else(|| anyhow!("The working file was already finished"))?;
        writer.finalize()?;
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_suffix(&format!(".{WORKING_SUFFIX}")))
            .map(str::to_string)
            .ok_or_else(|| {
                anyhow!(
                    "The working file name has no .{WORKING_SUFFIX} suffix: {}",
                    self.path.display()
                )
            })?;
        let finished = self.path.with_file_name(&file_name);
        std::fs::rename(&self.path, &finished)?;
        debug!("Finalized the meeting's recording as {file_name}");
        Ok(file_name)
    }

    /// Close and delete the working file: a cancelled meeting leaves nothing.
    fn discard(&mut self) -> Result<()> {
        self.writer = None; // close the file handle before removing
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(anyhow!(
                "Failed to remove the working file {}: {e}",
                self.path.display()
            )),
        }
    }
}

/* ---------- the system-audio track (S-5) ------------------------------------ */

/// What the system track's worker thread reports when the meeting ends it:
/// the finished recording's file name and how far into the meeting its
/// capture began (the mic had usually already been running for a moment),
/// or why there is nothing to add.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SystemOutcome {
    Captured {
        file_name: String,
        offset_ms: i64,
    },
    /// The tap ran but the machine played nothing.
    NoAudio,
    /// The tap could not run at all; the meeting is mic-only.
    Failed(String),
}

/// The system-audio capture a meeting runs beside its microphone track.
/// The worker thread owns the CoreAudio objects, so stopping is a flag, a
/// join and one final report — never a CoreAudio call from the meeting's
/// own thread.
struct SystemTrack {
    stop: Arc<AtomicU32>,
    join: Option<thread::JoinHandle<()>>,
    outcome: std::sync::mpsc::Receiver<SystemOutcome>,
}

impl SystemTrack {
    /// End the capture and collect its report. The worker always sends one
    /// outcome before it exits; a panicked worker is a failure like any
    /// other, logged and swallowed — the meeting's mic track decides what
    /// the entry keeps.
    fn stop(mut self) -> SystemOutcome {
        self.stop.store(1, Ordering::Release);
        if let Some(join) = self.join.take() {
            if join.join().is_err() {
                return SystemOutcome::Failed("the system audio thread panicked".into());
            }
        }
        self.outcome.recv().unwrap_or(SystemOutcome::Failed(
            "the system audio thread reported nothing".into(),
        ))
    }
}

/// Best-effort file removal; the caller is already handling the loss.
fn remove_recording_quietly(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            error!("Failed to remove {}: {e}", path.display());
        }
    }
}

/// End a meeting's system track and turn its report into the pointer the
/// history entry keeps. `None` (nothing captured, nothing to merge) is a
/// normal outcome on any machine, not an error.
fn drain_system_track(
    hm: &HistoryManager,
    track: Option<SystemTrack>,
) -> Option<crate::managers::history::SystemAudioTrack> {
    let track = track?;
    match track.stop() {
        SystemOutcome::Captured {
            file_name,
            offset_ms,
        } => {
            debug!(
                "System audio track captured {} ({} ms after the mic)",
                file_name, offset_ms
            );
            Some(crate::managers::history::SystemAudioTrack {
                file_name,
                offset_ms,
            })
        }
        SystemOutcome::NoAudio => {
            debug!("The machine played nothing the tap could capture");
            None
        }
        SystemOutcome::Failed(why) => {
            warn!("System audio was not captured; the meeting is mic-only: {why}");
            None
        }
    }
}

/// Begin the meeting's system-audio track: only when this Mac can tap at all
/// — the CoreAudio process tap exists from macOS 14.4, and the check runs at
/// runtime because the app still deploys to 10.15 — and the setting asks
/// for it. `None` on any other platform, and the meeting records mic-only
/// with no error and no trace.
fn start_system_track(
    app: &AppHandle,
    settings: &AppSettings,
    mic_started: Instant,
    working_path: PathBuf,
) -> Option<SystemTrack> {
    if !crate::system_audio::supported() {
        debug!("System audio needs macOS 14.4 or newer; this Mac is older");
        return None;
    }
    if !settings.record_system_audio {
        debug!("System audio is off in settings; the meeting records mic-only");
        return None;
    }
    let _ = app; // the decision is made; the worker needs no app handle
    let stop = Arc::new(AtomicU32::new(0));
    let (tx, rx) = std::sync::mpsc::channel();
    let join = thread::Builder::new()
        .name("flowstate-meeting-system".into())
        .spawn({
            let stop = Arc::clone(&stop);
            move || system_track_worker(working_path, mic_started, stop, tx)
        })
        .ok()?;
    Some(SystemTrack {
        stop,
        join: Some(join),
        outcome: rx,
    })
}

/// The system track's whole life, on its own thread: prepare the tap (the
/// permission-free step the TCC spike measured), write its frames to the
/// system working file, and on the stop flag tear the tap down and report.
/// Its own thread because the tap's setup is CoreAudio's to pace — a
/// machine with an unusual audio device must never stall the meeting.
fn system_track_worker(
    working_path: PathBuf,
    mic_started: Instant,
    stop: Arc<AtomicU32>,
    out: std::sync::mpsc::Sender<SystemOutcome>,
) {
    #[cfg(target_os = "macos")]
    {
        use crate::system_audio::{FramesSink, SystemTap};

        // Labeled block: every failure path reports one outcome and jumps
        // to the send, so the channel contract is easy to see.
        let outcome = 'capture: {
            let tap = match SystemTap::prepare() {
                Ok(tap) => tap,
                Err(e) => break 'capture SystemOutcome::Failed(e.to_string()),
            };
            let working = match WorkingFile::create(&working_path, tap.sample_rate()) {
                Ok(working) => Arc::new(Mutex::new(working)),
                Err(e) => {
                    break 'capture SystemOutcome::Failed(format!(
                        "the system working file could not be created: {e}"
                    ))
                }
            };
            let sink: FramesSink = {
                let working = Arc::clone(&working);
                Arc::new(move |frames: &[f32]| {
                    working.lock().unwrap().write_frame(frames);
                })
            };
            let recorder = match tap.start(sink) {
                Ok(recorder) => recorder,
                Err(e) => {
                    if let Err(e) = working.lock().unwrap().discard() {
                        error!("Failed to clean up the system working file: {e}");
                    }
                    break 'capture SystemOutcome::Failed(e.to_string());
                }
            };
            let offset_ms = mic_started.elapsed().as_millis() as i64;
            info!(
                "System audio track recording ({} Hz), {offset_ms} ms after the mic",
                recorder.sample_rate()
            );
            while stop.load(Ordering::Acquire) == 0 {
                thread::park_timeout(std::time::Duration::from_millis(50));
            }
            recorder.stop();
            let mut working = working.lock().unwrap();
            if working.frames() == 0 {
                if let Err(e) = working.discard() {
                    error!("Failed to remove the empty system working file: {e}");
                }
                break 'capture SystemOutcome::NoAudio;
            }
            match working.finish() {
                Ok(file_name) => SystemOutcome::Captured {
                    file_name,
                    offset_ms,
                },
                Err(e) => SystemOutcome::Failed(e.to_string()),
            }
        };
        let _ = out.send(outcome);
    }
    #[cfg(not(target_os = "macos"))]
    {
        // No CoreAudio to tap; supported() already said so and the worker
        // never runs. Kept only so the meeting code is platform-clean.
        let _ = (working_path, mic_started, stop, out);
    }
}

/* ---------- the manager ----------------------------------------------------- */

struct MeetingSession {
    state: MeetingState,
    /// The working file the recording's frames go to, present exactly while
    /// the session records.
    working: Option<Arc<Mutex<WorkingFile>>>,
    /// The system-audio capture running beside it (S-5), when the Mac can
    /// tap and the setting asks for it. `None` means mic-only.
    system: Option<SystemTrack>,
}

/// Owns the meeting session for the whole app: the lifecycle commands
/// ([`start`](MeetingManager::start), [`stop`](MeetingManager::stop),
/// [`cancel`](MeetingManager::cancel)) and the status the frontend polls.
///
/// The microphone is the only source in this story; system audio (S-5) will
/// join as a second track. A meeting deliberately differs from a dictation in
/// two ways: it does not mute the output while recording (an hour of silenced
/// music is not what `mute_while_recording` was for), and it does not touch
/// the tray or the overlay — those belong to the dictation pipeline, and the
/// meeting's UI arrives with S-4b.
pub struct MeetingManager {
    app: AppHandle,
    /// The session's decisions and its working file, kept together so a stop
    /// or cancel always finds the file its session created.
    session: Mutex<MeetingSession>,
}

impl MeetingManager {
    pub fn new(app: &AppHandle) -> Arc<Self> {
        Arc::new(Self {
            app: app.clone(),
            session: Mutex::new(MeetingSession {
                state: MeetingState::default(),
                working: None,
                system: None,
            }),
        })
    }

    /// Start recording a meeting. Its audio is written to a working file in
    /// the recordings directory as it is captured, and the same live decode a
    /// dictation's preview uses keeps a running transcript.
    pub fn start(&self) -> Result<MeetingStatus> {
        let app = &self.app;
        let rm = app.state::<Arc<AudioRecordingManager>>();
        let tm = app.state::<Arc<TranscriptionManager>>();

        // The same start a dictation gets: warm the model and the VAD first,
        // and don't open the microphone when nothing can transcribe the meeting.
        tm.initiate_model_load();
        let rm_preload = Arc::clone(&rm);
        thread::spawn(move || {
            if let Err(e) = rm_preload.preload_vad() {
                debug!("VAD pre-load failed: {e}");
            }
        });
        if !tm.is_model_loaded() {
            let selected_model = get_settings(app).selected_model;
            app.state::<Arc<ModelManager>>()
                .get_model_path(&selected_model)
                .map_err(|e| anyhow!("No model can transcribe the meeting: {e}"))?;
        }

        let settings = get_settings(app);
        let model_supports_streaming = app
            .state::<Arc<ModelManager>>()
            .get_model_info(&settings.selected_model)
            .as_ref()
            .map(|model| model.supports_streaming)
            .unwrap_or(false);
        let vad_policy = if !settings.vad_enabled {
            VadPolicy::Disabled
        } else if model_supports_streaming {
            VadPolicy::Streaming
        } else {
            VadPolicy::Offline
        };
        // The live decode reuses the chunking/VAD loop for a running
        // transcript while the meeting records, exactly as a dictation's
        // live preview does.
        if live_decode(
            ModelTraits {
                streaming: model_supports_streaming,
                long_form: false,
            },
            settings.long_form_chunking,
        ) != LiveDecode::None
        {
            tm.start_stream(true);
        }

        let hm = app.state::<Arc<HistoryManager>>();
        let started = Instant::now();
        let started_unix = chrono::Utc::now().timestamp();
        let stamp = chrono::Utc::now().timestamp_millis();
        let file_name = format!("flowstate-meeting-{stamp}.wav");
        let working_path = hm
            .recordings_dir()
            .join(format!("{file_name}.{WORKING_SUFFIX}"));

        let mut session = self.session.lock().unwrap();
        session.state.start(
            started,
            started_unix,
            working_path.clone(),
            rm.is_recording(),
        )?;

        let working = match WorkingFile::create(&working_path, constants::WHISPER_SAMPLE_RATE) {
            Ok(working) => Arc::new(Mutex::new(working)),
            Err(e) => {
                session.state.abort();
                return Err(anyhow!(
                    "Failed to create the meeting's working file {}: {e}",
                    working_path.display()
                ));
            }
        };

        // The tap receives exactly the frames the sink keeps, from the same
        // callback the stream router is fed by. Installed before the capture
        // starts and only while this meeting records, so no dictation's audio
        // can end up in a meeting's file.
        let previous_tap = rm.set_frame_tap(Some({
            let working = Arc::clone(&working);
            Arc::new(move |frame| {
                working.lock().unwrap().write_frame(frame);
            })
        }));
        debug_assert!(
            previous_tap.is_none(),
            "a frame tap was installed while no meeting was recording"
        );

        // The live transcript follows the live decode that already runs:
        // each segment it finishes becomes a `meeting-segment` event.
        // Installed before the capture starts, so not even the first chunk
        // can be decoded before the window can hear it.
        self.install_live_transcript();
        if let Err(e) = rm.try_start_recording(MEETING_BINDING_ID, vad_policy) {
            rm.set_frame_tap(None);
            self.clear_live_transcript();
            if let Err(discard_error) = working.lock().unwrap().discard() {
                error!("Failed to clean up the meeting's working file: {discard_error}");
            }
            session.state.abort();
            tm.cancel_stream();
            return Err(e.context("Failed to start the meeting recording"));
        }

        session.working = Some(working);
        // S-5: system audio joins as a second track — what the machine
        // itself plays (the far end of a Zoom or Teams call), tapped on the
        // process-tap route the TCC spike measured as permission-free.
        // Its own thread, started after the microphone: a slow or refused
        // tap never delays or fails the meeting, and a Mac that cannot tap
        // (macOS < 14.4, checked at runtime) simply records mic-only.
        session.system = start_system_track(
            app,
            &settings,
            started,
            hm.recordings_dir().join(format!(
                "flowstate-meeting-{stamp}-system.wav.{WORKING_SUFFIX}"
            )),
        );
        let status = session.state.status(Instant::now());
        drop(session);

        debug!("Meeting recording started: {}", working_path.display());
        emit_meeting_state(app, &status);
        // Starting from Settings also dismisses the disc's "hide for an
        // hour": a live session always brings the bar back.
        crate::overlay::unhide_meeting_bar();
        // The floating bar follows the session: the disc becomes the pill.
        crate::overlay::refresh_meeting_bar(app);
        Ok(status)
    }

    /// Stop the meeting: finalize its working file as a recording, save the
    /// history entry (`source: "meeting"`, duration in its metadata) and
    /// queue its transcription. The entry's text arrives when the background
    /// job finishes; this returns as soon as the entry exists.
    pub fn stop(&self) -> Result<StoppedMeeting> {
        let app = &self.app;
        let rm = app.state::<Arc<AudioRecordingManager>>();
        let tm = app.state::<Arc<TranscriptionManager>>();
        let hm = app.state::<Arc<HistoryManager>>();
        let jobs = app.state::<Arc<TranscriptionJobManager>>();

        let mut session = self.session.lock().unwrap();
        let decision = session.state.stop(Instant::now())?;
        let working = session
            .working
            .take()
            .expect("a recording session has its working file");
        let system_track = session.system.take();

        // Stop the capture first: no more frames flow, so the file can be
        // finalized. The samples the sink accumulated are dropped — the
        // working file already has every frame of them.
        let cancel_generation = rm.cancel_generation();
        let _ = rm.stop_recording(MEETING_BINDING_ID, cancel_generation);
        rm.set_frame_tap(None);
        // The live decode was the running preview; the entry's text comes
        // from the background job, which transcribes the finished recording
        // chunk by chunk and yields to dictation.
        tm.cancel_stream();
        // No more live segments: the job that follows is a batch one.
        self.clear_live_transcript();

        let mut working = working.lock().unwrap();
        // The system track ends with the meeting; whatever it captured rides
        // along on the entry so the transcription can merge the two.
        let system_info = drain_system_track(&hm, system_track);
        let entry_id = if working.frames() == 0 {
            debug!("The meeting captured no audio; discarding the working file");
            if let Err(e) = working.discard() {
                error!("Failed to clean up the empty meeting working file: {e}");
            }
            // No mic track means no entry, so the system track's finished
            // file must not be orphaned either.
            if let Some(system) = &system_info {
                remove_recording_quietly(&hm.recordings_dir().join(&system.file_name));
            }
            None
        } else {
            let file_name = working.finish()?;
            let entry = hm.save_meeting_entry(
                file_name,
                decision.started_at_unix,
                decision.duration_secs,
                system_info,
            )?;
            jobs.queue_saved_recording(entry.id);
            Some(entry.id)
        };
        drop(working);
        drop(session);

        info!(
            "Meeting stopped after {}s; history entry {:?}",
            decision.duration_secs, entry_id
        );
        emit_meeting_state(app, &MeetingStatus::Idle);
        // The history entry is in place: the pill gives the window back to the
        // idle disc. Skipped while a dictation overlay owns the window — its
        // hide path refreshes the bar when it is done.
        crate::overlay::refresh_meeting_bar(app);
        Ok(StoppedMeeting {
            entry_id,
            started_at: decision.started_at_unix,
            duration_secs: decision.duration_secs,
        })
    }

    /// Cancel the meeting: stop the capture, remove the working file, save
    /// no history entry.
    pub fn cancel(&self) -> Result<()> {
        let app = &self.app;
        let rm = app.state::<Arc<AudioRecordingManager>>();
        let tm = app.state::<Arc<TranscriptionManager>>();
        let hm = app.state::<Arc<HistoryManager>>();

        let mut session = self.session.lock().unwrap();
        let working_path = session.state.cancel()?;
        let working = session
            .working
            .take()
            .expect("a recording session has its working file");
        let system_track = session.system.take();

        rm.cancel_recording();
        rm.set_frame_tap(None);
        tm.cancel_stream();
        self.clear_live_transcript();

        if let Err(e) = working.lock().unwrap().discard() {
            error!(
                "Failed to remove the cancelled meeting's working file {}: {e}",
                working_path.display()
            );
        }
        drop(session);

        info!("Meeting cancelled; the working file was removed");
        emit_meeting_state(app, &MeetingStatus::Idle);
        crate::overlay::refresh_meeting_bar(app);
        Ok(())
    }

    /// Install the sink that turns the live decode's finished segments into
    /// `meeting-segment` events, numbered from 0. Present exactly while this
    /// session records, so no other live decode — a dictation's preview, a
    /// batch job — can feed the meeting window.
    fn install_live_transcript(&self) {
        let app = self.app.clone();
        let index = Arc::new(AtomicU32::new(0));
        self.app
            .state::<Arc<TranscriptionManager>>()
            .set_live_segment_sink(Some(Arc::new(move |segment| {
                let event = MeetingSegment {
                    index: index.fetch_add(1, Ordering::Relaxed),
                    t0_ms: segment.t0_ms,
                    t1_ms: segment.t1_ms,
                    text: segment.text,
                };
                if let Err(e) = event.emit(&app) {
                    warn!("Failed to emit a meeting segment: {e}");
                }
            })));
    }

    /// Remove the live-transcript sink: the session is over, so the meeting
    /// window's list is complete and later decodes report nothing.
    fn clear_live_transcript(&self) {
        self.app
            .state::<Arc<TranscriptionManager>>()
            .set_live_segment_sink(None);
    }

    /// The session's stage right now.
    pub fn status(&self) -> MeetingStatus {
        self.session.lock().unwrap().state.status(Instant::now())
    }

    /// Whether a meeting is recording.
    pub fn is_recording(&self) -> bool {
        self.session.lock().unwrap().state.is_recording()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio_toolkit::read_wav_samples;
    use std::time::Duration;

    fn working_path() -> PathBuf {
        PathBuf::from("/tmp/flowstate-meeting-unit.wav.partial")
    }

    /* ---- the session's decisions: start → stop and start → cancel -------- */

    #[test]
    fn start_then_stop_hands_back_the_working_file_and_duration() {
        let mut state = MeetingState::default();
        let now = Instant::now();

        state
            .start(now, 1_700_000_000, working_path(), false)
            .expect("an idle session starts");
        assert_eq!(
            state.status(now),
            MeetingStatus::Recording {
                started_at: 1_700_000_000,
                duration_secs: 0
            }
        );

        // A meeting is long: stop an hour and a half in.
        let later = now + Duration::from_secs(90 * 60);
        let decision = state.stop(later).expect("a recording session stops");
        assert_eq!(decision.working_path, working_path());
        assert_eq!(decision.started_at_unix, 1_700_000_000);
        assert_eq!(decision.duration_secs, 90 * 60);
        assert_eq!(state.status(later), MeetingStatus::Idle);

        // The session ended, so the next one can begin.
        state
            .start(later, 1_700_090_000, working_path(), false)
            .expect("a stopped session can start again");
    }

    #[test]
    fn start_then_cancel_hands_back_the_working_file_only() {
        let mut state = MeetingState::default();
        let now = Instant::now();
        state.start(now, 1, working_path(), false).unwrap();

        let path = state.cancel().expect("a recording session cancels");
        assert_eq!(path, working_path());
        assert_eq!(state.status(now), MeetingStatus::Idle);
        // A cancelled session is not a stopped one: nothing remains to stop.
        assert_eq!(state.stop(now), Err(MeetingError::NotRunning));
    }

    #[test]
    fn a_second_start_while_recording_is_refused() {
        let mut state = MeetingState::default();
        let now = Instant::now();
        state.start(now, 1, working_path(), false).unwrap();
        assert_eq!(
            state.start(now, 2, working_path(), false),
            Err(MeetingError::AlreadyRunning)
        );
    }

    #[test]
    fn a_start_while_a_dictation_holds_the_microphone_is_refused() {
        let mut state = MeetingState::default();
        assert_eq!(
            state.start(Instant::now(), 1, working_path(), true),
            Err(MeetingError::MicrophoneBusy)
        );
        assert_eq!(state.status(Instant::now()), MeetingStatus::Idle);
    }

    #[test]
    fn stop_or_cancel_without_a_session_is_refused() {
        let mut state = MeetingState::default();
        assert_eq!(state.stop(Instant::now()), Err(MeetingError::NotRunning));
        assert_eq!(state.cancel(), Err(MeetingError::NotRunning));
    }

    #[test]
    fn abort_rolls_back_a_failed_start() {
        let mut state = MeetingState::default();
        let now = Instant::now();
        state.start(now, 1, working_path(), false).unwrap();
        state.abort();
        assert_eq!(state.status(now), MeetingStatus::Idle);
        state
            .start(now, 2, working_path(), false)
            .expect("an aborted session can start again");
    }

    /* ---- the working file: written underways, cleaned on cancel ------------ */

    /// A unique working path in the temp directory and its finished name.
    fn temp_working_file(tag: &str) -> (PathBuf, String) {
        let file_name = format!("flowstate-meeting-test-{tag}-{}.wav", std::process::id());
        let path = std::env::temp_dir().join(format!("{file_name}.{WORKING_SUFFIX}"));
        (path, file_name)
    }

    #[test]
    fn working_file_is_written_underways_and_finishes_as_a_recording() {
        let (path, file_name) = temp_working_file("finish");
        let mut working = WorkingFile::create(&path, 16_000).expect("the working file is created");

        // Forty 32 ms frames of speech-level audio.
        let frame: Vec<f32> = vec![0.25; 512];
        for _ in 0..40 {
            working.write_frame(&frame);
        }
        assert_eq!(working.frames(), 40 * 512);

        // The periodic flush already left a playable WAV of the meeting so
        // far: after the 32nd frame the checkpoint covers 16384 samples.
        let checkpointed = read_wav_samples(&path).expect("the checkpointed file is readable");
        assert_eq!(checkpointed.len(), 32 * 512);

        working.finish().expect("the working file finishes");
        let finished = path.with_file_name(&file_name);
        let samples = read_wav_samples(&finished).expect("the finished recording is readable");
        assert_eq!(samples.len(), 40 * 512);
        // The 16-bit round trip keeps the level.
        assert!(samples.iter().all(|s| (s - 0.25).abs() < 0.01));
        // The working file is gone; only the recording remains.
        assert!(!path.exists());
    }

    #[test]
    fn a_discarded_working_file_leaves_nothing_behind() {
        let (path, _file_name) = temp_working_file("cancel");
        let mut working = WorkingFile::create(&path, 16_000).expect("the working file is created");
        working.write_frame(&[0.5; 512]);
        working.discard().expect("the working file is discarded");
        assert!(!path.exists());
    }
}
