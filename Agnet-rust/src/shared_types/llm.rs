//! LLM 调用服务跨插件契约
//!
//! 本文件定义 LlmService（服务方）与 LlmThinkerSlot/Assembler（消费方）之间的共享契约。
//!
//! 协议依据：
//! - protocol-shared_types契约协议.md §1-§7
//! - protocol-Service集成协议.md §2.2（Provider 注册）
//!
//! 包含：
//! - Provider key 常量（K-R01）
//! - Provider trait（T-R01）：LlmContract, LlmFormatAdapter
//! - 跨插件数据结构：LlmConfig, ChatResponse, StreamEvent, LlmError, ProviderKind, RetryBackoff
//! - 默认超时常量（跨平台规范 §1）：DEFAULT_TIMEOUT, DEFAULT_IDLE_TIMEOUT
//!
//! 迁移自：src/plugins/slots/llm_thinker/types.rs（设计文档 §1.2）

use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::UnboundedReceiver;

use super::{Message, Thought, ToolDefinition};

// ── Provider key 常量（shared_types契约协议 §2） ──────────────────────

/// LLM 对话能力——由 LlmService 注册，LlmThinkerSlot/RuleLlmSelector 消费
pub const PROVIDER_LLM: &str = "llm";

/// 厂商格式适配器——由 LlmService 注册，AssemblerSlot 消费
/// 与 Assembler 的 LlmOutputAdapter（上下文排版优化）职责不同
pub const PROVIDER_LLM_FORMAT_ADAPTER: &str = "llm_format_adapter";

/// 厂商输出适配器——由 LlmService 注册，AssemblerSlot 消费
pub const PROVIDER_LLM_OUTPUT_ADAPTER: &str = "llm_output_adapter";

// ── Provider trait（shared_types契约协议 §3） ─────────────────────────

/// LLM 调用契约（shared_types契约协议 §3.1）
/// 服务方：LlmService 实现此 trait
/// 消费方：LlmThinkerSlot/RuleLlmSelector 通过 provider_raw(PROVIDER_LLM) 调用
#[async_trait]
pub trait LlmContract: Send + Sync {
    /// 发送聊天请求
    /// - config: 调用级别配置覆盖（传 None 使用服务默认配置）
    /// - messages: 消息历史
    /// - tools: 可用工具定义
    /// - trace_id: 追踪 ID
    async fn chat(
        &self,
        config: Option<LlmConfig>,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError>;

    /// 获取当前服务配置（部分字段，不含 api_key）
    fn get_public_config(&self) -> LlmPublicConfig;
}

/// 厂商格式适配器（shared_types契约协议 §3.1）
/// 服务方：LlmService 根据 provider 类型注册对应适配器
/// 消费方：AssemblerSlot 输出阶段调用
///
/// 注意：和 Assembler 的 `LlmOutputAdapter`（上下文排版优化）是不同概念，
/// 本 trait 负责厂商级消息格式转换（provider↔provider），而非组装级输出排版。
#[async_trait]
pub trait LlmFormatAdapter: Send + Sync {
    /// 将 Thought 格式化为目标厂商的 System Prompt 格式
    fn format_system_prompt(&self, thought: &Thought) -> String;
    /// 将 Thought 格式化为目标厂商的 Assistant 消息格式
    fn format_assistant_message(&self, thought: &Thought) -> Message;
}

// ── 默认超时常量（跨平台规范 §1） ────────────────────────────────────

/// 默认请求超时（设计文档 §3.1 LlmConfig、跨平台规范 §1 避免魔法数字）
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// 默认空闲连接超时
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

// ── 跨插件数据结构（shared_types契约协议 §1） ────────────────────────

/// 公开配置（不含 api_key）
#[derive(Debug, Clone, Serialize)]
pub struct LlmPublicConfig {
    pub provider: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub stream: bool,
    pub max_tokens: Option<u32>,
}

/// 提供商类型
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum ProviderKind {
    #[default]
    OpenAi,
    OpenAiCompatible,
    Anthropic,
    Ollama,
}

impl ProviderKind {
    /// 默认 base_url 值（始终可被 LlmConfig.base_url 覆盖，不违反跨平台规范 §1）
    pub fn default_base_url(&self) -> &str {
        match self {
            ProviderKind::OpenAi => "https://api.openai.com/v1",
            ProviderKind::OpenAiCompatible => "",
            ProviderKind::Anthropic => "https://api.anthropic.com",
            ProviderKind::Ollama => "http://localhost:11434",
        }
    }
}

/// 认证方式（设计文档 §3.1 auth_mode）
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AuthMode {
    /// x-api-key header（Anthropic 默认）
    XApiKey,
    /// Authorization: Bearer header（OpenAI 默认）
    Bearer,
    /// 无认证（Ollama 默认）
    None,
}

/// 重试退避策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryBackoff {
    /// 固定间隔重试
    Fixed(Duration),
    /// 指数退避：delay(n) = min(initial * 2^n, max)
    Exponential { initial: Duration, max: Duration },
}

impl Default for RetryBackoff {
    fn default() -> Self {
        RetryBackoff::Exponential {
            initial: Duration::from_secs(1),
            max: Duration::from_secs(30),
        }
    }
}

/// LLM 配置——服务方持有默认值，消费方传入覆盖
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// Provider kind (设计文档 §3.1)
    pub provider: ProviderKind,
    /// API base URL; 默认值见 ProviderKind::default_base_url()
    #[serde(default)]
    pub base_url: String,
    /// API 认证密钥 (设计文档 §3.1)
    pub api_key: Option<String>,
    /// 模型标识 (设计文档 §3.1)
    pub model: String,
    /// 响应最大 token 数 (设计文档 §3.1)
    pub max_tokens: Option<u32>,
    /// 采样温度 (设计文档 §3.1)
    pub temperature: Option<f32>,
    /// 核采样阈值 (设计文档 §3.1)
    pub top_p: Option<f32>,
    /// 停止序列 (设计文档 §3.1)
    pub stop: Option<Vec<String>>,
    /// 频率惩罚 (设计文档 §3.1)
    pub frequency_penalty: Option<f32>,
    /// 存在惩罚 (设计文档 §3.1)
    pub presence_penalty: Option<f32>,
    /// 随机种子 (设计文档 §3.1)
    pub seed: Option<i64>,
    /// 请求超时 (设计文档 §3.1)
    #[serde(default = "default_timeout")]
    pub timeout: Duration,
    /// 空闲连接超时 (设计文档 §3.1)
    pub idle_timeout: Option<Duration>,
    /// 是否启用流式 (设计文档 §3.1)
    #[serde(default)]
    pub stream: bool,
    /// 是否启用工具调用 (设计文档 §3.1)
    #[serde(default = "default_true")]
    pub tools_enabled: bool,
    /// 是否启用多模态输入 (设计文档 §3.1)
    #[serde(default)]
    pub multimodal: bool,
    /// 最大重试次数 (设计文档 §3.1)
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// 重试退避策略 (设计文档 §3.1)
    #[serde(default)]
    pub retry_backoff: RetryBackoff,
    /// 上下文窗口 token 限制 (设计文档 §3.1)
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    /// 额外 HTTP 头 (设计文档 §3.1)
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    /// 认证方式 (设计文档 §3.1); None = 使用 executor 默认
    #[serde(default)]
    pub auth_mode: Option<AuthMode>,
    /// 是否启用追踪日志 (设计文档 §3.1)
    #[serde(default)]
    pub enable_tracing: bool,
}

// ── LlmConfig 辅助函数 ───────────────────────────────────────────────

fn default_timeout() -> Duration {
    DEFAULT_TIMEOUT
}

fn default_true() -> bool {
    true
}

fn default_max_retries() -> u32 {
    3
}

fn default_context_window() -> u32 {
    128000
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: ProviderKind::default(),
            base_url: String::new(),
            api_key: None,
            model: String::new(),
            max_tokens: None,
            temperature: None,
            top_p: None,
            stop: None,
            frequency_penalty: None,
            presence_penalty: None,
            seed: None,
            timeout: default_timeout(),
            idle_timeout: None,
            stream: false,
            tools_enabled: true,
            multimodal: false,
            max_retries: default_max_retries(),
            retry_backoff: RetryBackoff::default(),
            context_window: default_context_window(),
            extra_headers: HashMap::new(),
            auth_mode: None,
            enable_tracing: false,
        }
    }
}

/// 主/备 LLM 配置对
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmPairConfig {
    pub primary: LlmConfig,
    pub backup: Option<LlmConfig>,
}

// ── LLM 错误类型（原 ThinkerError，重命名为 LlmError） ────────────────

/// LLM 调用错误（跨插件错误类型，设计文档 §3.3）
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    /// API 返回错误响应（设计文档 §3.3）
    #[error("[{trace_id}] {provider}/{model} API 错误 (HTTP {status:?}): {message}")]
    ApiError {
        provider: String,
        model: String,
        status: Option<u16>,
        message: String,
        trace_id: String,
        retryable: bool,
    },

    /// 请求超时
    #[error("[{trace_id}] 请求超时 ({timeout:?})")]
    Timeout { trace_id: String, timeout: Duration },

    /// 网络错误
    #[error("[{trace_id}] 网络错误: {source}")]
    NetworkError {
        trace_id: String,
        #[source]
        source: reqwest::Error,
    },

    /// 响应解析失败
    #[error("[{trace_id}] 响应解析失败: {raw_response}")]
    ParseError {
        trace_id: String,
        raw_response: String,
    },

    /// 流处理错误
    #[error("[{trace_id}] 流处理错误: {message}")]
    StreamError { trace_id: String, message: String },

    /// 配置错误（新增变体）
    #[error("配置错误: {0}")]
    ConfigError(String),
}

impl LlmError {
    /// 错误是否可重试（设计文档 §3.3）
    pub fn is_retryable(&self) -> bool {
        match self {
            LlmError::ApiError { retryable, .. } => *retryable,
            LlmError::Timeout { .. } => true,
            LlmError::NetworkError { .. } => true,
            LlmError::ParseError { .. } => false,
            LlmError::StreamError { .. } => false,
            LlmError::ConfigError(_) => false,
        }
    }

    /// 用户友好的错误建议
    pub fn suggestion(&self) -> String {
        match self {
            LlmError::ApiError { .. } => {
                "请检查 API key(API密钥) 和 base_url(基础URL) 是否正确".to_string()
            }
            LlmError::Timeout { timeout, .. } => {
                format!("请增加 timeout(超时) 配置值，当前为 {timeout:?}")
            }
            LlmError::NetworkError { .. } => {
                "请检查 base_url(基础URL) 是否可达，网络是否正常".to_string()
            }
            LlmError::ParseError { .. } => {
                "请检查模型是否支持 tool calling(工具调用) 格式".to_string()
            }
            LlmError::StreamError { .. } => {
                "请检查模型是否支持流式输出，或关闭 stream(流式) 配置".to_string()
            }
            LlmError::ConfigError(msg) => format!("配置错误: {msg}"),
        }
    }
}

// ── 流事件 ───────────────────────────────────────────────────────────

/// LLM 流事件——LlmService（生产者）构造，Slot（消费者）匹配
/// 设计文档 §1.2：移入 shared_types/llm.rs 避免 services→slots 反向依赖
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// 文本增量
    TextDelta(String),
    /// 工具调用增量
    ToolCallDelta {
        index: usize,
        delta: serde_json::Value,
    },
    /// 流结束，携带完整 Thought
    End(Thought),
}

// ── 聊天响应 ─────────────────────────────────────────────────────────

/// LLM 响应
pub enum ChatResponse {
    /// 非流式完整响应（已解析为 Thought）
    Complete(Thought),
    /// 流式响应通道
    Stream(UnboundedReceiver<Result<StreamEvent, LlmError>>),
}

// ── 测试 ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    #[test]
    fn test_provider_kind_default_base_url() {
        assert_eq!(
            ProviderKind::OpenAi.default_base_url(),
            "https://api.openai.com/v1"
        );
        assert_eq!(ProviderKind::OpenAiCompatible.default_base_url(), "");
        assert_eq!(
            ProviderKind::Anthropic.default_base_url(),
            "https://api.anthropic.com"
        );
        assert_eq!(
            ProviderKind::Ollama.default_base_url(),
            "http://localhost:11434"
        );
    }

    #[test]
    fn test_provider_kind_default() {
        assert_eq!(ProviderKind::default(), ProviderKind::OpenAi);
    }

    #[test]
    fn test_retry_backoff_default() {
        match RetryBackoff::default() {
            RetryBackoff::Exponential { initial, max } => {
                assert_eq!(initial, Duration::from_secs(1));
                assert_eq!(max, Duration::from_secs(30));
            }
            _ => panic!("default should be Exponential"),
        }
    }

    #[test]
    fn test_llm_config_default() {
        let config = LlmConfig::default();
        assert_eq!(config.provider, ProviderKind::OpenAi);
        assert_eq!(config.timeout, DEFAULT_TIMEOUT);
        assert!(config.tools_enabled);
        assert!(!config.stream);
        assert!(!config.multimodal);
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.context_window, 128000);
    }

    #[test]
    fn test_llm_error_is_retryable() {
        let api_retryable = LlmError::ApiError {
            provider: "openai".into(),
            model: "gpt-4o".into(),
            status: Some(500),
            message: "server error".into(),
            trace_id: "t1".into(),
            retryable: true,
        };
        let api_non_retryable = LlmError::ApiError {
            provider: "openai".into(),
            model: "gpt-4o".into(),
            status: Some(401),
            message: "unauthorized".into(),
            trace_id: "t2".into(),
            retryable: false,
        };
        let config_err = LlmError::ConfigError("bad config".into());

        assert!(api_retryable.is_retryable());
        assert!(!api_non_retryable.is_retryable());
        assert!(!config_err.is_retryable());
    }

    #[test]
    fn test_llm_error_suggestion() {
        let config_err = LlmError::ConfigError("bad config".into());
        assert!(config_err.suggestion().contains("配置错误"));
    }

    #[test]
    fn test_constants_values() {
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(30));
        assert_eq!(DEFAULT_IDLE_TIMEOUT, Duration::from_secs(60));
    }

    #[test]
    fn test_provider_key_constants() {
        assert_eq!(PROVIDER_LLM, "llm");
        assert_eq!(PROVIDER_LLM_FORMAT_ADAPTER, "llm_format_adapter");
    }
}
