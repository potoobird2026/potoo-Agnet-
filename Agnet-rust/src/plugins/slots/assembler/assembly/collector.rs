/*! BlockCollector —— 按优先级收集 Provider 内容（设计文档 §7.4）*/

use super::super::config::LOG_PREFIX;
use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::*;
use std::collections::HashMap;
use std::sync::Arc;

/// 按优先级收集 Provider 内容（设计文档 §7.4）
pub struct BlockCollector;

impl BlockCollector {
    pub async fn collect(
        providers: &[Arc<dyn ContextProvider>],
        ap: &dyn SlotAccessPoint,
        quotas: &HashMap<String, ContextQuota>,
        _provider_configs: &HashMap<String, ProviderSlotConfig>,
    ) -> (Vec<ContextBlock>, Vec<ProviderStat>, Vec<AssemblyWarning>) {
        let mut all_blocks: Vec<ContextBlock> = Vec::new();
        let mut stats: Vec<ProviderStat> = Vec::new();
        let mut warnings: Vec<AssemblyWarning> = Vec::new();
        let mut _total_tokens = 0usize;

        let mut sorted_providers = providers.to_vec();
        sorted_providers.sort_by_key(|p| std::cmp::Reverse(p.priority()));

        for provider in &sorted_providers {
            let name = provider.name().to_string();
            let priority = provider.priority();
            let quota = quotas.get(&name).cloned().unwrap_or_default();

            if quota.max_tokens == 0 {
                stats.push(ProviderStat {
                    name,
                    priority,
                    tokens_used: 0,
                    blocks_count: 0,
                    success: true,
                    error: None,
                });
                continue;
            }

            // 尝试标记：$15 limit，大模型可用
            let provider_config = _provider_configs.get(&name).cloned().unwrap_or_default();
            match provider.provide(ap, &quota, &provider_config).await {
                Ok(result) => {
                    _total_tokens += result.tokens_used;
                    let blocks_count = result.blocks.len();
                    all_blocks.extend(result.blocks);
                    stats.push(ProviderStat {
                        name,
                        priority,
                        tokens_used: result.tokens_used,
                        blocks_count,
                        success: true,
                        error: None,
                    });
                }
                Err(e) => {
                    tracing::warn!("{} Provider '{}' 失败: {}", LOG_PREFIX, name, e);
                    warnings.push(AssemblyWarning {
                        code: "PROVIDER_FAILED".into(),
                        message: format!("{}: {}", name, e),
                    });
                    stats.push(ProviderStat {
                        name,
                        priority,
                        tokens_used: 0,
                        blocks_count: 0,
                        success: false,
                        error: Some(e.to_string()),
                    });
                }
            }
        }

        (all_blocks, stats, warnings)
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use std::any::Any;
    use std::collections::HashMap;
    use std::sync::Arc;

    use super::*;
    use crate::shared_types::Message;

    use crate::core::access::SlotAccessPoint;
    use crate::core::types::error::PluginError;

    struct MockProvider {
        name: String,
        priority: u8,
        success: bool,
    }

    #[async_trait]
    impl ContextProvider for MockProvider {
        fn name(&self) -> &str {
            &self.name
        }
        fn priority(&self) -> u8 {
            self.priority
        }
        fn estimate_max_tokens(&self, _: &ProviderSlotConfig) -> usize {
            0
        }
        async fn provide(
            &self,
            _: &dyn SlotAccessPoint,
            _: &ContextQuota,
            _: &ProviderSlotConfig,
        ) -> Result<ProvidedContext, ProviderError> {
            if self.success {
                Ok(ProvidedContext {
                    blocks: vec![ContextBlock {
                        section_title: self.name.clone(),
                        content: format!("{} content", self.name),
                        source: self.name.clone(),
                        token_count: 10,
                    }],
                    tokens_used: 10,
                })
            } else {
                Err(ProviderError::Internal("mock failure".into()))
            }
        }
    }

    struct MockAccessPoint;

    impl SlotAccessPoint for MockAccessPoint {
        fn messages(&self) -> &[crate::shared_types::Message] {
            &[]
        }
        fn session_id(&self) -> &str {
            "test"
        }
        fn phase_name(&self) -> &str {
            "test"
        }
        fn current_iteration(&self) -> usize {
            1
        }
        fn write_observation(&mut self, _: Box<dyn Any + Send + Sync>) -> Result<(), PluginError> {
            Ok(())
        }
        fn write_context_raw(
            &mut self,
            _: &str,
            _: Box<dyn Any + Send + Sync>,
        ) -> Result<(), PluginError> {
            Ok(())
        }
        fn read_context_raw(&self, _: &str) -> Option<&(dyn Any + Send + Sync)> {
            None
        }
        fn request_jump(&self, _: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }
        fn provider_raw(&self, _: &str) -> Option<Arc<dyn Any + Send + Sync>> {
            None
        }

        fn append_message(&mut self, _msg: Message) -> Result<(), PluginError> {
            Ok(())
        }
    }

    fn make_provider(name: &str, priority: u8, success: bool) -> Arc<dyn ContextProvider> {
        Arc::new(MockProvider {
            name: name.to_string(),
            priority,
            success,
        })
    }

    #[tokio::test]
    async fn test_collect_single_provider() {
        let providers = vec![make_provider("identity", 1, true)];
        let quotas: HashMap<String, ContextQuota> = [(
            "identity".into(),
            ContextQuota {
                max_tokens: 100,
                ..Default::default()
            },
        )]
        .into();
        let ap = MockAccessPoint;
        let (blocks, stats, warnings) =
            BlockCollector::collect(&providers, &ap, &quotas, &HashMap::new()).await;
        assert_eq!(blocks.len(), 1);
        assert_eq!(stats.len(), 1);
        assert!(stats[0].success);
        assert!(warnings.is_empty());
    }

    #[tokio::test]
    async fn test_collect_multiple_providers() {
        let providers = vec![
            make_provider("identity", 1, true),
            make_provider("working_memory", 2, true),
        ];
        let quotas: HashMap<String, ContextQuota> = [
            (
                "identity".into(),
                ContextQuota {
                    max_tokens: 100,
                    ..Default::default()
                },
            ),
            (
                "working_memory".into(),
                ContextQuota {
                    max_tokens: 200,
                    ..Default::default()
                },
            ),
        ]
        .into();
        let ap = MockAccessPoint;
        let (blocks, stats, warnings) =
            BlockCollector::collect(&providers, &ap, &quotas, &HashMap::new()).await;
        assert_eq!(blocks.len(), 2);
        assert_eq!(stats.len(), 2);
        assert!(warnings.is_empty());
    }

    #[tokio::test]
    async fn test_collect_provider_failure_does_not_block_others() {
        let providers = vec![
            make_provider("good", 1, true),
            make_provider("bad", 2, false),
            make_provider("also_good", 3, true),
        ];
        let quotas: HashMap<String, ContextQuota> = [
            (
                "good".into(),
                ContextQuota {
                    max_tokens: 100,
                    ..Default::default()
                },
            ),
            (
                "bad".into(),
                ContextQuota {
                    max_tokens: 100,
                    ..Default::default()
                },
            ),
            (
                "also_good".into(),
                ContextQuota {
                    max_tokens: 100,
                    ..Default::default()
                },
            ),
        ]
        .into();
        let ap = MockAccessPoint;
        let (blocks, stats, warnings) =
            BlockCollector::collect(&providers, &ap, &quotas, &HashMap::new()).await;
        assert_eq!(blocks.len(), 2);
        assert_eq!(stats.len(), 3);
        assert!(!stats[1].success);
        assert!(!warnings.is_empty());
    }

    #[tokio::test]
    async fn test_collect_zero_quota_skips_provider() {
        let providers = vec![make_provider("skipped", 1, true)];
        let quotas: HashMap<String, ContextQuota> = [(
            "skipped".into(),
            ContextQuota {
                max_tokens: 0,
                ..Default::default()
            },
        )]
        .into();
        let ap = MockAccessPoint;
        let (blocks, stats, _) =
            BlockCollector::collect(&providers, &ap, &quotas, &HashMap::new()).await;
        assert!(blocks.is_empty());
        assert_eq!(stats[0].tokens_used, 0);
    }
}
