use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::core::access::{ProviderRegistry, SlotAccessPoint};
use crate::core::types::error::PluginError;
use crate::core::types::Timestamp;
use crate::shared_types::{ContentBlock, Message};

/// 步骤执行上下文
///
/// 管线每次 Step 执行时创建，承载当前会话的全部状态。
/// ReAct 专属字段（thought/action/observation）已移除；
/// LLM Thinker 通过 `step_result`（类型擦除）写入 Thought/Turn，
/// 消费端通过 downcast 获取。
pub struct StepContext {
    pub messages: Vec<Message>,
    pub current_turn: usize,
    pub max_turns: usize,
    pub session_id: String,
    pub source: String,
    pub phase_name: String,
    /// 任意 Slot 写入的结果（消费端通过 downcast 获取具体类型）
    pub step_result: Option<Box<dyn Any + Send + Sync>>,
    pub step_started_at: Timestamp,
    /// 通用上下文数据（按 String key 索引，类型擦除）
    data: HashMap<String, Box<dyn Any + Send + Sync>>,
    pending_directive: Mutex<Option<super::slot::SlotDirective>>,
    /// Provider 注册表引用（用于实现 provider_raw() 查找）
    provider_registry: Option<Arc<ProviderRegistry>>,
    /// 当前 Slot 被授予的权限列表（空 = 未启用权限检查）
    allowed_permissions: Vec<String>,
}

impl StepContext {
    /// 创建新上下文
    pub fn new(session_id: String, messages: Vec<Message>, max_turns: usize) -> Self {
        Self {
            messages,
            current_turn: 0,
            max_turns,
            session_id,
            source: String::new(),
            phase_name: String::new(),
            step_result: None,
            step_started_at: Timestamp::now(),
            data: HashMap::new(),
            pending_directive: Mutex::new(None),
            provider_registry: None,
            allowed_permissions: Vec::new(),
        }
    }

    pub fn with_source(mut self, source: String) -> Self {
        self.source = source;
        self
    }

    /// 关联 ProviderRegistry（由 AgentRuntime 在创建后设置）
    pub fn with_provider_registry(mut self, registry: Arc<ProviderRegistry>) -> Self {
        self.provider_registry = Some(registry);
        self
    }

    /// 设置当前 Slot 被授予的权限列表
    pub fn with_permissions(mut self, permissions: Vec<String>) -> Self {
        self.allowed_permissions = permissions;
        self
    }

    /// 写入上下文数据（按 key 索引，类型安全）
    pub fn set_context<T: Send + Sync + 'static>(&mut self, key: &str, val: T) {
        self.data.insert(key.to_string(), Box::new(val));
    }

    /// 读取上下文数据（按 key 索引，类型安全）
    pub fn get_context<T: 'static>(&self, key: &str) -> Option<&T> {
        self.data.get(key).and_then(|b| b.downcast_ref::<T>())
    }

    pub fn elapsed_ms(&self) -> i64 {
        Timestamp::now()
            .duration_since(self.step_started_at)
            .as_millis() as i64
    }

    pub fn turn(&self) -> usize {
        self.current_turn
    }

    /// 检查是否有待处理的流程指令（Pipeline 使用）
    pub fn take_pending_directive(&self) -> Option<super::slot::SlotDirective> {
        self.pending_directive
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }

    /// 权限检查（内部使用）
    fn check_permission(&self, permission: &str) -> Result<(), PluginError> {
        if self.allowed_permissions.is_empty() {
            return Ok(()); // 未启用权限检查时放行
        }
        if self.allowed_permissions.iter().any(|p| p == permission) {
            Ok(())
        } else {
            Err(PluginError::PermissionDenied {
                required: permission.to_string(),
            })
        }
    }
}

impl SlotAccessPoint for StepContext {
    fn messages(&self) -> &[Message] {
        &self.messages
    }

    fn session_id(&self) -> &str {
        &self.session_id
    }

    fn phase_name(&self) -> &str {
        &self.phase_name
    }

    fn current_iteration(&self) -> usize {
        self.current_turn
    }

    fn write_observation(&mut self, obs: Box<dyn Any + Send + Sync>) -> Result<(), PluginError> {
        self.check_permission("observation:write")?;
        self.data.insert(
            crate::shared_types::context::CONTEXT_OBSERVATION.to_string(),
            obs,
        );
        Ok(())
    }

    fn write_context_raw(
        &mut self,
        key: &str,
        val: Box<dyn Any + Send + Sync>,
    ) -> Result<(), PluginError> {
        self.check_permission("context:write")?;
        self.data.insert(key.to_string(), val);
        Ok(())
    }

    fn read_context_raw(&self, key: &str) -> Option<&(dyn Any + Send + Sync)> {
        self.data.get(key).map(|b| b.as_ref())
    }

    fn request_jump(&self, phase: &str) -> Result<(), PluginError> {
        self.check_permission("phase:jump")?;
        *self
            .pending_directive
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(super::slot::SlotDirective::JumpTo(
            super::phase::Phase::new(phase),
        ));
        Ok(())
    }

    fn request_abort(&self) -> Result<(), PluginError> {
        self.check_permission("phase:abort")?;
        *self
            .pending_directive
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = Some(super::slot::SlotDirective::AbortPipeline);
        Ok(())
    }

    fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
        self.provider_registry
            .as_ref()
            .and_then(|reg| reg.get_raw(name))
    }
}

/// 步骤输入
#[derive(Debug)]
pub struct StepInput {
    pub session_id: String,
    pub message: Vec<ContentBlock>,
    pub source: Option<String>,
}

impl StepInput {
    pub fn new(session_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            session_id: session_id.into(),
            message: vec![ContentBlock::Text(message.into())],
            source: None,
        }
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }
}
