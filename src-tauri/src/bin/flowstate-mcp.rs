//! `flowstate-mcp` — FlowState's local MCP server, as its own binary.
//!
//! Lets AI assistants (Claude Desktop, Cursor) read FlowState's meeting and
//! transcription history over MCP. Everything stays on the machine: the
//! database is opened read-only, the HTTP transport binds 127.0.0.1 only and
//! demands a per-user token, and no request ever leaves localhost.
//!
//! Two transports:
//! - stdio, for clients that launch this binary themselves:
//!   `flowstate-mcp --stdio [--db <history.db>]`
//! - localhost HTTP, started by the app on the user's machine:
//!   `flowstate-mcp --db <history.db> --port <port> --token <token>`
//!
//! Without `--stdio` or `--port` it prints help and exits.

use clap::{CommandFactory, Parser};
use flowstate_app_lib::mcp::{db::McpDb, default_db_path, http, stdio};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

#[derive(Parser)]
#[command(
    name = "flowstate-mcp",
    version,
    about = "FlowState's local MCP server: reads meeting and transcription history, \
              never writes it, never leaves the machine."
)]
struct Args {
    /// FlowState's history database. Defaults to the standard app data
    /// location (portable mode's Data/ directory when marked).
    #[arg(long, value_name = "PATH")]
    db: Option<PathBuf>,
    /// Serve MCP over stdin/stdout (Claude Desktop, Cursor).
    #[arg(long)]
    stdio: bool,
    /// Serve MCP over localhost HTTP on 127.0.0.1:PORT.
    #[arg(long, value_name = "PORT")]
    port: Option<u16>,
    /// The Bearer token the HTTP server requires. Required with --port.
    #[arg(long, value_name = "TOKEN")]
    token: Option<String>,
}

fn main() {
    let args = Args::parse();

    let db_path = args.db.clone().unwrap_or_else(default_db_path);
    let db = match McpDb::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("flowstate-mcp: {e}");
            std::process::exit(1);
        }
    };

    if args.stdio {
        stdio::serve_stdio(db);
        return;
    }

    match (args.port, args.token) {
        (Some(port), Some(token)) => {
            let listener = match http::bind(port) {
                Ok(listener) => listener,
                Err(e) => {
                    eprintln!("flowstate-mcp: cannot listen on 127.0.0.1:{port}: {e}");
                    std::process::exit(1);
                }
            };
            // A standalone server runs until the app (or the user) kills it.
            eprintln!("flowstate-mcp: serving MCP on http://127.0.0.1:{port}/mcp");
            let never = Arc::new(AtomicBool::new(false));
            http::serve_on(listener, db_path, token, never);
        }
        (Some(_), None) => {
            eprintln!("flowstate-mcp: --port also needs --token");
            std::process::exit(2);
        }
        _ => {
            let mut cmd = Args::command();
            let _ = cmd.print_help();
            std::process::exit(2);
        }
    }
}
