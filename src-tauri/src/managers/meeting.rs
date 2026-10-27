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
use crate::settings::get_settings;
use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::path::{Path, PathBuf};
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

/// The WAV every recording is stored as: 16 kHz mono 16-bit.
const WAV_SPEC: hound::WavSpec = hound::WavSpec {
    channels: 1,
    sample_rate: constants::WHISPER_SAMPLE_RATE,
    bits_per_sample: 16,
    sample_format: hound::SampleFormat::Int,
};

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
    fn create(path: &Path) -> Result<Self> {
        let writer = hound::WavWriter::create(path, WAV_SPEC)?;
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

/* ---------- the manager ----------------------------------------------------- */

struct MeetingSession {
    state: MeetingState,
    /// The working file the recording's frames go to, present exactly while
    /// the session records.
    working: Option<Arc<Mutex<WorkingFile>>>,
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
            tm.start_stream();
        }

        let hm = app.state::<Arc<HistoryManager>>();
        let file_name = format!(
            "flowstate-meeting-{}.wav",
            chrono::Utc::now().timestamp_millis()
        );
        let working_path = hm
            .recordings_dir()
            .join(format!("{file_name}.{WORKING_SUFFIX}"));

        let mut session = self.session.lock().unwrap();
        session.state.start(
            Instant::now(),
            chrono::Utc::now().timestamp(),
            working_path.clone(),
            rm.is_recording(),
        )?;

        let working = match WorkingFile::create(&working_path) {
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

        if let Err(e) = rm.try_start_recording(MEETING_BINDING_ID, vad_policy) {
            rm.set_frame_tap(None);
            if let Err(discard_error) = working.lock().unwrap().discard() {
                error!("Failed to clean up the meeting's working file: {discard_error}");
            }
            session.state.abort();
            tm.cancel_stream();
            return Err(e.context("Failed to start the meeting recording"));
        }

        session.working = Some(working);
        let status = session.state.status(Instant::now());
        drop(session);

        debug!("Meeting recording started: {}", working_path.display());
        emit_meeting_state(app, &status);
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

        let mut working = working.lock().unwrap();
        let entry_id = if working.frames() == 0 {
            debug!("The meeting captured no audio; discarding the working file");
            if let Err(e) = working.discard() {
                error!("Failed to clean up the empty meeting working file: {e}");
            }
            None
        } else {
            let file_name = working.finish()?;
            let entry =
                hm.save_meeting_entry(file_name, decision.started_at_unix, decision.duration_secs)?;
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

        let mut session = self.session.lock().unwrap();
        let working_path = session.state.cancel()?;
        let working = session
            .working
            .take()
            .expect("a recording session has its working file");

        rm.cancel_recording();
        rm.set_frame_tap(None);
        tm.cancel_stream();

        if let Err(e) = working.lock().unwrap().discard() {
            error!(
                "Failed to remove the cancelled meeting's working file {}: {e}",
                working_path.display()
            );
        }
        drop(session);

        info!("Meeting cancelled; the working file was removed");
        emit_meeting_state(app, &MeetingStatus::Idle);
        Ok(())
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
        let mut working = WorkingFile::create(&path).expect("the working file is created");

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
        let mut working = WorkingFile::create(&path).expect("the working file is created");
        working.write_frame(&[0.5; 512]);
        working.discard().expect("the working file is discarded");
        assert!(!path.exists());
    }
}
