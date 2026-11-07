use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::history_read::HistoryDb;

/// Read commands that print history and exit without starting the app.
///
/// They open the app's history database read-only, so the CLI can never
/// write to what the app owns.
#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    /// List meeting sessions, newest first
    Meetings {
        #[arg(long, default_value_t = 20)]
        limit: u64,
        #[arg(long, default_value_t = 0)]
        offset: u64,
    },
    /// Print one meeting with its transcript and note
    Meeting { id: i64 },
    /// Print the verbatim transcript of any entry, meeting or dictation
    Transcript { id: i64 },
    /// Search titles and transcripts
    Search {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: u64,
    },
}

#[derive(Parser, Debug, Clone, Default)]
#[command(name = "flowstate", about = "FlowState — lokal diktering")]
pub struct CliArgs {
    /// Read from the history instead of starting the app
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Path to history.db (default: the app's data directory)
    #[arg(long, global = true)]
    pub db: Option<PathBuf>,

    /// Start with the main window hidden
    #[arg(long)]
    pub start_hidden: bool,

    /// Disable the system tray icon
    #[arg(long)]
    pub no_tray: bool,

    /// Toggle transcription on/off (sent to running instance)
    #[arg(long)]
    pub toggle_transcription: bool,

    /// Toggle transcription with post-processing on/off (sent to running instance)
    #[arg(long)]
    pub toggle_post_process: bool,

    /// Cancel the current operation (sent to running instance)
    #[arg(long)]
    pub cancel: bool,

    /// Enable debug mode with verbose logging
    #[arg(long)]
    pub debug: bool,

    /// Transcribe this WAV (16 kHz mono) headlessly and exit. Runs the same
    /// batch transcription path as the app — no mic, no silence removal, no
    /// download (the model must already be installed). Long audio is still
    /// chunked at pauses, as in the app.
    #[arg(short = 'f', long, value_name = "WAV")]
    pub transcribe_file: Option<PathBuf>,

    /// Model id to load for --transcribe-file (default: the selected model).
    #[arg(long)]
    pub model: Option<String>,

    /// Hard-select the compute device for --transcribe-file by its registry
    /// index (see --list-devices). Omit to use the persisted accelerator
    /// setting. transcribe-cpp (whisper-family) models only.
    #[arg(long, value_name = "N")]
    pub device_index: Option<usize>,

    /// List the transcribe-cpp compute devices (with indices) and exit.
    #[arg(long)]
    pub list_devices: bool,

    /// List the available models (with ids) and exit. Pass an id to --model.
    /// Honors --json for machine-readable output.
    #[arg(long)]
    pub list_models: bool,

    /// Repeat the transcription N times (best_ms reports the fastest run).
    #[arg(long, value_name = "N")]
    pub repeat: Option<usize>,

    /// Emit results as JSON (read commands and --transcribe-file).
    #[arg(long, global = true)]
    pub json: bool,
}

/// Run a read command and return its exit code. `None` means "no read command
/// was given, start the app".
pub fn run_command(args: &CliArgs) -> Option<i32> {
    let command = args.command.as_ref()?;
    let path = args
        .db
        .clone()
        .unwrap_or_else(crate::history_read::default_db_path);

    let db = match HistoryDb::open(&path) {
        Ok(db) => db,
        Err(error) => {
            eprintln!("{error}");
            return Some(1);
        }
    };

    let result = match command {
        Command::Meetings { limit, offset } => db
            .list_meetings(*limit, *offset)
            .map(|meetings| print_meetings(&meetings, args.json)),
        Command::Meeting { id } => db.get_meeting(*id).map(|meeting| match meeting {
            Some(meeting) => {
                if args.json {
                    print_json(&meeting)
                } else {
                    println!("{}  {}", meeting.date, meeting.title);
                    if let Some(duration) = meeting.duration_secs {
                        println!("{} min  #{}", duration / 60, meeting.id);
                    }
                    println!();
                    println!("{}", meeting.transcript);
                    if let Some(note) = meeting.note {
                        println!();
                        println!("{}", note.summary);
                    }
                }
            }
            None => eprintln!("No meeting with id {id}"),
        }),
        Command::Transcript { id } => db.get_transcript(*id).map(|transcript| match transcript {
            Some(transcript) => {
                if args.json {
                    print_json(&transcript)
                } else {
                    println!("{}", transcript.text);
                }
            }
            None => eprintln!("No entry with id {id}"),
        }),
        Command::Search { query, limit } => db
            .search_history(query, *limit, 0)
            .map(|results| print_hits(&results, args.json)),
    };

    match result {
        Ok(()) => Some(0),
        Err(error) => {
            eprintln!("{error}");
            Some(1)
        }
    }
}

fn print_json<T: serde::Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{json}"),
        Err(error) => eprintln!("{error}"),
    }
}

fn print_meetings(meetings: &[crate::history_read::MeetingSummary], json: bool) {
    if json {
        print_json(&meetings);
        return;
    }
    if meetings.is_empty() {
        println!("No meetings yet.");
        return;
    }
    for meeting in meetings {
        let duration = meeting
            .duration_secs
            .map(|seconds| format!("{:>3} min", seconds / 60))
            .unwrap_or_else(|| "      -".to_string());
        let note = if meeting.has_note { " *" } else { "" };
        println!(
            "{:<16}  {}  {}{}",
            meeting.date, duration, meeting.title, note
        );
    }
}

fn print_hits(results: &crate::history_read::SearchResults, json: bool) {
    if json {
        print_json(results);
        return;
    }
    println!("{} hits", results.total);
    for hit in &results.hits {
        println!();
        println!("{}  {}  #{}", hit.date, hit.title, hit.id);
        println!("{}", hit.snippet.replace('\n', " "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> CliArgs {
        CliArgs::try_parse_from(args).expect("the arguments should parse")
    }

    #[test]
    fn read_commands_parse_beside_the_app_flags() {
        let meetings = parse(&["flowstate", "meetings", "--limit", "5", "--json"]);
        assert!(meetings.json);
        assert!(matches!(
            meetings.command,
            Some(Command::Meetings {
                limit: 5,
                offset: 0
            })
        ));

        let transcript = parse(&["flowstate", "transcript", "42"]);
        assert!(matches!(
            transcript.command,
            Some(Command::Transcript { id: 42 })
        ));

        // No subcommand still means "start the app".
        assert!(parse(&["flowstate", "--start-hidden"]).command.is_none());
    }
}
