//! Tauri commands for the local MCP server (Settings → Integrations).
//!
//! The server itself is read-only and local (see [`crate::mcp`]); these
//! commands only move settings and tell the manager to reconcile.

use crate::mcp;
use crate::settings::{get_settings, write_settings, AppSettings};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};

/// What the Integrations panel asks about the server: only whether it is up.
/// Everything else (enabled, port, token) is plain settings.
#[derive(Serialize, Debug, Clone, Type)]
pub struct McpStatus {
    pub running: bool,
}

fn emit_settings_changed(app: &AppHandle, setting: &str, value: serde_json::Value) {
    let _ = app.emit(
        "settings-changed",
        serde_json::json!({ "setting": setting, "value": value }),
    );
}

/// Turn the MCP server on or off. The first time it is turned on, a random
/// per-user token is generated; the Integrations panel shows it that once,
/// and the server will not start without one.
///
/// Returns the settings so the panel can show the freshly generated token
/// immediately, without waiting for the settings-changed round-trip.
#[tauri::command]
#[specta::specta]
pub fn change_mcp_enabled_setting(app: AppHandle, enabled: bool) -> Result<AppSettings, String> {
    let mut settings = get_settings(&app);
    settings.mcp_enabled = enabled;
    if enabled && settings.mcp_token.is_none() {
        settings.mcp_token = Some(mcp::token::generate()?);
    }
    write_settings(&app, settings);
    mcp::manager::apply_current(&app);
    emit_settings_changed(&app, "mcp_enabled", enabled.into());
    Ok(get_settings(&app))
}

/// Change the loopback port the MCP server listens on. Port 0 is refused: it
/// would hand the choice to the OS, and the panel could not show it.
#[tauri::command]
#[specta::specta]
pub fn change_mcp_port_setting(app: AppHandle, port: u16) -> Result<(), String> {
    if port == 0 {
        return Err("Port 0 is not a port the MCP server can announce.".into());
    }
    let mut settings = get_settings(&app);
    settings.mcp_port = port;
    write_settings(&app, settings);
    mcp::manager::apply_current(&app);
    emit_settings_changed(&app, "mcp_port", port.into());
    Ok(())
}

/// Generate a fresh token, invalidating the old one. Returns the settings so
/// the panel can show the new token once, right where it was requested.
#[tauri::command]
#[specta::specta]
pub fn regenerate_mcp_token(app: AppHandle) -> Result<AppSettings, String> {
    let mut settings = get_settings(&app);
    let token = mcp::token::generate()?;
    settings.mcp_token = Some(token.clone());
    write_settings(&app, settings);
    mcp::manager::apply_current(&app);
    emit_settings_changed(&app, "mcp_token", token.into());
    Ok(get_settings(&app))
}

/// Whether the MCP server is currently listening, for the Integrations panel.
#[tauri::command]
#[specta::specta]
pub fn get_mcp_status(app: AppHandle) -> McpStatus {
    let running = app
        .try_state::<mcp::manager::McpManager>()
        .map(|manager| manager.is_running())
        .unwrap_or(false);
    McpStatus { running }
}
