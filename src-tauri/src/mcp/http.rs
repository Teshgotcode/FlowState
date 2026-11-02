//! The localhost HTTP transport: MCP JSON-RPC over HTTP on `127.0.0.1` only,
//! guarded by a per-user Bearer token.
//!
//! The listener is bound by [`bind`] before [`serve_on`] runs, so a port
//! already in use fails where the caller can tell the user. Requests are
//! answered on their own thread, and checking [`serve_on`]'s shutdown flag
//! between accepts keeps a stop request from waiting on a client.

use super::db::McpDb;
use super::protocol::handle_message;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Bind the MCP port — on loopback and nowhere else. Binding anywhere else
/// would put the user's transcriptions on the network.
pub fn bind(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("127.0.0.1", port))
}

/// Serve MCP over HTTP on an already-bound loopback listener until
/// `shutdown` is set. Never returns an error: once the listener is bound the
/// only reasons to stop are the shutdown flag or the listener dying.
pub fn serve_on(listener: TcpListener, db_path: PathBuf, token: String, shutdown: Arc<AtomicBool>) {
    // Poll for shutdown between accepts instead of blocking in accept(2).
    listener.set_nonblocking(true).ok();

    while !shutdown.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                let db_path = db_path.clone();
                let token = token.clone();
                std::thread::spawn(move || handle_connection(stream, db_path, token));
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
}

/// One HTTP request/response cycle on one connection.
fn handle_connection(mut stream: TcpStream, db_path: PathBuf, token: String) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let (head, body_start) = match read_request(&mut stream) {
        Ok(Some(parts)) => parts,
        Ok(None) => return, // client left before sending a request
        Err(()) => {
            respond(
                &mut stream,
                "400 Bad Request",
                "{\"error\":\"bad request\"}",
            );
            return;
        }
    };

    let (method, path) = match parse_request_line(&head.request_line) {
        Some(parts) => parts,
        None => {
            respond(
                &mut stream,
                "400 Bad Request",
                "{\"error\":\"bad request\"}",
            );
            return;
        }
    };

    // Every route wants the token first: wrong token gets 401 no matter the
    // method or path, so nothing about the server leaks to the uninvited.
    let authorized = header(&head.headers, "authorization")
        .map(|value| authorized_token(value, &token))
        .unwrap_or(false);
    if !authorized {
        respond_unauthorized(&mut stream);
        return;
    }

    if method != "POST" {
        respond_with_headers(
            &mut stream,
            "405 Method Not Allowed",
            &[("Allow", "POST")],
            "{\"error\":\"POST only\"}",
        );
        return;
    }

    if path != "/mcp" && path != "/" {
        respond(&mut stream, "404 Not Found", "{\"error\":\"not found\"}");
        return;
    }

    let body = match read_body(&mut stream, &head.headers, body_start) {
        Ok(Some(body)) => body,
        Ok(None) => {
            respond(
                &mut stream,
                "400 Bad Request",
                "{\"error\":\"missing body\"}",
            );
            return;
        }
        Err(()) => {
            respond(
                &mut stream,
                "413 Payload Too Large",
                "{\"error\":\"payload too large\"}",
            );
            return;
        }
    };

    // Every connection opens its own read-only database handle; SQLite
    // opens are cheap and this keeps one request from blocking another.
    let db = match McpDb::open(&db_path) {
        Ok(db) => db,
        Err(e) => {
            respond(
                &mut stream,
                "500 Internal Server Error",
                &format!("{{\"error\":\"{e}\"}}"),
            );
            return;
        }
    };

    match answer(&db, &body) {
        Some(response) => respond(&mut stream, "200 OK", &response),
        // Only notifications: acknowledged, nothing to return.
        None => respond(&mut stream, "202 Accepted", ""),
    }
}

/// Run one message (or a batch of them) and serialize the response, if any.
fn answer(db: &McpDb, body: &str) -> Option<String> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return None;
    }
    // A JSON array is a batch: answer each request in it, in order.
    if trimmed.starts_with('[') {
        let messages: Vec<serde_json::Value> = match serde_json::from_str(trimmed) {
            Ok(messages) => messages,
            Err(_) => return None,
        };
        let responses: Vec<String> = messages
            .iter()
            .filter_map(|msg| {
                serde_json::to_string(msg)
                    .ok()
                    .and_then(|raw| handle_message(db, &raw))
            })
            .collect();
        return if responses.is_empty() {
            None
        } else {
            Some(format!("[{}]", responses.join(",")))
        };
    }
    handle_message(db, trimmed)
}

/// The request head: the request line plus all headers.
struct RequestHead {
    request_line: String,
    headers: Vec<(String, String)>,
}

/// Read the request head, keeping any body bytes that arrived with it —
/// one `read` commonly returns the head and the body together, and those
/// bytes must not be lost on the way to [`read_body`]. `Ok(None)` is a clean
/// EOF before any bytes arrived.
fn read_request(stream: &mut TcpStream) -> Result<Option<(RequestHead, Vec<u8>)>, ()> {
    const HEAD_LIMIT: usize = 64 * 1024;
    let mut buf: Vec<u8> = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    loop {
        let n = stream.read(&mut chunk).map_err(|_| ())?;
        if n == 0 {
            if buf.is_empty() {
                return Ok(None);
            }
            return Err(());
        }
        buf.extend_from_slice(&chunk[..n]);
        if buf.len() > HEAD_LIMIT {
            return Err(());
        }
        if let Some(split) = find_head_end(&buf) {
            let head = String::from_utf8_lossy(&buf[..split]).to_string();
            let mut lines = head.split("\r\n");
            let request_line = lines.next().unwrap_or_default().to_string();
            let headers = lines
                .filter_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    Some((name.trim().to_ascii_lowercase(), value.trim().to_string()))
                })
                .collect();
            let body_start = buf[split..].to_vec();
            return Ok(Some((
                RequestHead {
                    request_line,
                    headers,
                },
                body_start,
            )));
        }
    }
}

/// Where the blank line ending the head sits, if it has arrived yet.
fn find_head_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| p + 4)
}

/// Read the body the head's Content-Length promised, starting from the bytes
/// [`read_request`] already collected. `Ok(None)` means the head never named
/// a length, so there is nothing to read.
fn read_body(
    stream: &mut TcpStream,
    headers: &[(String, String)],
    mut body: Vec<u8>,
) -> Result<Option<String>, ()> {
    const BODY_LIMIT: usize = 1024 * 1024;
    let Some(length) = header(headers, "content-length") else {
        return Ok(None);
    };
    let length: usize = length.parse().map_err(|_| ())?;
    if length > BODY_LIMIT {
        return Err(());
    }
    while body.len() < length {
        let remaining = length - body.len();
        let mut chunk = vec![0u8; remaining.min(4096)];
        let n = stream.read(&mut chunk).map_err(|_| ())?;
        if n == 0 {
            return Err(()); // less body than the head promised
        }
        body.extend_from_slice(&chunk[..n]);
    }
    Ok(Some(String::from_utf8_lossy(&body).to_string()))
}

fn header<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(header_name, _)| header_name == name)
        .map(|(_, value)| value.as_str())
}

/// "METHOD /path" from a request line; anything else is malformed.
fn parse_request_line(line: &str) -> Option<(String, String)> {
    let mut parts = line.split(' ');
    let method = parts.next()?.to_string();
    let target = parts.next()?.to_string();
    let path = target.split('?').next().unwrap_or("/").to_string();
    Some((method, path))
}

/// Whether an Authorization header value carries this server's token.
fn authorized_token(header_value: &str, token: &str) -> bool {
    match header_value.strip_prefix("Bearer ") {
        Some(offered) => super::token::constant_time_eq(offered.trim(), token),
        None => false,
    }
}

/// Write a plain JSON response with a `Connection: close`.
fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    respond_with_headers(stream, status, &[], body);
}

fn respond_unauthorized(stream: &mut TcpStream) {
    respond_with_headers(
        stream,
        "401 Unauthorized",
        &[("WWW-Authenticate", "Bearer realm=\"flowstate-mcp\"")],
        "{\"error\":\"unauthorized\"}",
    );
}

fn respond_with_headers(stream: &mut TcpStream, status: &str, extra: &[(&str, &str)], body: &str) {
    let extra_headers: String = extra
        .iter()
        .map(|(name, value)| format!("{name}: {value}\r\n"))
        .collect();
    let response = format!(
        "HTTP/1.1 {status}\r\n{extra_headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    /// The same schema and seed rows the protocol tests use.
    fn seeded_db(dir: &std::path::Path) -> PathBuf {
        let path = dir.join("history.db");
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
            VALUES ('meeting.wav', 1700000000, 1, 'Standup', 'Hei alle sammen.', 'nb', 'meeting',
                '{\"duration_secs\":600,\"segments\":[],\"meeting_note\":null}');
            ",
        )
        .expect("seed");
        path
    }

    /// One raw HTTP request; returns (status code, body).
    fn request(
        port: u16,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: &str,
    ) -> (u16, String) {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        let req = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{auth}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(req.as_bytes()).unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).expect("read response");
        let (status_line, rest) = raw.split_once("\r\n").unwrap();
        let code: u16 = status_line
            .split(' ')
            .nth(1)
            .unwrap()
            .parse()
            .expect("status code");
        let body = rest
            .split_once("\r\n\r\n")
            .map(|(_, b)| b.to_string())
            .unwrap_or_default();
        (code, body)
    }

    /// A running server plus a way to stop it, on a free loopback port.
    fn running_server() -> (u16, Arc<AtomicBool>, std::path::PathBuf, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = seeded_db(dir.path());
        let listener = bind(0).expect("bind loopback");
        let port = listener.local_addr().unwrap().port();
        let shutdown = Arc::new(AtomicBool::new(false));
        let serve_shutdown = shutdown.clone();
        let serve_path = db_path.clone();
        std::thread::spawn(move || {
            serve_on(
                listener,
                serve_path,
                "test-token".to_string(),
                serve_shutdown,
            )
        });
        (port, shutdown, db_path, dir)
    }

    #[test]
    fn listener_binds_loopback_only() {
        let listener = bind(0).expect("bind");
        let addr = listener.local_addr().unwrap();
        assert!(addr.ip().is_loopback());
        assert_eq!(addr.ip().to_string(), "127.0.0.1");
    }

    #[test]
    fn missing_or_wrong_token_gets_401() {
        let (port, shutdown, _db, _dir) = running_server();
        let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;

        let (code, _) = request(port, "POST", "/mcp", None, body);
        assert_eq!(code, 401);

        let (code, _) = request(port, "POST", "/mcp", Some("wrong-token"), body);
        assert_eq!(code, 401);

        shutdown.store(true, Ordering::Relaxed);
    }

    #[test]
    fn right_token_lists_tools_and_gets_verbatim_transcript() {
        let (port, shutdown, _db, _dir) = running_server();

        let (code, body) = request(
            port,
            "POST",
            "/mcp",
            Some("test-token"),
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
        );
        assert_eq!(code, 200);
        assert!(body.contains("list_meetings"), "tools listed: {body}");

        let (code, body) = request(
            port,
            "POST",
            "/mcp",
            Some("test-token"),
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"get_transcript","arguments":{"id":1}}}"#,
        );
        assert_eq!(code, 200);
        // The transcript comes back verbatim, escaped inside the JSON-RPC
        // content string.
        assert!(
            body.contains("Hei alle sammen."),
            "verbatim transcript: {body}"
        );

        shutdown.store(true, Ordering::Relaxed);
    }

    #[test]
    fn get_is_rejected_and_notifications_are_acknowledged() {
        let (port, shutdown, _db, _dir) = running_server();

        let (code, _) = request(port, "GET", "/mcp", Some("test-token"), "");
        assert_eq!(code, 405);

        let (code, _) = request(
            port,
            "POST",
            "/mcp",
            Some("test-token"),
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        );
        assert_eq!(code, 202);

        shutdown.store(true, Ordering::Relaxed);
    }

    #[test]
    fn stopped_server_leaves_no_port_behind() {
        let (port, shutdown, _db, _dir) = running_server();

        // Prove the port was live, then stop the server and prove the port
        // is gone: "avslått i Innstillinger = ingen prosess, ingen port".
        let (code, _) = request(
            port,
            "POST",
            "/mcp",
            Some("test-token"),
            r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        );
        assert_eq!(code, 200);

        shutdown.store(true, Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(300));
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    }
}
