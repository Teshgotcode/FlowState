use crate::managers::history::{HistoryEntry, HistoryManager};
use crate::managers::meeting::{MeetingManager, MeetingStatus, StoppedMeeting};
use crate::managers::meeting_ask::{ask_about_entry, cancel_ask, MeetingAskOutcome};
use crate::managers::meeting_note::{cancel_run, generate_for_entry, MeetingNoteOutcome};
use crate::settings::{get_settings, write_settings};
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Start a meeting session: a recording that runs until it is stopped, not
/// until a dictation hotkey comes up. Its audio is written to a working file
/// while it records, and a live decode keeps a running transcript.
#[tauri::command]
#[specta::specta]
pub fn start_meeting(app: AppHandle) -> Result<MeetingStatus, String> {
    let meetings = app.state::<Arc<MeetingManager>>();
    meetings.start().map_err(|e| e.to_string())
}

/// Stop the meeting: save its recording as a history entry with
/// `source: "meeting"` and its duration in the entry's metadata, and queue
/// the entry's transcription. The text arrives when the background job runs.
#[tauri::command]
#[specta::specta]
pub fn stop_meeting(app: AppHandle) -> Result<StoppedMeeting, String> {
    let meetings = app.state::<Arc<MeetingManager>>();
    meetings.stop().map_err(|e| e.to_string())
}

/// Cancel the meeting: remove its working file and save no history entry.
#[tauri::command]
#[specta::specta]
pub fn cancel_meeting(app: AppHandle) -> Result<MeetingStatus, String> {
    let meetings = app.state::<Arc<MeetingManager>>();
    meetings
        .cancel()
        .map_err(|e| e.to_string())
        .map(|()| MeetingStatus::Idle)
}

/// The meeting session's stage right now, for a window that needs no event.
#[tauri::command]
#[specta::specta]
pub fn get_meeting_status(app: AppHandle) -> MeetingStatus {
    app.state::<Arc<MeetingManager>>().status()
}

/// Begin dragging the floating meeting bar (idle disc or recording pill).
/// The frontend calls this once the pointer has crossed its click/drag
/// threshold, so a plain click still starts or stops the meeting; the native
/// drag carries the window with the pointer, and the bar docks to the
/// nearest screen edge when it settles — the choice is saved and resumed on
/// restart (the reference statusDockEdge).
#[tauri::command]
#[specta::specta]
pub fn start_meeting_bar_drag(app: AppHandle) -> Result<(), String> {
    crate::overlay::begin_meeting_bar_drag(&app);
    Ok(())
}

/// Pop the meeting bar's context menu at the cursor (right-click on the idle
/// disc): "Hide for 1 hour". Native, because the disc's window is exactly the
/// disc — no room for HTML.
#[tauri::command]
#[specta::specta]
pub fn open_meeting_bar_menu(app: AppHandle) -> Result<(), String> {
    crate::overlay::show_meeting_bar_menu(&app);
    Ok(())
}

/// Widen the idle disc into its action row while the pointer is over it. The
/// window is an exact fit for its content, so the row needs a real resize.
#[tauri::command]
#[specta::specta]
pub fn set_meeting_bar_expanded(app: AppHandle, expanded: bool) {
    crate::overlay::set_meeting_bar_expanded(&app, expanded);
}

/// Grow or shrink the meeting bar's window for the "still going?" prompt
/// (S-24): the frontend shows the prompt once the session has recorded past
/// `meeting_reminder_minutes`, and asks for the wider footprint here. The
/// resize never focuses the window, so the app underneath keeps the
/// keyboard.
#[tauri::command]
#[specta::specta]
pub fn set_meeting_bar_nudge(app: AppHandle, show: bool) -> Result<(), String> {
    crate::overlay::set_meeting_bar_nudge(&app, show);
    Ok(())
}

/// How long a meeting may record before the bar shows its "still going?"
/// prompt, in minutes. `None` turns the prompt off (the setting's "Off"
/// choice).
#[tauri::command]
#[specta::specta]
pub fn set_meeting_reminder_minutes(app: AppHandle, minutes: Option<u64>) {
    let mut settings = get_settings(&app);
    settings.meeting_reminder_minutes = minutes;
    write_settings(&app, settings);
}
/// Whether this machine can capture system audio at all: the CoreAudio
/// process tap exists from macOS 14.4, and FlowState deploys to 10.15, so
/// the answer is a runtime fact. A Mac (or any other OS) that cannot gets
/// its system-audio setting hidden rather than broken — meetings there are
/// mic-only, without an error.
#[tauri::command]
#[specta::specta]
pub fn system_audio_supported() -> bool {
    crate::system_audio::supported()
}

/// Turn the meeting system-audio track on or off: what the machine itself
/// plays (the far end of a Zoom or Teams call) captured beside the
/// microphone and merged into the transcript by timestamp. Everything is
/// captured locally through a CoreAudio tap that needs no permission
/// dialog; nothing ever leaves the machine.
#[tauri::command]
#[specta::specta]
pub fn set_record_system_audio(app: AppHandle, enabled: bool) {
    let mut settings = get_settings(&app);
    settings.record_system_audio = enabled;
    write_settings(&app, settings);
}

/// The people the meeting app's window names, offered to the transcript
/// window as suggestions. Read once, on demand: an app that isn't running,
/// a refused permission and a window without names all give an empty list,
/// and nothing is assigned to a speaker automatically.
#[tauri::command]
#[specta::specta]
pub async fn suggest_meeting_names(app: AppHandle, entry_id: i64) -> Vec<String> {
    let hm = app.state::<Arc<HistoryManager>>();
    let Some(name) = hm
        .metadata_of(entry_id)
        .ok()
        .and_then(|metadata| metadata.meeting_app)
    else {
        return Vec::new();
    };
    crate::meeting_names::suggested_names(&name)
}

/// The notes typed in the live window while this meeting runs. They live on
/// the session until it stops, then land on the finished entry's metadata.
#[tauri::command]
#[specta::specta]
pub fn set_current_meeting_notes(app: AppHandle, notes: String) {
    let mm = app.state::<Arc<MeetingManager>>();
    mm.set_notes(notes);
}

/// What has been typed in the live window so far; empty when nothing has.
#[tauri::command]
#[specta::specta]
pub fn get_current_meeting_notes(app: AppHandle) -> String {
    let mm = app.state::<Arc<MeetingManager>>();
    mm.notes()
}

/// What the user typed during a meeting, in their own words. Kept apart from
/// the transcript and the generated note: neither is rewritten from it, and it
/// survives restarts and re-transcriptions. An empty string clears the field,
/// so "no notes" reads the same as never having written any.
#[tauri::command]
#[specta::specta]
pub fn set_meeting_user_notes(
    app: AppHandle,
    entry_id: i64,
    notes: String,
) -> Result<HistoryEntry, String> {
    let hm = app.state::<Arc<HistoryManager>>();
    hm.save_user_notes(entry_id, notes)
        .map_err(|e| e.to_string())
}

/// Name a finished meeting's speakers: the names are stored on the entry's
/// metadata, indexed by speaker number (S1 is index 0), shown in the
/// transcript window and carried into its exports. Names live per meeting —
/// one meeting's "S2 is Kari" never touches another's — and survive
/// restarts and re-transcriptions.
#[tauri::command]
#[specta::specta]
pub fn set_meeting_speaker_names(
    app: AppHandle,
    entry_id: i64,
    names: Vec<Option<String>>,
) -> Result<HistoryEntry, String> {
    let hm = app.state::<Arc<HistoryManager>>();
    hm.save_speaker_names(entry_id, names)
        .map_err(|e| e.to_string())
}

/// Diarize a finished meeting's history entry now: run the speaker model
/// over its recording and merge the speakers into its stored segments. The
/// model must already be on the machine — the window downloads it first —
/// and only the speakers are written: the entry's text, title and the names
/// already given are never touched. A meeting the pass cannot attribute
/// (no recording left, no segments) is an error the window can show; a
/// machine without the model never blocks the transcript itself.
#[tauri::command]
#[specta::specta]
pub async fn diarize_meeting_entry(app: AppHandle, entry_id: i64) -> Result<HistoryEntry, String> {
    crate::managers::diarize::diarize_entry(&app, entry_id).await
}

/// Generate a meeting note for a finished meeting's history entry: chunk its
/// transcript, extract a structured draft with the configured local LLM,
/// merge the parts and keep the note beside the transcript, above it in the
/// transcript window. Runs only on this explicit call; the note never leaves
/// the machine, and the transcript itself is never modified.
#[tauri::command]
#[specta::specta]
pub async fn generate_meeting_note(
    app: AppHandle,
    entry_id: i64,
) -> Result<MeetingNoteOutcome, String> {
    generate_for_entry(&app, entry_id)
        .await
        .map_err(|e| e.to_string())
}

/// Stop the note generation running for `entry_id`: the current LLM call is
/// dropped and nothing is saved. Returns whether a generation was running.
#[tauri::command]
#[specta::specta]
pub async fn cancel_meeting_note(entry_id: i64) -> Result<bool, String> {
    Ok(cancel_run(entry_id))
}
/// Ask the local LLM one question about a finished meeting's transcript and
/// return its answer with the transcript parts it cites, so the window can
/// jump to the right timestamp. The whole transcript rides along when it fits
/// the model's window, otherwise the parts the question points at are picked
/// with a plain keyword ranking. Nothing is saved: the transcript stands
/// untouched and the conversation lives only in the window that asked.
#[tauri::command]
#[specta::specta]
pub async fn ask_meeting_transcript(
    app: AppHandle,
    entry_id: i64,
    question: String,
) -> Result<MeetingAskOutcome, String> {
    ask_about_entry(&app, entry_id, &question)
        .await
        .map_err(|e| e.to_string())
}

/// Stop the ask running for `entry_id`: the current LLM call is dropped and
/// nothing is shown. Returns whether an ask was running.
#[tauri::command]
#[specta::specta]
pub async fn cancel_meeting_ask(entry_id: i64) -> Result<bool, String> {
    Ok(cancel_ask(entry_id))
}
