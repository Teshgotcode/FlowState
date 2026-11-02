//! The stdio transport: newline-delimited JSON-RPC over the process's own
//! stdin/stdout, the transport Claude Desktop and Cursor launch themselves.
//!
//! stderr is left for diagnostics; a word of protocol never goes to it, and
//! stdout carries nothing but responses.

use super::db::McpDb;
use super::protocol::handle_message;
use std::io::{BufRead, Write};

/// Serve MCP over stdin/stdout until stdin closes.
pub fn serve_stdio(db: McpDb) {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => break,
        };

        if let Some(response) = handle_message(&db, &line) {
            if writeln!(out, "{response}")
                .and_then(|_| out.flush())
                .is_err()
            {
                // The client went away; nothing left to serve.
                break;
            }
        }
    }
}
