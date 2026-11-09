//! Background transcriptions (a file, or an entry transcribed again), one at
//! a time in chunks that yield to dictation. Progress is kept in memory.

use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use std::cell::Cell;
use std::collections::VecDeque;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::actions::process_transcription_output;
use crate::audio_toolkit::{decode_audio_file, read_wav_samples, save_wav_file};
use crate::managers::history::{HistoryEntry, HistoryManager};
use crate::managers::transcription::{
    panic_payload_message, ChunkControl, Transcription, TranscriptionManager,
};

/// How often a waiting chunk checks whether dictation has finished.
const DICTATION_POLL: Duration = Duration::from_millis(200);

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Type)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum TranscriptionJobStatus {
    /// Waiting behind another job.
    Queued,
    /// `percent` of the audio is transcribed.
    Running {
        percent: u8,
    },
    /// The entry has its new text, or the job was cancelled.
    Finished,
    Failed {
        error: String,
    },
}

/// A background transcription of one History entry.
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct TranscriptionJob {
    pub entry_id: i64,
    pub status: TranscriptionJobStatus,
}

struct Running {
    entry_id: i64,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
struct Jobs {
    /// Entry ids waiting to be transcribed again. They go before files:
    /// someone is waiting on them.
    retries: VecDeque<i64>,
    /// Entry ids of files waiting to be transcribed.
    files: VecDeque<i64>,
    running: Option<Running>,
}

impl Jobs {
    fn has(&self, entry_id: i64) -> bool {
        self.running
            .as_ref()
            .is_some_and(|r| r.entry_id == entry_id)
            || self.retries.contains(&entry_id)
            || self.files.contains(&entry_id)
    }

    fn is_idle(&self) -> bool {
        self.running.is_none() && self.retries.is_empty() && self.files.is_empty()
    }

    /// Drops the entry from the queues; whether it was in one.
    fn unqueue(&mut self, entry_id: i64) -> bool {
        let before = self.retries.len() + self.files.len();
        self.retries.retain(|&id| id != entry_id);
        self.files.retain(|&id| id != entry_id);
        self.retries.len() + self.files.len() != before
    }
}

pub struct TranscriptionJobManager {
    app: AppHandle,
    jobs: Mutex<Jobs>,
    wake: Condvar,
}

impl TranscriptionJobManager {
    /// Starts the thread that runs the jobs.
    pub fn new(app: &AppHandle) -> Arc<Self> {
        let manager = Arc::new(Self {
            app: app.clone(),
            jobs: Mutex::new(Jobs::default()),
            wake: Condvar::new(),
        });
        let runner = Arc::clone(&manager);
        thread::Builder::new()
            .name("transcription-jobs".into())
            .spawn(move || runner.run_jobs())
            .expect("Failed to start the transcription job thread");
        manager
    }

    fn lock(&self) -> MutexGuard<'_, Jobs> {
        self.jobs
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Decodes `path` into a recording, then adds it to History and queues
    /// it. The recording exists first, so no entry is ever without audio.
    pub fn transcribe_file(&self, path: PathBuf) -> Result<HistoryEntry> {
        // A malformed file may panic the decoder.
        let samples = catch_unwind(AssertUnwindSafe(|| decode_audio_file(&path))).unwrap_or_else(
            |payload| {
                Err(anyhow!(
                    "Failed to read the file: {}",
                    panic_payload_message(payload.as_ref())
                ))
            },
        )?;
        if samples.is_empty() {
            return Err(anyhow!("The file has no audio"));
        }

        let hm = self.app.state::<Arc<HistoryManager>>();
        let file_name = format!("handy-file-{}.wav", chrono::Utc::now().timestamp_millis());
        let recording = hm.get_audio_file_path(&file_name);
        // Written aside and renamed, so a crash never leaves a partial one.
        let partial = recording.with_extension("wav.partial");
        if let Err(e) = save_wav_file(&partial, &samples)
            .and_then(|()| std::fs::rename(&partial, &recording).map_err(Into::into))
        {
            remove_recording(&partial);
            return Err(e);
        }
        drop(samples);

        let source_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        let entry = match hm.save_file_entry(file_name, source_name) {
            Ok(entry) => entry,
            Err(e) => {
                remove_recording(&recording);
                return Err(e);
            }
        };
        info!("Queued file transcription for history entry {}", entry.id);
        self.queue(entry.id, false);
        Ok(entry)
    }

    /// Queues the entry to be transcribed again from its recording, ahead
    /// of any file.
    pub fn retry(&self, entry_id: i64) {
        self.queue(entry_id, true);
    }

    /// Queues an entry whose recording is already saved (a stopped meeting)
    /// for background transcription. The entry exists with no text yet; the
    /// job reads the recording chunk by chunk and fills it in.
    pub fn queue_saved_recording(&self, entry_id: i64) {
        self.queue(entry_id, false);
    }

    /// Queues the entry to be transcribed from its recording, unless it
    /// already is.
    fn queue(&self, entry_id: i64, retry: bool) {
        {
            let mut jobs = self.lock();
            if jobs.has(entry_id) {
                return;
            }
            if retry {
                jobs.retries.push_back(entry_id);
            } else {
                jobs.files.push_back(entry_id);
            }
            self.sync_job_active(&jobs);
        }
        self.emit(entry_id, TranscriptionJobStatus::Queued);
        crate::tray::update_tray_menu(&self.app);
        self.wake.notify_one();
    }

    /// Stops the entry's job: a queued one is dropped, a running one stops
    /// before its next chunk.
    pub fn cancel(&self, entry_id: i64) {
        let (was_queued, idle) = {
            let mut jobs = self.lock();
            let was_queued = jobs.unqueue(entry_id);
            if let Some(running) = jobs.running.as_ref().filter(|r| r.entry_id == entry_id) {
                running.cancelled.store(true, Ordering::Release);
            }
            self.sync_job_active(&jobs);
            (was_queued, jobs.is_idle())
        };
        // A running job reports its own end from the job loop; a queued one
        // never reaches it, so it is reported here.
        if was_queued {
            if idle && !crate::tray::dictation_in_progress(&self.app) {
                self.app
                    .state::<Arc<TranscriptionManager>>()
                    .maybe_unload_immediately("background transcription");
            }
            crate::tray::update_tray_menu(&self.app);
            self.emit(entry_id, TranscriptionJobStatus::Finished);
        }
    }

    /// Keeps the model held for as long as any job is queued or running.
    fn sync_job_active(&self, jobs: &Jobs) {
        self.app
            .state::<Arc<TranscriptionManager>>()
            .set_job_active(!jobs.is_idle());
    }

    fn emit(&self, entry_id: i64, status: TranscriptionJobStatus) {
        if let Err(e) = (TranscriptionJob { entry_id, status }).emit(&self.app) {
            error!("Failed to emit transcription job event: {}", e);
        }
    }

    fn run_jobs(&self) {
        loop {
            let (entry_id, cancelled) = {
                let mut jobs = self.lock();
                let entry_id = loop {
                    if let Some(entry_id) =
                        jobs.retries.pop_front().or_else(|| jobs.files.pop_front())
                    {
                        break entry_id;
                    }
                    jobs = self
                        .wake
                        .wait(jobs)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                };
                let cancelled = Arc::new(AtomicBool::new(false));
                jobs.running = Some(Running {
                    entry_id,
                    cancelled: Arc::clone(&cancelled),
                });
                (entry_id, cancelled)
            };
            self.emit(entry_id, TranscriptionJobStatus::Running { percent: 0 });

            // A panic fails only this job; the rest of the loop still has to
            // run, or the model would stay held and the queue stuck.
            let result = catch_unwind(AssertUnwindSafe(|| self.run_job(entry_id, &cancelled)))
                .unwrap_or_else(|payload| Err(crashed(payload.as_ref())));

            let idle = {
                let mut jobs = self.lock();
                jobs.running = None;
                self.sync_job_active(&jobs);
                jobs.is_idle()
            };
            let tm = self.app.state::<Arc<TranscriptionManager>>();
            if idle {
                // A dictation under way unloads the model itself when it ends.
                if !crate::tray::dictation_in_progress(&self.app) {
                    tm.maybe_unload_immediately("background transcription");
                }
                crate::tray::update_tray_menu(&self.app);
            }
            let status = match result {
                Err(e) if !cancelled.load(Ordering::Acquire) => {
                    warn!("Transcription of history entry {} failed: {}", entry_id, e);
                    TranscriptionJobStatus::Failed {
                        error: e.to_string(),
                    }
                }
                _ => TranscriptionJobStatus::Finished,
            };
            self.emit(entry_id, status);
        }
    }

    fn run_job(&self, entry_id: i64, cancelled: &AtomicBool) -> Result<()> {
        let hm = Arc::clone(&self.app.state::<Arc<HistoryManager>>());
        let tm = Arc::clone(&self.app.state::<Arc<TranscriptionManager>>());
        let entry = tauri::async_runtime::block_on(hm.get_entry_by_id(entry_id))?
            .ok_or_else(|| anyhow!("History entry {} not found", entry_id))?;
        // A compressed meeting is decoded from whatever format it is in; WAV
        // keeps its direct reader so nothing changes for dictations.
        let audio_path = hm.get_audio_file_path(&entry.file_name);
        let samples = if entry.file_name.ends_with(".wav") {
            read_wav_samples(audio_path).map_err(|e| anyhow!("Failed to load audio: {}", e))?
        } else {
            crate::audio_toolkit::audio::decode_audio_file(&audio_path)
                .map_err(|e| anyhow!("Failed to load audio: {}", e))?
        };
        // Counted before the audio is handed to the transcriber: the encode is
        // checked against it afterwards.
        let wav_samples = samples.len();
        if samples.is_empty() {
            return Err(anyhow!("Recording has no audio samples"));
        }

        let control = Control {
            manager: self,
            entry_id: entry.id,
            cancelled,
            percent: Cell::new(0),
        };
        // S-29: a finished meeting's audio is diarized after the recording —
        // one batch pass of "who spoke when" — and its speaker turns are
        // merged into the transcript's timed segments. A meeting on a machine
        // without the model (or a run that failed) is a skip, not an error:
        // the transcript stands exactly as it was, without speakers.
        let is_meeting = entry.source == Some(crate::managers::history::HistorySource::Meeting);
        let speaker_turns =
            if is_meeting {
                crate::managers::diarize::diarize_meeting(&self.app, &samples)
            } else {
                None
            };
        let Some(Transcription {
            text,
            language,
            mut segments,
        }) = tm.transcribe_in_chunks(samples, &control, is_meeting)?
        else {
            return Ok(());
        };
        if text.is_empty() {
            return Err(anyhow!("Recording contains no speech"));
        }
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }

        let processed = tauri::async_runtime::block_on(process_transcription_output(
            &self.app,
            &text,
            entry.post_process_requested,
        ));
        // The speakers the diarization found ride along on the segments.
        if let Some(turns) = speaker_turns {
            crate::managers::diarize::apply_speakers(&mut segments, &turns);
        }
        // Post-processing can take a while; a cancel during it still wins.
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        hm.update_transcription(
            entry.id,
            text,
            processed.post_processed_text,
            processed.post_process_prompt,
            language,
            segments,
        )?;

        // S-5: a meeting that also captured system audio — what the machine
        // itself was playing, the far end of a call included — has its second
        // track transcribed on its own and merged into the segments by
        // timestamp, so the transcript reads as one conversation. A meeting
        // that captured none (the setting off, macOS < 14.4, a refused tap)
        // has nothing here, which is not an error: the mic track stands
        // exactly as it was.
        if entry.source == Some(crate::managers::history::HistorySource::Meeting) {
            if let Some(system) = entry.metadata.as_ref().and_then(|m| m.system_audio.clone()) {
                if let Err(e) = self.transcribe_system_track(&hm, &tm, entry.id, &system, cancelled)
                {
                    warn!(
                        "Entry {}'s system audio track was not transcribed: {}",
                        entry.id, e
                    );
                }
            }
        }
        // A finished meeting is done with its raw audio: the recording is
        // transcoded to a twelfth of the size, and the transcript gets the
        // cleanup pass the reference keeps beside its live text. Both are
        // best-effort — a failure leaves the WAV and the raw text in place.
        if is_meeting && !cancelled.load(Ordering::Acquire) {
            match crate::managers::compress_meeting::compress_meeting_recording(
                &self.app,
                &hm,
                entry.id,
                &entry.file_name,
                wav_samples,
            ) {
                Ok(new_name) => info!("Meeting {} recorded to {new_name}", entry.id),
                Err(e) => warn!("Meeting {}'s recording was left as WAV: {e}", entry.id),
            }
            if let Ok(Some(current)) =
                tauri::async_runtime::block_on(hm.get_entry_by_id(entry.id))
            {
                match crate::managers::meeting_refine::refine_meeting_transcript(
                    &self.app,
                    &hm,
                    entry.id,
                    &current.transcription_text,
                ) {
                    Ok(0) => debug!("Meeting {} had no cleaned transcript", entry.id),
                    Ok(chunks) => info!("Meeting {} cleaned in {chunks} chunks", entry.id),
                    Err(e) => warn!("Meeting {}'s transcript was not cleaned: {e}", entry.id),
                }
            }
        }
        Ok(())
    }

    /// Transcribe a meeting's system-audio track and merge it into the
    /// entry's segments. The track's own timestamps are shifted by the offset
    /// stored with it (how late its capture began after the mic's), the mic's
    /// segments keep their speakers, and the two are sorted into one line by
    /// start time; the entry's text is rebuilt from the merged segments.
    fn transcribe_system_track(
        &self,
        hm: &Arc<HistoryManager>,
        tm: &Arc<TranscriptionManager>,
        entry_id: i64,
        system: &crate::managers::history::SystemAudioTrack,
        cancelled: &AtomicBool,
    ) -> Result<()> {
        let path = hm.get_audio_file_path(&system.file_name);
        // decode_audio_file brings the track to the 16 kHz mono every
        // transcription reads, whatever rate the machine plays at.
        let samples = crate::audio_toolkit::audio::decode_audio_file(&path)?;
        if samples.is_empty() {
            return Err(anyhow!("the system track has no audio"));
        }
        let control = Control {
            manager: self,
            entry_id,
            cancelled,
            percent: Cell::new(0),
        };
        let Some(Transcription {
            text,
            segments: mut system_segments,
            ..
        // Always a meeting's second track, so same restriction.
        }) = tm.transcribe_in_chunks(samples, &control, true)?
        else {
            return Ok(());
        };
        // A machine that stayed silent is a skip, not an error.
        if text.is_empty() {
            return Ok(());
        }
        if cancelled.load(Ordering::Acquire) {
            return Ok(());
        }
        // Line the system track's timeline up with the mic's.
        for segment in &mut system_segments {
            segment.t0_ms += system.offset_ms;
            segment.t1_ms += system.offset_ms;
        }
        let entry = tauri::async_runtime::block_on(hm.get_entry_by_id(entry_id))?
            .ok_or_else(|| anyhow!("History entry {} not found", entry_id))?;
        let mut merged = entry
            .metadata
            .as_ref()
            .and_then(|m| m.segments.clone())
            .unwrap_or_default();
        merged.extend(system_segments);
        // Stable, so at equal timestamps the mic's segment (with its
        // diarized speakers) reads before the system's.
        merged.sort_by_key(|segment| segment.t0_ms);
        let text = merged
            .iter()
            .map(|segment| segment.text.as_str())
            .collect::<Vec<_>>()
            .join(" ");
        hm.save_merged_transcript(entry_id, text, merged)?;
        Ok(())
    }
}

/// Best-effort cleanup; the caller is already reporting the failure.
fn remove_recording(path: &Path) {
    if let Err(e) = std::fs::remove_file(path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            error!("Failed to remove {}: {}", path.display(), e);
        }
    }
}

fn crashed(payload: &(dyn std::any::Any + Send)) -> anyhow::Error {
    anyhow!("Transcription crashed: {}", panic_payload_message(payload))
}

struct Control<'a> {
    manager: &'a TranscriptionJobManager,
    entry_id: i64,
    cancelled: &'a AtomicBool,
    /// The last percent reported; the job starts at 0.
    percent: Cell<u8>,
}

impl ChunkControl for Control<'_> {
    fn proceed(&self) -> bool {
        // Dictation goes first: wait out a recording and its transcription.
        // A chunk started while a stream is open would end that stream.
        loop {
            if self.cancelled.load(Ordering::Acquire) {
                return false;
            }
            if !crate::tray::dictation_in_progress(&self.manager.app) {
                return true;
            }
            thread::sleep(DICTATION_POLL);
        }
    }

    fn progress(&self, done: usize, total: usize) {
        // The chunk a cancel caught still finishes; don't report it as running.
        if self.cancelled.load(Ordering::Acquire) {
            return;
        }
        let percent = (done * 100 / total.max(1)).min(100) as u8;
        if self.percent.replace(percent) == percent {
            return;
        }
        self.manager
            .emit(self.entry_id, TranscriptionJobStatus::Running { percent });
    }
}
