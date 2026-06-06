/*! Tools 配置 */
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "d10")]
    pub max_chain_attempts: usize,
    #[serde(default = "d5")]
    pub max_concurrent_tools: usize,
    #[serde(default = "d120")]
    pub default_timeout_secs: u64,
    #[serde(default = "dtrue")]
    pub circuit_breaker_enabled: bool,
    #[serde(default = "d5f")]
    pub circuit_breaker_max_failures: u32,
    #[serde(default = "d60")]
    pub circuit_breaker_cooldown_secs: u64,
    #[serde(default = "dtrue")]
    pub builtins_enabled: bool,
    #[serde(default = "default_tools_dir")]
    pub tools_dir: std::path::PathBuf,
}
fn d10() -> usize {
    10
}
fn d5() -> usize {
    5
}
fn d120() -> u64 {
    120
}
fn dtrue() -> bool {
    true
}
fn d5f() -> u32 {
    5
}
fn d60() -> u64 {
    60
}
fn default_tools_dir() -> std::path::PathBuf {
    dirs::data_dir()
        .unwrap_or_default()
        .join("potoobird")
        .join("tools")
}
impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            max_chain_attempts: 10,
            max_concurrent_tools: 5,
            default_timeout_secs: 120,
            circuit_breaker_enabled: true,
            circuit_breaker_max_failures: 5,
            circuit_breaker_cooldown_secs: 60,
            builtins_enabled: true,
            tools_dir: default_tools_dir(),
        }
    }
}
impl ToolsConfig {
    pub fn resolve_paths(&mut self) {
        let home = dirs::home_dir().unwrap_or_default();
        if let Some(s) = self.tools_dir.to_str() {
            if let Some(stripped) = s.strip_prefix("~/") {
                self.tools_dir = home.join(stripped);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_values() {
        let c = ToolsConfig::default();
        assert_eq!(c.max_chain_attempts, 10);
        assert_eq!(c.max_concurrent_tools, 5);
        assert_eq!(c.default_timeout_secs, 120);
        assert!(c.circuit_breaker_enabled);
        assert_eq!(c.circuit_breaker_max_failures, 5);
        assert_eq!(c.circuit_breaker_cooldown_secs, 60);
    }

    #[test]
    fn test_deserialize_empty() {
        let c: ToolsConfig = serde_json::from_value(serde_json::json!({})).unwrap();
        assert_eq!(c.max_chain_attempts, 10);
        assert!(c.circuit_breaker_enabled);
    }

    #[test]
    fn test_deserialize_custom() {
        let c: ToolsConfig = serde_json::from_value(serde_json::json!({
            "max_chain_attempts": 20,
            "circuit_breaker_enabled": false
        }))
        .unwrap();
        assert_eq!(c.max_chain_attempts, 20);
        assert!(!c.circuit_breaker_enabled);
    }
}
