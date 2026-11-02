//! The commands behind the Dictionary's "learn from corrections" feature.
//! Everything here reads and writes state on this device only.

use crate::managers::{
    history::{HistoryEntry, HistoryManager},
    word_suggestions::{WordSuggestion, WordSuggestionLearner},
};
use crate::settings;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// The words worth adding to the dictionary, most-corrected first. Empty
/// while learning is switched off — an off switch means nothing is learned,
/// not that learned data is hidden.
#[tauri::command]
#[specta::specta]
pub fn get_word_suggestions(
    app: AppHandle,
    learner: State<'_, WordSuggestionLearner>,
) -> Result<Vec<WordSuggestion>, String> {
    let settings = settings::get_settings(&app);
    if !settings.learn_from_corrections {
        return Ok(Vec::new());
    }
    Ok(learner.suggestions(&settings.custom_words))
}

/// "Legg til": puts the word in `custom_words` — so the next transcription
/// can use it — and retires the suggestion for good.
#[tauri::command]
#[specta::specta]
pub fn accept_word_suggestion(
    app: AppHandle,
    learner: State<'_, WordSuggestionLearner>,
    word: String,
) -> Result<(), String> {
    let word = word.trim().to_string();
    if word.is_empty() || word.chars().count() > 50 {
        return Err(format!("Invalid word: {word}"));
    }
    learner.accept(&word);
    let mut settings = settings::get_settings(&app);
    let already_known = settings
        .custom_words
        .iter()
        .any(|existing| existing.eq_ignore_ascii_case(&word));
    if !already_known {
        settings.custom_words.push(word);
        settings::write_settings(&app, settings);
    }
    Ok(())
}

/// "Ignorer": keeps the word out of the dictionary and out of the list until
/// fresh corrections earn it a second chance.
#[tauri::command]
#[specta::specta]
pub fn ignore_word_suggestion(
    learner: State<'_, WordSuggestionLearner>,
    word: String,
) -> Result<(), String> {
    learner.ignore(word.trim());
    Ok(())
}

/// "Rett denne": replaces a history entry's transcription with the user's
/// edited text, and — with learning on — counts the correction toward
/// dictionary suggestions. Returns the updated entry.
#[tauri::command]
#[specta::specta]
pub async fn correct_history_entry(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    learner: State<'_, WordSuggestionLearner>,
    id: i64,
    text: String,
) -> Result<HistoryEntry, String> {
    let entry = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("History entry {id} not found"))?;
    let old_text = entry.transcription_text.clone();
    if old_text == text {
        return Ok(entry);
    }
    let updated = history_manager
        .update_transcription_text(id, &text)
        .map_err(|e| e.to_string())?;
    let settings = settings::get_settings(&app);
    if settings.learn_from_corrections {
        learner.record_correction(&old_text, &text, &settings.custom_words);
    }
    Ok(updated)
}
