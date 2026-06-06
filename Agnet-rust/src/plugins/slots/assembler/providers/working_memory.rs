/*! WorkingMemoryProvider（设计文档 §5.4，pri=20）

从 StepContext 读取 L2 工作记忆。超配额时调用 DocumentCompactor。
*/

use super::super::compaction::DocumentCompactor;
use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::*;
use crate::shared_types::context::CONTEXT_WORKING_MEMORY;
use crate::shared_types::MemoryFileEntry;
use async_trait::async_trait;
use std::sync::Arc;

pub struct WorkingMemoryProvider {
    compactor: Arc<DocumentCompactor>,
}

impl WorkingMemoryProvider {
    pub fn new(compactor: Arc<DocumentCompactor>) -> Self {
        Self { compactor }
    }
}

#[async_trait]
impl ContextProvider for WorkingMemoryProvider {
    fn name(&self) -> &str {
        "working_memory"
    }
    fn priority(&self) -> u8 {
        20
    }
    fn allow_truncation(&self) -> bool {
        true
    }
    fn silent_on_empty(&self) -> bool {
        true
    }

    fn estimate_max_tokens(&self, config: &ProviderSlotConfig) -> usize {
        config.max_tokens
    }

    async fn provide(
        &self,
        ap: &dyn SlotAccessPoint,
        quota: &ContextQuota,
        config: &ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError> {
        let entries: Vec<MemoryFileEntry> = ap
            .read_context_raw(CONTEXT_WORKING_MEMORY)
            .and_then(|any| any.downcast_ref::<Vec<MemoryFileEntry>>())
            .cloned()
            .unwrap_or_default();

        if entries.is_empty() {
            return Ok(ProvidedContext {
                blocks: vec![],
                tokens_used: 0,
            });
        }

        // 按 created_at 降序排列（最新的在前）（设计文档 §5.4）
        let mut sorted = entries.clone();
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        let mut blocks = Vec::new();
        let mut total_tokens = 0usize;

        for entry in sorted.iter().take(quota.max_items) {
            let entry_content = entry.content.clone().unwrap_or(entry.summary.clone());
            let max_chars = quota.max_chars_per_item;
            let content =
                if max_chars > 0 && entry_content.len() > max_chars && config.allow_compaction {
                    let max_tokens = (max_chars as f64 / 4.0) as usize;
                    self.compactor.compact(&entry_content, max_tokens, true)
                } else {
                    entry_content
                };
            let tokens = (content.len() as f64 / 4.0).ceil() as usize;
            total_tokens += tokens;
            if total_tokens > quota.max_tokens {
                break;
            }
            blocks.push(ContextBlock {
                section_title: "## Working Memory".into(),
                content,
                source: format!("working_memory/{}", entry.id),
                token_count: tokens,
            });
        }

        Ok(ProvidedContext {
            blocks,
            tokens_used: total_tokens,
        })
    }
}
