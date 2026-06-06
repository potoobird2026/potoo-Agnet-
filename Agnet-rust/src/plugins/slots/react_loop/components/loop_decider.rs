use std::any::Any;

use async_trait::async_trait;

use crate::plugins::slots::component::{
    AccessPoint, Component, ComponentError, ComponentHandle, ComponentMeta, InitContext, Processing,
};
use crate::plugins::slots::react_loop::components::turn_limiter::{
    TurnLimitComponent, TurnLimitService,
};
use crate::plugins::slots::react_loop::types::{LoopAction, LOG_PREFIX};
use crate::shared_types::Thought;

/// 设计文档 §3.2——循环决策服务
pub trait LoopDecisionService: Send + Sync {
    /// 通过 AccessPoint 读取共享数据和调用兄弟组件
    fn decide(&self, ap: &mut dyn AccessPoint) -> Result<LoopAction, ComponentError>;
}

/// 设计文档 §3.2——无状态组件
pub struct LoopDecisionComponent;

impl LoopDecisionComponent {
    pub fn new() -> Self {
        Self
    }
}

impl LoopDecisionService for LoopDecisionComponent {
    fn decide(&self, ap: &mut dyn AccessPoint) -> Result<LoopAction, ComponentError> {
        // 遵循 C-R01：call() 后必须 downcast
        // 遵循 C-R02：requires 必须在代码中实际调用
        let handle = ap
            .call("turn_limiter")
            .map_err(|_| ComponentError::NotFound("turn_limiter".into()))?;
        let turn_limit = handle
            .as_any()
            .downcast_ref::<TurnLimitComponent>()
            .ok_or_else(|| ComponentError::Internal("turn_limiter: type mismatch".into()))?;

        let iteration = ap
            .read_any("current_iteration")
            .and_then(|v| v.downcast_ref::<usize>())
            .copied()
            .unwrap_or(0);
        let thought: Option<&Thought> = ap
            .read_any("thought")
            .and_then(|v| v.downcast_ref::<Thought>());

        let exceeded = turn_limit.is_exceeded(iteration);

        match (exceeded, thought) {
            (true, _) => {
                tracing::warn!(
                    "{} max_turns reached: iteration={}, max_turns={}",
                    LOG_PREFIX,
                    iteration,
                    turn_limit.max_turns(),
                );
                Ok(LoopAction::ForceBreak)
            }
            (false, Some(Thought::Action { .. })) => {
                tracing::debug!("{} Action detected, jumping back to THINK", LOG_PREFIX);
                Ok(LoopAction::JumpToThink)
            }
            (false, Some(Thought::Final { .. })) | (false, None) => {
                tracing::trace!("{} Final/None, continuing", LOG_PREFIX);
                Ok(LoopAction::Continue)
            }
        }
    }
}

#[async_trait]
impl Component for LoopDecisionComponent {
    fn meta(&self) -> &ComponentMeta {
        static META: ComponentMeta = ComponentMeta {
            name: "loop_decider",
            version: "0.1.0",
            priority: 20,
            provides: &["loop_decision"],
            requires: &["turn_check"],
            config_key: None,
        };
        &META
    }

    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        Box::new(Self)
    }

    // 设计文档 §3.2——无状态组件，无需初始化
    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
        Ok(())
    }

    // 设计文档 §3.2 process(): no-op
    async fn process(&mut self, _ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
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

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::collections::HashMap;

    use crate::core::types::Timestamp;
    use crate::shared_types::Action;

    use super::*;
    use crate::plugins::slots::react_loop::component::{MetricsHandle, ModuleConfig};

    // ============================================
    // MockAccessPoint
    // ============================================

    struct MockAccessPoint {
        data: HashMap<String, Box<dyn Any + Send + Sync>>,
        components: HashMap<String, Box<dyn ComponentHandle>>,
    }

    impl MockAccessPoint {
        fn new() -> Self {
            Self {
                data: HashMap::new(),
                components: HashMap::new(),
            }
        }

        fn register_mock_component(&mut self, name: &str, component: Box<dyn ComponentHandle>) {
            self.components.insert(name.to_string(), component);
        }
    }

    impl AccessPoint for MockAccessPoint {
        fn read_any(&self, key: &str) -> Option<&dyn Any> {
            self.data.get(key).map(|b| b.as_ref() as &dyn Any)
        }

        fn write_any(
            &mut self,
            key: &str,
            val: Box<dyn Any + Send + Sync>,
        ) -> Result<(), ComponentError> {
            self.data.insert(key.to_string(), val);
            Ok(())
        }

        fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError> {
            self.components
                .get(name)
                .map(|c| c.clone_box())
                .ok_or_else(|| ComponentError::NotFound(name.to_string()))
        }

        fn config(&self) -> &ModuleConfig {
            panic!("MockAccessPoint::config() called — not available in test context")
        }

        fn metrics(&self) -> &MetricsHandle {
            panic!("MockAccessPoint::metrics() called — not available in test context")
        }
    }

    // ============================================
    // LoopDecisionService 测试
    // ============================================

    #[test]
    fn test_decide_action_within_limit() {
        let mut ap = MockAccessPoint::new();
        ap.write_any("current_iteration", Box::new(3usize)).unwrap();
        ap.write_any(
            "thought",
            Box::new(Thought::Action {
                action: Action::new("tool", serde_json::json!({})),
                reasoning: "test".into(),
                generated_at: Timestamp::from_millis(0),
            }),
        )
        .unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(5)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::JumpToThink);
    }

    #[test]
    fn test_decide_action_exceeded() {
        let mut ap = MockAccessPoint::new();
        ap.write_any("current_iteration", Box::new(5usize)).unwrap();
        ap.write_any(
            "thought",
            Box::new(Thought::Action {
                action: Action::new("tool", serde_json::json!({})),
                reasoning: "test".into(),
                generated_at: Timestamp::from_millis(0),
            }),
        )
        .unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(5)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::ForceBreak);
    }

    #[test]
    fn test_decide_final_within_limit() {
        let mut ap = MockAccessPoint::new();
        ap.write_any("current_iteration", Box::new(2usize)).unwrap();
        ap.write_any(
            "thought",
            Box::new(Thought::Final {
                answer: "answer".into(),
                reasoning: "test".into(),
                generated_at: Timestamp::from_millis(0),
            }),
        )
        .unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(5)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::Continue);
    }

    #[test]
    fn test_decide_no_thought() {
        let mut ap = MockAccessPoint::new();
        ap.write_any("current_iteration", Box::new(2usize)).unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(5)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::Continue);
    }

    // ============================================
    // 边界情况测试
    // ============================================

    #[test]
    fn test_decide_boundary_not_exceeded() {
        // iteration=9, max_turns=10 → 9 < 10, Action → JumpToThink
        let mut ap = MockAccessPoint::new();
        ap.write_any("current_iteration", Box::new(9usize)).unwrap();
        ap.write_any(
            "thought",
            Box::new(Thought::Action {
                action: Action::new("tool", serde_json::json!({})),
                reasoning: "test".into(),
                generated_at: Timestamp::from_millis(0),
            }),
        )
        .unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(10)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::JumpToThink);
    }

    #[test]
    fn test_decide_boundary_exceeded() {
        // iteration=10, max_turns=10 → 10 >= 10, Action → ForceBreak
        let mut ap = MockAccessPoint::new();
        ap.write_any("current_iteration", Box::new(10usize))
            .unwrap();
        ap.write_any(
            "thought",
            Box::new(Thought::Action {
                action: Action::new("tool", serde_json::json!({})),
                reasoning: "test".into(),
                generated_at: Timestamp::from_millis(0),
            }),
        )
        .unwrap();
        ap.register_mock_component("turn_limiter", Box::new(TurnLimitComponent::new(10)));

        let decider = LoopDecisionComponent::new();
        let action = decider.decide(&mut ap).unwrap();

        assert_eq!(action, LoopAction::ForceBreak);
    }
}
