/*! AssemblyReport 可观测数据结构（设计文档 §3.3） */

use std::time::Duration;

/// 组装报告（设计文档 §3.3）
#[derive(Debug, Clone)]
pub struct AssemblyReport {
    pub request_id: String,
    pub session_id: String,
    pub context_window: usize,
    pub total_available: usize,
    pub history_tokens: usize,
    pub injection_budget: usize,
    pub final_total_tokens: usize,
    pub selected_policy: String,
    pub provider_stats: Vec<ProviderStat>,
    pub rules_group: String,
    pub adapter_used: Option<String>,
    pub truncation_applied: bool,
    pub warnings: Vec<AssemblyWarning>,
    pub assembly_duration: Duration,
}

/// Provider 执行统计（设计文档 §3.3）
#[derive(Debug, Clone)]
pub struct ProviderStat {
    pub name: String,
    pub priority: u8,
    pub tokens_used: usize,
    pub blocks_count: usize,
    pub success: bool,
    pub error: Option<String>,
}

/// 组装警告（设计文档 §3.3）
#[derive(Debug, Clone)]
pub struct AssemblyWarning {
    pub code: String,
    pub message: String,
}
