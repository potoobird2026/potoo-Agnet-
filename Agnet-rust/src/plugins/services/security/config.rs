/*!
 * Security 配置层
 *
 * 包含 SecurityPolicyConfig（顶层安全配置）、ApprovalConfig（审批配置）、
 * GuardianConfig（各 Guardian 的独立配置）。
 *
 * 红线：config.validate() 在 init 阶段执行，无效配置立即报错。
 */

use std::collections::HashMap;

use serde::Deserialize;

use crate::core::types::error::PluginError;

use super::models::{ApproveMergeStrategy, SecurityDecision};

// ============================================
// SecurityPolicyConfig
// ============================================

/// 顶层安全配置
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct SecurityPolicyConfig {
    /// 无 Guardian 匹配时的默认决策（安全优先：默认 Deny）
    pub default_decision: SecurityDecision,
    /// 用户确认超时秒数（默认 30）
    pub user_confirmation_timeout_secs: u64,
    /// 审批合并策略
    pub approve_merge_strategy: ApproveMergeStrategy,
    /// 每个 Guardian 的独立配置（key = Guardian 名称）
    pub guardian_configs: HashMap<String, GuardianConfig>,
    /// 审计开关（默认 true）
    pub audit_enabled: bool,
}

impl Default for SecurityPolicyConfig {
    fn default() -> Self {
        Self {
            default_decision: SecurityDecision::Deny {
                reason: "默认安全策略：无匹配 Guardian，拒绝操作".to_string(),
            },
            user_confirmation_timeout_secs: 30,
            approve_merge_strategy: ApproveMergeStrategy::First,
            guardian_configs: HashMap::new(),
            audit_enabled: true,
        }
    }
}

impl SecurityPolicyConfig {
    /// 校验配置合法性
    ///
    /// 红线：user_confirmation_timeout_secs 不能为 0。
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.user_confirmation_timeout_secs == 0 {
            return Err(PluginError::Config(
                "user_confirmation_timeout_secs 不能为 0".to_string(),
            ));
        }
        Ok(())
    }
}

// ============================================
// ApprovalConfig
// ============================================

/// 审批配置
#[derive(Debug, Clone, Deserialize)]
pub struct ApprovalConfig {
    /// 默认审批超时秒数
    pub default_timeout_secs: u64,
    /// 最大待审批数
    pub max_pending: usize,
    /// GC 间隔秒数
    pub gc_interval_secs: u64,
    /// 最大已完成记录数
    pub completed_max_count: usize,
    /// 已完成记录保留时间秒数
    pub completed_max_age_secs: u64,
}

impl Default for ApprovalConfig {
    fn default() -> Self {
        Self {
            default_timeout_secs: 30,
            max_pending: 200,
            gc_interval_secs: 60,
            completed_max_count: 500,
            completed_max_age_secs: 3600,
        }
    }
}

impl ApprovalConfig {
    /// 校验配置合法性
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.default_timeout_secs == 0 {
            return Err(PluginError::Config(
                "default_timeout_secs 不能为 0".to_string(),
            ));
        }
        if self.max_pending == 0 {
            return Err(PluginError::Config("max_pending 不能为 0".to_string()));
        }
        if self.gc_interval_secs == 0 {
            return Err(PluginError::Config("gc_interval_secs 不能为 0".to_string()));
        }
        Ok(())
    }
}

// ============================================
// GuardianConfig
// ============================================

/// 单个 Guardian 的独立配置
#[derive(Debug, Clone, Deserialize)]
pub struct GuardianConfig {
    /// 是否启用该 Guardian
    pub enabled: bool,
    /// Guardian 优先级（越高越先执行）
    pub priority: i32,
    /// 允许的目录列表（路径穿越/文件权限 Guardian 使用）
    pub allowed_dirs: Vec<String>,
    /// 允许的主机/域名列表（网络访问 Guardian 使用）
    pub allowed_hosts: Vec<String>,
    /// 拒绝的模式列表（命令注入 Guardian 使用）
    pub denied_patterns: Vec<String>,
}

impl Default for GuardianConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 50,
            allowed_dirs: Vec::new(),
            allowed_hosts: Vec::new(),
            denied_patterns: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_security_policy_config_default_validate_ok() {
        let cfg = SecurityPolicyConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[allow(clippy::field_reassign_with_default)]
    #[test]
    fn test_security_policy_config_zero_timeout_invalid() {
        let mut cfg = SecurityPolicyConfig::default();
        cfg.user_confirmation_timeout_secs = 0;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_approval_config_default_validate_ok() {
        let cfg = ApprovalConfig::default();
        assert!(cfg.validate().is_ok());
    }

    #[allow(clippy::field_reassign_with_default)]
    #[test]
    fn test_approval_config_zero_timeout_invalid() {
        let mut cfg = ApprovalConfig::default();
        cfg.default_timeout_secs = 0;
        assert!(cfg.validate().is_err());
    }
}
