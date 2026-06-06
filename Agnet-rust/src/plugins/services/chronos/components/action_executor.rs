use super::super::config::ActionsConfig;
use super::super::types::Decision;
use super::ActionExecutorService;
use crate::core::component::{
    Component, ComponentError, ComponentInitContext, InternalAccessPoint, Processing,
};
use crate::shared_types::{DynProvider, ToolProvider, PROVIDER_TOOL};
use async_trait::async_trait;
use std::time::Duration;

const NAME: &str = "action_executor";

pub struct ActionExecutorComponent {
    config: ActionsConfig,
    init_done: bool,
}

impl ActionExecutorComponent {
    pub fn new(config: ActionsConfig) -> Self {
        Self {
            config,
            init_done: false,
        }
    }
}

#[async_trait]
impl ActionExecutorService for ActionExecutorComponent {
    async fn execute(
        &self,
        decision: &Decision,
        _task_queue: &dyn super::TaskQueueService,
        ap: &crate::core::access::ServiceAccessPoint,
    ) -> Result<usize, String> {
        match decision {
            Decision::Execute { actions } => {
                let max = self.config.max_concurrent_actions;
                let mut executed: usize = 0;

                for action in actions.iter().take(max) {
                    match action.action_type {
                        super::super::types::ActionType::Notify => {
                            tracing::info!(
                                "Chronos ActionExecutor: 通知 [{}] — {}",
                                action.priority,
                                action.payload,
                            );
                            executed += 1;
                        }
                        super::super::types::ActionType::ExecuteTool => {
                            let tool_name = action
                                .payload
                                .get("tool")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            let tool_args = action
                                .payload
                                .get("args")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null);

                            // 动态查询 ToolProvider
                            match ap.provider_raw(PROVIDER_TOOL) {
                                Some(tool_raw) => {
                                    match tool_raw.downcast::<DynProvider<dyn ToolProvider>>() {
                                        Ok(wrapper) => {
                                            match wrapper
                                                .0
                                                .execute(tool_name, tool_args, Duration::from_secs(30))
                                                .await
                                            {
                                                Ok(_) => {
                                                    tracing::info!(
                                                        "Chronos: 工具 '{}' 执行成功",
                                                        tool_name
                                                    );
                                                    executed += 1;
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        "Chronos: 工具 '{}' 执行失败: {}",
                                                        tool_name,
                                                        e
                                                    );
                                                }
                                            }
                                        }
                                        Err(_) => {
                                            tracing::warn!(
                                                "Chronos: ToolProvider 类型不匹配，无法执行工具"
                                            );
                                        }
                                    }
                                }
                                None => {
                                    tracing::warn!(
                                        "Chronos: ToolProvider 未注册，无法执行工具 '{}'",
                                        tool_name
                                    );
                                }
                            }
                        }
                        super::super::types::ActionType::ProactiveMessage => {
                            let msg = action
                                .payload
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("(无内容)");
                            tracing::info!("Chronos ActionExecutor: 主动消息 — {}", msg,);
                            executed += 1;
                        }
                    }
                }

                Ok(executed)
            }
            Decision::Skip { reason } => {
                tracing::debug!("Chronos ActionExecutor: 跳过决策 — {}", reason);
                Ok(0)
            }
            Decision::Escalate { reason, timeout } => {
                tracing::warn!(
                    "Chronos ActionExecutor: 升级决策 — {} (超时={:?})",
                    reason,
                    timeout,
                );
                Ok(0)
            }
        }
    }
}

#[async_trait]
impl Component for ActionExecutorComponent {
    fn name(&self) -> &str {
        NAME
    }
    async fn init(&mut self, _ctx: &ComponentInitContext) -> Result<(), ComponentError> {
        self.init_done = true;
        Ok(())
    }
    async fn process(
        &mut self,
        _ap: &mut dyn InternalAccessPoint,
    ) -> Result<Processing, ComponentError> {
        Ok(Processing::Continue)
    }
    async fn shutdown(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
}
