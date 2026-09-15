pub mod extract;
pub mod protocol;
#[cfg(unix)]
pub mod reader_listener;

#[cfg(unix)]
pub fn mcp_socket_path() -> std::path::PathBuf {
    // Stable singleton path under the user's config dir; fall back to temp.
    match crate::settings::preferred_config_dir() {
        Some(dir) => dir.join("reader.sock"),
        None => std::env::temp_dir().join("bookokrat-reader.sock"),
    }
}

use serde_json::{Value, json};

fn tools_list_result(id: Value) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "tools": [
                {
                    "name": "get_current_page",
                    "description": "Get the text of the page/slide currently displayed in the running bookokrat reader.",
                    "inputSchema": { "type": "object", "properties": {} }
                },
                {
                    "name": "get_current_chapter",
                    "description": "Get the text of the current chapter (broader context) from the running bookokrat reader.",
                    "inputSchema": { "type": "object", "properties": {} }
                }
            ]
        }
    })
}

fn tool_call_ok(id: Value, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [ { "type": "text", "text": text } ]
        }
    })
}

fn error_result(id: Value, code: i64, message: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": { "code": code, "message": message }
    })
}

/// Minimal stdio JSON-RPC 2.0 MCP server. On each `tools/call` it connects to
/// the running reader's socket and returns the live page/chapter text.
#[cfg(unix)]
pub fn run_mcp_server() -> anyhow::Result<()> {
    use crate::mcp::protocol::{McpRequest, McpTool};
    use crate::mcp::reader_listener::send_request;
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let sock = mcp_socket_path();

    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("");

        let resp: Value = match method {
            "initialize" => json!({
                "jsonrpc": "2.0", "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": { "tools": {} },
                    "serverInfo": { "name": "bookokrat", "version": env!("CARGO_PKG_VERSION") }
                }
            }),
            "notifications/initialized" => continue, // notification -> no response
            "tools/list" => tools_list_result(id.clone()),
            "tools/call" => {
                let tool_name = req
                    .pointer("/params/name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let tool = match tool_name {
                    "get_current_page" => McpTool::GetCurrentPage,
                    "get_current_chapter" => McpTool::GetCurrentChapter,
                    _ => {
                        let _ = writeln!(
                            stdout,
                            "{}",
                            error_result(id.clone(), -32602, "unknown tool")
                        );
                        stdout.flush()?;
                        continue;
                    }
                };
                match send_request(&sock, &McpRequest { tool }) {
                    Ok(crate::mcp::protocol::McpResponse::Ok { result }) => {
                        let payload = serde_json::to_string(&result)?;
                        tool_call_ok(id.clone(), &payload)
                    }
                    Ok(crate::mcp::protocol::McpResponse::Error { error }) => {
                        error_result(id.clone(), -32000, &error)
                    }
                    Err(_) => error_result(
                        id.clone(),
                        -32000,
                        "bookokrat is not running. Open a book first.",
                    ),
                }
            }
            _ => error_result(id.clone(), -32601, "method not found"),
        };
        writeln!(stdout, "{}", resp)?;
        stdout.flush()?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn run_mcp_server() -> anyhow::Result<()> {
    eprintln!("bookokrat mcp is not supported on this platform (requires a Unix domain socket).");
    std::process::exit(1);
}

#[cfg(test)]
mod server_tests {
    use super::*;

    #[test]
    fn tools_list_advertises_both_tools() {
        let resp = tools_list_result(json!(1));
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("get_current_page"));
        assert!(json.contains("get_current_chapter"));
    }

    #[test]
    fn tool_call_success_wraps_text_content() {
        let resp = tool_call_ok(json!(1), "hello world");
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("\"type\":\"text\""));
        assert!(json.contains("hello world"));
    }
}
