//! LlmService 主模块
//!
//! 设计文档 §2.2-§2.3：LlmService 主 struct + ServicePlugin impl + LlmContract impl
//!
//! 持有：
//! - reqwest::Client（共享 HTTP 连接池）
//! - ConfigHolder（LLM 配置：api_key, base_url, model 等）
//! - AtomicBool 运行/暂停标志
//! - ChatInvoker（调用编排器）

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::Client;

use super::chat::ChatInvoker;
use super::config::ConfigHolder;
use crate::core::access::ServiceAccessPoint;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::llm::{
    ChatResponse, LlmConfig, LlmContract, LlmError, LlmFormatAdapter, LlmPublicConfig,
    PROVIDER_LLM, PROVIDER_LLM_FORMAT_ADAPTER, PROVIDER_LLM_OUTPUT_ADAPTER,
};
use crate::shared_types::{
    ContentBlock, DynProvider, Message, MessageRole, Thought, ToolDefinition,
};

/// 日志前缀（AI 宪法 Rule 3g：统一日志前缀常量，防散落）
const LOG_PREFIX: &str = "[llm_service]";

/// LlmService — LLM 调用服务
pub struct LlmService {
    client: Client,
    config: Arc<RwLock<ConfigHolder>>,
    running: AtomicBool,
    suspended: AtomicBool,
    #[allow(dead_code)]
    invoker: ChatInvoker,
}

impl LlmService {
    /// Create a new LlmService with default configuration.
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            config: Arc::new(RwLock::new(ConfigHolder::default())),
            running: AtomicBool::new(false),
            suspended: AtomicBool::new(false),
            invoker: ChatInvoker::new(),
        }
    }
}

impl Default for LlmService {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServicePlugin for LlmService {
    fn name(&self) -> &str {
        "llm"
    }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // Parse LlmConfig from plugin_config["llm"]
        let llm_cfg = ctx
            .plugin_config
            .get("llm")
            .ok_or_else(|| PluginError::config("Missing 'llm' configuration section"))?;

        let config: LlmConfig = serde_json::from_value(llm_cfg.clone())
            .map_err(|e| PluginError::config(format!("Failed to parse LlmConfig: {e}")))?;

        // Validate required fields
        if config.model.is_empty() {
            return Err(PluginError::config("LlmConfig.model is required"));
        }
        if config.base_url.is_empty() {
            return Err(PluginError::config("LlmConfig.base_url is required"));
        }

        // Build HTTP client
        let timeout = config.timeout;
        let client = Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|e| PluginError::init_failed(format!("Failed to build HTTP client: {e}")))?;

        self.client = client;
        *self.config.write().unwrap_or_else(|e| e.into_inner()) = ConfigHolder::new(config);
        self.running.store(true, Ordering::SeqCst);

        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        // Register PROVIDER_LLM — wrap existing LlmService fields for type-erased storage
        let contract: Arc<dyn LlmContract> = Arc::new(LlmContractWrapper {
            client: self.client.clone(),
            config: self.config.clone(),
            invoker: ChatInvoker::new(), // ChatInvoker 无状态，可复用构造
        });
        ap.register_provider(PROVIDER_LLM, Arc::new(DynProvider(contract)));

        // Register PROVIDER_LLM_FORMAT_ADAPTER + PROVIDER_LLM_OUTPUT_ADAPTER
        let adapter: Arc<dyn LlmFormatAdapter> = Arc::new(DefaultFormatAdapter);
        ap.register_provider(
            PROVIDER_LLM_FORMAT_ADAPTER,
            Arc::new(DynProvider(adapter.clone())),
        );
        ap.register_provider(PROVIDER_LLM_OUTPUT_ADAPTER, Arc::new(DynProvider(adapter)));

        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::GracefulShutdown => {
                self.running.store(false, Ordering::SeqCst);
                self.suspended.store(true, Ordering::SeqCst);
            }
            ServiceSignal::ImmediateShutdown => {
                self.running.store(false, Ordering::SeqCst);
                self.suspended.store(true, Ordering::SeqCst);
            }
            ServiceSignal::ConfigReload => {
                // Config reload handled by core; LlmService re-reads on next request
                tracing::info!("{LOG_PREFIX} received ConfigReload signal");
            }
            ServiceSignal::HealthCheck => {
                // Health check — respond with current state
                let running = self.running.load(Ordering::SeqCst);
                let suspended = self.suspended.load(Ordering::SeqCst);
                tracing::info!(
                    "{LOG_PREFIX} health check: running={running}, suspended={suspended}"
                );
            }
            ServiceSignal::Suspend => {
                self.suspended.store(true, Ordering::SeqCst);
            }
            ServiceSignal::Resume => {
                self.suspended.store(false, Ordering::SeqCst);
            }
        }
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        self.running.store(false, Ordering::SeqCst);
        self.suspended.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        self.running.store(false, Ordering::SeqCst);
        // Drop old client and rebuild with short timeout for draining
        self.client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| {
                PluginError::internal(format!("Failed to rebuild client during shutdown: {e}"))
            })?;
        Ok(())
    }
}

// ─── LlmContract implementation ─────────────────────────────────────────

/// Wrapper holding cloned LlmService fields for LlmContract registration.
/// Uses DynProvider for type-erased storage per shared_types契约协议 §4.
struct LlmContractWrapper {
    #[allow(dead_code)]
    client: Client,
    config: Arc<RwLock<ConfigHolder>>,
    invoker: ChatInvoker,
}

#[async_trait]
impl LlmContract for LlmContractWrapper {
    async fn chat(
        &self,
        config: Option<LlmConfig>,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError> {
        let base_config = self.config.read().unwrap_or_else(|e| e.into_inner()).get();
        let effective_config = if let Some(overrides) = config {
            LlmConfig {
                provider: overrides.provider,
                base_url: if overrides.base_url.is_empty() {
                    base_config.base_url
                } else {
                    overrides.base_url
                },
                api_key: overrides.api_key.or(base_config.api_key),
                model: if overrides.model.is_empty() {
                    base_config.model
                } else {
                    overrides.model
                },
                max_tokens: overrides.max_tokens.or(base_config.max_tokens),
                temperature: overrides.temperature.or(base_config.temperature),
                top_p: overrides.top_p.or(base_config.top_p),
                stop: overrides.stop.or(base_config.stop),
                frequency_penalty: overrides
                    .frequency_penalty
                    .or(base_config.frequency_penalty),
                presence_penalty: overrides.presence_penalty.or(base_config.presence_penalty),
                seed: overrides.seed.or(base_config.seed),
                timeout: overrides.timeout,
                idle_timeout: overrides.idle_timeout.or(base_config.idle_timeout),
                stream: overrides.stream,
                tools_enabled: overrides.tools_enabled,
                multimodal: overrides.multimodal,
                max_retries: overrides.max_retries,
                retry_backoff: overrides.retry_backoff,
                context_window: overrides.context_window,
                extra_headers: if overrides.extra_headers.is_empty() {
                    base_config.extra_headers
                } else {
                    overrides.extra_headers
                },
                auth_mode: overrides.auth_mode.or(base_config.auth_mode),
                enable_tracing: overrides.enable_tracing,
            }
        } else {
            base_config
        };

        self.invoker
            .invoke(&effective_config, messages, tools, trace_id)
            .await
    }

    fn get_public_config(&self) -> LlmPublicConfig {
        let cfg = self.config.read().unwrap_or_else(|e| e.into_inner()).get();
        LlmPublicConfig {
            provider: cfg.provider,
            base_url: cfg.base_url,
            model: cfg.model,
            stream: cfg.stream,
            max_tokens: cfg.max_tokens,
        }
    }
}

// ─── DefaultFormatAdapter ────────────────────────────────────────────

/// Default implementation of LlmFormatAdapter.
struct DefaultFormatAdapter;

#[async_trait]
impl LlmFormatAdapter for DefaultFormatAdapter {
    fn format_system_prompt(&self, thought: &Thought) -> String {
        match thought {
            Thought::Final { answer, .. } => answer.clone(),
            Thought::Action { action, .. } => {
                format!(
                    "Tool call: {}({})",
                    action.tool_name,
                    serde_json::to_string(&action.arguments).unwrap_or_default()
                )
            }
        }
    }

    fn format_assistant_message(&self, thought: &Thought) -> Message {
        match thought {
            Thought::Final { answer, .. } => Message {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::text(answer)],
                tool_calls: None,
                tool_call_id: None,
                reasoning: None,
                metadata: None,
                created_at: crate::core::types::Timestamp::now(),
            },
            Thought::Action { action, .. } => {
                let tool_calls: Vec<crate::shared_types::ToolCall> =
                    action.tool_calls.clone().unwrap_or_default();
                Message {
                    role: MessageRole::Assistant,
                    content: vec![],
                    tool_calls: Some(tool_calls),
                    tool_call_id: None,
                    reasoning: None,
                    metadata: None,
                    created_at: crate::core::types::Timestamp::now(),
                }
            }
        }
    }
}
