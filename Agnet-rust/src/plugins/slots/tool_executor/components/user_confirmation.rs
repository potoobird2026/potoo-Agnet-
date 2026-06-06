use std::any::Any;

use async_trait::async_trait;

use crate::plugins::slots::component::{
    AccessPoint, Component, ComponentError, ComponentHandle, ComponentMeta, InitContext, Processing,
};
use crate::shared_types::thought::Action;

/// 用户确认组件（可选）
///
/// 职责：向用户请求确认后才执行工具
/// 配置：require_confirmation = false 时跳过
/// 当前版本：始终通过（预留扩展点）
pub struct UserConfirmationComponent {
    enabled: bool,
    timeout_secs: u64,
}

const USER_CONFIRMATION_META: ComponentMeta = ComponentMeta {
    name: "user_confirmation",
    version: "0.1.0",
    priority: 30,
    provides: &["user_confirmation"],
    requires: &[],
    config_key: None,
};

impl UserConfirmationComponent {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            timeout_secs: 60,
        }
    }

    pub fn meta() -> &'static ComponentMeta {
        &USER_CONFIRMATION_META
    }
}

#[async_trait]
impl Component for UserConfirmationComponent {
    fn meta(&self) -> &ComponentMeta {
        Self::meta()
    }

    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        Box::new(Self {
            enabled: self.enabled,
            timeout_secs: self.timeout_secs,
        })
    }

    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError> {
        self.timeout_secs = ctx.config.confirmation_timeout_secs();
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
