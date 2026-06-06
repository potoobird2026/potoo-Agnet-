/*! CompressionHookSlot —— Memorize 阶段钩子（SlotPlugin 实现） */
use async_trait::async_trait;
use tokio::sync::mpsc;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;

use super::types::HookEvent;

pub struct CompressionHookSlot {
    event_tx: Option<mpsc::UnboundedSender<HookEvent>>,
    round_id: usize,
    last_round_at: Option<std::time::Instant>,
}

impl CompressionHookSlot {
    pub fn new(event_tx: Option<mpsc::UnboundedSender<HookEvent>>) -> Self {
        Self {
            event_tx,
            round_id: 0,
            last_round_at: None,
        }
    }
}

#[async_trait]
impl SlotPlugin for CompressionHookSlot {
    fn name(&self) -> &str {
        "compression_hook"
    }

    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        self.round_id += 1;
        let now = std::time::Instant::now();
        let interval_ms = self
            .last_round_at
            .map(|prev| now.duration_since(prev).as_millis() as u64)
            .unwrap_or(0);
        self.last_round_at = Some(now);

        let session_id = ap.session_id().to_string();

        // 发送 HookEvent 到 CompressionService
        if let Some(tx) = &self.event_tx {
            // 1. 发送 NewMessagesArrived 事件（触发压缩检查）
            let new_msg_event = HookEvent::NewMessagesArrived {
                session_id: session_id.clone(),
            };
            let _ = tx.send(new_msg_event);

            // 2. 发送 RoundComplete 事件（记录轮次信息）
            let round_event = HookEvent::RoundComplete {
                session_id,
                round_id: self.round_id,
                interval_ms,
            };
            let _ = tx.send(round_event);
        }

        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}
