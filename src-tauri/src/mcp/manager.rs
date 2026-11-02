//! Keeps the localhost MCP server in step with the user's settings.
//!
//! One server per app: it is started by the app, on the user's own machine,
//! against that user's own history — never a shared instance. When the
//! setting is off there is no process and no port.
//!
//! The server prefers the bundled `flowstate-mcp` binary (spawned as its own
//! process with the port, token and database path). When that binary is not
//! next to the app — a packaging it didn't ship in — the same server runs on
//! a thread inside the app instead. Either way the user sees the same thing:
//! loopback only, token-checked, gone when toggled off.

use crate::settings::{get_settings, AppSettings};
use log::{error, info, warn};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};

/// What is currently keeping the port open, if anything.
#[derive(Default)]
enum Runtime {
    #[default]
    Stopped,
    /// The bundled `flowstate-mcp` binary, spawned as its own process.
    Process(Child),
    /// The same server on an in-app thread, when the binary is unavailable.
    InProcess { stop: Arc<AtomicBool> },
}

impl Runtime {
    fn is_running(&mut self) -> bool {
        match self {
            Runtime::Stopped => false,
            Runtime::Process(child) => matches!(child.try_wait(), Ok(None)),
            Runtime::InProcess { stop } => !stop.load(Ordering::Relaxed),
        }
    }

    fn stop(&mut self) {
        match std::mem::replace(self, Runtime::Stopped) {
            Runtime::Stopped => {}
            Runtime::Process(mut child) => {
                // Kill first, then reap, so the port is free the moment the
                // user toggles the setting off.
                let _ = child.kill();
                let _ = child.wait();
            }
            Runtime::InProcess { stop } => {
                stop.store(true, Ordering::Relaxed);
            }
        }
    }
}

/// Where the bundled `flowstate-mcp` binary sits, if it shipped with us.
fn binary_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let binary = dir.join(super::MCP_BINARY_NAME);
    binary.is_file().then_some(binary)
}

/// FlowState's own history database, the only thing the server may read.
fn history_db_path(app_handle: &AppHandle) -> Option<PathBuf> {
    crate::portable::app_data_dir(app_handle)
        .ok()
        .map(|dir| dir.join("history.db"))
}

/// Whether the port answers within `timeout`, so a spawned binary that failed
/// to start is detected instead of assumed working.
fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

struct ServerState {
    runtime: Runtime,
    /// The port of the running server, if any.
    port: Option<u16>,
    /// The token of the running server, if any.
    token: Option<String>,
}

impl Default for ServerState {
    fn default() -> Self {
        ServerState {
            runtime: Runtime::Stopped,
            port: None,
            token: None,
        }
    }
}

pub struct McpManager {
    app_handle: AppHandle,
    state: Mutex<ServerState>,
}

impl McpManager {
    pub fn new(app_handle: &AppHandle) -> Self {
        let manager = Self {
            app_handle: app_handle.clone(),
            state: Mutex::new(ServerState::default()),
        };
        // Bring the server up at startup when the user left it enabled.
        manager.apply(&get_settings(app_handle));
        manager
    }

    /// Reconcile the running server with `settings`: start it, restart it on
    /// a port or token change, or stop it when the setting is off.
    pub fn apply(&self, settings: &AppSettings) {
        let mut state = self.state.lock().expect("mcp manager state");

        if !settings.mcp_enabled {
            state.runtime.stop();
            state.port = None;
            state.token = None;
            return;
        }

        // Nothing to serve without a token: enabling generates one first.
        let Some(token) = settings.mcp_token.clone() else {
            warn!("MCP enabled without a token; not starting the server");
            state.runtime.stop();
            state.port = None;
            state.token = None;
            return;
        };

        let port = settings.mcp_port;
        let unchanged = state.port == Some(port) && state.token.as_deref() == Some(token.as_str());
        if unchanged && state.runtime.is_running() {
            return; // already serving exactly this port and token
        }

        state.runtime.stop();
        match self.start(port, &token, &mut state) {
            Ok(()) => {
                info!("MCP server listening on 127.0.0.1:{port}");
                state.port = Some(port);
                state.token = Some(token);
            }
            Err(e) => {
                error!("Failed to start the MCP server on port {port}: {e}");
                state.port = None;
                state.token = None;
            }
        }
    }

    /// Whether a server is currently up, for the Integrations panel.
    pub fn is_running(&self) -> bool {
        self.state
            .lock()
            .expect("mcp manager state")
            .runtime
            .is_running()
    }

    fn start(&self, port: u16, token: &str, state: &mut ServerState) -> Result<(), String> {
        let db_path = history_db_path(&self.app_handle)
            .ok_or_else(|| "Could not resolve FlowState's data directory".to_string())?;
        if !db_path.exists() {
            return Err("FlowState's history database does not exist yet".to_string());
        }

        // Prefer the bundled binary: a real per-user process the app starts.
        if let Some(binary) = binary_path() {
            match Command::new(&binary)
                .arg("--db")
                .arg(&db_path)
                .arg("--port")
                .arg(port.to_string())
                .arg("--token")
                .arg(token)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    state.runtime = Runtime::Process(child);
                    if wait_for_port(port, Duration::from_secs(2)) {
                        return Ok(());
                    }
                    // The binary exited (a taken port, most likely) instead
                    // of serving; reap it and try the in-process server.
                    warn!(
                        "flowstate-mcp exited without listening on port {port}; serving in-process instead"
                    );
                    state.runtime.stop();
                }
                Err(e) => {
                    warn!(
                        "Failed to spawn {} ({e}); serving MCP in-process instead",
                        binary.display()
                    );
                }
            }
        }

        // In-process fallback: bind here so a taken port is a hard error the
        // Integrations panel can show.
        let listener =
            super::http::bind(port).map_err(|e| format!("Port {port} is not available: {e}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread_token = token.to_string();
        std::thread::Builder::new()
            .name("flowstate-mcp-http".into())
            .spawn(move || super::http::serve_on(listener, db_path, thread_token, thread_stop))
            .map_err(|e| format!("Failed to start the MCP server thread: {e}"))?;
        state.runtime = Runtime::InProcess { stop };
        Ok(())
    }
}

impl Drop for McpManager {
    fn drop(&mut self) {
        // Stop the server (kill the spawned binary, signal the in-process
        // thread) when the app goes down, so no orphan holds the port. Even a
        // poisoned lock still hands back its state.
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.runtime.stop();
    }
}

/// Re-apply the MCP settings from the store, so any settings write can bring
/// the server up or down without knowing the manager's details.
pub fn apply_current(app_handle: &AppHandle) {
    if let Some(manager) = app_handle.try_state::<McpManager>() {
        manager.apply(&get_settings(app_handle));
    }
}
