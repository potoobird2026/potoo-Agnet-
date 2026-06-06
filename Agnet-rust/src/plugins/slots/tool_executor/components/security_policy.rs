use std::any::Any;

use async_trait::async_trait;

use crate::plugins::slots::component::{
    AccessPoint, Component, ComponentError, ComponentHandle, ComponentMeta, InitContext, Processing,
};
use crate::shared_types::thought::Action;

/// 安全策略组件（可选）
///
/// 职责：评估工具调用是否违反安全策略
/// 配置：enable_security_policy = false 时跳过检查
/// 当前版本：始终通过（预留扩展点）
pub struct SecurityPolicyComponent {
    enabled: bool,
}

const SECURITY_POLICY_META: ComponentMeta = ComponentMeta {
    name: "security_policy",
    version: "0.1.0",
    priority: 20,
    provides: &["security_check"],
    requires: &[],
    config_key: None,
};

impl SecurityPolicyComponent {
    pub fn new(enabled: bool) -> Self {
        Self { enabled }
    }

    pub fn meta() -> &'static ComponentMeta {
        &SECURITY_POLICY_META
    }
}

#[async_trait]
impl Component for SecurityPolicyComponent {
    fn meta(&self) -> &ComponentMeta {
        Self::meta()
    }

    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        Box::new(Self {
            enabled: self.enabled,
        })
    }

    async fn init(&mut self, _ctx: &InitContext) -> Result<(), ComponentError> {
        Ok(())
    }

    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError> {
        if !self.enabled {
            return Ok(Processing::Continue);
        }

        let _action = ap
            .read_any("current_action")
            .and_then(|v| v.downcast_ref::<Action>().cloned())
            .ok_or_else(|| ComponentError::NotFound("current_action".into()))?;

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
