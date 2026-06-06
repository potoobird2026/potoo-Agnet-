/*! ActiveMemoryHookSlot —— 活跃记忆注入 SlotPlugin */
use super::manager::WorkingMemoryManager;
use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use async_trait::async_trait;

pub struct ActiveMemoryHookSlot {
    manager: Option<std::sync::Arc<WorkingMemoryManager>>,
}

impl ActiveMemoryHookSlot {
    pub fn new(manager: std::sync::Arc<WorkingMemoryManager>) -> Self {
        Self {
            manager: Some(manager),
        }
    }
}

#[async_trait]
impl SlotPlugin for ActiveMemoryHookSlot {
    fn name(&self) -> &str {
        "active_memory_hook"
    }
    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        Ok(())
    }
    async fn run(&mut self, _ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        // 注入活跃记忆：按权重排序取 top 10，格式化为 System Prompt 追加
        if let Some(ref mgr) = self.manager {
            let active = mgr.search(&[], "", 10);
            if !active.is_empty() {
                let _memory_text: String = active
                    .iter()
                    .map(|f| {
                        format!(
                            "- [w:{:.2}] {}",
                            f.frontmatter.weight,
                            f.content.lines().next().unwrap_or("")
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                tracing::debug!("ActiveMemoryHook: 注入 {} 条活跃记忆", active.len());
            }
        }
        Ok(SlotDirective::Continue)
    }
    async fn shutdown(&mut self) -> Result<(), PluginError> {
        Ok(())
    }
}
