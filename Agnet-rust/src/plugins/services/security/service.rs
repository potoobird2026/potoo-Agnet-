/*!
 * SecurityService —— 安全服务（ServicePlugin 实现）
 *
 * 实现 ServicePlugin trait，通过 ServiceAccessPoint 与核心交互。
 * 在 start() 中注册 security Provider，向其他插件提供安全策略引擎能力。
 */

use std::sync::Arc;

use async_trait::async_trait;

use crate::core::access::ServiceAccessPoint;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::{
    AuditContext, AuditWarning, DynProvider, SecurityDecision, SecurityError,
    SecurityPolicyProvider, PROVIDER_SECURITY,
};

use super::approval::ApprovalService;
use super::config::{ApprovalConfig, GuardianConfig, SecurityPolicyConfig};
use super::engine::{DefaultSecurityPolicyEngine, SecurityPolicyEngine};
use super::guardians::command_injection::CommandInjectionGuardian;
use super::guardians::file_permission::FilePermissionGuardian;
use super::guardians::network_access::NetworkAccessGuardian;
use super::guardians::path_traversal::PathTraversalGuardian;
use super::guardians::Guardian;

/// 安全服务——框架级安全策略执行点
pub struct SecurityService {
    engine: Option<Arc<DefaultSecurityPolicyEngine>>,
    approval: Option<Arc<ApprovalService>>,
    config: Option<SecurityPolicyConfig>,
    suspended: bool,
}

impl SecurityService {
    pub fn new() -> Self {
        Self {
            engine: None,
            approval: None,
            config: None,
            suspended: false,
        }
    }

    /// 创建默认 Guardian 列表
    fn create_default_guardians(config: &SecurityPolicyConfig) -> Vec<Box<dyn Guardian>> {
        let mut guardians: Vec<Box<dyn Guardian>> = Vec::new();

        // PathTraversalGuardian
        let pt_config = config
            .guardian_configs
            .get("path_traversal")
            .cloned()
            .unwrap_or_else(|| GuardianConfig {
                enabled: true,
                priority: 100,
                ..Default::default()
            });
        guardians.push(Box::new(PathTraversalGuardian::new(pt_config)));

        // CommandInjectionGuardian
        let ci_config = config
            .guardian_configs
            .get("command_injection")
            .cloned()
            .unwrap_or_else(|| GuardianConfig {
                enabled: true,
                priority: 90,
                ..Default::default()
            });
        guardians.push(Box::new(CommandInjectionGuardian::new(ci_config)));

        // FilePermissionGuardian
        let fp_config = config
            .guardian_configs
            .get("file_permission")
            .cloned()
            .unwrap_or_else(|| GuardianConfig {
                enabled: true,
                priority: 80,
                ..Default::default()
            });
        guardians.push(Box::new(FilePermissionGuardian::new(fp_config)));

        // NetworkAccessGuardian
        let na_config = config
            .guardian_configs
            .get("network_access")
            .cloned()
            .unwrap_or_else(|| GuardianConfig {
                enabled: true,
                priority: 70,
                ..Default::default()
            });
        guardians.push(Box::new(NetworkAccessGuardian::new(na_config)));

        guardians
    }
}

#[async_trait]
impl ServicePlugin for SecurityService {
    fn name(&self) -> &str {
        "security"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let config: SecurityPolicyConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("security: 配置解析失败: {}", e)))?;

        // 2. 校验配置
        config.validate()?;

        // 3. 创建策略引擎
        let engine = Arc::new(DefaultSecurityPolicyEngine::new(config.clone()));

        // 4. 注册默认 Guardian
        let guardians = Self::create_default_guardians(&config);
        for guardian in guardians {
            engine.register_guardian(guardian).await.map_err(|e| {
                PluginError::InitFailed(format!(
                    "注册 Guardian '{}' 失败: {}",
                    e.description, e.description
                ))
            })?;
        }

        // 5. 初始化审批服务
        let approval_config = ApprovalConfig::default();
        let approval = Arc::new(ApprovalService::new(approval_config));

        self.engine = Some(engine);
        self.approval = Some(approval);
        self.config = Some(config);

        tracing::info!("SecurityService: 初始化完成，已注册 {} 个 Guardian", 4);
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        let engine = self.engine.as_ref().ok_or_else(|| {
            PluginError::InitFailed("SecurityService: engine 未初始化".to_string())
        })?;

        let approval = self.approval.as_ref().ok_or_else(|| {
            PluginError::InitFailed("SecurityService: approval 未初始化".to_string())
        })?;

        // 注册 Provider——通过 DynProvider<dyn SecurityPolicyProvider> 包装
        let security_provider = SecurityProviderImpl {
            engine: engine.clone(),
        };
        ap.register_provider(
            PROVIDER_SECURITY,
            Arc::new(DynProvider(
                Arc::new(security_provider) as Arc<dyn SecurityPolicyProvider>
            )),
        );

        // 启动审批 GC 后台任务
        approval.start_gc().await;

        tracing::info!("SecurityService: 已启动，Provider 'security' 已注册");
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::GracefulShutdown => {
                tracing::info!("SecurityService: 收到 GracefulShutdown，拒绝新审批请求");
                if let Some(approval) = &self.approval {
                    approval.set_accepting_new(false).await;
                }
                Ok(())
            }
            ServiceSignal::ImmediateShutdown => {
                tracing::info!("SecurityService: 收到 ImmediateShutdown，清除所有 pending 审批");
                if let Some(approval) = &self.approval {
                    approval.clear_all_pending().await;
                }
                Ok(())
            }
            ServiceSignal::ConfigReload => {
                tracing::info!("SecurityService: 收到 ConfigReload");
                // 重载配置在 stop/start 周期中完成，这里仅记录
                Ok(())
            }
            ServiceSignal::HealthCheck => {
                // 红线 V-R01: 健康检查需在 5s 内返回
                if self.engine.is_some() && self.approval.is_some() {
                    Ok(())
                } else {
                    Err(PluginError::InitFailed(
                        "SecurityService: 健康检查失败，组件未就绪".to_string(),
                    ))
                }
            }
            ServiceSignal::Suspend => {
                tracing::info!("SecurityService: 收到 Suspend，暂停策略评估");
                self.suspended = true;
                if let Some(approval) = &self.approval {
                    approval.set_accepting_new(false).await;
                }
                Ok(())
            }
            ServiceSignal::Resume => {
                tracing::info!("SecurityService: 收到 Resume，恢复策略评估");
                self.suspended = false;
                if let Some(approval) = &self.approval {
                    approval.set_accepting_new(true).await;
                }
                Ok(())
            }
        }
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        tracing::info!("SecurityService: 停止服务，设置暂停标志");
        self.suspended = true;
        if let Some(approval) = &self.approval {
            approval.set_accepting_new(false).await;
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        tracing::info!("SecurityService: 关闭服务");

        // 1. 停止 GC 任务
        if let Some(approval) = &self.approval {
            approval.stop_gc().await;
        }

        // 2. 清除所有 pending 审批
        if let Some(approval) = &self.approval {
            approval.clear_all_pending().await;
        }

        // 3. 清理内部状态
        self.engine = None;
        self.approval = None;
        self.config = None;

        tracing::info!("SecurityService: 已关闭");
        Ok(())
    }
}

impl Default for SecurityService {
    fn default() -> Self {
        Self::new()
    }
}

/// 适配层：将 audit_phase 的 SecurityPolicyProvider 请求转换为 Security 内部的引擎调用
struct SecurityProviderImpl {
    engine: Arc<DefaultSecurityPolicyEngine>,
}

#[async_trait]
impl SecurityPolicyProvider for SecurityProviderImpl {
    async fn evaluate(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        _context: &AuditContext,
    ) -> Result<SecurityDecision, SecurityError> {
        let subject = super::models::Subject {
            session_id: _context.session_id.clone(),
            session_type: super::models::SessionType::Interactive,
            metadata: std::collections::HashMap::new(),
        };
        let action = super::models::Action {
            tool_name: tool_name.to_string(),
            operation: super::models::Operation::Execute,
            arguments: arguments.clone(),
        };
        let resource = super::models::Resource {
            resource_type: super::models::ResourceType::Tool,
            identifier: tool_name.to_string(),
            metadata: std::collections::HashMap::new(),
        };

        match self.engine.evaluate(&subject, &action, &resource).await {
            Ok(engine_decision) => Ok(match engine_decision {
                super::models::SecurityDecision::Allow => SecurityDecision::Allow,
                super::models::SecurityDecision::Deny { reason } => {
                    SecurityDecision::Deny { reason }
                }
                super::models::SecurityDecision::Guard { findings } => {
                    let warnings: Vec<AuditWarning> = findings
                        .into_iter()
                        .map(|f| AuditWarning {
                            rule_name: f.guardian,
                            severity: super::models::GuardSeverity::into_shared(f.severity),
                            description: f.message,
                            detail: f.recommendation.unwrap_or_default(),
                        })
                        .collect();
                    SecurityDecision::AllowWithWarnings { warnings }
                }
                super::models::SecurityDecision::Approve {
                    timeout: _,
                    prompt,
                    findings,
                } => {
                    let warnings: Vec<AuditWarning> = findings
                        .into_iter()
                        .map(|f| AuditWarning {
                            rule_name: f.guardian,
                            severity: super::models::GuardSeverity::into_shared(f.severity),
                            description: f.message,
                            detail: f.recommendation.unwrap_or_default(),
                        })
                        .collect();
                    if warnings.is_empty() {
                        SecurityDecision::RequireConfirmation { prompt }
                    } else {
                        SecurityDecision::AllowWithWarnings { warnings }
                    }
                }
            }),
            Err(e) => Err(SecurityError::EngineError(e.to_string())),
        }
    }
}
