/*!
 * SecurityPolicyEngine —— 安全策略引擎
 *
 * Guardian 链式评估，四级决策模型（Deny → Allow → Guard → Approve）。
 * 按 priority 降序遍历 Guardian，支持审计日志。
 */

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use super::config::SecurityPolicyConfig;
use super::guardians::Guardian;
use super::models::{
    Action, ApproveMergeStrategy, GuardFinding, GuardResult, Resource, SecurityDecision,
    SecurityError, Subject,
};

/// 安全策略引擎 trait
#[async_trait]
pub trait SecurityPolicyEngine: Send + Sync {
    /// 评估操作安全性
    async fn evaluate(
        &self,
        subject: &Subject,
        action: &Action,
        resource: &Resource,
    ) -> Result<SecurityDecision, SecurityError>;

    /// 注册 Guardian
    async fn register_guardian(&self, guardian: Box<dyn Guardian>) -> Result<(), SecurityError>;

    /// 列出所有已注册 Guardian 的名称
    async fn list_guardians(&self) -> Vec<String>;
}

/// 默认安全策略引擎实现
pub struct DefaultSecurityPolicyEngine {
    guardians: RwLock<Vec<Box<dyn Guardian>>>,
    default_decision: SecurityDecision,
    config: SecurityPolicyConfig,
}

impl DefaultSecurityPolicyEngine {
    pub fn new(config: SecurityPolicyConfig) -> Self {
        Self {
            guardians: RwLock::new(Vec::new()),
            default_decision: config.default_decision.clone(),
            config,
        }
    }

    /// 审计日志记录
    fn audit(
        &self,
        subject: &Subject,
        action: &Action,
        resource: &Resource,
        decision: &SecurityDecision,
    ) {
        if !self.config.audit_enabled {
            return;
        }

        let decision_str = match decision {
            SecurityDecision::Allow => "Allow".to_string(),
            SecurityDecision::Deny { reason } => format!("Deny: {}", reason),
            SecurityDecision::Guard { findings } => {
                format!("Guard: {} findings", findings.len())
            }
            SecurityDecision::Approve { prompt, .. } => {
                format!("Approve: {}", prompt)
            }
        };

        tracing::info!(
            session_id = %subject.session_id,
            tool_name = %action.tool_name,
            resource = %resource.identifier,
            decision = %decision_str,
            "SecurityPolicyEngine 审计"
        );
    }

    /// 合并多个审批决策
    fn merge_approve_decisions(
        &self,
        approve_decisions: Vec<(std::time::Duration, String, Vec<GuardFinding>)>,
    ) -> SecurityDecision {
        if approve_decisions.is_empty() {
            return self.default_decision.clone();
        }

        match self.config.approve_merge_strategy {
            ApproveMergeStrategy::First => {
                let Some((timeout, prompt, findings)) = approve_decisions.into_iter().next() else {
                    return self.default_decision.clone();
                };
                SecurityDecision::Approve {
                    timeout,
                    prompt,
                    findings,
                }
            }
            ApproveMergeStrategy::Strictest => {
                let mut strictest: Option<(std::time::Duration, String, Vec<GuardFinding>)> = None;
                for (timeout, prompt, findings) in approve_decisions {
                    match &strictest {
                        None => {
                            strictest = Some((timeout, prompt, findings));
                        }
                        Some((existing_timeout, _, _)) => {
                            if timeout < *existing_timeout {
                                strictest = Some((timeout, prompt, findings));
                            }
                        }
                    }
                }
                let Some((timeout, prompt, findings)) = strictest else {
                    return self.default_decision.clone();
                };
                SecurityDecision::Approve {
                    timeout,
                    prompt,
                    findings,
                }
            }
        }
    }
}

#[async_trait]
impl SecurityPolicyEngine for DefaultSecurityPolicyEngine {
    async fn evaluate(
        &self,
        subject: &Subject,
        action: &Action,
        resource: &Resource,
    ) -> Result<SecurityDecision, SecurityError> {
        let guardians = self.guardians.read().await;

        // 筛选启用的 Guardian 并按 priority 降序排列
        let mut active: Vec<&Box<dyn Guardian>> =
            guardians.iter().filter(|g| g.enabled()).collect();
        active.sort_by_key(|g| -g.priority());

        let mut guard_findings: Vec<GuardFinding> = Vec::new();
        let mut approve_decisions: Vec<(std::time::Duration, String, Vec<GuardFinding>)> =
            Vec::new();

        for guardian in active {
            let result = guardian.evaluate(subject, action, resource).await;

            match result {
                // Guardian 不适用，跳过
                None => continue,

                // Deny → 立即拒绝
                Some(GuardResult::Deny(reason)) => {
                    let decision = SecurityDecision::Deny {
                        reason: reason.clone(),
                    };
                    self.audit(subject, action, resource, &decision);
                    return Ok(decision);
                }

                // Allow → 立即放行（短路）
                Some(GuardResult::Allow) => {
                    let decision = SecurityDecision::Allow;
                    self.audit(subject, action, resource, &decision);
                    return Ok(decision);
                }

                // Guard → 累积 findings
                Some(GuardResult::Guard(finding)) => {
                    guard_findings.push(finding);
                }

                // Approve → 累积审批决策
                Some(GuardResult::Approve(timeout, prompt)) => {
                    approve_decisions.push((timeout, prompt, Vec::new()));
                }
            }
        }

        // 后处理：全部 Guardian 遍历完毕
        let decision = if !approve_decisions.is_empty() {
            self.merge_approve_decisions(approve_decisions)
        } else if !guard_findings.is_empty() {
            SecurityDecision::Guard {
                findings: guard_findings,
            }
        } else {
            self.default_decision.clone()
        };

        self.audit(subject, action, resource, &decision);

        Ok(decision)
    }

    async fn register_guardian(&self, guardian: Box<dyn Guardian>) -> Result<(), SecurityError> {
        let mut guardians = self.guardians.write().await;
        guardians.push(guardian);
        // 注册后重新排序
        guardians.sort_by_key(|g| -g.priority());
        Ok(())
    }

    async fn list_guardians(&self) -> Vec<String> {
        self.guardians
            .read()
            .await
            .iter()
            .map(|g| g.name().to_string())
            .collect()
    }
}

// ============================================
// 为 Arc<DefaultSecurityPolicyEngine> 实现 Engine trait
// ============================================

#[async_trait]
impl SecurityPolicyEngine for Arc<DefaultSecurityPolicyEngine> {
    async fn evaluate(
        &self,
        subject: &Subject,
        action: &Action,
        resource: &Resource,
    ) -> Result<SecurityDecision, SecurityError> {
        self.as_ref().evaluate(subject, action, resource).await
    }

    async fn register_guardian(&self, guardian: Box<dyn Guardian>) -> Result<(), SecurityError> {
        self.as_ref().register_guardian(guardian).await
    }

    async fn list_guardians(&self) -> Vec<String> {
        self.as_ref().list_guardians().await
    }
}

#[cfg(test)]
mod tests {
    use super::super::config::GuardianConfig;
    use super::super::guardians::path_traversal::PathTraversalGuardian;
    use super::*;
    use std::collections::HashMap;

    fn make_subject() -> Subject {
        Subject {
            session_id: "test".to_string(),
            session_type: super::super::models::SessionType::Interactive,
            metadata: HashMap::new(),
        }
    }

    fn make_action() -> Action {
        Action {
            tool_name: "test_tool".to_string(),
            operation: super::super::models::Operation::Read,
            arguments: serde_json::Value::Null,
        }
    }

    fn make_resource(path: &str) -> Resource {
        Resource {
            resource_type: super::super::models::ResourceType::File,
            identifier: path.to_string(),
            metadata: HashMap::new(),
        }
    }

    #[allow(clippy::field_reassign_with_default)]
    fn make_engine() -> DefaultSecurityPolicyEngine {
        let mut config = SecurityPolicyConfig::default();
        config.default_decision = SecurityDecision::Allow;
        config.audit_enabled = true;
        DefaultSecurityPolicyEngine::new(config)
    }

    #[tokio::test]
    async fn test_empty_guardians_returns_default_decision() {
        let engine = make_engine();
        let result = engine
            .evaluate(&make_subject(), &make_action(), &make_resource("test.txt"))
            .await;
        assert!(result.is_ok());
        assert!(matches!(result.unwrap(), SecurityDecision::Allow));
    }

    #[tokio::test]
    async fn test_guardian_deny_stops_chain() {
        let engine = make_engine();
        let guardian = PathTraversalGuardian::new(GuardianConfig {
            enabled: true,
            priority: 100,
            allowed_dirs: vec!["/tmp".to_string()],
            ..Default::default()
        });
        engine.register_guardian(Box::new(guardian)).await.ok();

        let result = engine
            .evaluate(
                &make_subject(),
                &make_action(),
                &make_resource("../../etc/passwd"),
            )
            .await;
        assert!(matches!(result.unwrap(), SecurityDecision::Deny { .. }));
    }

    #[tokio::test]
    async fn test_disabled_guardian_skipped() {
        let engine = make_engine();
        let guardian = PathTraversalGuardian::new(GuardianConfig {
            enabled: false, // 禁用
            priority: 100,
            allowed_dirs: vec!["/tmp".to_string()],
            ..Default::default()
        });
        engine.register_guardian(Box::new(guardian)).await.ok();

        let result = engine
            .evaluate(
                &make_subject(),
                &make_action(),
                &make_resource("../../etc/passwd"),
            )
            .await;
        // 禁用的 Guardian 被跳过，返回默认决策
        assert!(matches!(result.unwrap(), SecurityDecision::Allow));
    }

    #[tokio::test]
    async fn test_list_guardians() {
        let engine = make_engine();
        let guardian = PathTraversalGuardian::new(GuardianConfig::default());
        engine.register_guardian(Box::new(guardian)).await.ok();

        let names = engine.list_guardians().await;
        assert_eq!(names.len(), 1);
        assert_eq!(names[0], "path_traversal");
    }
}
