use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Provider key 常量——tool_registry slot 和 tool_executor slot 通过此 key 查找 ToolProvider
pub const PROVIDER_TOOL: &str = "tool";

/// Provider key 常量——MCP Service 通过此 key 向 ap 注册 McpBundle
///
/// K-R01 + K-R02: 跨插件 key 必须先在 shared_types 定义
pub const PROVIDER_MCP_TOOLS: &str = "mcp_tools";

/// 工具来源——描述 ToolDefinition 的来源（用于 UI 展示 + 审计 + 调度决策）
///
/// Default = Builtin（向后兼容，老 ToolManifest 不带 source 字段时反序列化不报错）
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq, Hash)]
pub enum ToolSource {
    #[default]
    Builtin,
    /// 已安装的本地工具（来自 ToolDiscover 扫描的 tool.toml）
    Installed,
    /// MCP 远程工具
    Mcp { connector: String },
}

/// 工具定义 —— 跨插件共享类型
///
/// 归属：shared_types
/// 引用者：tool_registry、llm_thinker、ToolsService
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: Value,
    pub entry: String,
    /// 工具来源（标签，不参与业务逻辑）
    #[serde(default)]
    pub source: ToolSource,
}

/// 工具执行错误
#[derive(Debug, Clone, thiserror::Error)]
pub enum ToolError {
    #[error("工具未找到: {0}")]
    NotFound(String),

    #[error("执行超时: {0}")]
    Timeout(String),

    #[error("执行失败: {0}")]
    ExecutionFailed(String),
}

/// 工具 Provider trait —— 由 ToolsService 实现并注册到 ProviderRegistry
#[async_trait::async_trait]
pub trait ToolProvider: Send + Sync {
    /// 返回所有已注册工具的定义列表
    fn list(&self) -> Vec<ToolDefinition>;

    /// Provider 唯一标识（用于 ToolRegistry 内部按 id 查找）
    ///
    /// 默认 "default"——老 Provider 不需要覆写。
    /// McpToolProxy 覆写返回 "mcp"；ToolsService 覆写返回 "tools"。
    fn provider_id(&self) -> &str {
        "default"
    }

    /// 执行工具调用
    async fn execute(
        &self,
        tool_name: &str,
        arguments: Value,
        timeout: std::time::Duration,
    ) -> Result<String, ToolError>;
}

// 不再需要独立的 DynToolProvider——统一使用 shared_types::DynProvider<T>。
// 参见 protocol-shared_types契约协议.md §4。
