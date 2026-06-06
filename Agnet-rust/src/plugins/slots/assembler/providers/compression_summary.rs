/*! CompressionSummaryProvider（设计文档 §5.3，pri=10）

通过 provider_raw(PROVIDER_COMPRESSION_SUMMARY) 获取压缩摘要。
*/

use super::super::compaction::DocumentCompactor;
use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::*;
use crate::shared_types::compression::{CompressionSummaryContract, PROVIDER_COMPRESSION_SUMMARY};
use crate::shared_types::DynProvider;
use async_trait::async_trait;
use std::sync::Arc;

pub struct CompressionSummaryProvider {
    compactor: Arc<DocumentCompactor>,
}

impl CompressionSummaryProvider {
    pub fn new(compactor: Arc<DocumentCompactor>) -> Self {
        Self { compactor }
    }
}

#[async_trait]
impl ContextProvider for CompressionSummaryProvider {
    fn name(&self) -> &str {
        "compression_summary"
    }
    fn priority(&self) -> u8 {
        10
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
        _config: &ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError> {
        let session_id = ap.session_id();

        let summary = match ap.provider_raw(PROVIDER_COMPRESSION_SUMMARY) {
            Some(raw) => match raw.downcast::<DynProvider<dyn CompressionSummaryContract>>() {
                Ok(wrapper) => wrapper.0.get_summary(session_id).await,
                Err(_) => None,
            },
            None => None,
        };

        match summary {
            Some(content) if !content.is_empty() => {
                let tokens = (content.len() as f64 / 4.0).ceil() as usize;
                let max_tokens = quota.max_tokens.min(tokens);
                let final_content = if tokens > quota.max_tokens {
                    self.compactor.compact(&content, quota.max_tokens, true)
                } else {
                    content
                };
                Ok(ProvidedContext {
                    blocks: vec![ContextBlock {
                        section_title: "## Conversation Context".into(),
                        content: final_content,
                        source: "compression_summary".into(),
                        token_count: max_tokens,
                    }],
                    tokens_used: max_tokens,
                })
            }
            _ => Ok(ProvidedContext {
                blocks: vec![],
                tokens_used: 0,
            }),
        }
    }
}
