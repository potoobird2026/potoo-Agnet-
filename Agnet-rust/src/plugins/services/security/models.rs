/*!
 * Security 数据模型
 *
 * 定义安全策略引擎所需的所有数据结构：
 * Subject（操作主体）、Action（操作动作）、Resource（目标资源）、
 * SecurityDecision（四级决策）、GuardResult（Guardian 返回结果）、
 * GuardFinding（安全发现）、SecurityError（安全错误）。
 */

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ============================================
// Subject —— 谁在操作
// ============================================

/// 会话类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    /// 交互式（用户实时操作）
    Interactive,
    /// 自动化（脚本/定时触发）
    Automated,
    /// 调试模式
    Debug,
}

/// 操作主体
#[derive(Debug, Clone)]
pub struct Subject {
    /// 会话 ID
    pub session_id: String,
    /// 会话类型
    pub session_type: SessionType,
    /// 附加元数据
    pub metadata: HashMap<String, String>,
}

// ============================================
// Action —— 做什么操作
// ============================================

/// 操作类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Read,
    Write,
    Execute,
    Delete,
    NetworkAccess,
    ConfigModify,
}

/// 操作
#[derive(Debug, Clone)]
pub struct Action {
    /// 工具名称
    pub tool_name: String,
    /// 操作类型
    pub operation: Operation,
    /// 工具参数（JSON）
    pub arguments: Value,
}

// ============================================
// Resource —— 操作什么资源
// ============================================

/// 资源类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceType {
    File,
    Directory,
    NetworkHost,
    Tool,
    Configuration,
    MemoryItem,
}

/// 目标资源
#[derive(Debug, Clone)]
pub struct Resource {
    /// 资源类型
    pub resource_type: ResourceType,
    /// 资源标识（如文件路径、主机名等）
    pub identifier: String,
    /// 附加元数据
    pub metadata: HashMap<String, String>,
}

// ============================================
// SecurityDecision —— 四级决策
// ============================================

/// 安全决策
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecurityDecision {
    /// 立即放行
    Allow,
    /// 立即拒绝
    Deny { reason: String },
    /// 标记问题（不阻断，仅记录）
    Guard { findings: Vec<GuardFinding> },
    /// 需要用户审批
    Approve {
        timeout: Duration,
        prompt: String,
        findings: Vec<GuardFinding>,
    },
}

// ============================================
// GuardResult —— Guardian 返回结果
// ============================================

/// Guardian 评估结果
#[derive(Debug, Clone)]
pub enum GuardResult {
    /// 拒绝操作
    Deny(String),
    /// 放行操作
    Allow,
    /// 标记问题
    Guard(GuardFinding),
    /// 需要审批（携带超时和提示）
    Approve(Duration, String),
}

// ============================================
// GuardFinding —— 安全发现
// ============================================

/// 严重级别
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum GuardSeverity {
    /// 信息提示
    Info,
    /// 低风险
    Low,
    /// 中风险
    Medium,
    /// 高风险
    High,
    /// 严重
    Critical,
}

impl GuardSeverity {
    /// 转换为 shared_types 中的 RiskSeverity
    pub fn into_shared(self) -> crate::shared_types::RiskSeverity {
        match self {
            GuardSeverity::Info => crate::shared_types::RiskSeverity::Info,
            GuardSeverity::Low => crate::shared_types::RiskSeverity::Low,
            GuardSeverity::Medium => crate::shared_types::RiskSeverity::Medium,
            GuardSeverity::High => crate::shared_types::RiskSeverity::High,
            GuardSeverity::Critical => crate::shared_types::RiskSeverity::Critical,
        }
    }
}

/// Guardian 产生的安全发现
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuardFinding {
    /// 产生发现的 Guardian 名称
    pub guardian: String,
    /// 严重级别
    pub severity: GuardSeverity,
    /// 描述消息
    pub message: String,
    /// 修复建议
    pub recommendation: Option<String>,
}

// ============================================
// ApproveMergeStrategy —— 审批合并策略
// ============================================

/// 审批合并策略
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ApproveMergeStrategy {
    /// 取第一个审批要求
    First,
    /// 取超时最短的审批要求
    Strictest,
}

// ============================================
// SecurityError —— 安全错误
// ============================================

/// 安全错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityErrorKind {
    /// 操作被拒绝
    Denied,
    /// 配置无效
    ConfigInvalid,
    /// 审批被取消
    ApprovalCancelled,
    /// 审批超时
    ApprovalTimeout,
    /// 内部错误
    Internal,
}

/// 安全错误
#[derive(Debug, Clone)]
pub struct SecurityError {
    /// 错误类型
    pub kind: SecurityErrorKind,
    /// 错误描述
    pub description: String,
    /// 修复建议
    pub recommendation: Option<String>,
}

impl std::fmt::Display for SecurityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{:?}] {}", self.kind, self.description)
    }
}

impl std::error::Error for SecurityError {}
