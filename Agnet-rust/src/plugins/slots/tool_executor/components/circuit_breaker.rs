use std::any::Any;

use async_trait::async_trait;

use super::super::types::CircuitBreakerState;
use crate::plugins::slots::component::{
    AccessPoint, Component, ComponentError, ComponentHandle, ComponentMeta, InitContext, Processing,
};
use crate::shared_types::thought::{Action, Observation};

/// 熔断器组件
///
/// 职责：检查工具是否处于熔断状态，若熔断则写入 FatalError Observation 并返回 BreakChain
///
/// 重要设计：
/// - process() 只负责"检查熔断状态"，不负责 record_success/record_failure
/// - record_success/record_failure 在 Slot::run() 中调用
pub struct CircuitBreakerComponent {
    threshold: u32,
}

const CIRCUIT_BREAKER_META: ComponentMeta = ComponentMeta {
    name: "circuit_breaker",
    version: "0.1.0",
    priority: 10,
    provides: &["circuit_breaker_check"],
    requires: &[],
    config_key: Some("circuit_breaker"),
};

impl CircuitBreakerComponent {
    pub fn new(threshold: u32) -> Self {
        Self { threshold }
    }

    pub fn meta() -> &'static ComponentMeta {
        &CIRCUIT_BREAKER_META
    }
}

#[async_trait]
impl Component for CircuitBreakerComponent {
    fn meta(&self) -> &ComponentMeta {
        Self::meta()
    }

    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        Box::new(Self {
            threshold: self.threshold,
        })
    }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        self.threshold = ctx.config.circuit_breaker_threshold();
        Ok(())
    }

    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        let state: CircuitBreakerState = ap
            .read_any("circuit_breaker")
            .and_then(|v| v.downcast_ref::<CircuitBreakerState>().cloned())
            .unwrap_or_default();

        let action = ap
            .read_any("current_action")
            .and_then(|v| v.downcast_ref::<Action>().cloned())
            .ok_or_else(|| ComponentError::NotFound("current_action".into()))?;

        if state.failure_count(&action.tool_name) >= self.threshold {
            let obs = Observation::fatal_error(action, "熔断器打开，工具暂时不可用".to_string());
            ap.write_any("observation", Box::new(obs))?;
            return Ok(Processing::BreakChain);
        }

        ap.write_any("circuit_breaker", Box::new(state))?;
        Ok(Processing::Continue)
    }

    fn name(&self) -> &str {
        self.meta().name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn clonable(&self) -> bool {
        true
    }
    fn ready(&self) -> bool {
        true
    }

    async fn shutdown(&mut self) -> Result<(), ComponentError> {
        Ok(())
    }
}
