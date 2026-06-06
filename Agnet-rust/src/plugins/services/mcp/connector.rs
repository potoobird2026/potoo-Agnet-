/*! StdioMcpConnector —— MCP 子进程连接器 */
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::time::timeout;

use super::config::{McpConnectionConfig, McpServerConfig};
use super::protocol::*;

pub struct StdioMcpConnector {
    server_config: McpServerConfig,
    /// B-3: per-server connection config（per-server timeout 覆写全局）
    conn_config: McpConnectionConfig,
    child: Option<Child>,
    request_id: u64,
}

impl StdioMcpConnector {
    /// B-3: 签名从 `(config)` 改为 `(server_config, conn_config)`
    pub fn new(server_config: McpServerConfig, conn_config: McpConnectionConfig) -> Self {
        Self {
            server_config,
            conn_config,
            child: None,
            request_id: 0,
        }
    }

    pub fn name(&self) -> &str {
        &self.server_config.name
    }
    pub fn conn_config(&self) -> &McpConnectionConfig {
        &self.conn_config
    }

    pub async fn connect(&mut self) -> Result<Vec<ToolManifest>, String> {
        // B-3: 整体连接超时包裹
        let connect_timeout = Duration::from_secs(self.conn_config.connect_timeout_secs);
        match timeout(connect_timeout, self.connect_with_retry()).await {
            Ok(result) => result,
            Err(_) => {
                // 超时——杀掉子进程
                if let Some(mut child) = self.child.take() {
                    let _ = child.kill().await;
                }
                Err(format!(
                    "MCP Server '{}' 连接超时（{}s）",
                    self.server_config.name, self.conn_config.connect_timeout_secs
                ))
            }
        }
    }

    /// B-3+B-4: 实际连接逻辑（含重试）
    async fn connect_with_retry(&mut self) -> Result<Vec<ToolManifest>, String> {
        // B-4: retry 骨架——失败重试 1 次，间隔 200ms
        match self.try_connect().await {
            Ok(tools) => Ok(tools),
            Err(first_err) => {
                tracing::warn!(
                    "MCP: '{}' 首次连接失败: {}，重试中...",
                    self.server_config.name,
                    first_err
                );
                // 超时已在外层处理，此处只做重试间隔（+ jitter）
                let jitter_ms = 150
                    + (std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_nanos() as u64
                        % 101);
                tokio::time::sleep(Duration::from_millis(jitter_ms)).await;
                self.try_connect().await.map_err(|retry_err| {
                    tracing::error!(
                        "MCP: '{}' 重试仍失败: {}",
                        self.server_config.name,
                        retry_err
                    );
                    format!("{}（重试仍失败: {}）", first_err, retry_err)
                })
            }
        }
    }

    /// B-4: 实际连接逻辑（提取自原 connect）
    async fn try_connect(&mut self) -> Result<Vec<ToolManifest>, String> {
        let mut cmd = Command::new(&self.server_config.command);
        cmd.args(&self.server_config.args);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::inherit());
        cmd.kill_on_drop(true);

        let child = cmd
            .spawn()
            .map_err(|e| format!("启动 MCP Server '{}' 失败: {}", self.server_config.name, e))?;
        self.child = Some(child);

        // 发送 initialize 请求（带 per-server connect timeout）
        let init_params = serde_json::json!({
            "protocolVersion": MCP_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {"name": "aagnet-mcp", "version": "0.1.0"}
        });
        let _init_response = self
            .send_request(
                METHOD_INITIALIZE,
                Some(init_params),
                self.conn_config.request_timeout_secs,
            )
            .await?;

        // 获取工具列表
        let tools_response = self
            .send_request(
                METHOD_TOOLS_LIST,
                None,
                self.conn_config.request_timeout_secs,
            )
            .await?;
        let tools: Vec<ToolManifest> = serde_json::from_value(
            tools_response
                .get("tools")
                .cloned()
                .unwrap_or(serde_json::Value::Array(vec![])),
        )
        .map_err(|e| format!("解析工具列表失败: {}", e))?;

        Ok(tools)
    }

    pub async fn execute(
        &mut self,
        tool_name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let params = serde_json::json!({"name": tool_name, "arguments": args});
        let response = self
            .send_request(
                METHOD_TOOLS_CALL,
                Some(params),
                self.conn_config.request_timeout_secs,
            )
            .await?;
        Ok(response
            .get("content")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    }

    /// B-3: 增加 request_timeout_secs 参数（per-server request timeout）
    async fn send_request(
        &mut self,
        method: &str,
        params: Option<serde_json::Value>,
        request_timeout_secs: u64,
    ) -> Result<serde_json::Value, String> {
        self.request_id += 1;
        let id = self.request_id;
        let request = JsonRpcRequest::new(id, method, params);
        let request_json =
            serde_json::to_string(&request).map_err(|e| format!("序列化请求失败: {}", e))?;

        let child = self.child.as_mut().ok_or("MCP Server 未连接")?;
        let stdin = child.stdin.as_mut().ok_or("stdin 不可用")?;
        stdin
            .write_all(request_json.as_bytes())
            .await
            .map_err(|e| format!("写入请求失败: {}", e))?;
        stdin
            .write_all(b"\n")
            .await
            .map_err(|e| format!("写入换行失败: {}", e))?;

        let stdout = child.stdout.as_mut().ok_or("stdout 不可用")?;
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();

        // B-3: per-server request timeout 包裹读取
        let dur = Duration::from_secs(request_timeout_secs);
        timeout(dur, reader.read_line(&mut line))
            .await
            .map_err(|_| format!("MCP 请求 '{}' 超时（{}s）", method, request_timeout_secs))?
            .map_err(|e| format!("读取响应失败: {}", e))?;

        let response: JsonRpcResponse =
            serde_json::from_str(&line).map_err(|e| format!("反序列化响应失败: {}", e))?;
        if let Some(err) = response.error {
            return Err(format!("MCP 错误 ({}): {}", err.code, err.message));
        }
        response.result.ok_or_else(|| "MCP 返回空结果".to_string())
    }

    pub async fn disconnect(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
    }
}
