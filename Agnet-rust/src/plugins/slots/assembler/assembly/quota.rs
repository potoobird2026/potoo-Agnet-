/*! QuotaAllocator（设计文档 §7.3）*/

use crate::shared_types::assembler::{AssemblerConfig, ContextQuota};
use std::collections::HashMap;

/// 5 种策略模板的百分比分配（设计文档 §7.3）
///
/// Phase C 改动（C-5）：增加 "skills" 配额
/// - balanced:        0.10 + 0.15 + 0.40 + 0.30 + **0.05** = 1.00
/// - memory_focused:  0.05 + 0.10 + 0.55 + 0.25 + **0.05** = 1.00
/// - token_efficient: 0.15 + 0.15 + 0.30 + 0.15 + **0.10** = 0.85
/// - identity_only: 0.90（不加 skills）
/// - minimal: 空
pub fn allocate_quotas(
    injection_budget: usize,
    policy: &str,
    config: &AssemblerConfig,
) -> HashMap<String, ContextQuota> {
    let ratios: HashMap<&str, f64> = match policy {
        "balanced" => vec![
            ("identity", 0.10),
            ("compression_summary", 0.15),
            ("working_memory", 0.40),
            ("vector_memory", 0.30),
            ("skills", 0.05),
        ]
        .into_iter()
        .collect(),
        "memory_focused" => vec![
            ("identity", 0.05),
            ("compression_summary", 0.10),
            ("working_memory", 0.55),
            ("vector_memory", 0.25),
            ("skills", 0.05),
        ]
        .into_iter()
        .collect(),
        "token_efficient" => vec![
            ("identity", 0.15),
            ("compression_summary", 0.15),
            ("working_memory", 0.30),
            ("vector_memory", 0.15),
            ("skills", 0.10),
        ]
        .into_iter()
        .collect(),
        "identity_only" => vec![("identity", 0.90)].into_iter().collect(),
        "minimal" => HashMap::new(),
        _ => return allocate_quotas(injection_budget, "balanced", config),
    };

    let mut quotas = HashMap::new();
    for (name, ratio) in &ratios {
        let provider_config = config.providers.get(*name);
        let computed = (injection_budget as f64 * ratio) as usize;
        let max_allowed = provider_config.map(|c| c.max_tokens).unwrap_or(usize::MAX);
        let max_tokens = computed.min(max_allowed);
        let cfg = provider_config.cloned().unwrap_or_default();
        quotas.insert(
            name.to_string(),
            ContextQuota {
                max_tokens,
                max_items: cfg.max_items,
                max_chars_per_item: cfg.max_chars_per_item,
                min_guaranteed_tokens: cfg.min_guaranteed_tokens,
                allow_compaction: cfg.allow_compaction,
            },
        );
    }
    quotas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> AssemblerConfig {
        AssemblerConfig::default()
    }

    #[test]
    fn test_allocate_balanced_policy() {
        let config = default_config();
        let quotas = allocate_quotas(10_000, "balanced", &config);
        assert_eq!(quotas.len(), 5, "balanced 策略应包含 5 个 key（含 skills）");
        assert!(quotas.contains_key("identity"));
        assert!(quotas.contains_key("compression_summary"));
        assert!(quotas.contains_key("working_memory"));
        assert!(quotas.contains_key("vector_memory"));
        assert!(quotas.contains_key("skills"));
        assert_eq!(quotas["identity"].max_tokens, (10_000_f64 * 0.10) as usize);
    }

    #[test]
    fn test_allocate_memory_focused_policy() {
        let config = default_config();
        let quotas = allocate_quotas(10_000, "memory_focused", &config);
        assert!(quotas["working_memory"].max_tokens > quotas["vector_memory"].max_tokens);
    }

    #[test]
    fn test_allocate_token_efficient_policy() {
        let config = default_config();
        let quotas = allocate_quotas(10_000, "token_efficient", &config);
        assert_eq!(
            quotas.len(),
            5,
            "token_efficient 策略应包含 5 个 key（含 skills）"
        );
        assert_eq!(quotas["identity"].max_tokens, (10_000_f64 * 0.15) as usize);
    }

    #[test]
    fn test_allocate_identity_only_policy() {
        let config = default_config();
        let quotas = allocate_quotas(10_000, "identity_only", &config);
        assert_eq!(quotas.len(), 1);
        assert!(quotas.contains_key("identity"));
    }

    #[test]
    fn test_allocate_minimal_policy() {
        let config = default_config();
        let quotas = allocate_quotas(10_000, "minimal", &config);
        assert!(quotas.is_empty());
    }

    #[test]
    fn test_allocate_unknown_policy_falls_back_to_balanced() {
        let config = default_config();
        let quotas = allocate_quotas(10_000, "unknown_policy", &config);
        assert_eq!(quotas.len(), 5, "fallback 到 balanced 应包含 5 个 key");
    }

    #[test]
    fn test_allocate_respects_provider_max_tokens() {
        let mut config = default_config();
        config
            .providers
            .get_mut("identity")
            .expect("identity provider config")
            .max_tokens = 50;
        let quotas = allocate_quotas(10_000, "balanced", &config);
        assert_eq!(quotas["identity"].max_tokens, 50);
    }

    #[test]
    fn test_allocate_zero_budget() {
        let config = default_config();
        let quotas = allocate_quotas(0, "balanced", &config);
        assert_eq!(quotas["identity"].max_tokens, 0);
    }

    // ── Phase C-5 新增测试 ──

    #[test]
    fn test_allocate_balanced_with_skills() {
        let config = default_config();
        let quotas = allocate_quotas(10_000, "balanced", &config);
        // 5 个 key 都存在且 skills.max_tokens > 0
        assert_eq!(quotas.len(), 5);
        assert!(quotas.contains_key("skills"));
        assert!(quotas["skills"].max_tokens > 0, "skills 应分配到非 0 配额");
        // 验证按 0.05 比例
        assert_eq!(quotas["skills"].max_tokens, (10_000_f64 * 0.05) as usize);
    }

    #[test]
    fn test_allocate_balanced_sum_not_exceed_budget() {
        // 10000 token budget 下，每个 quota 不应超过 provider 配置的 max_tokens 上限
        let config = default_config();
        let quotas = allocate_quotas(10_000, "balanced", &config);
        for (name, q) in &quotas {
            let provider_max = config
                .providers
                .get(name)
                .map(|c| c.max_tokens)
                .unwrap_or(usize::MAX);
            assert!(
                q.max_tokens <= provider_max,
                "provider={} 的 quota.max_tokens={} 超过 provider max_tokens={}",
                name,
                q.max_tokens,
                provider_max
            );
        }
        // 验证 5 个 provider 配额总和不超过 budget
        let total: usize = quotas.values().map(|q| q.max_tokens).sum();
        assert!(
            total <= 10_000,
            "balanced sum={} 不应超过 budget 10000",
            total
        );
    }
}
