/*! ToolRegistry —— 工具注册与执行引擎 */
use super::circuit_breaker::{CircuitBreaker, CircuitBreakerState};
use super::platform::NativePlatform;
use crate::shared_types::{ToolDefinition, ToolError, ToolProvider, ToolSource};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

const _MAX_CHAIN_ATTEMPTS: usize = 10;
const LOG_PREFIX: &str = "[tools]";

pub struct ToolRegistry {
    tools: Mutex<HashMap<String, Arc<ToolDefinition>>>,
    breakers: Mutex<HashMap<String, CircuitBreaker>>,
    /// Provider handles 供 mcp: entry 委托执行（A-3）
    provider_handles: Mutex<HashMap<String, Arc<dyn ToolProvider>>>,
    platform: NativePlatform,
    breaker_enabled: bool,
    max_failures: u32,
    cooldown_secs: u64,
}

impl ToolRegistry {
    pub fn new(
        platform: NativePlatform,
        breaker_enabled: bool,
        max_failures: u32,
        cooldown_secs: u64,
    ) -> Self {
        Self {
            tools: Mutex::new(HashMap::new()),
            breakers: Mutex::new(HashMap::new()),
            provider_handles: Mutex::new(HashMap::new()),
            platform,
            breaker_enabled,
            max_failures,
            cooldown_secs,
        }
    }

    pub fn register(&self, def: ToolDefinition) {
        let name = def.name.clone();
        self.tools
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(name.clone(), Arc::new(def));
        tracing::info!("{} 已注册工具: {}", LOG_PREFIX, name);
    }

    /// register_builtin 的内部 helper：构造 ToolDefinition 后调 register
    pub fn register_builtin(&self, name: &str, description: &str, parameters: Value, entry: &str) {
        let def = ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            parameters,
            entry: entry.to_string(),
            source: ToolSource::Builtin,
        };
        self.tools
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(name.to_string(), Arc::new(def));
    }

    /// A-3: 注册外部 provider handle（mcp: 等）
    pub fn register_provider(&self, provider_id: &str, provider: Arc<dyn ToolProvider>) {
        self.provider_handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(provider_id.to_string(), provider);
        tracing::info!("{} 已注册 provider: {}", LOG_PREFIX, provider_id);
    }

    /// A-3: 按 provider_id 查找
    pub fn get_provider(&self, provider_id: &str) -> Option<Arc<dyn ToolProvider>> {
        self.provider_handles
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(provider_id)
            .cloned()
    }

    pub fn get(&self, name: &str) -> Option<Arc<ToolDefinition>> {
        self.tools
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
    }

    pub fn list(&self) -> Vec<Arc<ToolDefinition>> {
        self.tools
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .cloned()
            .collect()
    }

    pub fn tool_count(&self) -> usize {
        self.tools.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn check_circuit_breaker(&self, tool_name: &str) -> Result<(), CircuitBreakerState> {
        if !self.breaker_enabled {
            return Ok(());
        }
        let mut breakers = self.breakers.lock().unwrap_or_else(|e| e.into_inner());
        let cb = breakers
            .entry(tool_name.to_string())
            .or_insert_with(|| CircuitBreaker::new(self.max_failures, self.cooldown_secs));
        if cb.is_open() {
            Err(cb.state())
        } else {
            Ok(())
        }
    }

    pub fn record_success(&self, tool_name: &str) {
        if !self.breaker_enabled {
            return;
        }
        self.breakers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(tool_name.to_string())
            .or_insert_with(|| CircuitBreaker::new(self.max_failures, self.cooldown_secs))
            .record_success();
    }

    pub fn record_failure(&self, tool_name: &str) {
        if !self.breaker_enabled {
            return;
        }
        self.breakers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(tool_name.to_string())
            .or_insert_with(|| CircuitBreaker::new(self.max_failures, self.cooldown_secs))
            .record_failure();
    }
}

#[async_trait::async_trait]
impl ToolProvider for ToolRegistry {
    fn list(&self) -> Vec<ToolDefinition> {
        self.tools
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|arc| (**arc).clone())
            .collect()
    }

    /// A-3: provider_id 覆写为 "tools"（标识 ToolsService 持有的本地 tool 集合）
    fn provider_id(&self) -> &str {
        "tools"
    }

    async fn execute(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
        timeout: std::time::Duration,
    ) -> Result<String, ToolError> {
        let def = self
            .get(tool_name)
            .ok_or_else(|| ToolError::NotFound(tool_name.to_string()))?;

        match def.entry.as_str() {
            "execute_command" => {
                let result = crate::plugins::services::tools::builtins::execute_command::execute(
                    arguments,
                    &self.platform,
                )
                .await
                .map_err(ToolError::ExecutionFailed)?;
                Ok(result.to_string())
            }
            "read_file" => {
                let result =
                    crate::plugins::services::tools::builtins::read_file::execute(arguments)
                        .await
                        .map_err(ToolError::ExecutionFailed)?;
                Ok(result.to_string())
            }
            "write_file" => {
                let result =
                    crate::plugins::services::tools::builtins::write_file::execute(arguments)
                        .await
                        .map_err(ToolError::ExecutionFailed)?;
                Ok(result.to_string())
            }
            "search_memory" => {
                let result =
                    crate::plugins::services::tools::builtins::search_memory::execute(arguments)
                        .await
                        .map_err(ToolError::ExecutionFailed)?;
                Ok(result.to_string())
            }
            // A-3: MCP 工具委托给 provider_handles 中注册的 McpToolProxy
            s if s.starts_with("mcp:") => {
                let provider_id = s;
                match self.get_provider(provider_id) {
                    Some(provider) => provider.execute(tool_name, arguments, timeout).await,
                    None => Err(ToolError::NotFound(format!(
                        "MCP provider {} 未注册",
                        provider_id
                    ))),
                }
            }
            _ => Err(ToolError::ExecutionFailed(format!(
                "未知的工具入口: {}",
                def.entry
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared_types::ToolSource;

    fn make_registry() -> ToolRegistry {
        ToolRegistry::new(NativePlatform::new(60), true, 3, 10)
    }

    fn make_def(name: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: format!("desc {name}"),
            parameters: serde_json::json!({}),
            entry: "builtin".to_string(),
            source: ToolSource::Builtin,
        }
    }

    #[test]
    fn test_register_and_get() {
        let r = make_registry();
        r.register(make_def("foo"));
        assert!(r.get("foo").is_some());
        assert_eq!(r.get("foo").unwrap().name, "foo");
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let r = make_registry();
        assert!(r.get("nope").is_none());
    }

    #[test]
    fn test_list_and_count() {
        let r = make_registry();
        r.register(make_def("a"));
        r.register(make_def("b"));
        assert_eq!(r.tool_count(), 2);
        assert_eq!(r.list().len(), 2);
    }

    #[test]
    fn test_register_builtin() {
        let r = make_registry();
        r.register_builtin("test_tool", "desc", serde_json::json!({}), "builtin");
        assert!(r.get("test_tool").is_some());
    }

    #[test]
    fn test_circuit_breaker_disabled() {
        let r = ToolRegistry::new(NativePlatform::new(60), false, 3, 10);
        // 即使失败多次也不应报错
        r.record_failure("x");
        r.record_failure("x");
        r.record_failure("x");
        assert!(r.check_circuit_breaker("x").is_ok());
    }

    #[test]
    fn test_circuit_breaker_opens_after_failures() {
        let r = make_registry();
        r.record_failure("t");
        r.record_failure("t");
        r.record_failure("t");
        assert!(r.check_circuit_breaker("t").is_err());
    }

    #[test]
    fn test_circuit_breaker_resets_on_success() {
        let r = make_registry();
        r.record_failure("t");
        r.record_failure("t");
        r.record_success("t");
        assert!(r.check_circuit_breaker("t").is_ok());
    }
}
