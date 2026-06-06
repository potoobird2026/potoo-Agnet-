//! react_loop 组件协议——统一 re-export
//!
//! 所有 react_loop 内部组件使用此模块访问统一协议类型，
//! 避免各模块重复定义相同接口。

#[allow(unused_imports)]
pub use crate::plugins::slots::component::{
    AccessPoint, Component, ComponentError, ComponentHandle, ComponentMeta, InitContext,
    MetricsHandle, ModuleConfig, Processing,
};
