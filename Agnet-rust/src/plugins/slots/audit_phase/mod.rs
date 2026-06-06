// ============================================
// 模块：audit_phase 槽口
//
// 模块职责：
// 在 Pipeline Audit 阶段对 LLM 产生的工具调用动作进行安全审查
//
// 模块边界：
// - 本模块负责：安全策略评估、敏感信息检测、风险评级、审计日志
// - 本模块不负责：工具执行（ToolExecutorSlot）、LLM 思考（LlmThinkerSlot）
//
// 依赖 Provider：
// - "security"（可选，由 SecurityService 注册，未注册时使用内置规则）
//
// 被依赖模块：
// - tool_executor 读取本模块写入的 audit_result 和 audit_warnings
//
// 协议合规：
// - S-R01：Continue（放行）/ AbortStep（拦截）按场景正确返回
// - S-R03：审计日志缓冲区存入 StepContext，编译后的正则缓存在 Slot 字段中（init() 时一次性编译）
// - C-R03：run() 可重入
// ============================================

pub mod config;
pub mod error;
pub mod plugin;
pub mod types;

pub use config::{AuditPhaseConfig, RiskAction, SensitiveRuleConfig};
pub use plugin::AuditPhaseSlot;
