pub(crate) mod component;
pub(crate) mod components;
pub(crate) mod orchestrator;
pub(crate) mod types;

pub use types::{ReactLoopConfig, ReactLoopError};

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::core::access::SlotAccessPoint;
use crate::core::phase::Phase;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::context::CONTEXT_THOUGHT;
use crate::shared_types::Thought;

use self::component::{AccessPoint, ModuleConfig};
use self::components::loop_decider::{LoopDecisionComponent, LoopDecisionService};
use self::components::turn_limiter::TurnLimitComponent;
use self::orchestrator::Orchestrator;
use self::types::{DEFAULT_MAX_TURNS, LOG_PREFIX};

/// 设计文档 §1.1——ReActLoopSlot 入口
pub struct ReActLoopSlot {
    orchestrator: Arc<RwLock<Orchestrator>>,
}

impl ReActLoopSlot {
    pub fn new() -> Self {
        Self {
            orchestrator: Arc::new(RwLock::new(Orchestrator::new(ModuleConfig::new(
                serde_json::json!({"max_turns": DEFAULT_MAX_TURNS}),
            )))),
        }
    }
}

impl Default for ReActLoopSlot {
    fn default() -> Self {
        Self::new()
    }
}

fn map_init_err(e: impl std::fmt::Display) -> PluginError {
    PluginError::InitFailed(e.to_string())
}

fn map_runtime_err(e: impl std::fmt::Display) -> PluginError {
    PluginError::Internal(e.to_string())
}

#[async_trait]
impl SlotPlugin for ReActLoopSlot {
    fn name(&self) -> &str {
        "react_loop"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let max_turns = serde_json::from_value::<ReactLoopConfig>(ctx.plugin_config.clone())
            .ok()
            .and_then(|c| c.max_turns)
            .unwrap_or(DEFAULT_MAX_TURNS);

        let mut orch = self.orchestrator.write().await;
        orch.set_config(ModuleConfig::new(
            serde_json::json!({"max_turns": max_turns}),
        ));

        orch.register(Box::new(TurnLimitComponent::new(max_turns)))
            .await
            .map_err(map_init_err)?;
        orch.register(Box::new(LoopDecisionComponent::new()))
            .await
            .map_err(map_init_err)?;

        orch.init_all().await.map_err(map_init_err)?;

        tracing::info!("{} initialized, max_turns={}", LOG_PREFIX, max_turns);
        Ok(())
    }

    async fn run(
        &mut self,
        ap_slot: &mut dyn SlotAccessPoint,
    ) -> Result<SlotDirective, PluginError> {
        // [A] 通过 SlotAccessPoint 读取外部数据
        let iteration = ap_slot.current_iteration();
        let thought_raw: Option<Thought> = ap_slot
            .read_context_raw(CONTEXT_THOUGHT)
            .and_then(|any| any.downcast_ref::<Thought>())
            .cloned();

        // [B] 获取 InternalAccessPoint
        let orch = self.orchestrator.read().await;
        let ap_int = orch.access_point();
        let mut ap_guard = ap_int.write().await;

        // [C] 将外部数据注入共享数据区
        ap_guard
            .write_any("current_iteration", Box::new(iteration))
            .map_err(map_runtime_err)?;
        if let Some(ref t) = thought_raw {
            ap_guard
                .write_any("thought", Box::new(t.clone()))
                .map_err(map_runtime_err)?;
        }

        // [D] 获取 LoopDecisionComponent 句柄
        let handle = ap_guard.call("loop_decider").map_err(map_runtime_err)?;
        let decider = handle
            .as_any()
            .downcast_ref::<LoopDecisionComponent>()
            .ok_or_else(|| PluginError::Internal("loop_decider: type mismatch".into()))?;

        // [E] 执行决策
        let action = decider.decide(&mut *ap_guard).map_err(map_runtime_err)?;
        drop(ap_guard);
        drop(orch);

        // [F] 映射为 SlotDirective
        match action {
            self::types::LoopAction::Continue => Ok(SlotDirective::Continue),
            self::types::LoopAction::JumpToThink => Ok(SlotDirective::JumpTo(Phase::think())),
            self::types::LoopAction::ForceBreak => Ok(SlotDirective::BreakStep),
        }
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        let mut orch = self.orchestrator.write().await;
        orch.shutdown_all().await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;
    use std::collections::HashMap;

    use crate::core::types::plugin::AgentConfig;
    use crate::shared_types::Message;

    use crate::core::types::Timestamp;
    use crate::shared_types::Action;

    use super::*;

    struct MockSlotAccessPoint {
        iteration: usize,
        thought: Option<Thought>,
        _data: HashMap<String, Box<dyn Any + Send + Sync>>,
    }

    impl MockSlotAccessPoint {
        fn new(iteration: usize, thought: Option<Thought>) -> Self {
            Self {
                iteration,
                thought,
                _data: HashMap::new(),
            }
        }
    }

    impl SlotAccessPoint for MockSlotAccessPoint {
        fn messages(&self) -> &[crate::shared_types::Message] {
            &[]
        }
        fn session_id(&self) -> &str {
            "mock-session"
        }
        fn phase_name(&self) -> &str {
            "mock-phase"
        }
        fn current_iteration(&self) -> usize {
            self.iteration
        }
        fn write_observation(
            &mut self,
            _obs: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            Ok(())
        }
        fn write_context_raw(
            &mut self,
            key: &str,
            val: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            self._data.insert(key.to_string(), val);
            Ok(())
        }
        fn read_context_raw(&self, key: &str) -> Option<&(dyn Any + Send + Sync)> {
            match key {
                "thought" => self.thought.as_ref().map(|t| t as &(dyn Any + Send + Sync)),
                _ => self
                    ._data
                    .get(key)
                    .map(|b| b.as_ref() as &(dyn Any + Send + Sync)),
            }
        }
        fn request_jump(&self, _phase: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }
        fn provider_raw(&self, _name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
            None
        }

        fn append_message(&mut self, _msg: Message) -> Result<(), PluginError> {
            Ok(())
        }
    }

    fn make_ctx(config: serde_json::Value) -> PluginInitContext {
        PluginInitContext::new(
            "react_loop",
            config,
            AgentConfig::default(),
            std::env::temp_dir(),
        )
    }

    fn action_thought(reasoning: &str) -> Thought {
        Thought::Action {
            action: Action::new("test_tool", serde_json::json!({})),
            reasoning: reasoning.to_string(),
            generated_at: Timestamp::now(),
        }
    }

    fn final_thought(answer: &str) -> Thought {
        Thought::Final {
            answer: answer.to_string(),
            reasoning: "done".to_string(),
            generated_at: Timestamp::now(),
        }
    }

    #[tokio::test]
    async fn test_init_ok_with_default_config() {
        let mut slot = ReActLoopSlot::new();
        let ctx = make_ctx(serde_json::json!({}));
        let result = slot.init(&ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_init_ok_with_custom_max_turns() {
        let mut slot = ReActLoopSlot::new();
        let ctx = make_ctx(serde_json::json!({"max_turns": 5}));
        let result = slot.init(&ctx).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_run_action_within_limit() {
        let mut slot = ReActLoopSlot::new();
        let ctx = make_ctx(serde_json::json!({"max_turns": 5}));
        slot.init(&ctx).await.unwrap(); // 测试中安全
        let mut mock = MockSlotAccessPoint::new(2, Some(action_thought("test")));
        let result = slot.run(&mut mock).await;
        assert_eq!(result.unwrap(), SlotDirective::JumpTo(Phase::think())); // 测试中安全
    }

    #[tokio::test]
    async fn test_run_action_exceeded() {
        let mut slot = ReActLoopSlot::new();
        let ctx = make_ctx(serde_json::json!({"max_turns": 5}));
        slot.init(&ctx).await.unwrap(); // 测试中安全
        let mut mock = MockSlotAccessPoint::new(5, Some(action_thought("test")));
        let result = slot.run(&mut mock).await;
        assert_eq!(result.unwrap(), SlotDirective::BreakStep); // 测试中安全
    }

    #[tokio::test]
    async fn test_run_final_within_limit() {
        let mut slot = ReActLoopSlot::new();
        let ctx = make_ctx(serde_json::json!({"max_turns": 5}));
        slot.init(&ctx).await.unwrap(); // 测试中安全
        let mut mock = MockSlotAccessPoint::new(2, Some(final_thought("done")));
        let result = slot.run(&mut mock).await;
        assert_eq!(result.unwrap(), SlotDirective::Continue); // 测试中安全
    }

    #[tokio::test]
    async fn test_run_no_thought() {
        let mut slot = ReActLoopSlot::new();
        let ctx = make_ctx(serde_json::json!({"max_turns": 5}));
        slot.init(&ctx).await.unwrap(); // 测试中安全
        let mut mock = MockSlotAccessPoint::new(2, None);
        let result = slot.run(&mut mock).await;
        assert_eq!(result.unwrap(), SlotDirective::Continue); // 测试中安全
    }

    #[tokio::test]
    async fn test_shutdown_ok() {
        let mut slot = ReActLoopSlot::new();
        let ctx = make_ctx(serde_json::json!({}));
        slot.init(&ctx).await.unwrap(); // 测试中安全
        let result = slot.shutdown().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_max_turns_is_clamped() {
        let mut slot = ReActLoopSlot::new();
        let ctx = make_ctx(serde_json::json!({"max_turns": 0}));
        slot.init(&ctx).await.unwrap(); // 测试中安全
                                        // max_turns=0 → clamped to 1 → iteration=0 not exceeded → Action → JumpToThink
        let mut mock = MockSlotAccessPoint::new(0, Some(action_thought("test")));
        let result = slot.run(&mut mock).await;
        assert_eq!(result.unwrap(), SlotDirective::JumpTo(Phase::think())); // 测试中安全
    }

    #[tokio::test]
    async fn test_run_cycle_behavior() {
        let mut slot = ReActLoopSlot::new();
        let ctx = make_ctx(serde_json::json!({"max_turns": 3}));
        slot.init(&ctx).await.unwrap(); // 测试中安全

        // Round 1: iter=0, Action → JumpToThink
        let mut mock1 = MockSlotAccessPoint::new(0, Some(action_thought("round1")));
        assert_eq!(
            slot.run(&mut mock1).await.unwrap(), // 测试中安全
            SlotDirective::JumpTo(Phase::think())
        );

        // Round 2: iter=1, Action → JumpToThink
        let mut mock2 = MockSlotAccessPoint::new(1, Some(action_thought("round2")));
        assert_eq!(
            slot.run(&mut mock2).await.unwrap(), // 测试中安全
            SlotDirective::JumpTo(Phase::think())
        );

        // Round 3: iter=2, Action → JumpToThink
        let mut mock3 = MockSlotAccessPoint::new(2, Some(action_thought("round3")));
        assert_eq!(
            slot.run(&mut mock3).await.unwrap(), // 测试中安全
            SlotDirective::JumpTo(Phase::think())
        );

        // Round 4: iter=3, Action → BreakStep (exceeded)
        let mut mock4 = MockSlotAccessPoint::new(3, Some(action_thought("round4")));
        assert_eq!(
            slot.run(&mut mock4).await.unwrap(), // 测试中安全
            SlotDirective::BreakStep
        );
    }
}
