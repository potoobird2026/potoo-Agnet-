/*! BudgetCalculator（设计文档 §7.1）*/

use crate::shared_types::assembler::AssemblerConfig;

/// 三层预算计算结果（设计文档 §7.1）
#[allow(dead_code)]
pub struct Budget {
    pub context_window: usize,
    pub system_overhead: usize,
    pub tools_tokens: usize,
    pub response_reserve: usize,
    pub total_available: usize,
    pub history_budget: usize,
}

/// 纯函数，无状态（设计文档 §7.1）
pub fn compute_budget(
    context_window: usize,
    tools_tokens: usize,
    _history_tokens: usize,
    config: &AssemblerConfig,
) -> Budget {
    let system_overhead = 500;
    let response_reserve = (context_window as f64 * config.response_reserve_ratio) as usize;
    let total_available = context_window
        .saturating_sub(system_overhead)
        .saturating_sub(tools_tokens)
        .saturating_sub(response_reserve);
    let history_budget = (total_available as f64 * config.history_budget_ratio) as usize;
    Budget {
        context_window,
        system_overhead,
        tools_tokens,
        response_reserve,
        total_available,
        history_budget,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> AssemblerConfig {
        AssemblerConfig::default()
    }

    #[test]
    fn test_budget_computation_normal_case() {
        let config = default_config();
        let budget = compute_budget(128_000, 500, 10_000, &config);
        assert_eq!(budget.context_window, 128_000);
        assert_eq!(budget.system_overhead, 500);
        assert_eq!(budget.tools_tokens, 500);
        let expected_reserve = (128_000_f64 * config.response_reserve_ratio) as usize;
        assert_eq!(budget.response_reserve, expected_reserve);
        let expected_available = 128_000usize
            .saturating_sub(500)
            .saturating_sub(500)
            .saturating_sub(expected_reserve);
        assert_eq!(budget.total_available, expected_available);
        let expected_history = (expected_available as f64 * config.history_budget_ratio) as usize;
        assert_eq!(budget.history_budget, expected_history);
    }

    #[test]
    fn test_budget_overflow_protection() {
        let config = default_config();
        let budget = compute_budget(1000, 2000, 500, &config);
        assert_eq!(budget.total_available, 0);
        assert_eq!(budget.history_budget, 0);
    }

    #[test]
    fn test_budget_zero_context_window() {
        let config = default_config();
        let budget = compute_budget(0, 0, 0, &config);
        assert_eq!(budget.total_available, 0);
    }

    #[test]
    fn test_budget_zero_response_reserve() {
        let mut config = default_config();
        config.response_reserve_ratio = 0.0;
        let budget = compute_budget(10_000, 200, 1000, &config);
        assert_eq!(budget.response_reserve, 0);
        assert!(budget.total_available > 0);
    }

    #[test]
    fn test_budget_custom_history_ratio() {
        let mut config = default_config();
        config.history_budget_ratio = 1.0;
        let budget = compute_budget(10_000, 0, 0, &config);
        assert_eq!(budget.history_budget, budget.total_available);
    }
}
