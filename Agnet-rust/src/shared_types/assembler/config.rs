/*! AssemblerConfig + ProviderSlotConfig（设计文档 §3.7） */

use super::compaction::CompactionConfig;
use super::rule_pool::RulePoolConfig;
use std::collections::HashMap;

/// ConversationAssembler 完整配置（设计文档 §3.7）
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(default)]
pub struct AssemblerConfig {
    pub enabled: bool,
    pub debug: bool,
    pub response_reserve_ratio: f64,
    pub history_budget_ratio: f64,
    pub min_recent_messages: usize,
    pub max_injection_tokens: usize,
    pub minimum_context_size: usize,
    pub injection_policy: String,
    pub disabled_providers: Vec<String>,
    pub providers: HashMap<String, ProviderSlotConfig>,
    pub injection_order: Vec<String>,
    pub compaction: CompactionConfig,
    pub rule_pool: RulePoolConfig,
    pub output_adapter_enabled: bool,
    pub base_prompt_path: String,
    pub injection_layout_path: String,
}

/// Provider 独立配置（设计文档 §3.7）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProviderSlotConfig {
    pub enabled: bool,
    pub max_tokens: usize,
    pub max_items: usize,
    pub max_chars_per_item: usize,
    pub min_guaranteed_tokens: usize,
    pub allow_compaction: bool,
    pub allow_truncation: bool,
}

impl Default for ProviderSlotConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_tokens: 3000,
            max_items: 10,
            max_chars_per_item: 2000,
            min_guaranteed_tokens: 0,
            allow_compaction: true,
            allow_truncation: true,
        }
    }
}

impl Default for AssemblerConfig {
    fn default() -> Self {
        let mut providers = HashMap::new();
        providers.insert(
            "identity".into(),
            ProviderSlotConfig {
                max_tokens: 2000,
                max_items: 1,
                max_chars_per_item: 0,
                min_guaranteed_tokens: 500,
                allow_compaction: false,
                allow_truncation: false,
                ..Default::default()
            },
        );
        providers.insert(
            "working_memory".into(),
            ProviderSlotConfig {
                max_tokens: 10000,
                max_items: 10,
                max_chars_per_item: 2000,
                min_guaranteed_tokens: 500,
                allow_compaction: true,
                allow_truncation: true,
                ..Default::default()
            },
        );
        providers.insert(
            "vector_memory".into(),
            ProviderSlotConfig {
                max_tokens: 8000,
                max_items: 5,
                max_chars_per_item: 1000,
                min_guaranteed_tokens: 0,
                allow_compaction: true,
                allow_truncation: true,
                ..Default::default()
            },
        );
        providers.insert(
            "compression_summary".into(),
            ProviderSlotConfig {
                max_tokens: 5000,
                max_items: 1,
                max_chars_per_item: 0,
                min_guaranteed_tokens: 0,
                allow_compaction: true,
                allow_truncation: true,
                ..Default::default()
            },
        );
        providers.insert(
            "skills".into(),
            ProviderSlotConfig {
                max_tokens: 3000,
                max_items: 5,
                max_chars_per_item: 1500,
                min_guaranteed_tokens: 0,
                allow_compaction: true,
                allow_truncation: true,
                ..Default::default()
            },
        );
        Self {
            enabled: false,
            debug: false,
            response_reserve_ratio: 0.2,
            history_budget_ratio: 0.7,
            min_recent_messages: 4,
            max_injection_tokens: 30000,
            minimum_context_size: 1000,
            injection_policy: "balanced".into(),
            disabled_providers: vec![],
            providers,
            injection_order: vec![
                "system_prompt".into(),
                "identity".into(),
                "compression_summary".into(),
                "working_memory".into(),
                "vector_memory".into(),
                "skills".into(),
            ],
            compaction: CompactionConfig::default(),
            rule_pool: RulePoolConfig::default(),
            output_adapter_enabled: true,
            base_prompt_path: "templates/base_prompt.md".into(),
            injection_layout_path: "templates/injection_layout.md".into(),
        }
    }
}
