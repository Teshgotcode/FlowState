use std::path::{Path, PathBuf};
use tauri::Manager;

/// The extensions the frontend's save dialogs offer (json for dictionary and
/// snippet exports, md/srt/vtt for transcripts), plus txt as a harmless
/// plain-text extra.
const ALLOWED_EXTENSIONS: [&str; 6] = ["txt", "md", "json", "csv", "srt", "vtt"];

/// Writes UTF-8 `contents` to `path`.
///
/// For exports the user asked for: the frontend shows a save dialog and hands
/// the chosen path to this command, so the destination is always the user's
/// own pick and nothing leaves the machine.
// The restrictions below are defense in depth for a compromised webview:
// the save dialog never returns paths inside the app's own directories (M1).
#[tauri::command]
#[specta::specta]
pub fn export_text_file(
    app: tauri::AppHandle,
    path: String,
    contents: String,
) -> Result<(), String> {
    write_text_file(Path::new(&path), &contents, &forbidden_export_dirs(&app))
}

/// Directories an export must never land in: the app's own data and config
/// directories (settings, API keys, the database) and the user's LaunchAgents
/// folder. Missing directories are skipped; nothing can be written under them.
fn forbidden_export_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    let home_launch_agents = app
        .path()
        .resolve("Library/LaunchAgents", tauri::path::BaseDirectory::Home)
        .ok();
    [
        app.path().app_data_dir().ok(),
        app.path().app_config_dir().ok(),
        home_launch_agents,
    ]
    .into_iter()
    .flatten()
    .filter_map(|dir| std::fs::canonicalize(&dir).ok())
    .collect()
}

fn write_text_file(
    path: &Path,
    contents: &str,
    forbidden_dirs: &[PathBuf],
) -> Result<(), String> {
    // The save dialog always returns an absolute path; an empty or bare
    // file name has no directory to write into and is refused up front
    // with a clearer message than the OS would give.
    if path
        .parent()
        .is_none_or(|parent| parent.as_os_str().is_empty())
    {
        return Err("The export path has no directory to write to".to_string());
    }
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_lowercase());
    if !extension.is_some_and(|extension| ALLOWED_EXTENSIONS.contains(&extension.as_str())) {
        return Err(
            "The export path must end in .txt, .md, .json, .csv, .srt or .vtt".to_string(),
        );
    }
    let parent = path.parent().expect("checked above");
    let canonical_parent = std::fs::canonicalize(parent)
        .map_err(|e| format!("Failed to resolve {}: {e}", parent.display()))?;
    if forbidden_dirs
        .iter()
        .any(|forbidden| canonical_parent.starts_with(forbidden))
    {
        return Err("Exports cannot be written into the app's own or protected directories".to_string());
    }
    std::fs::write(path, contents).map_err(|e| format!("Failed to write {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_contents_to_the_chosen_path() {
        let dir = std::env::temp_dir().join("flowstate-export-test");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let file = dir.join("transcript.md");

        write_text_file(&file, "# Notes\n\n[00:12] Hello\n", &[]).expect("write file");

        let read_back = std::fs::read_to_string(&file).expect("read the file back");
        assert_eq!(read_back, "# Notes\n\n[00:12] Hello\n");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn refuses_paths_without_a_directory() {
        for bad in ["", "notes.md"] {
            let err = write_text_file(Path::new(bad), "text", &[])
                .expect_err("path without a directory is refused");
            assert!(err.contains("directory"), "{bad}: {err}");
        }
    }

    #[test]
    fn refuses_extensions_the_dialogs_never_offer() {
        let dir = std::env::temp_dir().join("flowstate-export-test");
        std::fs::create_dir_all(&dir).expect("create test dir");
        for bad in ["notes.sh", "notes", "notes.md5", "notes.PLIST"] {
            let err = write_text_file(&dir.join(bad), "text", &[])
                .expect_err("disallowed extension is refused");
            assert!(err.contains("must end in"), "{bad}: {err}");
        }
    }

    #[test]
    fn allows_the_extensions_the_dialogs_offer() {
        let dir = std::env::temp_dir().join("flowstate-export-test");
        std::fs::create_dir_all(&dir).expect("create test dir");
        for good in ["notes.txt", "notes.md", "notes.json", "notes.SRT"] {
            write_text_file(&dir.join(good), "text", &[])
                .unwrap_or_else(|e| panic!("{good} should be allowed: {e}"));
            let _ = std::fs::remove_file(dir.join(good));
        }
    }

    #[test]
    fn refuses_paths_inside_the_forbidden_directories() {
        let dir = std::env::temp_dir().join("flowstate-export-test");
        std::fs::create_dir_all(&dir).expect("create test dir");
        let forbidden = std::fs::canonicalize(&dir).expect("canonicalize");
        let err = write_text_file(&dir.join("notes.md"), "text", &[forbidden])
            .expect_err("write into a forbidden directory is refused");
        assert!(err.contains("protected"), "{err}");
    }

    #[test]
    fn reports_os_errors_for_unwritable_paths() {
        let dir = std::env::temp_dir().join("flowstate-export-test");
        std::fs::create_dir_all(&dir).expect("create test dir");
        // A directory can't be overwritten with a file.
        let blocker = dir.join("blocker.md");
        std::fs::create_dir_all(&blocker).expect("create blocking dir");
        let err = write_text_file(&blocker, "text", &[]).expect_err("directory write fails");
        assert!(
            err.contains("Failed to write"),
            "error names the path: {err}"
        );
        let _ = std::fs::remove_dir(&blocker);
    }
}
