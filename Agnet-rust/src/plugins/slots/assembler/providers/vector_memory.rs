/*! VectorMemoryProvider（设计文档 §5.5，pri=30）

通过 provider_raw(PROVIDER_VECTOR) 获取 VectorMemoryContract 检索 L3 向量知识库。
当 L3 不可用或检索失败时，降级到 provider_raw(PROVIDER_MEMORY) 的 search_memory（L2 关键词搜索）。
*/

use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::*;
use crate::shared_types::{
    DynProvider, MemoryProvider, MessageRole, VectorMemoryContract, PROVIDER_MEMORY,
    PROVIDER_VECTOR,
};
use async_trait::async_trait;

pub struct VectorMemoryProvider;

#[async_trait]
impl ContextProvider for VectorMemoryProvider {
    fn name(&self) -> &str {
        "vector_memory"
    }
    fn priority(&self) -> u8 {
        30
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
        let query = ap
            .messages()
            .iter()
            .rev()
            .find(|m| m.role == MessageRole::User)
            .map(|m| m.text_content())
            .unwrap_or_default();

        if query.is_empty() {
            return Ok(ProvidedContext {
                blocks: vec![],
                tokens_used: 0,
            });
        }

        // Step 1: 尝试 L3 向量检索
        if let Some(raw) = ap.provider_raw(PROVIDER_VECTOR) {
            if let Ok(wrapper) = raw.downcast::<DynProvider<dyn VectorMemoryContract>>() {
                let provider = wrapper.0.clone();
                match provider.search(&query, quota.max_items).await {
                    Ok(hits) if !hits.is_empty() => {
                        let content: Vec<String> = hits.iter().map(|h| h.text.clone()).collect();
                        let content = content.join("\n");
                        let tokens = (content.len() as f64 / 4.0).ceil() as usize;
                        let max_tokens = quota.max_tokens.min(tokens);
                        return Ok(ProvidedContext {
                            blocks: vec![ContextBlock {
                                section_title: "## Related Knowledge".into(),
                                content,
                                source: "vector_memory".into(),
                                token_count: max_tokens,
                            }],
                            tokens_used: max_tokens,
                        });
                    }
                    Err(e) => {
                        tracing::warn!("VectorMemoryProvider: L3 检索失败, 降级: {}", e);
                    }
                    _ => {}
                }
            }
        }

        // Step 2: 降级到 L2 关键词搜索
        if let Some(raw) = ap.provider_raw(PROVIDER_MEMORY) {
            if let Ok(wrapper) = raw.downcast::<DynProvider<dyn MemoryProvider>>() {
                let provider = wrapper.0.clone();
                if let Ok(entries) = provider.search_memory(&query, quota.max_items).await {
                    if !entries.is_empty() {
                        let content: Vec<String> = entries
                            .iter()
                            .map(|e| e.content.clone().unwrap_or(e.summary.clone()))
                            .collect();
                        let content = content.join("\n");
                        let tokens = (content.len() as f64 / 4.0).ceil() as usize;
                        let max_tokens = quota.max_tokens.min(tokens);
                        return Ok(ProvidedContext {
                            blocks: vec![ContextBlock {
                                section_title: "## Related Knowledge".into(),
                                content,
                                source: "vector_memory".into(),
                                token_count: max_tokens,
                            }],
                            tokens_used: max_tokens,
                        });
                    }
                }
            }
        }

        // Step 3: 都不可用
        Ok(ProvidedContext {
            blocks: vec![],
            tokens_used: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::error::PluginError;
    use crate::shared_types::assembler::{ContextQuota, ProviderSlotConfig};
    use crate::shared_types::{DynProvider, MemoryFileEntry, MemoryProvider, Message, MessageRole};
    use crate::shared_types::{VectorError, VectorMemoryContract, VectorSearchHit, VectorStats};
    use std::any::Any;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct MockSlotAccess {
        providers: HashMap<String, Arc<dyn Any + Send + Sync>>,
        messages: Vec<Message>,
    }

    impl MockSlotAccess {
        fn new() -> Self {
            Self {
                providers: HashMap::new(),
                messages: vec![],
            }
        }
        fn with_messages(mut self, msgs: Vec<Message>) -> Self {
            self.messages = msgs;
            self
        }
        fn with_provider(mut self, name: &str, provider: Arc<dyn Any + Send + Sync>) -> Self {
            self.providers.insert(name.to_string(), provider);
            self
        }
    }

    impl crate::core::access::SlotAccessPoint for MockSlotAccess {
        fn messages(&self) -> &[Message] {
            &self.messages
        }
        fn session_id(&self) -> &str {
            "test-session"
        }
        fn phase_name(&self) -> &str {
            "test-phase"
        }
        fn current_iteration(&self) -> usize {
            0
        }
        fn write_observation(
            &mut self,
            _obs: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            Ok(())
        }
        fn write_context_raw(
            &mut self,
            _key: &str,
            _val: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            Ok(())
        }
        fn read_context_raw(&self, _key: &str) -> Option<&(dyn Any + Send + Sync)> {
            None
        }
        fn request_jump(&self, _phase: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }
        fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>> {
            self.providers.get(name).cloned()
        }
    }

    struct MockVectorProvider {
        hits: Vec<VectorSearchHit>,
        should_error: bool,
    }

    #[async_trait]
    impl VectorMemoryContract for MockVectorProvider {
        async fn search(
            &self,
            _query: &str,
            _top_k: usize,
        ) -> Result<Vec<VectorSearchHit>, VectorError> {
            if self.should_error {
                Err(VectorError::SearchFailed("mock error".into()))
            } else {
                Ok(self.hits.clone())
            }
        }
        async fn upsert(
            &self,
            _id: &str,
            _text: &str,
            _metadata: serde_json::Value,
        ) -> Result<(), VectorError> {
            Ok(())
        }
        async fn delete(&self, _ids: &[String]) -> Result<(), VectorError> {
            Ok(())
        }
        async fn stats(&self) -> Result<VectorStats, VectorError> {
            Ok(VectorStats {
                total_vectors: 0,
                dim: 128,
            })
        }
    }

    struct MockMemoryProvider {
        entries: Vec<MemoryFileEntry>,
    }

    #[async_trait]
    impl MemoryProvider for MockMemoryProvider {
        async fn persist_messages(
            &self,
            _sid: &str,
            _msgs: &[Message],
        ) -> Result<(), crate::shared_types::MemoryError> {
            Ok(())
        }
        async fn persist_observation(
            &self,
            _sid: &str,
            _obs: &str,
        ) -> Result<(), crate::shared_types::MemoryError> {
            Ok(())
        }
        async fn trigger_vector_index(
            &self,
            _sid: &str,
        ) -> Result<(), crate::shared_types::MemoryError> {
            Ok(())
        }
        async fn extract_experiences(
            &self,
            _sid: &str,
        ) -> Result<Vec<crate::shared_types::ExperienceEntry>, crate::shared_types::MemoryError>
        {
            Ok(vec![])
        }
        async fn stats(
            &self,
            _sid: &str,
        ) -> Result<crate::shared_types::MemoryStats, crate::shared_types::MemoryError> {
            Ok(Default::default())
        }
        async fn load_identity(
            &self,
            _sid: &str,
        ) -> Result<crate::shared_types::IdentitySection, crate::shared_types::MemoryError>
        {
            Err(crate::shared_types::MemoryError::NotFound("test".into()))
        }
        async fn load_working_memory(
            &self,
            _sid: &str,
            _limit: usize,
        ) -> Result<Vec<MemoryFileEntry>, crate::shared_types::MemoryError> {
            Ok(vec![])
        }
        async fn is_new_session(
            &self,
            _sid: &str,
        ) -> Result<bool, crate::shared_types::MemoryError> {
            Ok(false)
        }
        async fn search_memory(
            &self,
            _query: &str,
            _limit: usize,
        ) -> Result<Vec<MemoryFileEntry>, crate::shared_types::MemoryError> {
            Ok(self.entries.clone())
        }
    }

    fn user_msg(text: &str) -> Message {
        Message::text(MessageRole::User, text)
    }
    fn quota() -> ContextQuota {
        ContextQuota::default()
    }
    fn provider_config() -> ProviderSlotConfig {
        ProviderSlotConfig {
            max_tokens: 1000,
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn provide_with_vector_provider_returns_hits() {
        let vp = MockVectorProvider {
            hits: vec![
                VectorSearchHit {
                    id: "1".into(),
                    score: 0.9,
                    text: "hello world".into(),
                    source: "doc1".into(),
                },
                VectorSearchHit {
                    id: "2".into(),
                    score: 0.8,
                    text: "foo bar".into(),
                    source: "doc2".into(),
                },
            ],
            should_error: false,
        };
        let ap = MockSlotAccess::new()
            .with_messages(vec![user_msg("test query")])
            .with_provider(
                PROVIDER_VECTOR,
                Arc::new(DynProvider(Arc::new(vp) as Arc<dyn VectorMemoryContract>)),
            );
        let result = VectorMemoryProvider
            .provide(&ap, &quota(), &provider_config())
            .await
            .unwrap();
        assert_eq!(result.blocks.len(), 1);
        assert!(result.blocks[0].content.contains("hello world"));
        assert!(result.blocks[0].content.contains("foo bar"));
    }

    #[tokio::test]
    async fn provide_with_vector_provider_empty_falls_back_to_memory() {
        let vp = MockVectorProvider {
            hits: vec![],
            should_error: false,
        };
        let mp = MockMemoryProvider {
            entries: vec![MemoryFileEntry {
                id: "e1".into(),
                summary: "s".into(),
                content: Some("memory hit".into()),
                created_at: "".into(),
                entry_type: "".into(),
            }],
        };
        let ap = MockSlotAccess::new()
            .with_messages(vec![user_msg("test query")])
            .with_provider(
                PROVIDER_VECTOR,
                Arc::new(DynProvider(Arc::new(vp) as Arc<dyn VectorMemoryContract>)),
            )
            .with_provider(
                PROVIDER_MEMORY,
                Arc::new(DynProvider(Arc::new(mp) as Arc<dyn MemoryProvider>)),
            );
        let result = VectorMemoryProvider
            .provide(&ap, &quota(), &provider_config())
            .await
            .unwrap();
        assert_eq!(result.blocks.len(), 1);
        assert!(result.blocks[0].content.contains("memory hit"));
    }

    #[tokio::test]
    async fn provide_without_vector_provider_falls_back_to_memory() {
        let mp = MockMemoryProvider {
            entries: vec![MemoryFileEntry {
                id: "e1".into(),
                summary: "s".into(),
                content: Some("fallback".into()),
                created_at: "".into(),
                entry_type: "".into(),
            }],
        };
        let ap = MockSlotAccess::new()
            .with_messages(vec![user_msg("test query")])
            .with_provider(
                PROVIDER_MEMORY,
                Arc::new(DynProvider(Arc::new(mp) as Arc<dyn MemoryProvider>)),
            );
        let result = VectorMemoryProvider
            .provide(&ap, &quota(), &provider_config())
            .await
            .unwrap();
        assert_eq!(result.blocks.len(), 1);
        assert!(result.blocks[0].content.contains("fallback"));
    }

    #[tokio::test]
    async fn provide_with_both_unavailable_returns_empty() {
        let ap = MockSlotAccess::new().with_messages(vec![user_msg("test query")]);
        let result = VectorMemoryProvider
            .provide(&ap, &quota(), &provider_config())
            .await
            .unwrap();
        assert!(result.blocks.is_empty());
    }

    #[tokio::test]
    async fn provide_with_vector_error_falls_back_to_memory() {
        let vp = MockVectorProvider {
            hits: vec![],
            should_error: true,
        };
        let mp = MockMemoryProvider {
            entries: vec![MemoryFileEntry {
                id: "e1".into(),
                summary: "s".into(),
                content: Some("error fallback".into()),
                created_at: "".into(),
                entry_type: "".into(),
            }],
        };
        let ap = MockSlotAccess::new()
            .with_messages(vec![user_msg("test query")])
            .with_provider(
                PROVIDER_VECTOR,
                Arc::new(DynProvider(Arc::new(vp) as Arc<dyn VectorMemoryContract>)),
            )
            .with_provider(
                PROVIDER_MEMORY,
                Arc::new(DynProvider(Arc::new(mp) as Arc<dyn MemoryProvider>)),
            );
        let result = VectorMemoryProvider
            .provide(&ap, &quota(), &provider_config())
            .await
            .unwrap();
        assert_eq!(result.blocks.len(), 1);
        assert!(result.blocks[0].content.contains("error fallback"));
    }
}
