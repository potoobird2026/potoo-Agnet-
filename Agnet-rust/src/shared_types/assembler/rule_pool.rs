/*! RulePoolConfig 规则池配置（设计文档 §3.6） */

/// 规则池配置（设计文档 §3.6）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct RulePoolConfig {
    pub enabled: bool,
    pub llm_name: String,
    pub rules_file: String,
    pub selection_timeout_ms: u64,
    pub fallback_enabled: bool,
    pub l3_rules: L3RulesConfig,
}

impl Default for RulePoolConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            llm_name: "secondary".into(),
            rules_file: String::new(),
            selection_timeout_ms: 5000,
            fallback_enabled: false,
            l3_rules: L3RulesConfig {
                enabled: false,
                max_items: 3,
                query_template: "{user_text} 行业经验教训".into(),
            },
        }
    }
}

/// L3 规则来源配置（设计文档 §3.6）
#[derive(Debug, Clone, serde::Deserialize)]
pub struct L3RulesConfig {
    pub enabled: bool,
    pub max_items: usize,
    pub query_template: String,
}

/// 规则组（设计文档 §3.6）
#[derive(Debug, Clone)]
pub struct RuleGroup {
    pub name: String,
    pub rules: Vec<String>,
}

impl RuleGroup {
    pub fn empty() -> Self {
        Self {
            name: "empty".into(),
            rules: vec![],
        }
    }
}
