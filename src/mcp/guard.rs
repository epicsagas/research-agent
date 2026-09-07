//! Pre-handshake stdin guard for the stdio MCP transport.
//!
//! Some MCP clients (Antigravity's language server, at least) send non-MCP
//! JSON-RPC probes (e.g. `server/discover`) before the `initialize` handshake.
//! rmcp answers a pre-init `ping` but aborts the connection on anything else,
//! while other SDKs respond `-32601` and keep the connection open. This module
//! consumes stdin line by line until the first message rmcp may see, answering
//! unknown requests itself so the handshake starts on a clean line.

use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// What to do with a line read before the `initialize` handshake.
enum Probe {
    /// Pass the line through to rmcp (`initialize`, `ping`, unparseable).
    Forward,
    /// Unknown request: answer with `-32601` and drop the line.
    Answer(Value),
    /// Unknown notification: no response expected, drop the line.
    Ignore,
}

/// Classify one raw JSON-RPC line read before the handshake.
fn classify(line: &[u8]) -> Probe {
    let Ok(msg) = serde_json::from_slice::<Value>(line) else {
        return Probe::Forward; // garbage → let rmcp's parse-error path respond
    };
    let Some(method) = msg.get("method").and_then(Value::as_str) else {
        return Probe::Forward; // response/error → nothing to gate pre-init
    };
    match method {
        "initialize" | "ping" => Probe::Forward,
        // Unknown request (e.g. Antigravity's `server/discover`): answer
        // JSON-RPC `-32601` so the client survives until `initialize`.
        _ => match msg.get("id") {
            Some(id) => Probe::Answer(id.clone()),
            None => Probe::Ignore,
        },
    }
}

/// Read stdin until a line rmcp should see arrives; return that line
/// (newline included) or `None` if the client disconnected first.
pub(super) async fn read_until_forwardable() -> Option<Vec<u8>> {
    let mut stdin = tokio::io::stdin();
    let mut stdout = tokio::io::stdout();
    loop {
        // Byte-at-a-time so nothing past the newline is consumed; the rest of
        // the stream stays available to rmcp's transport. Stdin carries only
        // control-plane traffic, so the syscall cost is irrelevant.
        let mut line = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            match stdin.read(&mut byte).await {
                Ok(0) | Err(_) => return None,
                Ok(_) => {
                    line.push(byte[0]);
                    if byte[0] == b'\n' {
                        break;
                    }
                }
            }
        }
        match classify(&line) {
            Probe::Forward => return Some(line),
            Probe::Ignore => continue,
            Probe::Answer(id) => {
                let resp = json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": {"code": -32601, "message": "Method not found"},
                });
                let _ = stdout.write_all(resp.to_string().as_bytes()).await;
                let _ = stdout.write_all(b"\n").await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_initialize_and_ping() {
        let init = br#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
        assert!(matches!(classify(init), Probe::Forward));
        let ping = br#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#;
        assert!(matches!(classify(ping), Probe::Forward));
    }

    #[test]
    fn answers_unknown_request_with_id() {
        let probe = br#"{"jsonrpc":"2.0","id":7,"method":"server/discover","params":{}}"#;
        match classify(probe) {
            Probe::Answer(id) => assert_eq!(id, Value::from(7)),
            _ => panic!("expected Answer"),
        }
    }

    #[test]
    fn ignores_unknown_notification() {
        let note = br#"{"jsonrpc":"2.0","method":"server/discover"}"#;
        assert!(matches!(classify(note), Probe::Ignore));
    }

    #[test]
    fn forwards_garbage_and_responses() {
        assert!(matches!(classify(b"not json\n"), Probe::Forward));
        let resp = br#"{"jsonrpc":"2.0","id":1,"result":{}}"#;
        assert!(matches!(classify(resp), Probe::Forward));
    }
}
