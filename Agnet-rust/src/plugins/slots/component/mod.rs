//! 统一 Component 协议
//!
//! react_loop 和 tool_executor 共享相同的组件接口：
//! Component → ComponentHandle → AccessPoint → ModuleConfig → Processing
//!
//! 设计依据：AI开发红线与纪律.md §4.1

use std::any::Any;
use std::fmt;

use async_trait::async_trait;

use crate::core::types::error::PluginError;

/// 组件元数据
#[derive(Debug, Clone)]
pub struct ComponentMeta {
    pub name: &'static str,
    pub version: &'static str,
    pub priority: u8,
    pub provides: &'static [&'static str],
    pub requires: &'static [&'static str],
    pub config_key: Option<&'static str>,
}

/// 组件生命周期（8 方法）
#[async_trait]
pub trait Component: Send + Sync {
    fn meta(&self) -> &ComponentMeta;
    fn clone_box(&self) -> Box<dyn ComponentHandle>;
    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError>;
    async fn process(&mut self, ap: &mut dyn AccessPoint) -> Result<Processing, ComponentError>;
    async fn shutdown(&mut self) -> Result<(), ComponentError>;
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clonable(&self) -> bool;
    fn ready(&self) -> bool;
}

/// 组件句柄——通过 downcast 获得具体类型接口
pub trait ComponentHandle: Send + Sync {
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clone_box(&self) -> Box<dyn ComponentHandle>;
}

impl<T: Component + 'static> ComponentHandle for T {
    fn name(&self) -> &str {
        self.meta().name
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn clone_box(&self) -> Box<dyn ComponentHandle> {
        self.clone_box()
    }
}

impl Clone for Box<dyn ComponentHandle> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

impl fmt::Debug for dyn ComponentHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ComponentHandle")
            .field("name", &self.name())
            .finish()
    }
}

/// 处理结果
#[derive(Debug, Clone)]
pub enum Processing {
    Continue,
    BreakChain,
    Restart,
    Warn { message: String },
}

/// 组件错误
#[derive(Debug, thiserror::Error)]
pub enum ComponentError {
    #[error("组件配置错误: {0}")]
    Config(String),
    #[error("组件运行时错误: {0}")]
    Internal(String),
    #[error("组件未找到: {0}")]
    NotFound(String),
}

impl From<ComponentError> for PluginError {
    fn from(e: ComponentError) -> Self {
        PluginError::Internal(e.to_string())
    }
}

/// 模块级配置（通用 JSON 值，消费者按需反序列化）
#[derive(Debug, Clone)]
pub struct ModuleConfig {
    pub data: serde_json::Value,
}

impl ModuleConfig {
    pub fn new(data: serde_json::Value) -> Self {
        Self { data }
    }

    /// react_loop: max_turns
    pub fn max_turns(&self) -> usize {
        self.data
            .get("max_turns")
            .and_then(|v| v.as_u64())
            .unwrap_or(3) as usize
    }

    /// tool_executor: circuit_breaker_threshold
    pub fn circuit_breaker_threshold(&self) -> u32 {
        self.data
            .get("circuit_breaker_threshold")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as u32
    }

    /// tool_executor: circuit_breaker_reset_secs
    pub fn circuit_breaker_reset_secs(&self) -> u64 {
        self.data
            .get("circuit_breaker_reset_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(60)
    }

    /// tool_executor: confirmation_timeout_secs
    pub fn confirmation_timeout_secs(&self) -> u64 {
        self.data
            .get("confirmation_timeout_secs")
            .and_then(|v| v.as_u64())
            .unwrap_or(30)
    }
}

/// 初始化上下文
#[derive(Debug, Clone)]
pub struct InitContext {
    pub config: ModuleConfig,
}

impl InitContext {
    pub fn new(config: ModuleConfig) -> Self {
        Self { config }
    }
}

/// 内部数据共享通道（object-safe 版本）
pub trait AccessPoint: Send + Sync {
    fn read_any(&self, key: &str) -> Option<&dyn Any>;
    fn write_any(
        &mut self,
        key: &str,
        val: Box<dyn Any + Send + Sync>,
    ) -> Result<(), ComponentError>;
    fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError>;
    fn config(&self) -> &ModuleConfig;
    fn metrics(&self) -> &MetricsHandle;
}

/// 指标句柄（占位）
#[derive(Debug, Clone)]
pub struct MetricsHandle;
