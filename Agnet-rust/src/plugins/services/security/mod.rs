/*!
 * Security（安全策略引擎）模块
 *
 * Agent 的安全策略执行点（Policy Enforcement Point），在 ToolRegistry
 * 执行工具之前通过 Guardian 链进行安全检查。
 *
 * 四级决策模型：Deny > Allow > Guard > Approve
 */

pub mod approval;
pub mod config;
pub mod engine;
pub mod guardians;
pub mod models;
pub mod service;

// ── 配置 ──
pub use config::{ApprovalConfig, GuardianConfig, SecurityPolicyConfig};

// ── 引擎 ──
pub use engine::{DefaultSecurityPolicyEngine, SecurityPolicyEngine};

// ── 审批 ──
pub use approval::{ApprovalDecision, ApprovalReceiver, ApprovalService};

// ── Guardian ──
pub use guardians::Guardian;

// ── 模型 ──
pub use models::{
    Action, ApproveMergeStrategy, GuardFinding, GuardResult, GuardSeverity, Operation, Resource,
    ResourceType, SecurityDecision, SecurityError, SecurityErrorKind, SessionType, Subject,
};

// ── 服务 ──
pub use service::SecurityService;
