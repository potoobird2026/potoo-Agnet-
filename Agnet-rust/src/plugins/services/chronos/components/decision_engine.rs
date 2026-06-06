use super::super::config::DecisionConfig;
use super::super::types::{
    Action, ActionType, Decision, RuleDecision, ScheduledTask, StateSnapshot,
};
use super::DecisionEngineService;
use crate::core::component::{
    Component, ComponentError, ComponentInitContext, InternalAccessPoint, Processing,
};
use async_trait::async_trait;
use std::time::Duration;

const NAME: &str = "decision_engine";

pub struct DecisionEngineComponent {
    config: DecisionConfig,
    init_done: bool,
}

impl DecisionEngineComponent {
    pub fn new(config: DecisionConfig) -> Self {
        Self {
            config,
            init_done: false,
        }
    }
}

#[async_trait]
impl DecisionEngineService for DecisionEngineComponent {
    async fn decide(
        &self,
        _snapshot: &StateSnapshot,
        due_tasks: &[ScheduledTask],
        rule_decision: RuleDecision,
    ) -> Result<Decision, String> {
        // 规则决策优先
        match rule_decision {
            RuleDecision::Execute => {
                let actions = due_tasks
                    .iter()
                    .map(|t| Action {
                        action_type: ActionType::Notify,
                        payload: t.payload.clone(),
                        priority: 100,
                    })
                    .collect();
                return Ok(Decision::Execute { actions });
            }
            RuleDecision::Skip => {
                return Ok(Decision::Skip {
                    reason: "规则引擎判定跳过".to_string(),
                });
            }
            RuleDecision::Escalate => {
                return Ok(Decision::Escalate {
                    reason: "规则引擎判定需升级".to_string(),
                    timeout: Duration::from_secs(self.config.escalation.timeout_secs),
                });
            }
            RuleDecision::None => {}
        }

        // 无规则匹配 → 简单启发式决策（避免 LLM 依赖）
        if due_tasks.is_empty() {
            return Ok(Decision::Skip {
                reason: "无到期任务".to_string(),
            });
        }

        let actions: Vec<Action> = due_tasks
            .iter()
            .take(5)
            .map(|t| Action {
                action_type: ActionType::Notify,
                payload: t.payload.clone(),
                priority: 50,
            })
            .collect();

        Ok(Decision::Execute { actions })
    }
}

#[async_trait]
impl Component for DecisionEngineComponent {
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
