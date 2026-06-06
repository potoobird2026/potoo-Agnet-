/*!
 * aagnet —— 微内核 Agent 框架 v3
 *
 * 核心只提供：
 *   - SlotPlugin（管道内处理单元接口，含 init/run/shutdown 完整生命周期）
 *   - SlotDirective（执行指令，含 JumpTo 用于循环控制）
 *   - Pipeline（按阶段顺序执行 Slot）
 *   - ServicePlugin（后台服务接口，含 init/start/stop/shutdown 完整生命周期）
 *   - ServiceAccessPoint / SlotAccessPoint（受控接入通道）
 *   - ProviderRegistry（运行时 Provider 注册表——Service 注册能力，Slot 查找能力）
 *   - AgentRuntime（主循环 + 会话管理）
 *
 * 所有业务功能都在 plugins/ 下实现 SlotPlugin / ServicePlugin。
 * 业务能力通过 Provider 扩展机制注册和查找，core 不定义任何业务接口。
 * 业务类型（Message/Thought/Action 等）已迁移至 shared_types / plugins。
 */

pub mod access;
pub mod component;
pub mod context;
pub mod phase;
pub mod pipeline;
pub mod runtime;
pub mod service;
pub mod service_manager;
pub mod slot;
pub mod types;

// ── 只暴露核心基础设施 API ──
pub use access::{ProviderRegistry, ServiceAccessPoint, SlotAccessPoint};
pub use context::{StepContext, StepInput};
pub use phase::Phase;
pub use pipeline::Pipeline;
pub use runtime::AgentRuntime;
pub use service::{ServicePlugin, ServiceSignal};
pub use service_manager::ServiceManager;
pub use slot::{SlotDirective, SlotPlugin};
pub use types::error::{AgentError, PluginError};
pub use types::plugin::{AgentConfig, PluginInitContext, PluginMetadata, RunMode};
pub use types::{CancellationToken, Timestamp, Version};
