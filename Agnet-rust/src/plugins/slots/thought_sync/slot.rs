/// ThoughtSyncSlot —— 将 LlmThinkerSlot 输出的 Thought 同步为 ctx.messages 中的 Assistant 消息
///
/// 设计依据：protocol-Slot接入协议.md —— Pipeline 引擎不应包含业务逻辑。
/// 此 Slot 替代旧 pipeline.rs 中 hardcoded 的 \"think\" 阶段消息组装代码。
use async_trait::async_trait;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::core::types::Timestamp;
use crate::shared_types::context::CONTEXT_THOUGHT;
use crate::shared_types::{ContentBlock, Message, MessageRole, Thought};

pub struct ThoughtSyncSlot;

impl ThoughtSyncSlot {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ThoughtSyncSlot {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SlotPlugin for ThoughtSyncSlot {
    fn name(&self) -> &str {
        "thought_sync"
    }

    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        // 读取 LlmThinkerSlot 写入的 Thought
        let thought = match ap.read_context_raw(CONTEXT_THOUGHT) {
            Some(any) => match any.downcast_ref::<Thought>() {
                Some(t) => t.clone(),
                None => return Ok(SlotDirective::Continue),
            },
            None => return Ok(SlotDirective::Continue),
        };

        let reasoning = match &thought {
            Thought::Final { reasoning, .. } => reasoning.clone(),
            Thought::Action { reasoning, .. } => reasoning.clone(),
        };
        let reasoning_for_log = reasoning.clone();

        let msg = match &thought {
            Thought::Final { answer, .. } => Message {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::Text(answer.clone())],
                tool_calls: None,
                tool_call_id: None,
                reasoning: Some(reasoning),
                metadata: None,
                created_at: Timestamp::now(),
            },
            Thought::Action { action, .. } => Message {
                role: MessageRole::Assistant,
                content: vec![],
                tool_calls: action.tool_calls.clone(),
                tool_call_id: None,
                reasoning: Some(reasoning),
                metadata: None,
                created_at: Timestamp::now(),
            },
        };

        ap.append_message(msg)?;

        tracing::debug!(
            "[thought_sync] Assistant 消息已追加: reasoning={}",
            &reasoning_for_log
        );
        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}
