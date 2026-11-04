//! Post-recording speaker diarization for a finished meeting (S-29): who
//! spoke when, over the meeting's own recording — never in real time.
//!
//! The model is the catalog's `diarize` entry (Streaming Sortformer
//! 4-Speaker v2.1, NVIDIA Open Model License — attribution in NOTICE, and
//! downloaded by the model manager, never bundled). After a meeting stops,
//! its audio runs through the model once, and the speaker turns that come
//! back are merged into the transcript's timed segments: every segment gets
//! the speaker whose turn it overlaps the most.
//!
//! A machine without the model — not downloaded, still downloading, or a
//! run that failed — is a skip, never an error: the transcript stands
//! exactly as it was, without speakers. That is the whole failure mode.

use crate::catalog::DIARIZE_MODEL;
use crate::managers::history::{HistoryEntry, HistoryManager, HistorySource};
use crate::managers::model::ModelManager;
use crate::managers::transcription::TranscriptSegment;
use log::{debug, info, warn};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tauri::{AppHandle, Manager};

/// One diarized speaker turn: a stretch of the recording one person spoke,
/// with that speaker's number — 1 is the first voice heard ("S1").
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeakerTurn {
    pub t0_ms: i64,
    pub t1_ms: i64,
    pub speaker_id: u8,
}

/// The speaker turns of a finished meeting's audio, as one batch pass of
/// the catalog's diarization model. `None` means "nothing to show": the
/// model is not on this machine (or is still downloading), the catalog
/// names none, or the run itself failed. `None` is a skip — the caller's
/// transcript simply carries no speakers, and no error reaches the user.
pub fn diarize_meeting(app: &AppHandle, pcm: &[f32]) -> Option<Vec<SpeakerTurn>> {
    if pcm.is_empty() {
        return None;
    }
    // The catalog names the model (it is not an `asr` model, so it has no
    // registry entry); the manager knows where its file is.
    let model = DIARIZE_MODEL.as_ref()?;
    let mm = app.state::<Arc<ModelManager>>();
    let path = match mm.diarize_model_path() {
        Some(path) => path,
        None => {
            debug!(
                "Diarization skipped: the model ({}) is not on this machine",
                model.id
            );
            return None;
        }
    };
    let started = Instant::now();
    match run_model(&path, pcm) {
        Some(turns) if !turns.is_empty() => {
            let speakers = turns.iter().map(|turn| turn.speaker_id).max().unwrap_or(0);
            info!(
                "Diarized the meeting in {:.1?}: {} turn(s) over {} speaker(s)",
                started.elapsed(),
                turns.len(),
                speakers
            );
            Some(turns)
        }
        _ => None,
    }
}

/// One batch pass of the diarization model over the meeting's 16 kHz mono
/// audio. The model is loaded for the pass and dropped with it: diarization
/// has no reason to hold memory between meetings. A failure is logged and
/// swallowed — a transcript without speakers is a valid result, not an
/// error the user has to clear.
fn run_model(path: &Path, pcm: &[f32]) -> Option<Vec<SpeakerTurn>> {
    let model = match transcribe_cpp::Model::load(path) {
        Ok(model) => model,
        Err(e) => {
            warn!("Diarization model failed to load: {e}");
            return None;
        }
    };
    let mut session = match model.diarize_session() {
        Ok(session) => session,
        Err(e) => {
            warn!("Diarization failed to start: {e}");
            return None;
        }
    };
    let options = transcribe_cpp::DiarizeOptions {
        family: Some(transcribe_cpp::DiarizeExtension::Sortformer(
            Default::default(),
        )),
    };
    match session.run(pcm, &options) {
        Ok(raw) => Some(number_speakers(raw)),
        Err(e) => {
            warn!("Diarization failed: {e}");
            None
        }
    }
}

/// Number the model's speakers by first appearance: whoever speaks first is
/// S1, the next new voice S2, and so on. The model's own ids can be sparse —
/// a 4-slot model may use only slots 2 and 4 — and consecutive ids keep the
/// transcript's labels ("S1", "S2") and the naming list as small as the
/// meeting's real speaker count.
fn number_speakers(raw: Vec<transcribe_cpp::SpeakerSegment>) -> Vec<SpeakerTurn> {
    let mut ordered = raw;
    ordered.sort_by_key(|segment| (segment.t0_ms, segment.speaker_id));
    let mut order: Vec<i32> = Vec::new();
    let mut turns = Vec::with_capacity(ordered.len());
    for segment in ordered {
        let speaker_id = match order.iter().position(|id| *id == segment.speaker_id) {
            Some(index) => index as u8 + 1,
            None => {
                order.push(segment.speaker_id);
                order.len() as u8
            }
        };
        turns.push(SpeakerTurn {
            t0_ms: segment.t0_ms,
            t1_ms: segment.t1_ms,
            speaker_id,
        });
    }
    turns
}

/// Give every transcript segment its speaker: the one whose turn overlaps it
/// the most, by time on the same recording. A segment that overlaps no turn
/// (it sits in a pause the model heard as silence) keeps the previous
/// segment's speaker, so a short segment on a turn boundary is not left
/// unattributed for want of a millisecond.
pub fn apply_speakers(segments: &mut [TranscriptSegment], turns: &[SpeakerTurn]) {
    let mut last: Option<u8> = None;
    for segment in segments.iter_mut() {
        let speaker = loudest_overlap(segment, turns).or(last);
        segment.speaker_id = speaker;
        last = speaker;
    }
}

/// The speaker whose turn overlaps `segment` the most; ties go to the lower
/// speaker number, so the result is deterministic.
fn loudest_overlap(segment: &TranscriptSegment, turns: &[SpeakerTurn]) -> Option<u8> {
    turns
        .iter()
        .filter_map(|turn| {
            let overlap = segment.t1_ms.min(turn.t1_ms) - segment.t0_ms.max(turn.t0_ms);
            (overlap > 0).then_some((overlap, turn.speaker_id))
        })
        .max_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)))
        .map(|(_, id)| id)
}

/// Diarize a finished meeting's history entry now — the same pass
/// [`stop`](crate::managers::meeting::MeetingManager::stop) queues for a
/// fresh recording, offered on entries that predate the model or whose
/// first pass had no model on the machine. Only the speakers are written:
/// the entry's text, title and names are never touched. An error is
/// reserved for what the user asked for and cannot get (no recording left,
/// a missing model): a meeting without segments has no timeline to merge
/// onto and is refused rather than silently returning nothing.
pub async fn diarize_entry(app: &AppHandle, entry_id: i64) -> Result<HistoryEntry, String> {
    let hm = app.state::<Arc<HistoryManager>>();
    let entry = hm
        .get_entry_by_id(entry_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {entry_id} not found"))?;
    if entry.source != Some(HistorySource::Meeting) {
        return Err("Only a meeting has speakers to find".to_string());
    }
    let metadata = entry
        .metadata
        .clone()
        .ok_or_else(|| "The entry has no timed segments".to_string())?;
    let mut segments = metadata
        .segments
        .clone()
        .ok_or_else(|| "The entry has no timed segments".to_string())?;

    let recording = hm.get_audio_file_path(&entry.file_name);
    let samples = crate::audio_toolkit::read_wav_samples(&recording)
        .map_err(|e| format!("Failed to load the meeting's audio: {e}"))?;
    let turns = diarize_meeting(app, &samples)
        .ok_or_else(|| "The speaker model is not on this machine".to_string())?;
    apply_speakers(&mut segments, &turns);
    hm.save_diarized_segments(entry_id, segments)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segment(t0_ms: i64, t1_ms: i64, text: &str) -> TranscriptSegment {
        TranscriptSegment {
            t0_ms,
            t1_ms,
            text: text.to_string(),
            speaker_id: None,
        }
    }

    fn raw_turn(speaker_id: i32, t0_ms: i64, t1_ms: i64) -> transcribe_cpp::SpeakerSegment {
        transcribe_cpp::SpeakerSegment {
            t0_ms,
            t1_ms,
            speaker_id,
            p: 1.0,
        }
    }

    #[test]
    fn number_speakers_starts_with_the_first_voice_heard() {
        // The model's ids are sparse: only slots 4 and 2 spoke, and slot 4
        // spoke first. The first voice heard is S1 regardless of its slot.
        let raw = vec![
            raw_turn(4, 20_000, 30_000),
            raw_turn(2, 10_000, 20_000),
            raw_turn(4, 0, 10_000),
        ];
        assert_eq!(
            number_speakers(raw),
            vec![
                SpeakerTurn {
                    t0_ms: 0,
                    t1_ms: 10_000,
                    speaker_id: 1
                },
                SpeakerTurn {
                    t0_ms: 10_000,
                    t1_ms: 20_000,
                    speaker_id: 2
                },
                SpeakerTurn {
                    t0_ms: 20_000,
                    t1_ms: 30_000,
                    speaker_id: 1
                },
            ]
        );
    }

    #[test]
    fn apply_speakers_gives_each_segment_its_own_voice() {
        let turns = vec![
            SpeakerTurn {
                t0_ms: 0,
                t1_ms: 10_000,
                speaker_id: 1,
            },
            SpeakerTurn {
                t0_ms: 10_000,
                t1_ms: 20_000,
                speaker_id: 2,
            },
            SpeakerTurn {
                t0_ms: 20_000,
                t1_ms: 30_000,
                speaker_id: 1,
            },
        ];
        let mut segments = vec![
            segment(1_000, 5_000, "Hei, Kari her."),
            segment(11_000, 15_000, "Hei! Ola her."),
            segment(21_000, 25_000, "Fint å høre deg."),
        ];
        apply_speakers(&mut segments, &turns);
        assert_eq!(
            segments.iter().map(|s| s.speaker_id).collect::<Vec<_>>(),
            vec![Some(1), Some(2), Some(1)]
        );
    }

    #[test]
    fn apply_speakers_picks_the_voice_it_overlaps_most() {
        // Both speakers overlap the middle segment; S1 owns most of it.
        let turns = vec![
            SpeakerTurn {
                t0_ms: 0,
                t1_ms: 6_000,
                speaker_id: 1,
            },
            SpeakerTurn {
                t0_ms: 5_000,
                t1_ms: 12_000,
                speaker_id: 2,
            },
        ];
        let mut segments = vec![segment(2_000, 6_000, "delvis begge")];
        apply_speakers(&mut segments, &turns);
        assert_eq!(segments[0].speaker_id, Some(1));
    }

    #[test]
    fn apply_speakers_carries_the_last_voice_over_a_silent_gap() {
        let turns = vec![SpeakerTurn {
            t0_ms: 0,
            t1_ms: 5_000,
            speaker_id: 1,
        }];
        let mut segments = vec![
            segment(1_000, 4_000, "før pausen"),
            segment(6_000, 7_000, "i pausen"),
        ];
        apply_speakers(&mut segments, &turns);
        assert_eq!(segments[1].speaker_id, Some(1));
    }

    #[test]
    fn apply_speakers_without_turns_marks_nothing() {
        let mut segments = vec![segment(0, 1_000, "bare tekst")];
        apply_speakers(&mut segments, &[]);
        assert_eq!(segments[0].speaker_id, None);
    }
}
