/*! MCP Mock Server —— 真实 stdio 子进程
 *
 * 实现 JSON-RPC 2.0 三个方法：
 * - initialize: 返回 serverInfo + protocolVersion
 * - tools/list: 返回 1 个工具 "echo"
 * - tools/call: name="echo" → 返回 arguments 作为 content
 *
 * 行为契约：
 * - 读 stdin 一行（JSON-RPC 请求）
 * - 写 stdout 一行（JSON-RPC 响应）
 * - 收到 EOF 时退出
 */
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn handle_request(req: &Value) -> Value {
    let id = req.get("id").cloned().unwrap_or(json!(0));
    let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");

    match method {
        "initialize" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "aagnet-mcp-mock", "version": "0.1.0"}
            }
        }),
        "tools/list" => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {
                "tools": [{
                    "name": "echo",
                    "description": "echoes back the arguments",
                    "inputSchema": {"type": "object", "properties": {"msg": {"type": "string"}}}
                }]
            }
        }),
        "tools/call" => {
            let args = req
                .get("params")
                .and_then(|p| p.get("arguments"))
                .cloned()
                .unwrap_or(json!({}));
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{
                        "type": "text",
                        "text": format!("echo: {}", args)
                    }]
                }
            })
        }
        _ => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {"code": -32601, "message": format!("method not found: {}", method)}
        }),
    }
}

fn main() {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut reader = stdin.lock();

    loop {
        let mut line = String::new();
        let n = match reader.read_line(&mut line) {
            Ok(n) => n,
            Err(_) => break,
        };
        if n == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let req: Value = match serde_json::from_str(trimmed) {
            Ok(v) => v,
            Err(_) => continue,
        };
        let resp = handle_request(&req);
        let resp_str = serde_json::to_string(&resp).unwrap_or_default();
        let _ = writeln!(stdout, "{}", resp_str);
        let _ = stdout.flush();
    }
}
