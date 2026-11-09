//! Suggested speaker names, read from the app the meeting ran in.
//!
//! The reference reads the call's participant list while the meeting runs. We
//! diarize afterwards, so the same information is only useful at naming time:
//! when the transcript window opens, the meeting app's window text is read
//! once and the strings that look like people are offered as chips. Nothing is
//! guessed onto a speaker — the user still decides who is who.
//!
//! macOS reads the text through System Events, which asks for Automation
//! permission the first time; a refusal or a missing app yields an empty list
//! and the window simply shows no chips.

use std::process::Command;

/// How many names the window offers at once. A call rarely has more, and a
/// short list stays readable.
const MAX_NAMES: usize = 12;

/// Words that a call window shows as buttons or labels, which would otherwise
/// pass for names once capitalized by the app.
const UI_WORDS: &[&str] = &[
    "mute", "unmute", "video", "camera", "chat", "leave", "share", "screen", "participants",
    "recording", "you", "host", "meeting", "settings", "raise", "hand", "reactions", "more",
    "close", "join", "call", "end", "start", "stop", "notes", "details", "invite", "copy",
    // App names show up in the same windows and are not people.
    "zoom", "teams", "meet", "facetime", "chrome", "safari", "edge", "webex", "slack",
];

/// Whether a string from a call window could be a person's name: one to three
/// capitalized words, no digits, no address. UI words and anything long enough
/// to be a sentence are rejected.
pub fn looks_like_name(candidate: &str) -> bool {
    let candidate = candidate.trim();
    let chars = candidate.chars().count();
    if !(2..=32).contains(&chars) {
        return false;
    }
    if candidate.chars().any(|c| c.is_ascii_digit())
        || candidate.contains('@')
        || candidate.to_lowercase().contains("http")
    {
        return false;
    }
    let words: Vec<&str> = candidate.split_whitespace().collect();
    if words.is_empty() || words.len() > 3 {
        return false;
    }
    if !words
        .iter()
        .all(|word| word.chars().next().is_some_and(char::is_uppercase))
    {
        return false;
    }
    !UI_WORDS.contains(&candidate.to_lowercase().as_str())
}

/// The names to offer for a meeting recorded in `app` (its localized name).
/// An empty list means the app isn't running, permission was refused, or its
/// window text held nothing that looks like a person.
#[cfg(target_os = "macos")]
pub fn suggested_names(app: &str) -> Vec<String> {
    let script = format!(
        "tell application \"System Events\" to tell process \"{}\" to get value of every static text of every window",
        app.replace('"', "")
    );
    let output = match Command::new("osascript").arg("-e").arg(&script).output() {
        Ok(output) if output.status.success() => output.stdout,
        _ => return Vec::new(),
    };
    let text = String::from_utf8_lossy(&output);
    collect_names(text.split([',', '\n']))
}

#[cfg(not(target_os = "macos"))]
pub fn suggested_names(_app: &str) -> Vec<String> {
    Vec::new()
}

/// Keep the candidates that read as names, first spelling wins, capped.
pub fn collect_names<'a>(candidates: impl Iterator<Item = &'a str>) -> Vec<String> {
    let mut seen = Vec::new();
    for candidate in candidates {
        let candidate = candidate.trim().trim_matches('"');
        if !looks_like_name(candidate) {
            continue;
        }
        if seen.iter().any(|kept: &String| kept.eq_ignore_ascii_case(candidate)) {
            continue;
        }
        seen.push(candidate.to_string());
        if seen.len() == MAX_NAMES {
            break;
        }
    }
    seen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_pass_and_ui_words_do_not() {
        for name in ["Kari Nordmann", "Ola", "Anne-Kari Lie", "Tejas Sharma"] {
            assert!(looks_like_name(name), "{name} should read as a name");
        }
        for other in [
            "Mute", "Leave", "participants", "kari nordmann", "Zoom", "Team 2",
            "hei@example.com", "Share screen", "Skru av kamera", "",
        ] {
            assert!(!looks_like_name(other), "{other} should not read as a name");
        }
    }

    #[test]
    fn collection_dedupes_case_and_stops_at_the_cap() {
        let text = "Kari Nordmann\nkari nordmann\nOla\nMute\nChat\nOla\n";
        assert_eq!(
            collect_names(text.lines()),
            vec!["Kari Nordmann".to_string(), "Ola".to_string()]
        );

        // 26 distinct names, all spelled differently: the cap holds at 12.
        let many: Vec<String> = (0..26)
            .map(|i| format!("{} Navn", char::from(b'A' + i)))
            .collect();
        assert_eq!(
            collect_names(many.iter().map(String::as_str)).len(),
            MAX_NAMES
        );
    }
}
