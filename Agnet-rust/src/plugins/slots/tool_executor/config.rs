use serde::{Deserialize, Serialize};

/// 工具调用默认超时秒数
pub const DEFAULT_TOOL_TIMEOUT_SECS: u64 = 30;
/// 用户确认默认超时秒数
pub const DEFAULT_CONFIRMATION_TIMEOUT_SECS: u64 = 60;
/// 熔断默认阈值（连续失败次数）
pub const DEFAULT_CIRCUIT_BREAKER_THRESHOLD: u32 = 5;
/// 熔断恢复默认秒数
pub const DEFAULT_CIRCUIT_BREAKER_RESET_SECS: u64 = 60;
/// 日志前缀
pub const LOG_PREFIX: &str = "tool_executor:";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ToolExecutorConfig {
    /// 工具调用超时秒数，默认 DEFAULT_TOOL_TIMEOUT_SECS
    #[serde(default = "default_tool_timeout_secs")]
    pub timeout_secs: u64,

    /// 是否启用用户确认流程，默认 false
    #[serde(default)]
    pub require_confirmation: bool,

    /// 确认超时秒数，默认 DEFAULT_CONFIRMATION_TIMEOUT_SECS
    #[serde(default = "default_confirmation_timeout_secs")]
    pub confirmation_timeout_secs: u64,

    /// 是否启用安全策略检查，默认 false
    #[serde(default)]
    pub enable_security_policy: bool,

    /// 熔断阈值：连续失败 N 次后熔断，默认 DEFAULT_CIRCUIT_BREAKER_THRESHOLD
    #[serde(default = "default_circuit_breaker_threshold")]
    pub circuit_breaker_threshold: u32,

    /// 熔断恢复时间（秒），默认 DEFAULT_CIRCUIT_BREAKER_RESET_SECS
    #[serde(default = "default_circuit_breaker_reset_secs")]
    pub circuit_breaker_reset_secs: u64,
}

impl Default for ToolExecutorConfig {
    fn default() -> Self {
        Self {
            timeout_secs: DEFAULT_TOOL_TIMEOUT_SECS,
            require_confirmation: false,
            confirmation_timeout_secs: DEFAULT_CONFIRMATION_TIMEOUT_SECS,
            enable_security_policy: false,
            circuit_breaker_threshold: DEFAULT_CIRCUIT_BREAKER_THRESHOLD,
            circuit_breaker_reset_secs: DEFAULT_CIRCUIT_BREAKER_RESET_SECS,
        }
    }
}

fn default_tool_timeout_secs() -> u64 {
    DEFAULT_TOOL_TIMEOUT_SECS
}
fn default_confirmation_timeout_secs() -> u64 {
    DEFAULT_CONFIRMATION_TIMEOUT_SECS
}
fn default_circuit_breaker_threshold() -> u32 {
    DEFAULT_CIRCUIT_BREAKER_THRESHOLD
}
fn default_circuit_breaker_reset_secs() -> u64 {
    DEFAULT_CIRCUIT_BREAKER_RESET_SECS
}
