use serde::{Deserialize, Serialize};

use crate::shared_types::RiskSeverity;

pub const DEFAULT_AUDIT_LOG_CAPACITY: usize = 1000;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AuditPhaseConfig {
    #[serde(default = "default_true")]
    pub enable_security_check: bool,

    #[serde(default = "default_true")]
    pub enable_sensitive_detection: bool,

    #[serde(default = "default_sensitive_rules")]
    pub sensitive_rules: Vec<SensitiveRuleConfig>,

    #[serde(default = "default_high_risk_tools")]
    pub high_risk_tools: Vec<String>,

    #[serde(default = "default_medium_risk_tools")]
    pub medium_risk_tools: Vec<String>,

    #[serde(default = "default_high_risk_action")]
    pub high_risk_action: RiskAction,

    #[serde(default = "default_true")]
    pub enable_audit_log: bool,

    #[serde(default = "default_audit_log_capacity")]
    pub audit_log_capacity: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub enum RiskAction {
    Block,
    Warn,
    LogOnly,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct SensitiveRuleConfig {
    pub name: String,
    pub pattern: String,
    pub description: String,
    pub severity: RiskSeverity,
}

fn default_true() -> bool {
    true
}

fn default_high_risk_action() -> RiskAction {
    RiskAction::Block
}

fn default_audit_log_capacity() -> usize {
    DEFAULT_AUDIT_LOG_CAPACITY
}

fn default_sensitive_rules() -> Vec<SensitiveRuleConfig> {
    vec![
        SensitiveRuleConfig {
            name: "api_key".to_string(),
            pattern: r#"(?i)(api[_-]?key|apikey|access[_-]?key|secret[_-]?key)\s*[:=]\s*['"]?[a-zA-Z0-9_\-]{16,}['"]?"#.to_string(),
            description: "API 密钥泄露".to_string(),
            severity: RiskSeverity::High,
        },
        SensitiveRuleConfig {
            name: "password".to_string(),
            pattern: r#"(?i)(password|passwd|pwd)\s*[:=]\s*['"]?[^\s'"]{8,}['"]?"#.to_string(),
            description: "密码泄露".to_string(),
            severity: RiskSeverity::High,
        },
        SensitiveRuleConfig {
            name: "private_key".to_string(),
            pattern: r"-----BEGIN (RSA |EC |DSA |OPENSSH )?PRIVATE KEY-----".to_string(),
            description: "私钥泄露".to_string(),
            severity: RiskSeverity::Critical,
        },
    ]
}

fn default_high_risk_tools() -> Vec<String> {
    vec![
        "execute_command".to_string(),
        "write_file".to_string(),
        "delete_file".to_string(),
        "http_request".to_string(),
    ]
}

fn default_medium_risk_tools() -> Vec<String> {
    vec![
        "read_file".to_string(),
        "list_directory".to_string(),
        "search_web".to_string(),
    ]
}
