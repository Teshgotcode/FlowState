//! The MCP wire protocol: JSON-RPC 2.0 requests over stdio or HTTP, with the
//! four read-only tools the server offers.
//!
//! One function, [`handle_message`], is the whole protocol — both transports
//! feed it one message at a time and write whatever it returns back to the
//! client. Requests get a response; notifications (no `id`) get `None`.

use super::db::McpDb;
use serde_json::{json, Value};

/// The MCP protocol revision this server speaks. `initialize` echoes back
/// whatever the client asked for when it named one, so newer clients that
/// only use the common subset stay connected.
pub const PROTOCOL_VERSION: &str = "2024-11-05";
pub const SERVER_NAME: &str = "flowstate-mcp";
pub const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Parse a positive integer argument, clamping it to a sane range.
fn bounded_int(args: &Value, key: &str, default: u64, max: u64) -> u64 {
    match args.get(key).and_then(Value::as_i64) {
        Some(n) if n > 0 => n.min(max as i64) as u64,
        _ => default,
    }
}

/// JSON Schema for `list_meetings`' arguments.
fn list_meetings_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 200,
                "default": 50,
                "description": "How many meetings to return (newest first)."
            },
            "offset": {
                "type": "integer",
                "minimum": 0,
                "default": 0,
                "description": "How many meetings to skip, for paging."
            }
        },
        "required": []
    })
}

fn get_meeting_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "integer",
                "description": "The meeting's id, from list_meetings."
            }
        },
        "required": ["id"]
    })
}

fn get_transcript_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "id": {
                "type": "integer",
                "description": "The entry's id, from list_meetings or search_history."
            }
        },
        "required": ["id"]
    })
}

fn search_history_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Text to look for in titles and transcriptions."
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": 100,
                "default": 25,
                "description": "How many hits to return (newest first)."
            },
            "offset": {
                "type": "integer",
                "minimum": 0,
                "default": 0,
                "description": "How many hits to skip, for paging."
            }
        },
        "required": ["query"]
    })
}

/// The tools this server offers, as `tools/list` reports them.
fn tools() -> Value {
    json!([
        {
            "name": "list_meetings",
            "description": "List recorded FlowState meeting sessions, newest first. \
                Each meeting has an id the other tools take.",
            "inputSchema": list_meetings_schema()
        },
        {
            "name": "get_meeting",
            "description": "Get one recorded meeting: its verbatim transcript, timed \
                segments and draft meeting note when one was generated.",
            "inputSchema": get_meeting_schema()
        },
        {
            "name": "get_transcript",
            "description": "Get the verbatim transcript of one history entry (meeting \
                or dictation), exactly as the model transcribed it.",
            "inputSchema": get_transcript_schema()
        },
        {
            "name": "search_history",
            "description": "Search every FlowState transcription (meetings and \
                dictations) for a word or phrase, newest first.",
            "inputSchema": search_history_schema()
        }
    ])
}

/// A successful tool result: one text block with pretty-printed JSON.
fn tool_ok(payload: Value) -> Value {
    json!({
        "content": [{
            "type": "text",
            "text": serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string())
        }],
        "isError": false
    })
}

/// A tool that ran but failed: the message is content, not a protocol error,
/// per the MCP spec.
fn tool_err(message: &str) -> Value {
    json!({
        "content": [{ "type": "text", "text": message }],
        "isError": true
    })
}

/// A JSON-RPC error response.
fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

/// Dispatch one `tools/call` to its implementation.
fn call_tool(db: &McpDb, name: &str, args: &Value) -> Value {
    match name {
        "list_meetings" => {
            let limit = bounded_int(args, "limit", 50, 200);
            let offset = args
                .get("offset")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0) as u64;
            match db.list_meetings(limit, offset) {
                Ok(meetings) => tool_ok(json!({ "meetings": meetings })),
                Err(e) => tool_err(&format!("list_meetings failed: {e}")),
            }
        }
        "get_meeting" => {
            let Some(id) = args.get("id").and_then(Value::as_i64) else {
                return tool_err("get_meeting needs an integer 'id' argument.");
            };
            match db.get_meeting(id) {
                Ok(Some(meeting)) => tool_ok(serde_json::to_value(meeting).unwrap_or(Value::Null)),
                Ok(None) => tool_err(&format!("No meeting with id {id}.")),
                Err(e) => tool_err(&format!("get_meeting failed: {e}")),
            }
        }
        "get_transcript" => {
            let Some(id) = args.get("id").and_then(Value::as_i64) else {
                return tool_err("get_transcript needs an integer 'id' argument.");
            };
            match db.get_transcript(id) {
                Ok(Some(transcript)) => {
                    tool_ok(serde_json::to_value(transcript).unwrap_or(Value::Null))
                }
                Ok(None) => tool_err(&format!("No history entry with id {id}.")),
                Err(e) => tool_err(&format!("get_transcript failed: {e}")),
            }
        }
        "search_history" => {
            let Some(query) = args.get("query").and_then(Value::as_str) else {
                return tool_err("search_history needs a 'query' argument.");
            };
            if query.trim().is_empty() {
                return tool_err("search_history needs a non-empty 'query' argument.");
            }
            let limit = bounded_int(args, "limit", 25, 100);
            let offset = args
                .get("offset")
                .and_then(Value::as_i64)
                .unwrap_or(0)
                .max(0) as u64;
            match db.search_history(query, limit, offset) {
                Ok(results) => tool_ok(serde_json::to_value(results).unwrap_or(Value::Null)),
                Err(e) => tool_err(&format!("search_history failed: {e}")),
            }
        }
        other => tool_err(&format!("Unknown tool '{other}'.")),
    }
}

/// Handle one MCP message (a JSON-RPC request or notification).
///
/// Returns `Some(response)` for requests, `None` for notifications and empty
/// input. Transport errors come back as proper JSON-RPC error responses, so
/// a malformed line never kills the server.
pub fn handle_message(db: &McpDb, raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let msg: Value = match serde_json::from_str(trimmed) {
        Ok(msg) => msg,
        Err(e) => {
            return serde_json::to_string(&rpc_error(
                Value::Null,
                -32700,
                &format!("Parse error: {e}"),
            ))
            .ok();
        }
    };

    let id = msg.get("id").cloned();
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        // No method: nothing to answer even if it claims an id.
        return None;
    };

    // Notifications (no id) are acknowledged with silence, including
    // `notifications/initialized`, which ends every client handshake.
    let id = id?;

    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => {
            let requested = params.get("protocolVersion").and_then(Value::as_str);
            json!({
                "protocolVersion": requested.unwrap_or(PROTOCOL_VERSION),
                "capabilities": { "tools": {} },
                "serverInfo": { "name": SERVER_NAME, "version": SERVER_VERSION }
            })
        }
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tools() }),
        "tools/call" => {
            let name = params.get("name").and_then(Value::as_str).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            call_tool(db, name, &args)
        }
        other => {
            return serde_json::to_string(&rpc_error(
                id,
                -32601,
                &format!("Method not found: {other}"),
            ))
            .ok();
        }
    };

    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::path::PathBuf;

    /// A temp history database with one meeting (with a note and segments)
    /// and one dictation, matching the app's schema closely enough for the
    /// MCP queries.
    fn test_db() -> (tempfile::TempDir, McpDb) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path: PathBuf = dir.path().join("history.db");
        let conn = Connection::open(&path).expect("open");
        conn.execute_batch(
            "CREATE TABLE transcription_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                file_name TEXT NOT NULL,
                timestamp INTEGER NOT NULL,
                saved BOOLEAN NOT NULL DEFAULT 0,
                title TEXT NOT NULL,
                transcription_text TEXT NOT NULL,
                post_processed_text TEXT,
                post_process_prompt TEXT,
                post_process_requested BOOLEAN NOT NULL DEFAULT 0,
                source_name TEXT,
                language TEXT,
                language_source TEXT,
                source TEXT,
                metadata TEXT
            );
            INSERT INTO transcription_history
                (file_name, timestamp, saved, title, transcription_text, language, source, metadata)
            VALUES
                ('meeting.wav', 1700000000, 1, 'Standup 14. november', 'Hei alle sammen. Hva skjedde siden sist?', 'nb',
                 'meeting',
                 '{\"duration_secs\":600,\"segments\":[{\"t0_ms\":0,\"t1_ms\":900,\"text\":\"Hei alle sammen.\"}],\"meeting_note\":{\"summary\":\"Kort mote\",\"decisions\":[],\"action_items\":[{\"task\":\"Skrive referat\",\"owner\":\"Ola\"}],\"open_questions\":[],\"model\":\"llama\",\"generated_at\":1700000100}}');
            INSERT INTO transcription_history
                (file_name, timestamp, saved, title, transcription_text)
            VALUES
                ('dictation.wav', 1700001000, 0, ' handleliste ', 'melk og brød');
            ",
        )
        .expect("seed");
        let db = McpDb::open(&path).expect("open read-only");
        (dir, db)
    }

    fn call(db: &McpDb, method: &str, params: Value) -> Value {
        let response = handle_message(
            db,
            &serde_json::to_string(&json!({
                "jsonrpc": "2.0", "id": 1, "method": method, "params": params
            }))
            .unwrap(),
        )
        .expect("request gets a response");
        let parsed: Value = serde_json::from_str(&response).unwrap();
        parsed["result"].clone()
    }

    #[test]
    fn initialize_answers_with_server_info() {
        let (_dir, db) = test_db();
        let result = call(
            &db,
            "initialize",
            json!({ "protocolVersion": "2024-11-05", "capabilities": {} }),
        );
        assert_eq!(result["serverInfo"]["name"], "flowstate-mcp");
        assert_eq!(result["protocolVersion"], "2024-11-05");
    }

    #[test]
    fn initialized_notification_gets_no_response() {
        let (_dir, db) = test_db();
        assert!(handle_message(
            &db,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#
        )
        .is_none());
    }

    #[test]
    fn tools_list_names_every_tool() {
        let (_dir, db) = test_db();
        let result = call(&db, "tools/list", json!({}));
        let names: Vec<&str> = result["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "list_meetings",
                "get_meeting",
                "get_transcript",
                "search_history"
            ]
        );
    }

    #[test]
    fn list_meetings_returns_the_meeting_not_the_dictation() {
        let (_dir, db) = test_db();
        let result = call(
            &db,
            "tools/call",
            json!({ "name": "list_meetings", "arguments": {} }),
        );
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        let meetings = parsed["meetings"].as_array().unwrap();
        assert_eq!(meetings.len(), 1);
        assert_eq!(meetings[0]["title"], "Standup 14. november");
        assert_eq!(meetings[0]["duration_secs"], 600);
        assert_eq!(meetings[0]["has_note"], true);
    }

    #[test]
    fn get_meeting_returns_verbatim_transcript_and_note() {
        let (_dir, db) = test_db();
        let result = call(
            &db,
            "tools/call",
            json!({ "name": "get_meeting", "arguments": { "id": 1 } }),
        );
        assert_eq!(result["isError"], false);
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        // Verbatim: the model's own text, character for character.
        assert_eq!(
            parsed["transcript"],
            "Hei alle sammen. Hva skjedde siden sist?"
        );
        assert_eq!(parsed["note"]["action_items"][0]["task"], "Skrive referat");
        assert_eq!(parsed["segments"][0]["start_ms"], 0);
    }

    #[test]
    fn get_transcript_is_verbatim_for_any_entry() {
        let (_dir, db) = test_db();
        let result = call(
            &db,
            "tools/call",
            json!({ "name": "get_transcript", "arguments": { "id": 2 } }),
        );
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["text"], "melk og brød");
        assert_eq!(parsed["source"], "dictation");
    }

    #[test]
    fn get_meeting_rejects_a_dictation_id() {
        let (_dir, db) = test_db();
        let result = call(
            &db,
            "tools/call",
            json!({ "name": "get_meeting", "arguments": { "id": 2 } }),
        );
        assert_eq!(result["isError"], true);
    }

    #[test]
    fn search_history_matches_text_and_escapes_wildcards() {
        let (_dir, db) = test_db();
        let result = call(
            &db,
            "tools/call",
            json!({ "name": "search_history", "arguments": { "query": "brød" } }),
        );
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["total"], 1);
        assert_eq!(parsed["hits"][0]["id"], 2);

        // A lone % is a literal, not a match-everything wildcard.
        let result = call(
            &db,
            "tools/call",
            json!({ "name": "search_history", "arguments": { "query": "%" } }),
        );
        let text = result["content"][0]["text"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(text).unwrap();
        assert_eq!(parsed["total"], 0);
    }

    #[test]
    fn unknown_method_is_a_jsonrpc_error() {
        let (_dir, db) = test_db();
        let response =
            handle_message(&db, r#"{"jsonrpc":"2.0","id":7,"method":"resources/list"}"#).unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32601);
    }

    #[test]
    fn malformed_json_is_a_parse_error() {
        let (_dir, db) = test_db();
        let response = handle_message(&db, "{not json").unwrap();
        let parsed: Value = serde_json::from_str(&response).unwrap();
        assert_eq!(parsed["error"]["code"], -32700);
    }
}
