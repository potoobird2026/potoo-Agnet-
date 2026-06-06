/*!
 * shared_types —— 跨插件共享的数据契约
 *
 * 这一层位于 core 之上、plugins 之下。
 * core 依赖 shared_types（Pipeline 返回 StepResponse、StepContext 持有 Message），
 * 所有 plugins 依赖 shared_types。
 *
 * shared_types 不依赖任何 plugin 的具体实现。
 *
 * 所有跨插件共享的类型必须定义在此处：
 * - Provider trait（ToolProvider、MemoryProvider）
 * - 跨插件数据结构（Thought、Action、Observation、ToolDefinition 等）
 * - 错误类型（ToolError、MemoryError）
 */

use std::sync::Arc;

/// 通用 Provider 包装结构体——用于跨 Arc<dyn Any> 的类型安全传递
///
/// 协议参考：protocol-shared_types契约协议.md §4
pub struct DynProvider<T: ?Sized + Send + Sync + 'static>(pub Arc<T>);

pub mod assembler;
pub mod compression;
pub mod context;
pub mod llm;

// 手动实现 Clone：Arc<T> 始终是 Clone 的，不需要 T: Clone
impl<T: ?Sized + Send + Sync + 'static> Clone for DynProvider<T> {
    fn clone(&self) -> Self {
        DynProvider(self.0.clone())
    }
}

pub mod mcp;
pub mod memory;
pub mod message;
pub mod skills;
pub mod step_response;
pub mod thought;
pub mod tool;
pub mod vector;

pub mod chronos;
pub mod cli;
pub mod security;

pub use chronos::{ChronosContract, ChronosError, ChronosStatus, PROVIDER_CHRONOS};
pub use cli::{CliError, CliProvider, PROVIDER_CLI_CHANNEL};
pub use context::*;
pub use mcp::McpBundle;
pub use memory::{
    ExperienceEntry, IdentitySection, MemoryError, MemoryFileEntry, MemoryProvider, MemoryStats,
    PROVIDER_MEMORY,
};
pub use message::{ContentBlock, Message, MessageRole, ToolCall};
pub use security::{
    AuditContext, AuditWarning, RiskSeverity, SecurityDecision, SecurityError,
    SecurityPolicyProvider, PROVIDER_SECURITY,
};
pub use skills::{
    InjectionPolicy, QuotaPreference, SkillContract, SkillLevel, SkillsContractBundle,
    PROVIDER_SKILLS,
};
pub use step_response::StepResponse;
pub use thought::{Action, ActionResult, Observation, Thought};
pub use tool::{
    ToolDefinition, ToolError, ToolProvider, ToolSource, PROVIDER_MCP_TOOLS, PROVIDER_TOOL,
};
pub use vector::{
    VectorError, VectorMemoryContract, VectorSearchHit, VectorStats, PROVIDER_VECTOR,
};
