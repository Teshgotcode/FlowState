use std::path::Path;

/// Writes UTF-8 `contents` to `path`.
///
/// For exports the user asked for: the frontend shows a save dialog and hands
/// the chosen path to this command, so the destination is always the user's
/// own pick and nothing leaves the machine.
#[tauri::command]
#[specta::specta]
pub fn export_text_file(path: String, contents: String) -> Result<(), String> {
    write_text_file(Path::new(&path), &contents)
}

fn write_text_file(path: &Path, contents: &str) -> Result<(), String> {
    // The save dialog always returns an absolute path; an empty or bare
    // file name has no directory to write into and is refused up front
    // with a clearer message than the OS would give.
    if path
        .parent()
        .is_none_or(|parent| parent.as_os_str().is_empty())
    {
        return Err("The export path has no directory to write to".to_string());
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

        write_text_file(&file, "# Notes\n\n[00:12] Hello\n").expect("write file");

        let read_back = std::fs::read_to_string(&file).expect("read the file back");
        assert_eq!(read_back, "# Notes\n\n[00:12] Hello\n");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn refuses_paths_without_a_directory() {
        for bad in ["", "notes.md"] {
            let err = write_text_file(Path::new(bad), "text")
                .expect_err("path without a directory is refused");
            assert!(err.contains("directory"), "{bad}: {err}");
        }
    }

    #[test]
    fn reports_os_errors_for_unwritable_paths() {
        let dir = std::env::temp_dir().join("flowstate-export-test");
        std::fs::create_dir_all(&dir).expect("create test dir");
        // A directory can't be overwritten with a file.
        let err = write_text_file(&dir, "text").expect_err("directory write fails");
        assert!(
            err.contains("Failed to write"),
            "error names the path: {err}"
        );
    }
}
