//! tool_executor 组件协议——统一 re-export
//!
//! 所有 tool_executor 内部组件使用此模块访问统一协议类型。

pub use crate::plugins::slots::component::{
    AccessPoint, Component, ComponentError, ComponentHandle, InitContext, MetricsHandle,
    ModuleConfig, Processing,
};
