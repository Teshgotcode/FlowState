use tauri::AppHandle;

/// Open the Scratchpad: the small note window with the caret ready, shown
/// from the meeting bar's menu. The note persists locally between opens.
#[tauri::command]
#[specta::specta]
pub fn open_scratchpad(app: AppHandle) -> Result<(), String> {
    crate::scratchpad::open_window(&app);
    Ok(())
}

/// The Scratchpad note's current text ("" before the first save).
#[tauri::command]
#[specta::specta]
pub fn get_scratchpad_note(app: AppHandle) -> Result<String, String> {
    crate::scratchpad::read_note(&app)
}

/// Save the note's text to `scratchpad.txt` in the app data directory.
/// Local only — the note never leaves the machine.
#[tauri::command]
#[specta::specta]
pub fn save_scratchpad_note(app: AppHandle, content: String) -> Result<(), String> {
    crate::scratchpad::write_note(&app, &content)
}
