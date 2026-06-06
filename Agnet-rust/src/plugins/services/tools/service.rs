/*! ToolsService —— 工具服务 ServicePlugin */
use super::config::ToolsConfig;
use super::discover::ToolDiscover;
use super::platform::NativePlatform;
use super::registry::ToolRegistry;
use crate::core::access::ServiceAccessPoint;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::{
    DynProvider, McpBundle, ToolDefinition, ToolProvider, PROVIDER_MCP_TOOLS, PROVIDER_TOOL,
};
use async_trait::async_trait;
use std::sync::Arc;

pub struct ToolsService {
    config: Option<ToolsConfig>,
    registry: Arc<ToolRegistry>,
    running: bool,
}

impl ToolsService {
    pub fn new() -> Self {
        Self {
            config: None,
            registry: Arc::new(ToolRegistry::new(NativePlatform::new(120), true, 5, 60)),
            running: false,
        }
    }
}

#[async_trait]
impl ServicePlugin for ToolsService {
    fn name(&self) -> &str {
        "tools"
    }
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let mut config: ToolsConfig = serde_json::from_value(ctx.plugin_config.clone())
            .map_err(|e| PluginError::Config(format!("tools: 配置解析失败: {}", e)))?;
        config.resolve_paths();
        let platform = NativePlatform::new(config.default_timeout_secs);
        let registry = ToolRegistry::new(
            platform,
            config.circuit_breaker_enabled,
            config.circuit_breaker_max_failures,
            config.circuit_breaker_cooldown_secs,
        );

        // 注册内建工具
        if config.builtins_enabled {
            registry.register_builtin(
                super::builtins::read_file::NAME,
                super::builtins::read_file::DESCRIPTION,
                super::builtins::read_file::parameters(),
                "builtin",
            );
            registry.register_builtin(
                super::builtins::write_file::NAME,
                super::builtins::write_file::DESCRIPTION,
                super::builtins::write_file::parameters(),
                "builtin",
            );
            registry.register_builtin(
                super::builtins::execute_command::NAME,
                super::builtins::execute_command::DESCRIPTION,
                super::builtins::execute_command::parameters(),
                "builtin",
            );
            registry.register_builtin(
                super::builtins::search_memory::NAME,
                super::builtins::search_memory::DESCRIPTION,
                super::builtins::search_memory::parameters(),
                "builtin",
            );
        }

        // 扫描已安装工具——ToolManifest → ToolDefinition 转换（registry.register 收 ToolDefinition）
        let discover = ToolDiscover::new(config.tools_dir.clone());
        for manifest in discover.discover() {
            let def = ToolDefinition {
                name: manifest.name,
                description: manifest.description,
                parameters: manifest.parameters,
                entry: manifest.entry,
                source: manifest.source,
            };
            registry.register(def);
        }

        self.registry = Arc::new(registry);
        self.config = Some(config);
        tracing::info!(
            "ToolsService: 初始化完成，已注册 {} 个工具",
            self.registry.tool_count()
        );
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        self.running = true;

        // C-1: 拉取 McpBundle（若 McpService 未启动或未注册，则跳过）
        let mcp_tools: Vec<Arc<dyn ToolProvider>> = match ap.provider_raw(PROVIDER_MCP_TOOLS) {
            Some(raw) => match raw.downcast::<DynProvider<dyn McpBundle>>() {
                Ok(wrapper) => wrapper.0.all(),
                Err(_) => Vec::new(),
            },
            None => Vec::new(),
        };
        if !mcp_tools.is_empty() {
            tracing::info!("ToolsService: 拉取到 {} 个 MCP 工具代理", mcp_tools.len());
            for provider in mcp_tools {
                let defs = provider.list();
                // 一个 McpToolProxy 暴露一个 tool——取第一个 def 注册到 registry
                if let Some(def) = defs.into_iter().next() {
                    self.registry
                        .register_provider(&def.entry, provider.clone());
                    self.registry.register(def);
                }
            }
        }

        ap.register_provider(
            PROVIDER_TOOL,
            Arc::new(DynProvider(self.registry.clone() as Arc<dyn ToolProvider>)),
        );
        Ok(())
    }
    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::HealthCheck => Ok(()),
            _ => {
                self.running = signal != ServiceSignal::GracefulShutdown
                    && signal != ServiceSignal::ImmediateShutdown;
                Ok(())
            }
        }
    }
    async fn stop(&mut self) -> Result<(), PluginError> {
        self.running = false;
        Ok(())
    }
    async fn shutdown(&mut self) -> Result<(), PluginError> {
        self.config = None;
        Ok(())
    }
}
impl Default for ToolsService {
    fn default() -> Self {
        Self::new()
    }
}
