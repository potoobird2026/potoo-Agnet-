/*! McpToolProxy —— MCP 工具代理
 *
 * A-5: 实现 shared_types::ToolProvider
 *      entry 字段填 "mcp:{connector_name}"，source 填 ToolSource::Mcp
 *      provider_id() 覆写返回 "mcp:{connector_name}"（与 entry 匹配，ToolRegistry.execute 委托依赖）
 */
use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use super::connector::StdioMcpConnector;
use super::protocol::ToolManifest;
use crate::shared_types::{ToolDefinition, ToolError, ToolProvider, ToolSource};

/// MCP 工具代理——将远程 MCP 工具包装为可执行的标准工具
pub struct McpToolProxy {
    name: String,
    description: String,
    connector: Arc<Mutex<StdioMcpConnector>>,
    connector_name: String,
    /// provider_id 必须与 entry 一致（"mcp:{connector}"），否则 ToolRegistry.execute 委托失败
    provider_id_str: String,
    manifest: ToolManifest,
}

impl McpToolProxy {
    pub fn new(
        manifest: ToolManifest,
        connector: Arc<Mutex<StdioMcpConnector>>,
        connector_name: &str,
    ) -> Self {
        let name = format!("mcp/{}/{}", connector_name, manifest.name);
        let description = format!("[MCP:{}] {}", connector_name, manifest.description);
        let provider_id_str = format!("mcp:{}", connector_name);
        Self {
            name,
            description,
            connector,
            connector_name: connector_name.to_string(),
            provider_id_str,
            manifest,
        }
    }

    pub fn connector_name(&self) -> &str {
        &self.connector_name
    }

    /// 构造 ToolDefinition 列表——entry 填 "mcp:{connector_name}"，source 填 Mcp
    pub fn into_tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![ToolDefinition {
            name: self.name.clone(),
            description: self.description.clone(),
            parameters: self.manifest.input_schema.clone(),
            entry: format!("mcp:{}", self.connector_name),
            source: ToolSource::Mcp {
                connector: self.connector_name.clone(),
            },
        }]
    }
}

#[async_trait]
impl ToolProvider for McpToolProxy {
    fn list(&self) -> Vec<ToolDefinition> {
        self.into_tool_definitions()
    }

    fn provider_id(&self) -> &str {
        &self.provider_id_str
    }

    async fn execute(
        &self,
        _tool_name: &str,
        arguments: Value,
        _timeout: Duration,
    ) -> Result<String, ToolError> {
        let mut conn = self.connector.lock().await;
        match conn.execute(&self.manifest.name, arguments).await {
            Ok(value) => Ok(value.to_string()),
            Err(e) => Err(ToolError::ExecutionFailed(e)),
        }
    }
}
