//! The local MCP server: lets AI assistants (Claude Desktop, Cursor) read
//! FlowState's own meeting and transcription history over MCP.
//!
//! Everything here is strictly local: the HTTP transport binds `127.0.0.1`
//! only, the database is opened read-only, and no request ever leaves the
//! machine. The server is per machine and per user — the app starts its own
//! instance against its own data; there is no shared or central server.
//!
//! Two transports share one implementation:
//! - stdio (`stdio`), for clients that launch `flowstate-mcp` themselves.
//! - localhost HTTP (`http`), started by the app on a port the user picks,
//!   guarded by a per-user token (`Authorization: Bearer …`).

pub mod db;
pub mod http;
pub mod manager;
pub mod protocol;
pub mod stdio;
pub mod token;

/// Where the bundled `flowstate-mcp` binary sits relative to the app binary.
pub const MCP_BINARY_NAME: &str = "flowstate-mcp";

/// Resolve the history database for a standalone `flowstate-mcp` process when
/// `--db` was not passed: portable mode's `Data/` directory when the marker
/// is present next to the binary, otherwise the platform's app data dir.
///
/// The app itself always passes `--db` explicitly, so this only matters to
/// users wiring the binary into Claude Desktop/Cursor by hand.
pub fn default_db_path() -> std::path::PathBuf {
    // Portable detection works without an AppHandle: it only looks at the
    // marker file next to the executable.
    crate::portable::init();
    if let Some(data_dir) = crate::portable::data_dir() {
        return data_dir.join("history.db");
    }

    let identifier = "com.teshgotcode.flowstate";
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        home.join("Library")
            .join("Application Support")
            .join(identifier)
            .join("history.db")
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let data_root = std::env::var_os("XDG_DATA_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
                Some(home.join(".local").join("share"))
            })
            .unwrap_or_default();
        data_root.join(identifier).join("history.db")
    }
    #[cfg(windows)]
    {
        let appdata = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_default();
        appdata.join(identifier).join("history.db")
    }
}
