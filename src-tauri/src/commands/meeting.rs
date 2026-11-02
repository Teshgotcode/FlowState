use crate::managers::meeting::{MeetingManager, MeetingStatus, StoppedMeeting};
use crate::managers::meeting_ask::{ask_about_entry, cancel_ask, MeetingAskOutcome};
use crate::managers::meeting_note::{cancel_run, generate_for_entry, MeetingNoteOutcome};
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
/// restart (Wispr's statusDockEdge).
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
