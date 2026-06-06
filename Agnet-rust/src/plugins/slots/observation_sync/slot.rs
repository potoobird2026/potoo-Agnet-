/// ObservationSyncSlot —— 将 ToolExecutorSlot 输出的 Observation 同步为 ctx.messages 中的 Tool 消息
///
/// 设计依据：protocol-Slot接入协议.md —— Pipeline 引擎不应包含业务逻辑。
/// 此 Slot 替代旧 pipeline.rs 中 hardcoded 的 \"execute\" 阶段消息组装代码。
use async_trait::async_trait;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::context::CONTEXT_OBSERVATION;
use crate::shared_types::{ActionResult, ContentBlock, Message, MessageRole, Observation};

pub struct ObservationSyncSlot;

impl ObservationSyncSlot {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ObservationSyncSlot {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SlotPlugin for ObservationSyncSlot {
    fn name(&self) -> &str {
        "observation_sync"
    }

    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        // 读取 ToolExecutorSlot 写入的 Observation 列表
        let obs_list = match ap.read_context_raw(CONTEXT_OBSERVATION) {
            Some(any) => match any.downcast_ref::<Vec<Observation>>() {
                Some(list) => list.clone(),
                None => return Ok(SlotDirective::Continue),
            },
            None => return Ok(SlotDirective::Continue),
        };

        let obs_count = obs_list.len();
        for obs in obs_list {
            let output = match &obs.result {
                ActionResult::Success { output, .. } => output.clone(),
                ActionResult::RetryableError { error } => error.clone(),
                ActionResult::FatalError { error } => error.clone(),
            };

            let msg = Message {
                role: MessageRole::Tool,
                content: vec![ContentBlock::Text(output)],
                tool_calls: None,
                tool_call_id: obs.action.tool_call_id.clone(),
                reasoning: None,
                metadata: None,
                created_at: obs.completed_at,
            };

            ap.append_message(msg)?;
        }

        tracing::debug!("[observation_sync] {} 条 Tool 消息已追加", obs_count);
        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}
