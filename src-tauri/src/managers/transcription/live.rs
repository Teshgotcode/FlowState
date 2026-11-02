//! A recording's live decode: a stream the model decodes as audio arrives,
//! or long-form chunks decoded as the recording goes.

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use log::{debug, error, info};
use transcribe_cpp::StreamOptions;

use super::chunked::{ChunkSetup, ChunkedDecode, PendingSetup};
use super::policy::LiveDecode;
use super::{
    emit_stream_text, join_chunks, lock_perf, timed_segments, LiveSegment, PreviewScript,
    RunOutcome, RunSetup, StreamFinalizeReply, StreamPerf, TranscriptSegment, TranscriptionManager,
};
use crate::audio_toolkit::OutputLanguageEvidence;
use crate::engine_supervisor::{EngineError, LoadedInfo, StreamHandle, StreamProgress};
use crate::settings::{get_settings, AppSettings};

/// One recording's live decode.
pub(super) struct LiveSession<'a> {
    decoder: LiveDecoder<'a>,
}

enum LiveDecoder<'a> {
    /// With the run setup its result finishes with.
    Stream(StreamSession<'a>, Box<RunSetup>),
    /// Its setup is resolved by its first chunk.
    Chunked(Box<ChunkedDecode<'a>>),
}

/// A decoded text, the language the model detected in it (if any), and its
/// timed segments on the recording's timeline.
type Decoded = (String, Option<String>, Vec<TranscriptSegment>);

impl<'a> LiveSession<'a> {
    /// Begins the live decode `mode` picks on the loaded model. `None` when
    /// there is none or it couldn't begin: the recording is then transcribed
    /// in batch when it stops.
    pub(super) fn begin(
        tm: &'a TranscriptionManager,
        mode: LiveDecode,
        info: &LoadedInfo,
        model_id: &str,
    ) -> Option<Self> {
        if mode == LiveDecode::None {
            return None;
        }
        // The same run setup as the batch path: task + language gated
        // against what the model actually advertises.
        let settings = get_settings(&tm.app_handle);
        let plan = tm.language_plan(&settings, model_id, info);
        let decoder = match mode {
            LiveDecode::None => unreachable!("returned above"),
            LiveDecode::Stream => {
                // A stream's language is fixed before any audio, so it can't
                // be detected; a detect plan falls back.
                let setup = tm.setup_for(info, &settings, &plan, &[]);
                let stream = StreamSession::begin(tm, info, &settings, &setup, model_id)?;
                LiveDecoder::Stream(stream, Box::new(setup))
            }
            LiveDecode::Chunked => {
                info!("Live long-form chunking started (model '{}')", model_id);
                let backend = settings.vad_backend;
                LiveDecoder::Chunked(Box::new(ChunkedDecode::new(
                    tm,
                    tm.engine.cancel_mark(),
                    ChunkSetup::Pending(Box::new(PendingSetup {
                        plan,
                        info: info.clone(),
                        settings,
                    })),
                    model_id,
                    backend,
                    // A live chunked decode reports every chunk it finishes,
                    // so a meeting window can show the transcript as it is
                    // spoken (a batch clip's chunks do not).
                    true,
                )))
            }
        };
        Some(Self { decoder })
    }

    pub(super) fn feed(&mut self, pcm: Vec<f32>) {
        match &mut self.decoder {
            LiveDecoder::Stream(stream, _) => stream.feed(pcm),
            LiveDecoder::Chunked(chunks) => chunks.push(pcm),
        }
    }

    /// The live result; `None` to transcribe the recording in batch instead.
    /// `recorded_samples` is the whole recording's length.
    pub(super) fn finish(self, recorded_samples: usize) -> StreamFinalizeReply {
        match self.decoder {
            LiveDecoder::Stream(stream, setup) => {
                Ok(stream
                    .finish()?
                    .map(|(text, model_detected_language, segments)| RunOutcome {
                        setup: *setup,
                        text,
                        model_detected_language,
                        segments,
                    }))
            }
            LiveDecoder::Chunked(chunks) => finish_chunks(*chunks, recorded_samples),
        }
    }
}

fn finish_chunks(
    chunks: ChunkedDecode<'_>,
    recorded_samples: usize,
) -> Result<Option<RunOutcome>, EngineError> {
    // A failure stops the decode short of the recording; report the failure.
    if chunks.samples() != recorded_samples && !chunks.failed() {
        debug!(
            "Live chunks saw {} of the recording's {} samples; transcribing it in batch",
            chunks.samples(),
            recorded_samples
        );
        return Ok(None);
    }
    let while_recording = chunks.decoded();
    match chunks.finish() {
        Ok((transcripts, setup)) => {
            if while_recording > 0 {
                info!(
                    "{} of {} chunks were decoded while recording",
                    while_recording,
                    transcripts.len()
                );
            }
            let (text, model_detected_language, segments) = join_chunks(transcripts);
            Ok(Some(RunOutcome {
                setup,
                text,
                model_detected_language,
                segments,
            }))
        }
        Err(EngineError::Cancelled) => Err(EngineError::Cancelled),
        Err(e) => {
            info!("Live chunks failed ({}); transcribing in batch", e);
            Ok(None)
        }
    }
}

/// A stream the model decodes as audio arrives, with preview text emitted
/// to the overlay.
struct StreamSession<'a> {
    tm: &'a TranscriptionManager,
    model_id: String,
    stream: StreamHandle,
    perf: Arc<Mutex<StreamPerf>>,
    /// In auto mode the model's own LID is the best remaining evidence; the
    /// snapshot is only materialized when it can change the outcome.
    want_language: bool,
}

impl<'a> StreamSession<'a> {
    fn begin(
        tm: &'a TranscriptionManager,
        info: &LoadedInfo,
        settings: &AppSettings,
        setup: &RunSetup,
        model_id: &str,
    ) -> Option<Self> {
        let output_language = setup.output_language(settings, None);

        // Feed results arrive on the engine's thread; this callback only
        // converts and emits preview text.
        let perf = Arc::new(Mutex::new(StreamPerf::new()));
        let on_progress = {
            let perf = Arc::clone(&perf);
            let app_handle = tm.app_handle.clone();
            let languages = setup.languages.clone();
            let mut preview_script = PreviewScript::new(settings.chinese_script, &output_language);
            // The meeting window's live transcript: the committed prefix only
            // grows, so each growth is a finished piece of speech. The last
            // length emitted and the audio it covered make the next segment's
            // t0; the sink itself is installed only while a meeting records.
            let tm = tm.clone();
            let committed_seen: Arc<Mutex<(usize, i64)>> = Arc::new(Mutex::new((0, 0)));
            move |progress: StreamProgress| {
                let mut perf = lock_perf(&perf);
                perf.record_compute(progress.elapsed);
                perf.record_update(&progress.update);
                if let Some(text) = progress.text {
                    perf.record_emit();
                    let (committed, tentative) =
                        preview_script.convert(&text.committed, &text.tentative, &languages);
                    emit_stream_text(&app_handle, &committed, &tentative);
                    // The segment carries the model's own committed text —
                    // the same text the finished entry's segments keep — not
                    // the script-converted preview the overlay shows.
                    report_committed(
                        &tm,
                        &committed_seen,
                        &text.committed,
                        progress.update.audio_committed_ms,
                    );
                }
                perf.maybe_log();
            }
        };

        // Run the stream in the engine's worker process. Feeds are queued
        // without waiting; a crashed or hung worker makes finalize report no
        // result, and the caller falls back to batch transcription.
        // StreamOptions::default() uses CommitPolicy::Auto and lets the
        // family pick its own streaming strategy (no family-specific ext).
        let stream = match tm.engine.start_stream(
            setup.run_options.clone(),
            StreamOptions::default(),
            on_progress,
        ) {
            Ok(stream) => stream,
            Err(e) => {
                error!("Failed to begin stream: {}", e);
                tm.forget_model_if_engine_dropped(model_id, &e.to_string());
                return None;
            }
        };

        tm.stream_active.store(true, Ordering::Release);
        info!(
            "Live streaming transcription started (model '{}', backend '{}')",
            model_id, info.backend
        );
        Some(Self {
            tm,
            model_id: model_id.to_string(),
            stream,
            perf,
            want_language: matches!(output_language, OutputLanguageEvidence::Unknown),
        })
    }

    fn feed(&mut self, pcm: Vec<f32>) {
        lock_perf(&self.perf).record_feed(pcm.len());
        self.stream.feed(pcm);
    }

    fn finish(self) -> Result<Option<Decoded>, EngineError> {
        // Segments always: the result lands in the history whatever the
        // language plan was, and a finalize's snapshot is one copy of what the
        // stream already decoded.
        let result = match self.stream.finalize(self.want_language, true) {
            // After finalize the committed prefix holds the full text;
            // display() = committed + tentative is the safe read.
            Ok(Some(finalized)) => {
                let mut perf = lock_perf(&self.perf);
                perf.record_compute(finalized.elapsed);
                perf.record_update(&finalized.update);
                if finalized.text.full.trim().is_empty() {
                    // A stream can miss what a run over the whole recording
                    // finds, so empty text retries in batch.
                    info!("Live transcription produced no text; retrying in batch");
                    Ok(None)
                } else {
                    Ok(Some((
                        finalized.text.full,
                        finalized.language,
                        timed_segments(&finalized.segments),
                    )))
                }
            }
            Ok(None) => {
                error!(
                    "Live transcription could not be finalized; \
                     falling back to batch transcription"
                );
                self.tm.forget_model_if_engine_dropped(
                    &self.model_id,
                    "live transcription failed and the model could not be restarted",
                );
                Ok(None)
            }
            Err(e) => {
                info!("Live transcription finalize stopped: {}", e);
                Err(e)
            }
        };
        let chars = match &result {
            Ok(Some((text, _, _))) => text.len(),
            _ => 0,
        };
        lock_perf(&self.perf).log_finalized(chars);
        result
    }
}

/// Reports the streaming decode's newly committed text as one live segment.
/// `seen` carries how much of the committed prefix was already emitted and
/// the audio time it covered, so the delta becomes a segment of its own.
fn report_committed(
    tm: &TranscriptionManager,
    seen: &Mutex<(usize, i64)>,
    committed: &str,
    audio_committed_ms: i64,
) {
    let mut seen = seen.lock().unwrap();
    let (segment, next) = committed_segment(*seen, committed, audio_committed_ms);
    if let Some(segment) = segment {
        tm.report_live_segment(segment);
    }
    *seen = next;
}

/// The live segment a streaming decode's committed prefix growth makes, with
/// the state the next call continues from: how much of the prefix was emitted
/// and the audio time it covered. Committed text is append-only, so its new
/// tail is one finished piece of speech; a prefix that shrank (a model
/// rewriting it) only resynchronizes without emitting anything.
fn committed_segment(
    seen: (usize, i64),
    committed: &str,
    audio_committed_ms: i64,
) -> (Option<LiveSegment>, (usize, i64)) {
    let (last_len, last_ms) = seen;
    let now_ms = audio_committed_ms.max(last_ms);
    let grows = committed.len() >= last_len && committed.is_char_boundary(last_len);
    let delta = if grows {
        committed[last_len..].trim()
    } else {
        ""
    };
    let segment = (!delta.is_empty()).then(|| LiveSegment {
        t0_ms: last_ms,
        t1_ms: now_ms,
        text: delta.to_string(),
    });
    (segment, (committed.len(), now_ms))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn committed_growth_becomes_one_segment_with_the_audio_it_covered() {
        // Nothing was emitted yet, and 4 s of audio committed the first words.
        let (segment, seen) = committed_segment((0, 0), "Hello there", 4_000);
        assert_eq!(
            segment,
            Some(LiveSegment {
                t0_ms: 0,
                t1_ms: 4_000,
                text: "Hello there".to_string(),
            })
        );
        // The next growth starts where the last one ended.
        let (segment, seen) = committed_segment(seen, "Hello there friend", 7_500);
        assert_eq!(
            segment,
            Some(LiveSegment {
                t0_ms: 4_000,
                t1_ms: 7_500,
                text: "friend".to_string(),
            })
        );
        assert_eq!(seen, ("Hello there friend".len(), 7_500));
    }

    #[test]
    fn whitespace_only_growth_emits_nothing_but_advances_the_state() {
        let (segment, seen) = committed_segment((0, 0), "  \n", 1_000);
        assert_eq!(segment, None);
        assert_eq!(seen, (3, 1_000));
    }

    #[test]
    fn a_rewritten_prefix_resynchronizes_without_emitting() {
        let after_first = ("Hello there".len(), 4_000);
        // The model rewrote what it had committed: nothing is emitted, and
        // the next growth is measured against the rewritten prefix.
        let (segment, seen) = committed_segment(after_first, "Hi", 4_500);
        assert_eq!(segment, None);
        assert_eq!(seen, ("Hi".len(), 4_500));
        let (segment, _) = committed_segment(seen, "Hi there", 6_000);
        assert_eq!(
            segment,
            Some(LiveSegment {
                t0_ms: 4_500,
                t1_ms: 6_000,
                text: "there".to_string(),
            })
        );
    }

    #[test]
    fn committed_audio_never_moves_backwards() {
        // A commit whose audio time lags an earlier one keeps the earlier
        // end, so a segment never ends before it starts.
        let after_first = ("Hello".len(), 5_000);
        let (segment, seen) = committed_segment(after_first, "Hello there", 3_000);
        assert_eq!(
            segment,
            Some(LiveSegment {
                t0_ms: 5_000,
                t1_ms: 5_000,
                text: "there".to_string(),
            })
        );
        assert_eq!(seen, ("Hello there".len(), 5_000));
    }
}
