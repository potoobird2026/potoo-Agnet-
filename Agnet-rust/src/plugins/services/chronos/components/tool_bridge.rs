use super::ToolBridgeService;
use crate::core::component::{
    Component, ComponentError, ComponentInitContext, InternalAccessPoint, Processing,
};
use async_trait::async_trait;
use serde_json::Value;

const NAME: &str = "tool_bridge";

pub struct ToolBridgeComponent {
    init_done: bool,
}

impl Default for ToolBridgeComponent {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolBridgeComponent {
    pub fn new() -> Self {
        Self { init_done: false }
    }
}

#[async_trait]
impl ToolBridgeService for ToolBridgeComponent {
    async fn execute_tool(&self, name: &str, _args: Value) -> Result<Value, String> {
        // P2 组件：工具桥接暂为占位实现，由主循环通过 ServiceAccessPoint 调用
        tracing::debug!("ToolBridge: 调用工具 '{}' (占位)", name);
        Ok(Value::Null)
    }
}

#[async_trait]
impl Component for ToolBridgeComponent {
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
