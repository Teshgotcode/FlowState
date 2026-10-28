use crate::managers::meeting::{MeetingManager, MeetingStatus, StoppedMeeting};
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
