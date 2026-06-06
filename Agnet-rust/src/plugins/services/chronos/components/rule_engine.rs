use super::super::types::StateSnapshot;
use super::{RuleDecision, RuleEngineService};
use crate::core::component::{
    Component, ComponentError, ComponentInitContext, InternalAccessPoint, Processing,
};
use async_trait::async_trait;

const NAME: &str = "rule_engine";

pub struct RuleEngineComponent {
    init_done: bool,
}

impl Default for RuleEngineComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl RuleEngineComponent {
    pub fn new() -> Self {
        Self { init_done: false }
    }
}

#[async_trait]
impl RuleEngineService for RuleEngineComponent {
    fn decide(&self, snapshot: &StateSnapshot) -> RuleDecision {
        use super::super::types::IdleLevel;
        // 空闲/休眠状态 → 跳过（不打扰用户）
        if matches!(snapshot.idle_level, IdleLevel::Idle | IdleLevel::Dormant) {
            return RuleDecision::Skip;
        }
        // 深夜 → 跳过
        if matches!(
            snapshot.time_category,
            super::super::types::TimeCategory::Night
        ) {
            return RuleDecision::Skip;
        }
        // 有紧急任务 → 必须执行
        if snapshot.urgent_count > 0 {
            return RuleDecision::Execute;
        }
        // 无待处理任务 → 跳过
        if snapshot.pending_task_count == 0 {
            return RuleDecision::Skip;
        }
        // 其他情况 → 交给 LLM 决策
        RuleDecision::None
    }
}

#[async_trait]
impl Component for RuleEngineComponent {
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
