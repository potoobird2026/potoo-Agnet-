/*! ContextProvider 实现模块（设计文档 §5） */

mod compression_summary;
mod identity;
pub mod skills;
mod system_prompt;
mod vector_memory;
mod working_memory;

use super::compaction::DocumentCompactor;
use super::rule_pool::RuleLlmSelector;
use crate::shared_types::assembler::{AssemblerConfig, ContextProvider};
use std::sync::Arc;

pub use compression_summary::CompressionSummaryProvider;
pub use identity::IdentityProvider;
pub use skills::SkillsProvider;
pub use system_prompt::SystemPromptProvider;
pub use vector_memory::VectorMemoryProvider;
pub use working_memory::WorkingMemoryProvider;

/// 构建 Provider 列表（按 injection_order 排序，跳过 disabled）
pub fn build_providers(
    config: &AssemblerConfig,
    compactor: &DocumentCompactor,
    rule_selector: &Option<RuleLlmSelector>,
    base_template: &str,
    injection_template: &str,
) -> Vec<Arc<dyn ContextProvider>> {
    let compactor = Arc::new(compactor.clone());

    let provider_map: Vec<(&str, Arc<dyn ContextProvider>)> = vec![
        (
            "system_prompt",
            Arc::new(SystemPromptProvider::new(
                rule_selector,
                base_template,
                injection_template,
            )) as Arc<dyn ContextProvider>,
        ),
        (
            "identity",
            Arc::new(IdentityProvider) as Arc<dyn ContextProvider>,
        ),
        (
            "compression_summary",
            Arc::new(CompressionSummaryProvider::new(compactor.clone()))
                as Arc<dyn ContextProvider>,
        ),
        (
            "working_memory",
            Arc::new(WorkingMemoryProvider::new(compactor.clone())) as Arc<dyn ContextProvider>,
        ),
        (
            "vector_memory",
            Arc::new(VectorMemoryProvider) as Arc<dyn ContextProvider>,
        ),
        (
            "skills",
            Arc::new(SkillsProvider) as Arc<dyn ContextProvider>,
        ),
    ];

    // 按 injection_order 排序，跳过 disabled
    let disabled: std::collections::HashSet<&str> = config
        .disabled_providers
        .iter()
        .map(|s| s.as_str())
        .collect();

    let provider_map_ref: std::collections::HashMap<&str, Arc<dyn ContextProvider>> =
        provider_map.into_iter().collect();

    config
        .injection_order
        .iter()
        .filter_map(|name| {
            if disabled.contains(name.as_str()) {
                None
            } else {
                provider_map_ref.get(name.as_str()).cloned()
            }
        })
        .collect()
}
