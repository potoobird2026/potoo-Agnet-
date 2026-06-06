/*!
 * shared_types/mcp —— MCP 服务跨插件契约
 *
 * 定义内容（按 protocol-shared_types契约协议.md §1）：
 * 1. Provider trait McpBundle（MCP 工具代理集合的契约）
 *
 * 归属：shared_types（中立层，不归属 McpService 也不归属其他消费者）
 * 服务方：McpService::start() 注册 Arc<DynProvider<dyn McpBundle>>
 * 消费方：Assembler 等需要直接列举 MCP 工具的消费者
 *
 * 红线遵守：
 * - T-R01: trait 在此定义，禁止在 services/mcp/ 或 slots/ 内部定义
 * - T-R02: 谁先开发谁定义 trait——本计划先定义
 * - T-R03: trait 不写归属注释
 * - D-R01: 用现有的 DynProvider<T>，不造 DynMcpProvider
 */
use crate::shared_types::ToolProvider;
use std::sync::Arc;

/// MCP 工具代理集合契约
///
/// 注册时以 `Arc<dyn McpBundle>` 形式注册到 PROVIDER_MCP_TOOLS。
pub trait McpBundle: Send + Sync {
    /// 获取当前所有 MCP 工具代理列表
    fn all(&self) -> Vec<Arc<dyn ToolProvider>>;
}
