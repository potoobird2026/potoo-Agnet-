use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 熔断状态——跨 run() 存入 StepContext（S-R03 合规）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CircuitBreakerState {
    failures: HashMap<String, u32>,
}

impl CircuitBreakerState {
    pub fn record_failure(&mut self, tool_name: &str) {
        *self.failures.entry(tool_name.to_string()).or_insert(0) += 1;
    }

    pub fn record_success(&mut self, tool_name: &str) {
        self.failures.remove(tool_name);
    }

    pub fn failure_count(&self, tool_name: &str) -> u32 {
        self.failures.get(tool_name).copied().unwrap_or(0)
    }
}
