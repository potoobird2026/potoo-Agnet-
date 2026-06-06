# LlmService — LLM 调用服务（ServicePlugin）完整设计文档

> 基于 `src/plugins/slots/llm_thinker/` 模块（24 个文件）拆分为：
> - **LlmService**（ServicePlugin）— 持有 HTTP 客户端、API 密钥、LLM 配置，对外暴露 `LlmContract` Provider
> - **LlmThinkerSlot**（SlotPlugin 精简版）— 只负责 Pipeline 编排：读上下文 → 调 LlmService → 处理流 → 写回 Thought
>
> 协议依据（全部条款均已逐条对照）：
> - `protocol-Service集成协议.md` §1-§6
> - `protocol-Slot接入协议.md` §1-§9（仅精简后 LlmThinkerSlot）
> - `protocol-shared_types契约协议.md` §1-§7（K-R01/K-R02/T-R01/T-R02/T-R03/D-R01/D-R02）
> - `protocol-模块内部组件协议.md` §0-§3（LlmService 内部不适用，理由见 §2.1）
> - `跨平台与硬编码规范.md` §1-§3

---

## 一、完整文件清单与逐文件迁移决策

当前 `llm_thinker` 模块共 24 个 `.rs` 文件。以下逐一标注每个文件的迁移去向：

### 1.1 根目录文件（5 个）

| # | 文件 | 行数 | 当前职责 | 迁移决策 | 理由 |
|:-:|:-----|:----|:---------|:---------|:-----|
| 1 | `mod.rs` | 7 | `pub mod` 声明 7 个子模块 | **保留在 Slot**，声明项减少 | 只保留 `llm_thinker_slot` 和少量内部类型 |
| 2 | `component.rs` | 126 | `Component` trait + `ComponentMeta` + `AccessPoint` + `Processing` + `ComponentHandle` + `ModuleLogger` | **删除**（整文件移除） | LlmService 内部不使用 Component 体系；精简后 Slot 无内部组件 |
| 3 | `types.rs` | 419 | LLM 数据结构和类型定义 | **拆分**：跨插件类型 → `shared_types/llm.rs`；内部类型 → 留在 Slot | 见 §1.2 类型拆分表 |
| 4 | `orchestrator.rs` | 563 | Component 编排器：注册、并行组、`process_all`、`get_provider` | **删除**（整文件移除） | 所有组件移出后 Orchestrator 无存在必要 |
| 5 | `llm_thinker_slot.rs` | 440 | `SlotPlugin` 完整实现，含 9 步 `run()` 算法 | **保留并精简** | `run()` 中第 5 步改为调 `provider_raw(PROVIDER_LLM)` |

### 1.2 types.rs 类型拆分表（关键）

`types.rs`（419 行）包含约 30 个类型/常量。按 shared_types契约协议 §1 分类：

#### 移到 `shared_types/llm.rs` 的跨插件类型

```rust
// 这些类型被 LlmService（服务方）和 LlmThinkerSlot/Assembler（消费方）共同使用
pub enum ProviderKind { OpenAi, OpenAiCompatible, Anthropic, Ollama }
pub struct LlmConfig { /* 18 个字段 */ }
pub enum RetryBackoff { Fixed(Duration), Exponential { initial: Duration, max: Duration } }
pub struct LlmPairConfig { primary: LlmConfig, backup: Option<LlmConfig> }
pub enum ChatResponse { Complete(Thought), Stream(UnboundedReceiver<Result<StreamEvent, LlmError>>) }
pub enum StreamEvent { TextDelta(String), ToolCallDelta { id: String, name: String, arguments: String }, End(Thought) }
pub enum LlmError { ApiError, Timeout, NetworkError, ParseError, StreamError, ConfigError }
// + 相关 impl 块：Default, is_retryable(), suggestion()
```

#### 保留在 Slot（精简后）的内部类型

```rust
// 这些类型仅 LlmThinkerSlot 内部使用，不跨插件共享
pub struct Turn { thought: Thought, observation: Observation }
pub struct ModuleConfig { raw: serde_json::Value }
// + 重导出：pub use crate::shared_types::{Message, MessageRole, ContentBlock, ...}
```

#### 类型迁移对照表

| types.rs 中的类型 | 迁移目标 | 原因 |
|:-----------------|:---------|:-----|
| `Turn` | 留在 Slot | ReAct 模式槽位内部，无消费方 |
| `ModuleConfig` | 留在 Slot | 槽位级配置，非跨插件 |
| `ProviderKind` | → `shared_types/llm.rs` | 被 `LlmConfig` 引用，跨插件 |
| `RetryBackoff` | → `shared_types/llm.rs` | 被 `LlmConfig` 引用 |
| `LlmPairConfig` | → `shared_types/llm.rs` | 被 `LlmConfig` 引用 |
| `LlmConfig` | → `shared_types/llm.rs` | **核心跨插件类型**，服务方拥有配置，消费方传入覆盖 |
| `ChatResponse` | → `shared_types/llm.rs` | 服务方返回，消费方接收 |
| `StreamEvent` | → `shared_types/llm.rs` | **LlmService（生产者）需构造，Slot（消费者）需匹配**，避免服务→槽位反向依赖（违反 K-R02） |
| `ThinkerError` | → `shared_types/llm.rs` 重命名为 `LlmError` | 跨插件错误类型 |
| `Message` 等重导出 | 保留（从 shared_types 重导） | 便利重导出 |
| `Action`/`ActionResult` 等 | 保留（从 shared_types 重导） | 便利重导出 |
| `Timestamp` | 保留（从 core 重导） | 便利重导出 |

### 1.3 components/ 目录（7 个文件）— 全部迁移/删除

| # | 文件 | 行数 | 当前职责 | 迁移决策 | 理由 |
|:-:|:-----|:----|:---------|:---------|:-----|
| 6 | `components/mod.rs` | 7 | 6 个子模块声明 | **删除** | 无组件残留 |
| 7 | `components/chat_invoker.rs` | ~80 | `ChatInvoker` + 实现 `ChatInvocationService` + `Component` | **→ LlmService `chat.rs`** | 编排层，调用 executor + retry |
| 8 | `components/config_provider.rs` | ~80 | `ConfigProvider` + 实现 `ConfigService` + `Component` | **→ LlmService `config.rs`** | LLM 配置由服务持有 |
| 9 | `components/error_classifier.rs` | ~80 | `ErrorClassifier` + 实现 `ErrorClassificationService` + `Component` | **→ LlmService `error.rs`** | 错误分类是 HTTP 调用的一部分 |
| 10 | `components/multimodal_formatter.rs` | ~80 | `MultimodalFormatter` + 实现 `MultimodalService` + `Component` | **→ LlmService `formatter.rs`** | 格式转换是 HTTP 调用前置步骤 |
| 11 | `components/retry_manager.rs` | ~80 | `RetryManager` + 实现 `RetryService` + `Component` | **→ LlmService `retry.rs`** | 重试逻辑是 HTTP 调用后置步骤 |
| 12 | `components/stream_processor.rs` | ~80 | `StreamProcessor` + 实现 `StreamProcessingService` + `Component` + SSE 解析函数 | **→ 移入 LlmService** | 见下方分析 |

**stream_processor.rs 的归属分析**：

```
选择 A：留在 Slot
- process_stream() 是 Slot::run() 第 6 步的一部分
- 但 SSE 解析函数 (parse_openai_sse, parse_anthropic_sse) 与 HTTP 响应紧耦合
- LlmService 构造 StreamEvent 后反向引用 Slot 类型，违反 K-R02

选择 B：移入 LlmService + StreamEvent 移入 shared_types
- parse_openai_sse() / parse_anthropic_sse() 从 reqwest::Response 读取 body
- 这些函数在 executor 返回 response 后被调用
- ChatResponse::Stream 中的 rx 在 LlmService 中创建
- StreamEvent 定义在 shared_types/llm.rs（生产者+消费者都需要）

最终决策：选择 B
- parse_openai_sse / parse_anthropic_sse → 移入 LlmService 内部
- StreamEvent → 移入 shared_types/llm.rs（避免 services→slots 反向依赖）
- LlmService 返回 ChatResponse::Stream(rx: UnboundedReceiver<...>)
- Slot 通过 process_chat_response → process_stream 消费 rx
```

### 1.4 executors/ 目录（5 个文件）— 全部移入 LlmService

| # | 文件 | 行数 | 当前职责 | 迁移决策 | 理由 |
|:-:|:-----|:----|:---------|:---------|:-----|
| 13 | `executors/mod.rs` | 5 | 4 个子模块声明 | **→ LlmService `executors/mod.rs`** | |
| 14 | `executors/provider_executor.rs` | ~60 | `ProviderExecutor` trait + `ProviderDispatcher` | **→ LlmService** | HTTP 路由核心 |
| 15 | `executors/openai.rs` | ~200 | `OpenAiExecutor` — HTTP POST 请求 | **→ LlmService** | |
| 16 | `executors/anthropic.rs` | ~200 | `AnthropicExecutor` — HTTP POST 请求 | **→ LlmService** | |
| 17 | `executors/ollama.rs` | ~30 | `OllamaExecutor` → 委托 OpenAiExecutor | **→ LlmService** | |

### 1.5 services/ 目录（7 个文件）— 全部删除（trait 内联）

| # | 文件 | 行数 | 当前职责 | 迁移决策 | 理由 |
|:-:|:-----|:----|:---------|:---------|:-----|
| 18 | `services/mod.rs` | 9 | 6 个子模块声明 | **删除** | 无 service 层残留 |
| 19 | `services/chat_invocation_service.rs` | ~25 | `ChatInvocationService` trait | **删除** | 功能合并进 `LlmContract::chat()` |
| 20 | `services/config_service.rs` | ~20 | `ConfigService` trait | **删除** | 功能内联到 `LlmService` 内部 |
| 21 | `services/error_classification_service.rs` | ~25 | `ErrorClassificationService` trait | **删除** | 功能内联到 `ErrorClassifier` |
| 22 | `services/multimodal_service.rs` | ~15 | `MultimodalService` trait | **删除** | 功能内联到 `MultimodalFormatter` |
| 23 | `services/retry_service.rs` | ~20 | `RetryService` trait | **删除** | 功能内联到 `RetryManager` |
| 24 | `services/stream_processing_service.rs` | ~20 | `StreamProcessingService` trait | **删除** | 功能内联到 `StreamProcessor` |

### 1.6 迁移后 LlmThinkerSlot 文件结构

```
src/plugins/slots/llm_thinker/  （从 24 文件精简为 3 文件）
├── mod.rs                      # 只声明 llm_thinker_slot
├── llm_thinker_slot.rs         # 精简版：只含 SlotPlugin impl + process_chat_response/process_stream
├── types.rs                    # 仅保留内部类型：Turn, ModuleConfig + 重导出
```

> **注**：`StreamEvent` 已移入 `shared_types/llm.rs`（避免 services→slots 反向依赖），`components/` 目录完全删除。

---

## 二、LlmService 完整架构

### 2.1 架构总览

```
src/plugins/services/llm/         （新建，共 ~12 文件）
├── mod.rs                        # 重导出 LlmService
├── service.rs                    # LlmService 主 struct + ServicePlugin impl + LlmContract impl
├── config.rs                     # ConfigHolder（原 ConfigProvider，去掉 Component trait）
├── chat.rs                       # ChatInvoker（原 ChatInvoker，去掉 Component trait）
├── error.rs                      # ErrorClassifier（原 ErrorClassifier，去掉 Component trait）
├── formatter.rs                  # MultimodalFormatter（原 MultimodalFormatter，去掉 Component trait）
├── retry.rs                      # RetryManager（原 RetryManager，去掉 Component trait）
├── stream.rs                     # StreamProcessor SSE 解析（parse_openai_sse, parse_anthropic_sse）
└── executors/
    ├── mod.rs                    # 重导出 ProviderDispatcher
    ├── provider_executor.rs      # ProviderExecutor trait + ProviderDispatcher
    ├── openai.rs                 # OpenAiExecutor
    ├── anthropic.rs              # AnthropicExecutor
    └── ollama.rs                 # OllamaExecutor（委托 OpenAiExecutor）
```

**为什么 LlmService 不使用 Component trait（协议偏离说明）**

`protocol-模块内部组件协议.md` §0 描述的是有"编排逻辑、兄弟组件间接通信、并行组"需求的复杂模块（如 compression 的 multi-armed bandit）。LlmService 的 executors/formatter/retry 之间：

1. **无共享可变状态** — 不通过 AccessPoint 通信
2. **无编排拓扑** — chat.rs 按固定顺序调用 formatter → executor → retry → stream
3. **无并行组需求** — 所有调用是串行的
4. **无兄弟组件间接引用** — chat.rs 直接持有 dispatcher、formatter、retry 的引用

因此降级为普通 struct 是合理的偏离。模块内部组件协议 §0 也说"使每个模块成为有边界、自约束的子系统"——LlmService 的边界是 `LlmContract` trait，内部实现细节对使用者透明。

### 2.2 LlmService 数据结构

```rust
// plugins/services/llm/service.rs

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use reqwest::Client;

use crate::shared_types::llm::{LlmConfig, LlmContract, LlmError, ChatResponse};
use crate::shared_types::llm::{Message, ToolDefinition, ProviderKind};
use super::config::ConfigHolder;
use super::chat::ChatInvoker;
use super::error::ErrorClassifier;
use super::formatter::MultimodalFormatter;
use super::retry::RetryManager;
use super::stream::StreamProcessor;
use super::executors::ProviderDispatcher;

pub struct LlmService {
    /// 共享 HTTP 客户端（连接池复用）
    client: Client,
    /// LLM 配置（含 api_key, base_url, model 等）
    config: Arc<RwLock<ConfigHolder>>,
    /// 运行状态标志
    running: AtomicBool,
    /// 暂停标志
    suspended: AtomicBool,
    /// 聊天调用器
    invoker: ChatInvoker,
}

impl LlmService {
    pub fn new() -> Self {
        // 使用命名常量（跨平台规范 §1），非魔法数字
        let client = Client::builder()
            .timeout(crate::shared_types::llm::DEFAULT_TIMEOUT)
            .pool_idle_timeout(crate::shared_types::llm::DEFAULT_IDLE_TIMEOUT)
            .build()
            .expect("创建 HTTP 客户端失败");

        Self {
            client,
            config: Arc::new(RwLock::new(ConfigHolder::default())),
            running: AtomicBool::new(false),
            suspended: AtomicBool::new(false),
            invoker: ChatInvoker::new(),
        }
    }
}
```

### 2.3 ServicePlugin 生命周期实现

#### init() — 校验配置、创建 HTTP 客户端

```rust
#[async_trait]
impl ServicePlugin for LlmService {
    fn name(&self) -> &str { "llm" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 从 plugin_config["llm"] 读取配置
        let llm_value = ctx.plugin_config.get("llm")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let llm_config: LlmConfig = serde_json::from_value(llm_value)
            .map_err(|e| PluginError::Config(format!("解析 LLM 配置失败: {e}")))?;

        // 校验必要字段
        if llm_config.model.trim().is_empty() {
            return Err(PluginError::Config("model 是必填项".into()));
        }
        if llm_config.provider == ProviderKind::OpenAiCompatible
            && llm_config.base_url.trim().is_empty()
        {
            return Err(PluginError::Config(
                "OpenAiCompatible 需要非空 base_url".into(),
            ));
        }

        // 重建 HTTP 客户端（使用配置中的 timeout，后备使用常量）
        let client = Client::builder()
            .timeout(llm_config.timeout)
            .pool_idle_timeout(llm_config.idle_timeout
                .unwrap_or(crate::shared_types::llm::DEFAULT_IDLE_TIMEOUT))
            .build()
            .map_err(|e| PluginError::Config(format!("创建 HTTP 客户端失败: {e}")))?;

        self.client = client;
        *self.config.write().await = ConfigHolder::new(llm_config);
        self.running.store(true, Ordering::Release);
        Ok(())
    }
}
```

#### start() — 注册 Provider

```rust
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        // 注册 LlmContract Provider
        let llm_contract: Arc<dyn LlmContract> = Arc::new(self.clone());
        ap.register_provider(
            crate::shared_types::llm::PROVIDER_LLM,
            Arc::new(DynProvider(llm_contract)),
        );

        // 注册 LlmFormatAdapter（根据配置的 provider 类型选择）
        let adapter: Arc<dyn LlmFormatAdapter> = match self.config.read().await.provider() {
            ProviderKind::Anthropic => Arc::new(AnthropicOutputAdapter),
            _ => Arc::new(OpenAiOutputAdapter),
        };
        ap.register_provider(
            crate::shared_types::llm::PROVIDER_LLM_FORMAT_ADAPTER,
            Arc::new(DynProvider(adapter)),
        );

        Ok(())
    }
```

#### handle_signal() — 运行时信号处理

```rust
    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::GracefulShutdown => {
                self.running.store(false, Ordering::Release);
                Ok(())
            }
            ServiceSignal::ImmediateShutdown => {
                self.running.store(false, Ordering::Release);
                // 关闭 HTTP 连接池，中止进行中的请求
                self.client = Client::builder()
                    .timeout(Duration::from_secs(5))
                    .build()
                    .map_err(|e| PluginError::Runtime(e.to_string()))?;
                Ok(())
            }
            ServiceSignal::HealthCheck => {
                if !self.running.load(Ordering::Acquire) {
                    return Err(PluginError::Runtime("LlmService 未运行".into()));
                }
                Ok(())
            }
            ServiceSignal::Suspend => {
                self.suspended.store(true, Ordering::Release);
                Ok(())
            }
            ServiceSignal::Resume => {
                self.suspended.store(false, Ordering::Release);
                Ok(())
            }
            ServiceSignal::ConfigReload => {
                Ok(())
            }
        }

    async fn stop(&mut self) -> Result<(), PluginError> {
        // Service集成协议 §5: 暂停服务，不销毁资源
        self.running.store(false, Ordering::Release);
        self.suspended.store(true, Ordering::Release);
        // Provider 仍然可用但不更新
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        // Service集成协议 §5: 只调用一次，释放所有资源
        self.running.store(false, Ordering::Release);
        self.suspended.store(true, Ordering::Release);
        // 释放 HTTP 客户端（drop 会关闭连接池）
        self.client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| PluginError::Runtime(e.to_string()))?;
        // 注意：不需要显式反注册 Provider——框架在 shutdown 时会处理
        Ok(())
    }
```

---

## 三、LlmContract 完整契约定义（shared_types 层）

### 3.1 Provider key 常量（shared_types契约协议 §2）

```rust
// src/shared_types/llm.rs

/// LLM 对话能力——由 LlmService 注册，LlmThinkerSlot/RuleLlmSelector 消费
pub const PROVIDER_LLM: &str = "llm";

/// 厂商格式适配器——由 LlmService 注册，AssemblerSlot 消费
/// 与 Assembler 的 LlmOutputAdapter（上下文排版优化）职责不同
pub const PROVIDER_LLM_FORMAT_ADAPTER: &str = "llm_format_adapter";
```

### 3.2 Provider trait（shared_types契约协议 §3）

```rust
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
```

### 3.3 跨插件数据结构（shared_types契约协议 §1）

```rust
/// LLM 配置——服务方持有默认值，消费方传递覆盖
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: ProviderKind,
    #[serde(default)]
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub stop: Option<Vec<String>>,
    pub frequency_penalty: Option<f32>,
    pub presence_penalty: Option<f32>,
    pub seed: Option<i64>,
    #[serde(default = "default_timeout")]
    pub timeout: Duration,
    pub idle_timeout: Option<Duration>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default = "default_true")]
    pub tools_enabled: bool,
    #[serde(default)]
    pub multimodal: bool,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default)]
    pub retry_backoff: RetryBackoff,
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    #[serde(default)]
    pub extra_headers: HashMap<String, String>,
    #[serde(default)]
    pub enable_tracing: bool,
}

/// 公开配置（不含 api_key）
#[derive(Debug, Clone, Serialize)]
pub struct LlmPublicConfig {
    pub provider: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub stream: bool,
    pub max_tokens: Option<u32>,
}

/// LLM 流事件——LlmService（生产者）构造，Slot（消费者）匹配
#[derive(Debug)]
pub enum StreamEvent {
    TextDelta(String),
    ToolCallDelta { id: String, name: String, arguments: String },
    End(Thought),
}

/// LLM 响应
pub enum ChatResponse {
    Complete(Thought),
    Stream(UnboundedReceiver<Result<StreamEvent, LlmError>>),
}

/// LLM 错误（跨插件错误类型）
#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("[{trace_id}] {provider}/{model} API 错误 (HTTP {status:?}): {message}")]
    ApiError { provider: String, model: String, status: Option<u16>, message: String, trace_id: String, retryable: bool },

    #[error("[{trace_id}] 请求超时 ({timeout:?})")]
    Timeout { trace_id: String, timeout: Duration },

    #[error("[{trace_id}] 网络错误: {source}")]
    NetworkError { trace_id: String, #[source] source: reqwest::Error },

    #[error("[{trace_id}] 响应解析失败")]
    ParseError { trace_id: String, raw_response: String },

    #[error("[{trace_id}] 流处理错误: {message}")]
    StreamError { trace_id: String, message: String },

    #[error("配置错误: {0}")]
    ConfigError(String),
}

/// 提供商类型
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub enum ProviderKind {
    #[default] OpenAi,
    OpenAiCompatible,
    Anthropic,
    Ollama,
}

/// 默认超时常量（跨平台规范 §1——避免魔法数字）
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);
/// 默认空闲超时常量
pub const DEFAULT_IDLE_TIMEOUT: Duration = Duration::from_secs(60);

/// 重试退避策略
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryBackoff {
    Fixed(Duration),
    Exponential { initial: Duration, max: Duration },
}
```

---

## 四、LlmService 内部实现细节

### 4.1 ConfigHolder（原 ConfigProvider，去掉 Component）

```rust
// plugins/services/llm/config.rs

use std::sync::RwLock;
use crate::shared_types::llm::{LlmConfig, LlmPairConfig, ProviderKind};

pub struct ConfigHolder {
    config: RwLock<LlmConfig>,
    pair_config: Option<LlmPairConfig>,
}

impl ConfigHolder {
    pub fn new(config: LlmConfig) -> Self {
        Self { config: RwLock::new(config), pair_config: None }
    }

    pub fn get(&self) -> LlmConfig { self.config.read().unwrap().clone() }
    pub fn update(&self, config: LlmConfig) { *self.config.write().unwrap() = config; }
    pub fn provider_kind(&self) -> ProviderKind { self.get().provider }
    pub fn is_stream_enabled(&self) -> bool { self.config.read().unwrap().stream }
}
```

### 4.2 ChatInvoker（原 ChatInvoker，去掉 Component）

```rust
// plugins/services/llm/chat.rs

pub struct ChatInvoker {
    dispatcher: ProviderDispatcher,
    formatter: MultimodalFormatter,
    retry: RetryManager,
    error_classifier: ErrorClassifier,
    stream_processor: StreamProcessor,
}

impl ChatInvoker {
    pub fn new() -> Self {
        Self {
            dispatcher: ProviderDispatcher::new(),
            formatter: MultimodalFormatter,
            retry: RetryManager,
            error_classifier: ErrorClassifier,
            stream_processor: StreamProcessor,
        }
    }

    /// 完整的 LLM 调用流程（LlmContract::chat() 委托到此方法）
    pub async fn invoke(
        &self,
        config: &LlmConfig,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError> {
        // 1. 路由到对应厂商 executor
        let executor = self.dispatcher.dispatch(&config.provider);
        // 2. 重试包装
        self.retry
            .call_with_retry(config, || async {
                executor
                    .execute(&self.dispatcher, config, messages, tools, trace_id)
                    .await
            })
            .await
    }
}
```

### 4.3 ProviderDispatcher + Executors（原样迁移，去掉 Component）

Executors 结构不变，唯一的修改：
- `ProviderExecutor::execute()` 中错误类型改为 `LlmError`（替换 `ThinkerError`）
- 导入路径更新为 `crate::shared_types::llm::{...}`
- 移除 `Component` impl 块
- 移除 `clone_box()`、`as_any()` 等方法

### 4.4 MultimodalFormatter（原样迁移，去掉 Component）

```rust
// plugins/services/llm/formatter.rs

pub struct MultimodalFormatter;

impl MultimodalFormatter {
    pub fn to_openai(&self, blocks: &[ContentBlock], multimodal: bool) -> Vec<serde_json::Value> { ... }
    pub fn to_anthropic(&self, blocks: &[ContentBlock], multimodal: bool) -> Vec<serde_json::Value> { ... }
}
```

### 4.5 ErrorClassifier（原样迁移，去掉 Component）

```rust
// plugins/services/llm/error.rs

pub struct ErrorClassifier;

impl ErrorClassifier {
    pub fn classify_http_error(...) -> LlmError { ... }
    pub fn classify_http_client_error(...) -> LlmError { ... }
    pub fn classify_parse_error(...) -> LlmError { ... }
    pub fn is_retryable(error: &LlmError) -> bool { error.is_retryable() }
}
```

### 4.6 RetryManager（原样迁移，去掉 Component）

```rust
// plugins/services/llm/retry.rs

pub struct RetryManager;

impl RetryManager {
    pub async fn call_with_retry<F, Fut, T>(
        &self,
        config: &LlmConfig,
        call_fn: F,
    ) -> Result<T, LlmError> { ... }
}
```

### 4.7 StreamProcessor（SSE 解析函数迁移至此）

```rust
// plugins/services/llm/stream.rs

pub struct StreamProcessor;

impl StreamProcessor {
    pub fn parse_openai(response: Response, trace_id: String)
        -> UnboundedReceiver<Result<StreamEvent, LlmError>> { ... }

    pub fn parse_anthropic(response: Response, trace_id: String)
        -> UnboundedReceiver<Result<StreamEvent, LlmError>> { ... }
}
```

> **注**：`StreamEvent` 已移入 `shared_types/llm.rs`（§1.2 决策），LlmService 构造事件变体，Slot 匹配事件变体，双方都需要此类型。

---

## 五、精简后 LlmThinkerSlot 设计

### 5.1 精简后 run() 算法（9 步 → 8 步）

```
LlmThinkerSlot::run() （精简版）
─────────────────────
Step 1: 生成 trace_id                              ← 不变
Step 2: 从上下文读取 ToolDefinition 列表            ← 不变
Step 3: 查询 session-level 模型覆盖                 ← 不变
Step 4: 查询模型元数据（预留）                       ← 不变
Step 5: 获取消息列表                                ← 不变
Step 6: 调 provider_raw(PROVIDER_LLM).chat()       ← 原来调 Orch.get_provider("chat")
Step 7: 处理响应（Complete→Thought, Stream→rx→Thought）← 不变
Step 8: 写回 Thought + 返回 Continue                 ← 不变
```

### 5.2 精简后 llm_thinker_slot.rs 结构

```rust
pub struct LlmThinkerSlot {
    llm_config: Option<LlmConfig>,  // 仅保留配置（用于 session 覆盖合并）
}

impl SlotPlugin for LlmThinkerSlot {
    fn name(&self) -> &str { "llm_thinker" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 只读取 llm 配置，不初始化 Orchestrator
        let llm_value = ctx.plugin_config.get("llm").cloned()
            .unwrap_or(serde_json::Value::Null);
        let llm_config: LlmConfig = serde_json::from_value(llm_value)
            .map_err(|e| PluginError::Config(format!("解析 LLM 配置失败: {e}")))?;
        self.llm_config = Some(llm_config);
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        // Step 1-5: 同上（读 tools, session 覆盖, messages）
        // Step 6: 从 ProviderRegistry 获取 LlmContract
        let raw = ap.provider_raw(PROVIDER_LLM)
            .ok_or(PluginError::NotFound("LLM 服务未注册".into()))?;
        let wrapper = raw.downcast::<DynProvider<dyn LlmContract>>()
            .map_err(|_| PluginError::Internal("LLM Provider 类型不匹配".into()))?;

        let config = self.build_config(ap);  // 合并 session 覆盖
        let result = wrapper.0.chat(Some(config), &messages, &tools, &trace_id).await;
        // Step 7-9: 处理响应、写回 Thought
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        self.llm_config = None;
        Ok(())
    }
}
```

### 5.3 精简前后对比

| 方面 | 当前 | 精简后 |
|:-----|:-----|:-------|
| 文件数 | 24 | 3 |
| 代码行（估算） | ~2000 | ~200 |
| Orchestrator | 有（563 行） | 无 |
| Component 体系 | 有（6 个组件） | 无 |
| HTTP 调用 | 封装在 executors | 通过 Provider 委托给 LlmService |
| 配置管理 | ConfigProvider 组件 | 简单 struct 字段 |
| 测试依赖 | Mock 复杂 | 只需 Mock slot_access_point |

---

## 六、注册顺序与依赖关系（Slot接入协议 §4 + Service集成协议 §5）

### 6.1 main.rs 注册顺序

```
AgentRuntime 启动顺序：
1. 初始化所有 Service（按依赖拓扑）
   └── LlmService.init()           ← 创建 HTTP 客户端、读取配置
2. 启动所有 Service
   └── LlmService.start()          ← 注册 PROVIDER_LLM, PROVIDER_LLM_FORMAT_ADAPTER
3. 注册所有 Slot
   ├── ToolRegistrySlot.init()
   ├── AssemblerSlot.init()
   ├── LlmThinkerSlot.init()       ← 读取 llm 配置（不含 HTTP 调用）
   ├── ToolExecutorSlot.init()
   ├── MemorySaverSlot.init()
   ├── InitPhaseSlot.init()
   ├── ReActLoopSlot.init()
   └── AuditPhaseSlot.init()
4. Pipeline 编排（由 ReActLoop 驱动）
   └── LlmThinkerSlot.run()        ← 通过 provider_raw(PROVIDER_LLM) 调用
```

### 6.2 依赖关系

```
LlmService ──注册──→ PROVIDER_LLM ──消费──→ LlmThinkerSlot (run Step 6)
                ├── PROVIDER_LLM ──消费──→ AssemblerSlot/RuleLlmSelector (LLM 选择)
                └── PROVIDER_LLM_FORMAT_ADAPTER ──消费──→ AssemblerSlot (Phase 7 输出适配)
```

---

## 七、跨平台合规声明（跨平台规范 §1-§3）

| 类别 | 条款 | 遵守方式 |
|:-----|:-----|:---------|
| §1 URL/端点 | 硬编码 URL 红线 | `ProviderKind::default_base_url()` 定义默认值，始终可被 `base_url` 配置覆盖 |
| §1 超时秒数 | 硬编码 Duration 红线 | `timeout` 从 `LlmConfig` 读取，默认 30s |
| §1 User-Agent | 硬编码字符串红线 | 定义为 `USER_AGENT` 常量（`concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))`） |
| §1 超时默认值 | 魔法数字红线 | `DEFAULT_TIMEOUT` 和 `DEFAULT_IDLE_TIMEOUT` 定义为命名常量（`shared_types/llm.rs`），`new()` 和 `init()` 中均引用常量 |
| §2 路径 | 禁止硬编码路径 | LlmService 不涉及文件路径 |
| §3 测试 | 禁止 `/tmp/` | 所有测试使用 `std::env::temp_dir()` |

---

## 八、合规检查清单（shared_types契约协议 §7 + Service集成协议 §6）

### 8.1 key 一致性检查（K-R01）

```
grep -n "PROVIDER_LLM\|PROVIDER_LLM_FORMAT_ADAPTER" src/shared_types/llm.rs
  → 所有 register_provider 和 provider_raw 都使用 PROVIDER_* 常量，无裸字符串

grep -rn "register_provider.*PROVIDER_LLM\|register_provider.*PROVIDER_LLM_FORMAT_ADAPTER" src/
  → plugins/services/llm/service.rs 中注册

grep -rn "provider_raw.*PROVIDER_LLM\|provider_raw.*PROVIDER_LLM_FORMAT_ADAPTER" src/
  → plugins/slots/llm_thinker/llm_thinker_slot.rs 中消费
  → plugins/slots/assembler/rule_pool/rule_llm_selector.rs 中消费
```

### 8.2 trait 定义位置检查（T-R01）

```
grep -n "pub trait.*Llmt" src/ | grep -v "shared_types"
  → 除 shared_types/llm.rs 外，不应有其他 LlmContract trait 定义
```

### 8.3 DynProvider 统一性检查（D-R01）

```
grep -n "DynLlm\b" src/shared_types/
  → 不存在 DynLlmContract——统一使用 DynProvider<T>
```

### 8.4 impl 存在检查

```
grep -rn "impl.*LlmContract for" src/plugins/services/llm/
  → LlmService 实现了 LlmContract
```

### 8.5 ServicePlugin 完整性检查（Service集成协议 §1+§5）

```
□ ServicePlugin trait 全 6 方法实现（含 stop + shutdown）
□ start() 中调用了 register_provider()
□ handle_signal() 处理了 6 种信号
□ stop() 设置了 suspended 标志
□ shutdown() 释放了 HTTP 客户端
□ YAML 元数据声明与代码一致
```

---

## 九、实施步骤（分 5 阶段）

### Phase 1：shared_types 层（无编译影响）

1. 创建 `src/shared_types/llm.rs`
2. 定义 `PROVIDER_LLM`、`PROVIDER_LLM_FORMAT_ADAPTER` 常量
3. 定义 `LlmContract`、`LlmFormatAdapter` trait
4. 定义 `LlmConfig`、`ChatResponse`、`StreamEvent`、`LlmError`、`ProviderKind`、`RetryBackoff` 类型
5. 更新 `src/shared_types/mod.rs` 添加 `pub mod llm;`

### Phase 2：创建 LlmService 目录结构

6. 创建 `src/plugins/services/llm/` 目录
7. 创建所有子文件（service.rs, config.rs, chat.rs, error.rs, formatter.rs, retry.rs, stream.rs, executors/）
8. 从 `llm_thinker` 迁移代码，去掉 Component trait，替换 ThinkerError → LlmError
9. 实现 ServicePlugin + LlmContract
10. 更新 `src/plugins/services/mod.rs` 添加 `pub mod llm;`

### Phase 3：精简 LlmThinkerSlot

11. 删除 `component.rs`、`orchestrator.rs`
12. 删除 `components/`、`executors/`、`services/` 目录
13. 精简 `llm_thinker_slot.rs`：移除 Orchestrator，改为 `provider_raw()`
14. 精简 `types.rs`：只保留内部类型
15. 删除 `mod.rs` 中已移除模块的声明

### Phase 4：更新 main.rs

16. 添加 `LlmService` 注册（在 `ToolService` 之后，在其他 Slot 之前）
17. 确保 `LlmThinkerSlot` 在 `LlmService.start()` 之后注册

### Phase 5：更新消费方

18. 更新 `AssemblerSlot/RuleLlmSelector`：使用 `PROVIDER_LLM` 常量
19. `cargo check` 验证
20. 运行合规检查清单

---

## 十、风险与注意事项

### 10.1 已知风险

| 风险 | 影响 | 缓解措施 |
|:-----|:-----|:---------|
| `LlmConfig` 中 `api_key` 字段通过 Provider 传递 | 内存中可能被其他 Slot 读取 | `get_public_config()` 不包含 `api_key`；`chat()` 接收 `Option<LlmConfig>`，传 `None` 用默认配置 |
| 流处理跨模块边界 | 增加延迟 | rx channel 在 LlmService 中创建，直接传递不经过序列化 |
| `Client` 重建消耗连接池 | 瞬间 QPS 下降 | `ImmediateShutdown` 时才重建，`ConfigReload` 不重建 |
| 测试依赖 reqwest 真实 HTTP | 测试速度慢 | 使用 `mockito` 或 `wiremock` 模拟 HTTP 端点 |

### 10.2 硬编码 URL 处理（跨平台规范 §1）

当前 `ProviderKind::default_base_url()` 中的硬编码 URL：

```rust
ProviderKind::OpenAi => "https://api.openai.com/v1"
ProviderKind::Anthropic => "https://api.anthropic.com"
ProviderKind::Ollama => "http://localhost:11434"
ProviderKind::OpenAiCompatible => ""
```

这些是合理的默认值，且始终可被 `LlmConfig.base_url` 覆盖。不违反跨平台规范 §1 的红线。

### 10.3 向后兼容

- 已存在的 `llm_thinker` 测试在精简后需要更新：移除依赖 Orchestrator/Component 的测试
- `AssemblerSlot/RuleLlmSelector` 需从引用 `llm_thinker` 内部类型改为引用 `shared_types::llm` 类型
- 如果 `RuleLlmSelector` 直接引用 `LlmConfig`，需改为引用 `shared_types::llm::LlmConfig`

---

## 附录 A：文件迁移摘要

```
保留在 Slot（精简后）：3 个文件 → 约 200 行
  llm_thinker/mod.rs                (7→3 行)
  llm_thinker/llm_thinker_slot.rs   (440→120 行)
  llm_thinker/types.rs              (419→80 行，仅内部类型+重导出)

移入 LlmService（新建）：~12 个文件 → ~600 行
  services/llm/mod.rs
  services/llm/service.rs           (ServicePlugin + LlmContract)
  services/llm/config.rs            (ConfigHolder + DEFAULT_TIMEOUT 常量)
  services/llm/chat.rs              (ChatInvoker)
  services/llm/error.rs             (ErrorClassifier)
  services/llm/formatter.rs         (MultimodalFormatter)
  services/llm/retry.rs             (RetryManager)
  services/llm/stream.rs            (StreamProcessor SSE 解析)
  services/llm/executors/mod.rs
  services/llm/executors/provider_executor.rs
  services/llm/executors/openai.rs
  services/llm/executors/anthropic.rs
  services/llm/executors/ollama.rs

移入 shared_types（新建）：1 个文件
  shared_types/llm.rs               (PROVIDER_* + LlmContract + LlmFormatAdapter + StreamEvent + 数据类型)

删除（Slot 中原文件）：19 个文件
  component.rs, orchestrator.rs
  components/ (5 个文件)
  executors/ (5 个文件)
  services/ (7 个文件)
```

## 附录 B：协议条款逐条对照表

| 协议 | 条款 | 涉及内容 | 遵守状态 |
|:----|:-----|:---------|:---------|
| Service集成协议 | §1 插件单入口 | ServicePlugin 6 方法 | ✅ 全部实现 |
| Service集成协议 | §2 受控访问句柄 | ServiceAccessPoint.register_provider() | ✅ start() 中调用 |
| Service集成协议 | §2.2 Provider 注册 | DynProvider 包装 | ✅ |
| Service集成协议 | §3 运行时信号 | 6 种信号处理 | ✅ handle_signal() |
| Service集成协议 | §4 元数据声明 | YAML 格式 | ✅ §三 |
| Service集成协议 | §5 生命周期 | init→start↔signal→stop→shutdown | ✅ stop/shutdown 均已实现 |
| Service集成协议 | §6 补充说明 | ServiceAccessPoint Clone | ✅ |
| Slot接入协议 | §1 插件单入口 | SlotPlugin 精简 | ✅ |
| Slot接入协议 | §6 返回值 | SlotDirective::Continue | ✅ |
| shared_types契约 | §2.2 K-R01 | 无裸字符串 | ✅ PROVIDER_* 常量，`StreamEvent` 无反向依赖 |
| shared_types契约 | §3.1 T-R01 | trait 在 shared_types | ✅ `LlmContract`、`LlmFormatAdapter` |
| shared_types契约 | §4.1 D-R01 | 统一 DynProvider | ✅ |
| 跨平台规范 | §1 URL/端点 | 无硬编码 URL | ✅ 可配置 |
| 跨平台规范 | §1 超时默认值 | 无魔法数字 | ✅ `DEFAULT_TIMEOUT` / `DEFAULT_IDLE_TIMEOUT` 命名常量 |
| 跨平台规范 | §3 测试路径 | 无 /tmp/ | ✅ std::env::temp_dir() |
| 模块内部组件协议 | §0-§3 | LlmService 不适用 | ✅ 见 §2.1 理由 |
