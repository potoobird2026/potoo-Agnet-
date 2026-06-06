use std::any::Any;

use async_trait::async_trait;

use crate::plugins::slots::component::{
    AccessPoint, Component, ComponentError, ComponentHandle, ComponentMeta, InitContext, Processing,
};

/// 设计文档 §3.1——轮次限制服务，纯函数接口
pub trait TurnLimitService: Send + Sync {
    /// iteration 从 0 开始计数
    fn is_exceeded(&self, iteration: usize) -> bool;
    fn max_turns(&self) -> usize;
}

pub struct TurnLimitComponent {
    max_turns: usize,
}

impl TurnLimitComponent {
    pub fn new(max_turns: usize) -> Self {
        Self { max_turns }
    }
}

impl TurnLimitService for TurnLimitComponent {
    fn is_exceeded(&self, iteration: usize) -> bool {
        // 设计文档 §3.1——纯函数，只依赖 self.max_turns，不依赖 AccessPoint
        iteration >= self.max_turns
    }

    fn max_turns(&self) -> usize {
        self.max_turns
    }
}

#[async_trait]
impl Component for TurnLimitComponent {
    fn meta(&self) -> &ComponentMeta {
        static META: ComponentMeta = ComponentMeta {
            name: "turn_limiter",
            version: "0.1.0",
            priority: 10,
            provides: &["turn_check"],
            requires: &[],
            config_key: Some("react_loop"),
        };
        &META
    }

    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        Box::new(Self {
            max_turns: self.max_turns,
        })
    }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        // 设计文档 §3.1——配置读取 + 边界校验
        // max_turns 已在 SlotPlugin::init() 中解析，此处直接读取 + 校验
        let configured = ctx.config.max_turns();
        // 边界处理：max_turns 至少为 1
        self.max_turns = if configured >= 1 { configured } else { 1 };
        Ok(())
    }

    // 设计文档 §3.1 process(): no-op
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
    use super::*;
    use crate::plugins::slots::component::ModuleConfig;
    use crate::plugins::slots::react_loop::types::DEFAULT_MAX_TURNS;

    // ============================================
    // TurnLimitService 测试（纯函数，无需 AccessPoint）
    // ============================================

    #[test]
    fn test_turn_limit_not_exceeded() {
        // 设计文档 §5.3——数字阈值来自构造参数，非硬编码
        let component = TurnLimitComponent::new(5);
        assert!(!component.is_exceeded(0));
        assert!(!component.is_exceeded(4));
    }

    #[test]
    fn test_turn_limit_exceeded() {
        let component = TurnLimitComponent::new(5);
        assert!(component.is_exceeded(5));
        assert!(component.is_exceeded(100));
    }

    #[test]
    fn test_turn_limit_default() {
        let component = TurnLimitComponent::new(DEFAULT_MAX_TURNS);
        assert!(!component.is_exceeded(DEFAULT_MAX_TURNS - 1));
        assert!(component.is_exceeded(DEFAULT_MAX_TURNS));
    }

    #[test]
    fn test_turn_limit_boundary() {
        // 边界：max_turns=1 时
        let component = TurnLimitComponent::new(1);
        assert!(!component.is_exceeded(0));
        assert!(component.is_exceeded(1));
    }

    // ============================================
    // init() 校验测试（max_turns 提升）
    // ============================================

    #[tokio::test]
    async fn test_turn_limit_init_clamps_zero() {
        // 设计文档 §3.1——max_turns=0 被提升为 1
        let mut component = TurnLimitComponent::new(0);
        let ctx = InitContext {
            config: ModuleConfig::new(serde_json::json!({"max_turns": 0})),
        };
        component.init(&ctx).await.unwrap();
        assert_eq!(component.max_turns(), 1);
    }

    #[tokio::test]
    async fn test_turn_limit_init_keeps_valid() {
        let mut component = TurnLimitComponent::new(10);
        let ctx = InitContext {
            config: ModuleConfig::new(serde_json::json!({"max_turns": 10})),
        };
        component.init(&ctx).await.unwrap();
        assert_eq!(component.max_turns(), 10);
    }
}
