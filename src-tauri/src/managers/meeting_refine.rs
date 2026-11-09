//! Cleaning up a finished meeting's transcript.
//!
//! The reference keeps two versions of a meeting: the text as it was recognised
//! live, and a refined one that fixes the words the recogniser got wrong
//! («Strandveien» becoming «Strandvegen», «etasje 9 for dekkhotell» becoming
//! «Tast 9 for dekkhotell»). We keep the segments exactly as they were
//! recognised and write the cleaned text beside them, so the transcript window
//! still shows what was heard and the note reads cleanly.
//!
//! Three limits keep this honest. It only runs on the local LLM the user
//! configured, so a transcript never leaves the machine. It never summarises:
//! an answer that is much shorter than its chunk is thrown away and the raw
//! text kept. And the prompt forbids adding anything, because a cleaned
//! transcript that invents a sentence is worse than a rough one.

use anyhow::Result;
use tauri::AppHandle;

use crate::llm_client;
use crate::managers::history::HistoryManager;
use crate::managers::meeting_note::{self, NoteLlm};
use crate::settings::get_settings;

/// Chunk size in characters. Big enough to keep sentences in context, small
/// enough that a 106-minute lecture runs as a series of quick calls.
const CHUNK_TARGET: usize = 2_400;

/// What the model may not do, stated once and repeated per call: cleanup only.
const INSTRUCTIONS: &str = "Du rydder en møtetranskripsjon. Rett bare åpenbare gjenkjenningsfeil: \
ord som helt sikkert er hørt feil, navn som er skrevet feil når sammenhengen viser det, tall og \
forkortelser. Ikke oppsummer. Ikke forkort. Ikke legg til noe. Ikke endre talemåte eller \
setningslengde. Svar med teksten alene, uten forklaring.";

/// How far the cleaned chunk may stray in length before it is not trusted.
const MIN_RATIO: f64 = 0.6;
const MAX_RATIO: f64 = 1.5;

/// Whether a cleaned chunk still reads as the same chunk: not a summary, not an
/// elaboration. Empty answers never pass.
pub fn cleanup_is_plausible(source: &str, cleaned: &str) -> bool {
    let source_len = source.chars().count();
    let cleaned_len = cleaned.chars().count();
    if source_len == 0 || cleaned_len == 0 {
        return false;
    }
    let ratio = cleaned_len as f64 / source_len as f64;
    (MIN_RATIO..=MAX_RATIO).contains(&ratio)
}

/// Clean one meeting's transcript and store it beside the raw text. Returns the
/// number of chunks that survived; a machine without a local LLM, a cancelled
/// run or a provider that fails all leave the transcript untouched.
pub fn refine_meeting_transcript(
    app: &AppHandle,
    history: &HistoryManager,
    entry_id: i64,
    text: &str,
) -> Result<usize> {
    let settings = get_settings(app);
    let Ok(llm) = NoteLlm::resolve(&settings) else {
        return Ok(0);
    };
    let pieces = meeting_note::sentences(text);
    let chunks = meeting_note::pack_chunks(&pieces, CHUNK_TARGET);
    if chunks.is_empty() {
        return Ok(0);
    }

    let mut cleaned_chunks: Vec<String> = Vec::with_capacity(chunks.len());
    let mut kept = 0;
    for chunk in &chunks {
        let prompt = format!("{INSTRUCTIONS}\n\n{chunk}");
        let call = llm_client::send_chat_completion(
            &llm.provider,
            llm.api_key.clone(),
            &llm.model,
            prompt,
            llm.disable_reasoning,
        );
        let answer = tauri::async_runtime::block_on(call)
            .map_err(|detail| anyhow::anyhow!("the local LLM did not answer: {detail}"))?;
        match answer {
            Some(content) => {
                let cleaned = content.trim().to_string();
                if cleanup_is_plausible(chunk, &cleaned) {
                    cleaned_chunks.push(cleaned);
                    kept += 1;
                } else {
                    // A summary or an elaboration: keep what was heard.
                    cleaned_chunks.push(chunk.clone());
                }
            }
            None => cleaned_chunks.push(chunk.clone()),
        }
    }

    // Nothing survived: the raw text stands.
    if kept == 0 {
        return Ok(0);
    }
    let cleaned = cleaned_chunks.join(" ");
    history.save_cleaned_text(entry_id, &cleaned)?;
    Ok(kept)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_or_an_elaboration_is_refused() {
        let source = "Vi må bli enige om budsjettet før fredag, og Kari tar markedsføringen.";
        assert!(cleanup_is_plausible(
            source,
            "Vi må bli enige om budsjettet før fredag, og Kari tar markedsføringen."
        ));
        assert!(cleanup_is_plausible(
            source,
            "Vi må bli enige om budsjettet før fredag. Kari tar markedsføringen."
        ));
        assert!(!cleanup_is_plausible(source, "Budsjett og markedsføring."));
        assert!(!cleanup_is_plausible(source, ""));
        let long = format!("{source} Og her er en lang forklaring som ikke var i opptaket i det hele tatt, gang på gang på gang på gang.");
        assert!(!cleanup_is_plausible(source, &long));
    }
}
