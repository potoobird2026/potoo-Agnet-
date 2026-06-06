# LlmService — LLM 调用服务（ServicePlugin）严格 AI 开发计划

本计划用于指导 AI 严格按照 `docs/services/llm/LlmService-开发设计文档.md`（以下简称"设计文档"）以及 5 份协议文件生成 LlmService 模块的完整代码，并同步精简 LlmThinkerSlot。

---

## 项目背景

- **模块名称**：llm（LLM 调用服务）
- **模块定位**：将 LlmThinkerSlot 中的 LLM HTTP 调用能力拆分为独立的 ServicePlugin。Service 持有 HTTP 客户端、API 密钥、LLM 配置，对外暴露 `LlmContract` Provider；Slot 精简为纯编排层。
- **外部接口**：
  - `LlmService` — ServicePlugin 入口（新建）
  - `LlmContract` — Provider trait，定义在 `shared_types/llm.rs`（新建）
  - `LlmFormatAdapter` — 厂商格式适配器 trait，定义在 `shared_types/llm.rs`（新建）
  - `LlmConfig` / `ChatResponse` / `StreamEvent` / `LlmError` — 跨插件数据结构（从 `llm_thinker/types.rs` 迁移）
- **当前状态**：
  - `shared_types/llm.rs` ❌（需创建）
  - `plugins/services/llm/` ❌（12 个文件需创建）
  - `plugins/slots/llm_thinker/` ⚠️（24 文件需精简为 3 文件）
- **依赖项**：`tokio`、`reqwest`、`serde`/`serde_json`、`async-trait`、`thiserror`、`tracing`、`uuid`、`futures`
- **设计文档**：`docs/services/llm/LlmService-开发设计文档.md`
- **协议依据**：
  - `protocol-Service集成协议.md` §1-§6
  - `protocol-Slot接入协议.md` §1-§9
  - `protocol-shared_types契约协议.md` §1-§7（K-R01/K-R02/T-R01/T-R02/T-R03/D-R01/D-R02）
  - `protocol-模块内部组件协议.md` §0-§3（LlmService 不适用，见设计文档 §2.1）
  - `跨平台与硬编码规范.md` §1-§3

---

## 硬编码分类定义（llm 特有）

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| API 端点 URL | `"https://api.openai.com/v1"` 散落在 executor 中 | 定义为 `const OPENAI_CHAT_PATH: &str`，`base_url` 从 `LlmConfig.base_url` 读取，默认值在 `ProviderKind::default_base_url()` 中定义一次 |
| 超时值 | `Duration::from_secs(30)` 硬编码在 `new()` 中 | 使用 `DEFAULT_TIMEOUT: Duration` 常量（定义在 `shared_types/llm.rs`） |
| User-Agent | `"aagnet/0.1.0"` 字符串字面量 | 使用 `concat!(env!("CARGO_PKG_NAME"), "/", env!("CARGO_PKG_VERSION"))` |
| Content-Type | `"application/json"` 散落多处 | 定义为 `const CONTENT_TYPE_JSON: &str`，每个 executor 共享 |
| 流式 end 标记 | `"data: [DONE]"` 散落 | 定义为 `const SSE_DONE_MARKER: &str` |
| Anthropic API 版本 | `"2023-06-01"` 硬编码 | 定义为 `const ANTHROPIC_API_VERSION: &str` |
| 重试退避参数 | 指数退避 base=2 硬编码 | 从 `RetryBackoff::Exponential` 的 `initial`/`max` 读取 |
| Provider key | `"llm"` 裸字符串 | 使用 `PROVIDER_LLM` 常量（K-R01） |
| 消息角色 | `"user"`/`"assistant"`/`"system"` 字符串 | 使用 `MessageRole` 枚举的 `Serialize` |
| 日志前缀 | `"[llm_service]"` 散落 | 定义为 `const LOG_PREFIX: &str` |

---

## 项目目录结构

### 新建：LlmService（12 文件）

```
src/plugins/services/llm/
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

### 新建：shared_types/llm.rs（1 文件）

```
src/shared_types/llm.rs           # PROVIDER_* + LlmContract + LlmFormatAdapter + StreamEvent + 数据类型
```

### 精简：LlmThinkerSlot（24 文件 → 3 文件）

```
src/plugins/slots/llm_thinker/
├── mod.rs                        # 只声明 llm_thinker_slot（删除 component/components/executors/services/orchestrator）
├── llm_thinker_slot.rs           # 精简版：SlotPlugin impl + process_chat_response/process_stream
├── types.rs                      # 仅保留：Turn, ModuleConfig + 重导出
```

**删除 19 个文件**：
- `component.rs`、`orchestrator.rs`
- `components/`（5 文件：mod, chat_invoker, config_provider, error_classifier, multimodal_formatter, retry_manager, stream_processor）
- `executors/`（5 文件：mod, provider_executor, openai, anthropic, ollama）
- `services/`（7 文件：mod, chat_invocation_service, config_service, error_classification_service, multimodal_service, retry_service, stream_processing_service）

---

## AI 宪法

```
[宪法已生效]

1. 文档唯一真理：所有类型定义、函数签名、默认值、错误变体、流程步骤，必须与
   `docs/services/llm/LlmService-开发设计文档.md` 完全一致。

2. 零幻觉：
   a. LlmService 只有 1 种通信方式（HTTP POST JSON），不存在 gRPC/WebSocket。
   b. LlmService 内部使用普通 struct（非 Component trait），不存在 AccessPoint/InternalAccessPoint。
   c. 精简后的 LlmThinkerSlot 没有 Orchestrator，没有 ConfigProvider 等 Component。
   d. stream_processor.rs 中的 SSE 解析函数（parse_openai_sse, parse_anthropic_sse）移入 LlmService/stream.rs。
   e. StreamEvent 定义在 shared_types/llm.rs 中（不在 Slot 内部）。

3. 零硬编码：
   a. API 端点路径定义为 const（OPENAI_CHAT_PATH, ANTHROPIC_MESSAGES_PATH）。
   b. base_url 从 LlmConfig.base_url 读取，ProviderKind::default_base_url() 只提供默认值。
   c. 超时值使用 DEFAULT_TIMEOUT / DEFAULT_IDLE_TIMEOUT 命名常量。
   d. User-Agent 使用 env!("CARGO_PKG_NAME") / env!("CARGO_PKG_VERSION") 构建。
   e. Anthropic API 版本定义为 const ANTHROPIC_API_VERSION。
   f. Provider key 使用 PROVIDER_LLM / PROVIDER_LLM_FORMAT_ADAPTER 常量。
   g. 日志前缀定义为 const LOG_PREFIX。

4. 完整实现：
   - ServicePlugin 全部 6 个方法（init/start/handle_signal/stop/shutdown/name）必须完整实现。
   - handle_signal 必须处理全部 6 种信号（GracefulShutdown / ImmediateShutdown / HealthCheck / Suspend / Resume / ConfigReload）。
   - executors 中 ProviderExecutor::execute() 的请求构建、发送、响应解析必须完整。
   - SSE 解析函数必须能正确处理流式事件（TextDelta, ToolCallDelta, End）。
   - 不允许使用 todo!()、unimplemented!() 或空函数体。

5. 错误处理：
   - 所有 HTTP 调用错误必须分类为 LlmError（ApiError / Timeout / NetworkError / ParseError / StreamError / ConfigError）。
   - 重试逻辑必须检查 LlmError::is_retryable()，非可重试错误不会重试。
   - Slot::run() 捕获 LlmError 后转为 Thought::Final { answer: error.suggestion() }，不向上传播 Err。
   - 不允许 unwrap()（测试除外，测试中的 unwrap() 必须有 "测试中安全" 注释）。

6. 测试同步生成：
   - ConfigHolder：get/update/provider_kind/is_stream_enabled。
   - ErrorClassifier：4 种 classify_* 方法 + is_retryable。
   - MultimodalFormatter：to_openai / to_anthropic（含 text/image/audio/multimodal 开关）。
   - RetryManager：成功一次返回、可重试错误重试 N 次、不可重试错误不重试。
   - ProviderDispatcher：dispatch 路由正确性。
   - OpenAiExecutor / AnthropicExecutor：mock HTTP 响应，验证请求体格式和响应解析。
   - StreamProcessor：mock SSE 响应体，验证事件解析。
   - LlmService 生命周期：init/start(register_provider)/handle_signal/stop/shutdown。
   - LlmThinkerSlot（精简后）：init/run(直接 mock provider_raw)/shutdown。

7. 模块边界：
   - LlmService 不读 StepContext（由 Slot 负责），不写 Thought（由 Slot 负责），不执行工具（由 ToolExecutorSlot 负责）。
   - LlmService 不组装 System Prompt（由 AssemblerSlot 负责），不管理会话（由 AgentRuntime 负责）。
   - LlmThinkerSlot（精简后）不持有 HTTP 客户端，不执行 HTTP 调用，不管理重试。

8. 注释规则：
   - 只允许写"为什么"的注释，不允许写"做什么"的废话注释。
   - 引用设计文档时用 // 设计文档 §X.Y 格式。
   - 引用协议时用 // Service集成协议 §X 或 // shared_types契约协议 K-R01 格式。

9. 删除规范：
   - 从 llm_thinker 删除的文件必须同时从 mod.rs 中移除 pub mod 声明。
   - 删除 executors/services/components 目录前确认无其他模块引用。
   - 精简 types.rs 时确认不删除仍在使用的内部类型。
```

---

## 详细开发步骤

### Phase 0：确认环境与骨架

**目标**：确保模块声明链完整，当前代码可编译，为新模块做准备。

**操作**：

1. 确认当前 `cargo check` 结果为 0 errors（记录 baseline）
2. 创建 `src/shared_types/llm.rs` 空文件
3. 更新 `src/shared_types/mod.rs` 添加 `pub mod llm;`
4. 更新 `src/plugins/services/mod.rs` 添加 `pub mod llm;`（如不存在则创建该文件）
5. 创建 `src/plugins/services/llm/` 目录 + `mod.rs` 占位

**验收标准**：
- `cargo check` 无 error
- `use crate::shared_types::llm::*` 可被引用（虽然内容为空）
- 目录结构完整

---

### Phase 1：shared_types 契约层

**目标**：创建 `shared_types/llm.rs`，定义所有 Provider key 常量、Provider trait、跨插件数据类型。本阶段不影响其他模块的编译（只新增不修改）。

**文件**：`src/shared_types/llm.rs`

#### 步骤 1.1：Provider key 常量（shared_types契约协议 §2）

```rust
/// LLM 对话能力——由 LlmService 注册，LlmThinkerSlot/RuleLlmSelector 消费
pub const PROVIDER_LLM: &str = "llm";

/// 厂商格式适配器——由 LlmService 注册，AssemblerSlot 消费
/// 与 Assembler 的 LlmOutputAdapter（上下文排版优化）职责不同
pub const PROVIDER_LLM_FORMAT_ADAPTER: &str = "llm_format_adapter";
```

**红线检查**（K-R01）：禁止在 `register_provider()` 或 `provider_raw()` 中使用裸字符串——必须使用上述常量。

#### 步骤 1.2：Provider trait（shared_types契约协议 §3）

```rust
use async_trait::async_trait;

/// LLM 调用契约（shared_types契约协议 §3.1）
/// 服务方：LlmService 实现此 trait
/// 消费方：LlmThinkerSlot/RuleLlmSelector 通过 provider_raw(PROVIDER_LLM) 调用
#[async_trait]
pub trait LlmContract: Send + Sync {
    async fn chat(
        &self,
        config: Option<LlmConfig>,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError>;

    fn get_public_config(&self) -> LlmPublicConfig;
}

/// 厂商格式适配器（shared_types契约协议 §3.1）
/// 服务方：LlmService 根据 provider 类型注册对应适配器
/// 消费方：AssemblerSlot 输出阶段调用
/// 注意：和 Assembler 的 LlmOutputAdapter（上下文排版优化）是不同概念
#[async_trait]
pub trait LlmFormatAdapter: Send + Sync {
    fn format_system_prompt(&self, thought: &Thought) -> String;
    fn format_assistant_message(&self, thought: &Thought) -> Message;
}
```

**红线检查**（T-R01）：trait 定义在 `shared_types/llm.rs` 中，不能在 `services/*` 或 `slots/*` 内部模块中定义。

#### 步骤 1.3：跨插件数据结构

从 `plugins/slots/llm_thinker/types.rs` 迁移到 `shared_types/llm.rs` 的类型：

| 类型 | 说明 | 需要修改 |
|:-----|:------|:---------|
| `ProviderKind` | 枚举 + `default_base_url()` | 删除 `default_base_url()` 中的 `#[cfg(test)]` hack |
| `RetryBackoff` | 枚举 + `Default` | 原样迁移 |
| `LlmPairConfig` | 结构体 | 原样迁移 |
| `LlmConfig` | 结构体（18 字段） | 原样迁移 |
| `ChatResponse` | 枚举 | 原样迁移 |
| `StreamEvent` | 枚举 | 从 Slot 内部移到此处 |
| `ThinkerError` | 枚举 → 重命名为 `LlmError` | **重命名**，添加 `#[derive(thiserror::Error)]`，添加 `ConfigError` 变体 |
| `default_timeout()` / `default_true()` / `default_max_retries()` / `default_context_window()` | 函数 | 原样迁移 |
| `DEFAULT_TIMEOUT` / `DEFAULT_IDLE_TIMEOUT` | **新增**常量 | 新增 |

**关键修改**：`ThinkerError` → `LlmError`
- 添加 `ConfigError(String)` 变体
- 添加 `#[derive(thiserror::Error)]` 和 `#[error("...")]` 格式化
- `is_retryable()` 和 `suggestion()` 方法原样迁移
- `ApiError` 变体的 `retryable: bool` 字段保留

```rust
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
```

**类型重导出**：`shared_types/mod.rs` 中可能已有 `Message`、`ContentBlock`、`ToolDefinition`、`Thought` 等的重导出。`shared_types/llm.rs` 通过 `crate::shared_types::` 引用它们。

#### 步骤 1.4：DynProvider 适配

确保 `LlmContract` 和 `LlmFormatAdapter` 可以通过 `DynProvider<T>` 包装：

```rust
// 注册方（LlmService::start 中）：
let contract: Arc<dyn LlmContract> = Arc::new(self.clone());
ap.register_provider(PROVIDER_LLM, Arc::new(DynProvider(contract)));

// 消费方（LlmThinkerSlot::run 中）：
let raw = ap.provider_raw(PROVIDER_LLM)
    .ok_or(PluginError::NotFound("LLM 不可用".into()))?;
let wrapper = raw.downcast::<DynProvider<dyn LlmContract>>()
    .map_err(|_| PluginError::Internal("LLM 类型不匹配".into()))?;
let response = wrapper.0.chat(Some(config), &messages, &tools, &trace_id).await;
```

**红线检查**（D-R01）：不存在 `DynLlmContract`——统一使用 `DynProvider<T>`。

**验收标准**：
- `cargo check` 无 error
- `PROVIDER_LLM` 和 `PROVIDER_LLM_FORMAT_ADAPTER` 可被外部引用
- `LlmContract` 和 `LlmFormatAdapter` 可被外部引用
- `LlmConfig` / `ChatResponse` / `StreamEvent` / `LlmError` / `ProviderKind` / `RetryBackoff` 可被外部引用
- `DynProvider<dyn LlmContract>` 可通过编译

---

### Phase 2：创建 LlmService 目录结构

**目标**：从 `plugins/slots/llm_thinker/` 迁移 12 个文件到 `plugins/services/llm/`，去掉 Component trait，替换 `ThinkerError` → `LlmError`。

#### 步骤 2.1：ConfigHolder（config.rs）

从 `components/config_provider.rs` 迁移，去掉 Component trait：

```rust
/// 设计文档 §4.1：LlmService 配置持有者
/// 原 ConfigProvider，去掉 Component trait 后降级为普通 struct
pub struct ConfigHolder {
    config: RwLock<LlmConfig>,
    pair_config: Option<LlmPairConfig>,
}

impl ConfigHolder {
    pub fn new(config: LlmConfig) -> Self { ... }
    pub fn get(&self) -> LlmConfig { ... }
    pub fn update(&self, config: LlmConfig) { ... }
    pub fn provider_kind(&self) -> ProviderKind { ... }
    pub fn is_stream_enabled(&self) -> bool { ... }
}
```

**修改清单**：
- 删除 `Component` impl 块（`meta()`、`clone_box()`、`as_any()`、`init()`、`process()`、`shutdown()`）
- 删除 `AccessPoint` 相关导入
- 导入路径改为 `crate::shared_types::llm::{LlmConfig, LlmPairConfig, ProviderKind}`
- 删除 `validate_config()` 函数（校验逻辑移到 `LlmService::init()` 中）
- 添加 `pub` 到结构体和方法
- 实现 `Default`

#### 步骤 2.2：ErrorClassifier（error.rs）

从 `components/error_classifier.rs` 迁移，去掉 Component trait：

**修改清单**：
- 删除 `Component` impl 块
- 导入 `LlmError` 代替 `ThinkerError`
- 所有方法返回 `LlmError` 类型
- 保持 `is_retryable()` 和 `suggestion()` 方法

#### 步骤 2.3：MultimodalFormatter（formatter.rs）

从 `components/multimodal_formatter.rs` 迁移，去掉 Component trait：

**修改清单**：
- 删除 `Component` impl 块
- 保留 `to_openai()` 和 `to_anthropic()` 方法
- 保留内部函数 `to_openai_text_only()`、`block_to_openai()`、`block_to_anthropic()` 等

#### 步骤 2.4：RetryManager（retry.rs）

从 `components/retry_manager.rs` 迁移，去掉 Component trait：

**修改清单**：
- 删除 `Component` impl 块
- 所有 `ThinkerError` 替换为 `LlmError`
- 导入路径更新
- `call_with_retry()` 签名中的 `F: Fn() -> Fut + Send` 保持不变

#### 步骤 2.5：StreamProcessor（stream.rs）

从 `components/stream_processor.rs` 迁移，去掉 Component trait。**SSE 解析函数（`parse_openai_sse`、`parse_anthropic_sse`）随同迁移。**

**修改清单**：
- 删除 `Component` impl 块
- 导入路径更新
- 所有 `ThinkerError` 替换为 `LlmError`
- `StreamEvent` 从 `crate::shared_types::llm::StreamEvent` 导入（不再是 Slot 内部类型）

#### 步骤 2.6：Executors 目录

从 `llm_thinker/executors/` 整体迁移到 `llm_thinker/executors/`：

**`executors/provider_executor.rs`**：
- `ProviderExecutor` trait 中的 `ThinkerError` → `LlmError`
- `ProviderDispatcher` 结构不变

**`executors/openai.rs`**：
- 删除 `Component` impl 块
- `ThinkerError` → `LlmError`
- 导入路径更新
- 保留 HTTP POST 逻辑、请求体构建、响应解析

**`executors/anthropic.rs`**（同上）

**`executors/ollama.rs`**（同上，委托 OpenAiExecutor）

#### 步骤 2.7：ChatInvoker（chat.rs）

从 `components/chat_invoker.rs` 迁移，去掉 Component trait：

```rust
/// 设计文档 §4.2：LLM 调用编排器
/// 原 ChatInvoker，去掉 Component trait 后降级为普通 struct
pub struct ChatInvoker {
    dispatcher: ProviderDispatcher,
    formatter: MultimodalFormatter,
    retry: RetryManager,
    error_classifier: ErrorClassifier,
    stream_processor: StreamProcessor,
}

impl ChatInvoker {
    pub fn new() -> Self { ... }

    /// 设计文档 §4.2：完整调用流程
    pub async fn invoke(
        &self,
        config: &LlmConfig,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError> {
        let executor = self.dispatcher.dispatch(&config.provider);
        self.retry.call_with_retry(config, || async {
            executor.execute(&self.dispatcher, config, messages, tools, trace_id).await
        }).await
    }
}
```

**修改清单**：
- 删除 `Component` impl 块
- 删除 `ChatInvocationService` trait 的引用（trait 已删除，方法内联）
- `ThinkerError` → `LlmError`
- 持有 `ProviderDispatcher`、`MultimodalFormatter`、`RetryManager`、`ErrorClassifier`、`StreamProcessor` 的引用

#### 步骤 2.8：LlmService（service.rs）— 核心

实现 `ServicePlugin`（设计文档 §2.3）和 `LlmContract`：

```rust
/// 设计文档 §2.2：LlmService
pub struct LlmService {
    client: Client,
    config: Arc<RwLock<ConfigHolder>>,
    running: AtomicBool,
    suspended: AtomicBool,
    invoker: ChatInvoker,
}

impl LlmService {
    pub fn new() -> Self {
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

#[async_trait]
impl ServicePlugin for LlmService {
    fn name(&self) -> &str { "llm" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> { ... }
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> { ... }
    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> { ... }
    async fn stop(&mut self) -> Result<(), PluginError> { ... }
    async fn shutdown(&mut self) -> Result<(), PluginError> { ... }
}

#[async_trait]
impl LlmContract for LlmService {
    async fn chat(
        &self,
        config: Option<LlmConfig>,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, LlmError> {
        // 使用传入配置或默认配置
        let active_config = config.unwrap_or_else(|| self.config.read().unwrap().get());
        // 委托给 ChatInvoker
        self.invoker.invoke(&active_config, messages, tools, trace_id).await
    }

    fn get_public_config(&self) -> LlmPublicConfig { ... }
}
```

**实现细节**：
- `init()`：读取 `plugin_config["llm"]`，校验必填字段，重建 HTTP 客户端
- `start()`：注册 `PROVIDER_LLM` 和 `PROVIDER_LLM_FORMAT_ADAPTER`
- `handle_signal()`：处理 6 种信号
- `stop()`：设置 `running=false`，`suspended=true`
- `shutdown()`：设置 `running=false`，释放 HTTP 客户端（drop）
- `LlmContract::chat()`：合并配置覆盖后委托给 `ChatInvoker::invoke()`

#### 步骤 2.9：mod.rs

```rust
mod service;
mod config;
mod chat;
mod error;
mod formatter;
mod retry;
mod stream;
mod executors;

pub use service::LlmService;
```

#### 步骤 2.10：模块声明链

确保以下文件包含相应的 `pub mod` 声明：

- `src/plugins/services/mod.rs`：`pub mod llm;`（如不存在则创建）
- `src/shared_types/mod.rs`：`pub mod llm;`

**验收标准**：
- `cargo check` 无 error
- `LlmService` 可被外部导入
- 所有方法签名与设计文档一致
- 所有 `ThinkerError` 已替换为 `LlmError`
- 没有残留的 `Component` trait 实现

---

### Phase 3：精简 LlmThinkerSlot

**目标**：删除 19 个文件，将 24 文件模块精简为 3 文件。

#### 步骤 3.1：删除文件

按顺序删除：

```bash
# 1. 根目录文件
rm src/plugins/slots/llm_thinker/component.rs
rm src/plugins/slots/llm_thinker/orchestrator.rs

# 2. 组件目录
rm -rf src/plugins/slots/llm_thinker/components/

# 3. executors 目录
rm -rf src/plugins/slots/llm_thinker/executors/

# 4. services 目录
rm -rf src/plugins/slots/llm_thinker/services/
```

#### 步骤 3.2：精简 llm_thinker_slot.rs

**删除**：
- `Orchestrator` 相关字段（`orch: Option<Orchestrator>`）
- `ConfigProvider`、`ErrorClassifier` 等 6 个 Component 的 `use` 导入
- `init()` 中的 Orchestrator 创建和组件注册代码（6 个 `orch.register(Box::new(...))`）
- 组件相关的测试代码

**保留**：
- `LlmThinkerSlot` 结构体（字段简化为 `llm_config: Option<LlmConfig>`）
- `SlotPlugin` impl 块（`name()`、`init()`、`run()`、`shutdown()`）
- `run()` 中的 8 步算法框架
- `process_chat_response()` 和 `process_stream()` 方法

**修改**：
- `init()`：只解析 `LlmConfig`，不创建 Orchestrator
- `run()` Step 6：`orch.get_provider::<ChatInvoker>("chat")` → `ap.provider_raw(PROVIDER_LLM)` 然后 `downcast::<DynProvider<dyn LlmContract>>()`
- 导入路径：`LlmConfig`、`ChatResponse`、`LlmError` 等从 `crate::shared_types::llm` 导入
- 错误处理：`ThinkerError` → `LlmError`
- `shutdown()`：只需 `self.llm_config = None;`

```rust
/// 精简后 LlmThinkerSlot
pub struct LlmThinkerSlot {
    llm_config: Option<LlmConfig>,
}

#[async_trait]
impl SlotPlugin for LlmThinkerSlot {
    fn name(&self) -> &str { "llm_thinker" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        let llm_value = ctx.plugin_config.get("llm").cloned()
            .unwrap_or(serde_json::Value::Null);
        let llm_config: LlmConfig = serde_json::from_value(llm_value)
            .map_err(|e| PluginError::Config(format!("解析 LLM 配置失败: {e}")))?;
        self.llm_config = Some(llm_config);
        Ok(())
    }

    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        // Step 1: trace_id
        let trace_id = Uuid::new_v4().to_string();
        // Step 2: 读 tools
        let tools: Vec<ToolDefinition> = ap.read_context_raw("tools")
            .and_then(|any| any.downcast_ref::<Vec<ToolDefinition>>().cloned())
            .unwrap_or_default();
        // Step 3: session 覆盖
        let mut config = self.llm_config.clone()
            .ok_or_else(|| PluginError::Config("LlmConfig not initialized".into()))?;
        // ...（合并 session 覆盖）
        // Step 5: 消息
        let messages: Vec<Message> = ap.messages().to_vec();
        // Step 6: 调 LlmService
        let raw = ap.provider_raw(PROVIDER_LLM)
            .ok_or(PluginError::NotFound("LLM 服务不可用".into()))?;
        let wrapper = raw.downcast::<DynProvider<dyn LlmContract>>()
            .map_err(|_| PluginError::Internal("LLM 类型不匹配".into()))?;
        let result = wrapper.0.chat(Some(config), &messages, &tools, &trace_id).await;
        // Step 7-8: 处理响应、写回 Thought
        let thought = match result {
            Ok(response) => Self::process_chat_response(response, trace_id).await,
            Err(err) => {
                tracing::error!(trace_id, error = ?err, "Chat invocation failed");
                Thought::Final {
                    answer: format!("LLM API error: {}", err.suggestion()),
                    reasoning: String::new(),
                    generated_at: Timestamp::now(),
                }
            }
        };
        // Step 9: 写 context + 返回 Continue
        ap.write_context_raw("thought", Box::new(thought))
            .map_err(|e| PluginError::Runtime(format!("写入 thought 失败: {e}")))?;
        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        self.llm_config = None;
        Ok(())
    }
}
```

#### 步骤 3.3：精简 types.rs

**删除**（已移到 `shared_types/llm.rs`）：
- `ProviderKind` + `default_base_url()`
- `RetryBackoff` + `Default`
- `LlmPairConfig`
- `LlmConfig` + 所有辅助函数
- `ChatResponse`
- `StreamEvent`
- `ThinkerError` + 所有 impl

**保留**：
- `Turn`（ReAct 模式，仅 Slot 内部使用）
- `ModuleConfig`（槽位级配置）
- 重导出：`pub use crate::shared_types::{Message, MessageRole, ContentBlock, ...}`

**新增重导出**（从 shared_types/llm.rs）：
- `pub use crate::shared_types::llm::{LlmConfig, ChatResponse, StreamEvent, LlmError, ProviderKind, RetryBackoff};`

#### 步骤 3.4：更新 mod.rs

```rust
// 从 7 个 pub mod 精简为 1 个
pub mod llm_thinker_slot;
```

#### 步骤 3.5：更新测试

精简后 `llm_thinker_slot` 的单元测试需要：

**删除**：
- 依赖 Orchestrator 的测试（`test_section_3_7_init_success` 中验证 `slot.orch.is_some()` 的断言）
- 依赖 Component 的测试

**保留并修改**：
- `test_section_3_7_name()` — 保留
- `test_section_3_7_init_success()` — 改为只验证 `llm_config.is_some()`
- `test_section_3_7_init_invalid_config()` — 保留（验证 init 拒绝无效配置）
- `test_section_3_7_run_final()` — 改为 mock `provider_raw` 返回 fake `DynProvider<dyn LlmContract>`

**MockAccessPoint 修改**：
- 在 `providers` HashMap 中预注入 `PROVIDER_LLM` → `Arc::new(DynProvider(fake_contract))`
- 创建 `FakeLlmContract` 实现 `LlmContract`

**验收标准**：
- `cargo check` 无 error
- LlmThinkerSlot 只有 3 个文件
- 所有 `mod.rs` 引用已更新
- 测试通过

---

### Phase 4：更新 main.rs

**目标**：注册 LlmService 并在 Pipeline 中确保 LlmThinkerSlot 在其之后运行。

```rust
// main.rs 中注册顺序（设计文档 §6.1）

// 1. 初始化所有 Service
let mut llm_service = LlmService::new();
llm_service.init(&ctx)?;

// ... 其他 Service 初始化 ...

// 2. 启动所有 Service（按依赖拓扑）
let ap = runtime.create_service_access_point();
llm_service.start(ap.clone())?;  // ← 注册 PROVIDER_LLM

// 3. 注册所有 Slot
runtime.register_slot(Phase::Prepare, Box::new(ToolRegistrySlot::new()), &ctx)?;
runtime.register_slot(Phase::Prepare, Box::new(AssemblerSlot::new()), &ctx)?;
runtime.register_slot(Phase::Think, Box::new(LlmThinkerSlot::new()), &ctx)?;  // ← 此时 PROVIDER_LLM 已可用
// ...
```

**注意**：LlmService 必须在 LlmThinkerSlot 之前 start()，否则 `provider_raw(PROVIDER_LLM)` 返回 None。

**验收标准**：
- `cargo check` 无 error
- 运行时 LlmThinkerSlot::run() 能通过 `provider_raw(PROVIDER_LLM)` 获取 LlmContract

---

### Phase 5：更新消费方（Assembler — RuleLlmSelector）

**目标**：更新 `AssemblerSlot/RuleLlmSelector` 使用 `PROVIDER_LLM` 常量。

如果 `RuleLlmSelector` 当前直接引用 `llm_thinker` 内部类型：

```rust
// 修改前：
use crate::plugins::slots::llm_thinker::types::LlmConfig;

// 修改后：
use crate::shared_types::llm::{LlmConfig, LlmContract, PROVIDER_LLM};
```

如果 `RuleLlmSelector` 通过 `provider_raw("llm")` 查找（使用裸字符串），改为使用 `PROVIDER_LLM` 常量（K-R01 合规）。

**验收标准**：
- `cargo check` 无 error
- `grep -rn "provider_raw.*\"llm\"" src/` 返回空（无裸字符串）

---

### Phase 6：终态验证

**目标**：全模块验证，确保拆分完成且没有遗留问题。

```bash
# 1. 编译检查
cargo check 2>&1

# 2. key 一致性检查（K-R01）
grep -rn "register_provider.*\"llm\"\|provider_raw.*\"llm\"" src/
# 不应有输出——所有裸字符串替换为 PROVIDER_* 常量

# 3. trait 定义位置检查（T-R01）
grep -n "pub trait.*LlmContract\|pub trait.*LlmFormatAdapter" src/ | grep -v "shared_types"
# 不应有输出——trait 只应在 shared_types/llm.rs 中定义

# 4. DynProvider 统一性检查（D-R01）
grep -rn "DynLlmContract\|DynLlmFormatAdapter" src/
# 不应有输出——统一使用 DynProvider<T>

# 5. 残留检查
grep -rn "mod component\|mod orchestrator\|mod executors\|mod services" src/plugins/slots/llm_thinker/
# 不应有输出——已被删除

# 6. 模块文件数检查
Get-ChildItem -Recurse -Filter "*.rs" src/plugins/slots/llm_thinker/ | Measure-Object
# 应为 3

# 7. 测试
cargo test --lib 2>&1
```

**终态验收标准**：
- `cargo check` 0 errors, 0 warnings
- 所有 6 项合规检查通过
- Slot 文件数 = 3
- Service 文件数 = 12

---

## 附录 A：协议条款逐条对照

| 协议 | 条款 | 要求 | 合规状态 |
|:----|:-----|:------|:---------|
| Service集成协议 | §1 | ServicePlugin 6 方法 | ✅ Phase 2.8 |
| Service集成协议 | §2 | ServiceAccessPoint.register_provider | ✅ Phase 2.8 start() |
| Service集成协议 | §3 | 6 种信号处理 | ✅ Phase 2.8 handle_signal() |
| Service集成协议 | §4 | YAML 元数据 | ✅ 设计文档 §三 |
| Service集成协议 | §5 | init→start↔signal→stop→shutdown | ✅ Phase 2.8 |
| Slot接入协议 | §1 | SlotPlugin 3 方法 | ✅ Phase 3.2 |
| Slot接入协议 | §6 | SlotDirective::Continue | ✅ Phase 3.2 run() |
| shared_types契约 | K-R01 | 无裸字符串 | ✅ Phase 1.1 |
| shared_types契约 | T-R01 | trait 在 shared_types | ✅ Phase 1.2 |
| shared_types契约 | D-R01 | 统一 DynProvider | ✅ Phase 1.4 |
| 跨平台规范 | §1 | 无硬编码 URL/超时 | ✅ Phase 1.3 + 各步骤 const 定义 |
| 跨平台规范 | §3 | 无 `/tmp/` | ✅ 测试使用 `temp_dir()` |

## 附录 B：文件变更总结

| 操作 | 数量 | 路径 |
|:-----|:-----|:------|
| **新建** | 12 | `src/plugins/services/llm/`（12 文件） |
| **新建** | 1 | `src/shared_types/llm.rs` |
| **修改** | 1 | `src/shared_types/mod.rs`（加 `pub mod llm;`） |
| **修改** | 1 | `src/plugins/services/mod.rs`（加 `pub mod llm;`） |
| **修改** | 1 | `src/main.rs`（注册 LlmService） |
| **修改** | 3 | `src/plugins/slots/llm_thinker/`（3 文件精简） |
| **修改** | 1 | `src/plugins/slots/assembler/`（RuleLlmSelector 导入更新） |
| **删除** | 19 | `src/plugins/slots/llm_thinker/` 下 19 文件 |
