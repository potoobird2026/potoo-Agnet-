use std::time::Duration;

use async_trait::async_trait;

use super::component::{AccessPoint, ModuleConfig};
use super::components::circuit_breaker::CircuitBreakerComponent;
use super::components::security_policy::SecurityPolicyComponent;
use super::components::user_confirmation::UserConfirmationComponent;
use super::config::ToolExecutorConfig;
use super::orchestrator::ToolExecutorOrchestrator;
use super::types::CircuitBreakerState;
use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::context::{
    CONTEXT_CIRCUIT_BREAKER, CONTEXT_FINAL_ANSWER, CONTEXT_OBSERVATION, CONTEXT_THOUGHT,
};
use crate::shared_types::{
    DynProvider, Observation, Thought, ToolError, ToolProvider, PROVIDER_TOOL,
};

pub struct ToolExecutorSlot {
    orch: Option<ToolExecutorOrchestrator>,
    config: ToolExecutorConfig,
}

impl ToolExecutorSlot {
    pub fn new() -> Self {
        Self {
            orch: None,
            config: ToolExecutorConfig::default(),
        }
    }
}

impl Default for ToolExecutorSlot {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SlotPlugin for ToolExecutorSlot {
    fn name(&self) -> &str {
        "tool_executor"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let config: ToolExecutorConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("tool_executor 配置解析: {}", e)))?;

        let module_config = ModuleConfig::new(serde_json::json!({
            "circuit_breaker_threshold": config.circuit_breaker_threshold,
            "circuit_breaker_reset_secs": config.circuit_breaker_reset_secs,
            "confirmation_timeout_secs": config.confirmation_timeout_secs,
        }));

        let mut orch = ToolExecutorOrchestrator::new(module_config);

        orch.register(Box::new(CircuitBreakerComponent::new(
            config.circuit_breaker_threshold,
        )))
        .await;

        if config.enable_security_policy {
            orch.register(Box::new(SecurityPolicyComponent::new(true)))
                .await;
        }

        if config.require_confirmation {
            orch.register(Box::new(UserConfirmationComponent::new(true)))
                .await;
        }

        orch.init_all()
            .await
            .map_err(|e| PluginError::Internal(e.to_string()))?;

        self.orch = Some(orch);
        self.config = config;
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        let orch = self
            .orch
            .as_mut()
            .ok_or_else(|| PluginError::Runtime("tool_executor: 未初始化".into()))?;

        let thought = ap
            .read_context_raw(CONTEXT_THOUGHT)
            .and_then(|any| any.downcast_ref::<Thought>().cloned());

        let thought = match thought {
            Some(t) => t,
            None => return Ok(SlotDirective::Continue),
        };

        let action = match thought {
            Thought::Action { action, .. } => action,
            Thought::Final { answer, .. } => {
                ap.write_context_raw(CONTEXT_FINAL_ANSWER, Box::new(answer))?;
                return Ok(SlotDirective::Continue);
            }
        };

        let mut state: CircuitBreakerState = ap
            .read_context_raw(CONTEXT_CIRCUIT_BREAKER)
            .and_then(|any| any.downcast_ref::<CircuitBreakerState>().cloned())
            .unwrap_or_default();

        {
            let ap_arc = orch.access_point();
            let mut ap_int = ap_arc.write().await;
            ap_int.write_any("current_action", Box::new(action.clone()))?;
            ap_int.write_any("circuit_breaker", Box::new(state.clone()))?;
        }

        orch.process_all()
            .await
            .map_err(|e| PluginError::Internal(e.to_string()))?;

        {
            let ap_arc = orch.access_point();
            let ap_int = ap_arc.read().await;
            if let Some(obs) = ap_int
                .read_any("observation")
                .and_then(|v| v.downcast_ref::<Observation>().cloned())
            {
                ap.write_context_raw(CONTEXT_OBSERVATION, Box::new(vec![obs]))?;
                ap.write_context_raw(CONTEXT_CIRCUIT_BREAKER, Box::new(state))?;
                return Ok(SlotDirective::Continue);
            }
        }

        let timeout = Duration::from_secs(self.config.timeout_secs);
        let result = match ap.provider_raw(PROVIDER_TOOL) {
            Some(raw) => match raw.downcast::<DynProvider<dyn ToolProvider>>() {
                Ok(wrapper) => {
                    match wrapper
                        .0
                        .execute(&action.tool_name, action.arguments.clone(), timeout)
                        .await
                    {
                        Ok(output) => {
                            state.record_success(&action.tool_name);
                            Observation::success(action, output)
                        }
                        Err(ToolError::Timeout(msg)) => {
                            state.record_failure(&action.tool_name);
                            Observation::retryable_error(action, format!("超时: {}", msg))
                        }
                        Err(ToolError::ExecutionFailed(msg)) => {
                            state.record_failure(&action.tool_name);
                            Observation::fatal_error(action, msg)
                        }
                        Err(ToolError::NotFound(msg)) => {
                            state.record_failure(&action.tool_name);
                            Observation::fatal_error(action, format!("工具未找到: {}", msg))
                        }
                    }
                }
                Err(_) => Observation::fatal_error(action, "工具 Provider 类型不匹配".to_string()),
            },
            None => Observation::fatal_error(action, "工具 Provider 未注册".to_string()),
        };

        ap.write_context_raw(CONTEXT_CIRCUIT_BREAKER, Box::new(state))?;
        ap.write_context_raw(CONTEXT_OBSERVATION, Box::new(vec![result]))?;

        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        if let Some(orch) = self.orch.as_mut() {
            orch.shutdown_all().await;
        }
        Ok(())
    }
}
