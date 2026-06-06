# LLM Thinker(LLM(大模型)思考者) Slot(槽口) 设计文档

## 0. 协议依据

本文档严格遵循以下协议：

| 协议 | 应用层 | 说明 |
|------|--------|------|
| **Slot(槽口) 接入协议** | 模块对外接口 | `SlotPlugin`(槽口插件) trait(特质) + `SlotAccessPoint`(槽口访问点) |
| **模块内部组件协议** | 模块内部结构 | `Component`(组件) trait(特质) + `Orchestrator`(协调器) + `InternalAccessPoint`(内部访问点) |

---

## 0.5 功能清单

本模块提供以下能力：

| 功能 | 描述 | 对应 Component(组件) | 状态 |
|------|------|---------------------|------|
| OpenAI(OpenAI) / 兼容 API(接口) 调用 | 调用 OpenAI 官方及所有兼容端点（vLLM、SGLang、LM Studio、LiteLLM 等） | `ChatInvoker(聊天调用器)` → `OpenAiChat(OpenAI聊天)` | 已设计 |
| Anthropic(Anthropic) API(接口) 调用 | 调用 Anthropic Claude 系列模型，含独立 system(系统) 字段和 tool_use(工具使用) 格式 | `ChatInvoker(聊天调用器)` → `AnthropicChat(Anthropic聊天)` | 已设计 |
| Ollama 本地模型调用 | 调用本地 Ollama 部署的模型（复用 OpenAI 兼容端点） | `ChatInvoker(聊天调用器)` → `OllamaChat(Ollama聊天)` → 委托 `OpenAiChat(OpenAI聊天)` | 已设计 |
| 流式传输（SSE(服务器推送事件)） | 支持 OpenAI 和 Anthropic 两种 SSE(服务器推送事件) 格式的实时解析 | `StreamProcessor(流处理器)` | 已设计 |
| 流式超时控制 | 通过 HTTP(超文本传输协议) 客户端超时配置控制请求超时 | `ConfigProvider(配置提供器)` → `LlmConfig.timeout(超时)` | 已设计 |
| Extra Params(额外参数) | 结构化传递 temperature(温度)、max_tokens(最大令牌数)、top_p(核采样) 等参数 | `ConfigProvider(配置提供器)` → `LlmConfig(大模型配置)` | 已设计（部分参数未覆盖） |
| Thinking(思考链) 适配 | Google/DeepSeek/MiniMax/SiliconFlow 等模型的思考链字段提取 | `ChatInvoker(聊天调用器)` 响应解析层 | 未实现 |
| 流式事件拦截 | 在流式事件经过时插入 Hook(钩子)，用于计费增量、Token(令牌) 统计、日志 | `ChatInvoker(聊天调用器)` 流式处理循环 | 未实现 |
| 多模态图片输入 | 将 ContentBlock::Image(内容块::图片) 转换为目标 API(接口) 格式 | `MultimodalFormatter(多模态格式化器)` | 已设计 |
| 错误分类 | 将 LLM(大模型) 调用错误分为 5 类，每类附带解决建议 | `ErrorClassifier(错误分类器)` | 已设计 |
| 重试 | 可重试错误按固定/指数退避策略自动重试 | `RetryManager(重试管理器)` | 已设计 |

---

## 1. 模块定位（Slot(槽口) 接入协议视角）

### 1.1 外部身份

`LlmThinkerSlot`(LLM(大模型)思考者槽口) 是一个 **Slot(槽口) 插件**，实现 `SlotPlugin`(槽口插件) trait(特质)：

| 协议方法 | 调用次数 | 职责 |
|----------|---------|------|
| `name()` | 多次 | 返回 `"llm-thinker"`，用于日志/监控/依赖声明 |
| `init(ctx)` | 1 | 从 `PluginInitContext`(插件初始化上下文) 读取配置，初始化内部所有 Component(组件) |
| `run(ap)` | 多次 | 每次 THINK(思考) 阶段被 Pipeline(管道) 调用，执行 LLM(大模型) 对话 |
| `shutdown()` | 1 | 通知内部 Orchestrator(协调器) 销毁所有 Component(组件)、释放 HTTP(超文本传输协议) 客户端 |

### 1.2 元数据声明

```yaml
name: llm-thinker
category: slot
version: 0.2.0
permissions:
  - messages:read          # 读取对话历史
  - context:read           # 读取 StepContext(步骤上下文) 中的 ToolDefinition(工具定义)
  - context:write          # 写入 Thought(思考结果)
requires:
  - model-registry         # 需要模型目录 Service(服务) 提供 context_window(上下文窗口) 等元数据
  - session-context        # 需要会话上下文 Service(服务) 获取运行时模型覆盖配置
```

### 1.3 通过 SlotAccessPoint(槽口访问点) 获取的外部能力

| 能力 | 来源 | 获取方式 |
|------|------|---------|
| 对话消息(Messages) | Pipeline(管道) | `ap.messages()` — 当前会话的完整消息历史 |
| 会话 ID(Session ID) | Pipeline(管道) | `ap.session_id()` — 用于日志关联和 Provider(提供商) 调用 |
| 当前阶段(Phase) | Pipeline(管道) | `ap.phase_name()` — 校验是否处于 THINK(思考) 阶段 |
| 工具定义(ToolDefinition) | 上游 `tool_registry`(工具注册) Slot(槽口) | `ap.read_context_raw("tools")` |
| 模型元数据 | `model-registry` Service(服务) | `ap.provider_raw("model-registry")` → downcast(向下转型) |
| 会话级模型覆盖 | `session-context` Service(服务) | `ap.provider_raw("session-context")` → downcast(向下转型) |
| 写入 Thought(思考结果) | Pipeline(管道) | `ap.write_context_raw("thought", Box::new(thought))` |

### 1.4 输出契约

`run()` 结束后向 `SlotAccessPoint`(槽口访问点) 写入：

| 写入内容 | 类型 | 条件 | 消费方 |
|----------|------|------|--------|
| Thought(思考结果) | `Thought` | 总是写入 | `tool_executor`(工具执行) Slot(槽口) / `react_loop`(循环) Slot(槽口) |
| Assistant(助手) 消息 | `Message` | 仅当 `Thought::Final`(最终答案) 时 | 对话历史 |
| StepResult(步骤结果) | `StepResponse::Done`(完成) | 仅当 `Thought::Final`(最终答案) 时 | Pipeline(管道) 终止判断 |

返回值始终为 `SlotDirective::Continue`(继续)。

---

## 2. 内部架构总览（模块内部组件协议视角）

### 2.1 组件(Component) 一览

```
┌─────────────────────────────────────────────────────────────────────────┐
│  LlmThinkerSlot (SlotPlugin 入口)                                        │
│  ┌─────────────────────────────────────────────────────────────────────┐ │
│  │  Orchestrator(协调器)                                                │ │
│  │  职责：管理 Component(组件) 生命周期，不包含业务逻辑                    │ │
│  │  init_all() → [按依赖拓扑序 init]                                    │ │
│  │  process_all() → [按依赖拓扑序 process，同层并行]                     │ │
│  │  shutdown_all() → [反向序 shutdown]                                  │ │
│  └──────────────────────────┬──────────────────────────────────────────┘ │
│                             │                                            │
│               注入 InternalAccessPoint(内部访问点)                        │
│                             ▼                                            │
│  ┌─────────────────────────────────────────────────────────────────────┐ │
│  │  Components(组件池)                                                 │ │
│  │                                                                     │ │
│  │  层级 1 (priority=10, 无依赖)                                       │ │
│  │  ┌──────────────────────────────────────────────────────────┐      │ │
│  │  │  ConfigProvider(配置提供器)                                │      │ │
│  │  │    provides: ["config"]                                    │      │ │
│  │  │    职责: 持有 LlmConfig(大模型配置)，提供配置查询/更新接口   │      │ │
│  │  └──────────────────────────────────────────────────────────┘      │ │
│  │                                                                     │ │
│  │  层级 2 (priority=20, 无依赖, 可并行)                               │ │
│  │  ┌────────────────────┐ ┌───────────────────┐ ┌─────────────────┐ │ │
│  │  │ ErrorClassifier   │ │ StreamProcessor   │ │MultimodalFormat │ │ │
│  │  │ 错误分类器         │ │ 流处理器           │ │ 多模态格式化器   │ │ │
│  │  │ provides:         │ │ provides:          │ │ provides:       │ │ │
│  │  │  error_classify   │ │  stream_parse      │ │ multimodal_fmt  │ │ │
│  │  └────────────────────┘ └───────────────────┘ └─────────────────┘ │ │
│  │                                                                     │ │
│  │  层级 3 (priority=30)                                               │ │
│  │  ┌──────────────────────────────────────────────────────────┐      │ │
│  │  │  RetryManager(重试管理器)                                 │      │ │
│  │  │    provides: ["retry"]                                     │      │ │
│  │  │    requires: ["error_classification"]                      │      │ │
│  │  │    职责: 包装异步调用，按策略自动重试可重试错误              │      │ │
│  │  └──────────────────────────────────────────────────────────┘      │ │
│  │                                                                     │ │
│  │  层级 4 (priority=40)                                               │ │
│  │  ┌──────────────────────────────────────────────────────────┐      │ │
│  │  │  ChatInvoker(聊天调用器)                                  │      │ │
│  │  │    provides: ["chat"]                                       │      │ │
│  │  │    requires: ["multimodal_format", "stream_parse", "retry"]  │      │ │
│  │  │    职责: 核心业务编排，协调所有下层 Component(组件)        │      │ │
│  │  │                                                           │      │ │
│  │  │  ┌────────────────────────────────────────────────────┐  │      │ │
│  │  │  │  ProviderDispatcher(提供商分发器, ChatInvoker 内部) │  │      │ │
│  │  │  │  ├─ OpenAiChat(OpenAI聊天)                        │  │      │ │
│  │  │  │  ├─ AnthropicChat(Anthropic聊天)                   │  │      │ │
│  │  │  │  └─ OllamaChat(Ollama聊天) → 委托 OpenAiChat     │  │      │ │
│  │  │  └────────────────────────────────────────────────────┘  │      │ │
│  │  └──────────────────────────────────────────────────────────┘      │ │
│  └─────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

### 2.2 组件依赖关系（DAG(有向无环图)）

```
                          ConfigProvider(配置提供器)
                         priority=10, provides: [config]
                                │
                                ▼ (无数据依赖)
              ┌───────────────────────────────────────────┐
              │  ErrorClassifier(错误分类器)               │
              │  StreamProcessor(流处理器)                 │
              │  MultimodalFormatter(多模态格式化器)        │
              │ priority=20, 无 requires, 可并行 init      │
              └───────────────────┬───────────────────────┘
                                  │
                                  ▼
                       RetryManager(重试管理器)
                       priority=30
                       requires: [error_classification]
                       (需要 ErrorClassifier 的 is_retryable())
                                  │
                                  ▼
                        ChatInvoker(聊天调用器)
                        priority=40
                        requires: [multimodal_format, stream_parse, retry]
                        (需要 MultimodalFormatter 做格式转换，
                         StreamProcessor 做流解析，
                         RetryManager 做重试包装)
```

| 层级 | 组件 | priority(优先级) | provides(提供) | requires(依赖) | 并行 |
|------|------|----------------|---------------|---------------|------|
| 1 | ConfigProvider(配置提供器) | 10 | `config` | — | — |
| 2 | ErrorClassifier(错误分类器) | 20 | `error_classification` | — | ✅ |
| 2 | StreamProcessor(流处理器) | 20 | `stream_parse` | — | ✅ |
| 2 | MultimodalFormatter(多模态格式化器) | 20 | `multimodal_format` | — | ✅ |
| 3 | RetryManager(重试管理器) | 30 | `retry` | `error_classification` | — |
| 4 | ChatInvoker(聊天调用器) | 40 | `chat` | `multimodal_format`, `stream_parse`, `retry` | — |

**运行时并发模型**：上表中的依赖 DAG(有向无环图) 仅控制 `init_all()`(全初始化) 和 `process_all()`(全处理) 的编排顺序。**主业务流程**（`ChatInvocationService::chat()` 调用）不经过 `process_all()`，因此 DAG(有向无环图) **不约束业务并发**。多个 `run()` 可并发调用 `ChatInvoker`，各次调用共享 HTTP(超文本传输协议) 客户端连接池，Component(组件) 的无状态设计保证并发安全（详见第 8.4 节）。

---

## 3. Component(组件) 详解

### 3.1 ConfigProvider(配置提供器)

**元数据声明**：
```rust
ComponentMeta {
    name: "config_provider",
    version: "0.2.0",
    priority: 10,
    provides: &["config"],
    requires: &[],
    config_key: Some("llm"),
}
```

**业务接口 trait(特质)**：
```rust
pub trait ConfigService: Send + Sync {
    /// 获取当前激活的 LLM(大模型) 配置
    fn get_config(&self) -> &LlmConfig;
    /// 获取主备双配置（可选）
    fn get_pair_config(&self) -> &LlmPairConfig;
    /// 运行时更新配置，触发 ConfigReload(配置重载) 信号时调用
    fn update_config(&mut self, config: LlmConfig) -> Result<(), ComponentError>;
    /// 获取 ProviderKind(提供商类型) 枚举
    fn get_provider_kind(&self) -> &ProviderKind;
    /// 判断是否启用流式传输
    fn is_stream_enabled(&self) -> bool;
}
```

**LlmConfig(大模型配置) 完整结构**：

| 字段 | 类型 | 必需 | 默认值 | 说明 |
|------|------|------|--------|------|
| `provider` | `ProviderKind` | 是 | `OpenAi` | 提供商类型，决定请求格式和端点 |
| `base_url` | `String` | 条件 | 各 Provider(提供商) 默认值 | 非标准 API(接口) 地址 |
| `api_key` | `Option<String>` | 条件 | `None` | Anthropic(Anthropic) 必填 |
| `model` | `String` | **是** | — | 模型名称，如 `gpt-4o`、`claude-3-5-sonnet` |
| `max_tokens` | `Option<u32>` | 否 | Provider(提供商) 默认 | 最大生成 Token(令牌) 数 |
| `temperature` | `Option<f32>` | 否 | Provider(提供商) 默认 | 采样温度，0.0~2.0 |
| `top_p` | `Option<f32>` | 否 | Provider(提供商) 默认 | 核采样参数 |
| `stop` | `Option<Vec<String>>` | 否 | `None` | 停止序列 |
| `frequency_penalty` | `Option<f32>` | 否 | Provider(提供商) 默认 | 频率惩罚 |
| `presence_penalty` | `Option<f32>` | 否 | Provider(提供商) 默认 | 存在惩罚 |
| `seed` | `Option<i64>` | 否 | `None` | 随机种子 |
| `timeout` | `Duration` | 否 | 30s | HTTP(超文本传输协议) 连接超时，适用于非流式和流式的初始连接 |
| `idle_timeout` | `Option<Duration>` | 否 | `None` | 流式空闲超时：SSE(服务器推送事件) 流中相邻两个事件之间允许的最大间隔。`None` 表示不限制。非流式请求忽略此字段 |
| `stream` | `bool` | 否 | `false` | 是否启用流式传输 |
| `tools_enabled` | `bool` | 否 | `true` | 是否发送工具定义给 LLM(大模型) |
| `multimodal` | `bool` | 否 | `false` | 是否启用多模态内容转换 |
| `max_retries` | `u32` | 否 | `3` | 最大重试次数 |
| `retry_backoff` | `RetryBackoff` | 否 | `Exponential(1s, 30s)` | 重试退避策略 |
| `context_window` | `u32` | 否 | `128000` | 模型上下文窗口大小 |
| `extra_headers` | `HashMap<String, String>` | 否 | `{}` | 自定义 HTTP(超文本传输协议) 请求头 |
| `enable_tracing` | `bool` | 否 | `false` | 是否记录详细请求/响应日志 |

**ProviderKind(提供商类型) 枚举**：
```rust
pub enum ProviderKind {
    OpenAi,              // OpenAI 官方 API(接口)
    OpenAiCompatible,    // OpenAI 兼容接口（vLLM、SGLang、LM Studio 等）
    Anthropic,           // Anthropic Claude
    Ollama,              // Ollama 本地部署
}
```

**ProviderKind(提供商类型) 默认 base_url(基础URL)**：

| 类型 | 默认 base_url(基础URL) |
|------|----------------------|
| `OpenAi` | `https://api.openai.com/v1` |
| `OpenAiCompatible` | **必填**，无默认值 |
| `Anthropic` | `https://api.anthropic.com` |
| `Ollama` | `http://localhost:11434` |

**`init()` 校验规则**：
1. `model` 字段不能为空，否则返回 `ComponentError::Config("model is required")`
2. `ProviderKind::OpenAiCompatible` 必须提供 `base_url`，否则返回错误
3. `ProviderKind::Anthropic` 必须提供 `api_key`，否则返回错误
4. `temperature` 如果提供，必须在 0.0~2.0 范围内
5. `timeout` 不能低于 1s，否则自动提升到 1s
6. 如果校验失败，`init()` 返回 `Err`，Slot(槽口) 将终止

**`process()` 逻辑**：检查 `ConfigReload`(配置重载) 信号标记（该信号由 `ServiceSignal::ConfigReload`(服务信号::配置重载) 触发，经 Slot(槽口) 框架传递到 `Orchestrator`(协调器) 的 `process_all()`(全处理)）。如果有信号，则重新读取配置源并校验。校验通过后替换内部 `LlmConfig`(大模型配置)。注意：热更新**不重建 HTTP(超文本传输协议) 客户端连接池**，只替换配置值。这意味着新配置的 `timeout`(超时) 等参数在下一个 `chat()`(聊天) 调用时生效，但连接池（如 Keep-Alive(长连接) 缓存）不变——如果 `base_url`(基础URL) 变更，连接池可能包含旧端点的连接，但 `reqwest::Client`(请求客户端) 会自动按 URL(统一资源定位符) 匹配复用，不会串连到错误端点。

---

### 3.2 MultimodalFormatter(多模态格式化器)

**元数据声明**：
```rust
ComponentMeta {
    name: "multimodal_formatter",
    version: "0.2.0",
    priority: 20,
    provides: &["multimodal_format"],
    requires: &[],
    config_key: None,
}
```

**业务接口**：
```rust
pub trait MultimodalService: Send + Sync {
    /// 将 ContentBlock(内容块) 列表转为 OpenAI(OpenAI) API(接口) 格式的 content(内容) 数组
    fn to_openai(&self, blocks: &[ContentBlock], multimodal: bool) -> Vec<serde_json::Value>;
    /// 将 ContentBlock(内容块) 列表转为 Anthropic(Anthropic) API(接口) 格式的 content(内容) 数组
    fn to_anthropic(&self, blocks: &[ContentBlock], multimodal: bool) -> Vec<serde_json::Value>;
}
```

**ContentBlock(内容块) 类型体系**：
```rust
pub enum ContentBlock {
    Text(String),                         // 纯文本
    Image { base64: String, mime_type: String },  // 图片（base64 编码）
    Audio { base64: String, mime_type: String },  // 音频
    File { base64: String, mime_type: String, filename: String },  // 文件
}
```

**转换规则表（multimodal=true 时）**：

| 输入 ContentBlock(内容块) | OpenAI(OpenAI) API(接口) 格式 | Anthropic(Anthropic) API(接口) 格式 |
|--------------------------|-----------------------------|---------------------------------|
| `Text("hello")` | `{"type":"text","text":"hello"}` | `{"type":"text","text":"hello"}` |
| `Image{base64, mime}` | `{"type":"image_url","image_url":{"url":"data:mime;base64,base64"}}` | `{"type":"image","source":{"type":"base64","media_type":"mime","data":"base64"}}` |
| `Audio{base64, mime}` | `{"type":"input_audio","input_audio":{"data":"base64","format":"mime 去掉 audio/ 前缀"}}` | ❌ 忽略，发出 warning(警告) 日志 |
| `File{base64, mime, name}` | `{"type":"file","file":{"filename":"name","file_data":"data:mime;base64,base64"}}` | ❌ 忽略，发出 warning(警告) 日志 |

**multimodal=false 时的行为**：
- 遍历所有 ContentBlock(内容块)，只提取 `as_text()` 返回非空的块
- 所有非文本块（Image(图片)/Audio(音频)/File(文件)）被丢弃
- 文本块用 `\n` 拼接成一个字符串，包装为 `[{"type":"text","text":"拼接后的文本"}]`
- 如果全部丢弃后文本为空，返回 `[{"type":"text","text":""}]`

**用户可见性说明**：当 Anthropic(Anthropic) 模式下丢弃 Audio(音频)/File(文件) 块时，`MultimodalFormatter`(多模态格式化器) 只会发出 `warn!`(警告) 日志。这意味着**用户不会在界面上看到"你的文件被丢弃了"的提示**。如果业务上需要用户感知，建议在上游（调用 `chat()` 之前）由 `ContextAssembler`(上下文组装器) Slot(槽口) 提前检查并提示用户，而不是在格式转换层做——因为格式转换层只做 JSON(JSON格式) 编排，不应持有用户交互能力。`MultimodalFormatter`(多模态格式化器) 只承担"能转则转，不能转则忽略并告警"的职责。

**`init()` 逻辑**：无操作，全零成本。
**`process()` 逻辑**：无定期任务，返回 `Processing::Continue`(继续)。

**不包含的职责**：
- 图片/音频/文件的编解码（本模块只做 JSON(JSON格式) 格式编排）
- 文件类型检测（由上游写入 ContentBlock(内容块) 时确定）
- 大小校验（由 HTTP(超文本传输协议) 传输层或配置控制）

---

### 3.3 ErrorClassifier(错误分类器)

**元数据声明**：
```rust
ComponentMeta {
    name: "error_classifier",
    version: "0.2.0",
    priority: 20,
    provides: &["error_classification"],
    requires: &[],
    config_key: None,
}
```

**业务接口**：
```rust
pub trait ErrorClassificationService: Send + Sync {
    /// 根据 HTTP(超文本传输协议) 状态码和响应体分类为 ThinkerError(思考者错误)
    fn classify_http_error(
        status: u16, body: &str, trace_id: &str, provider: &str, model: &str,
    ) -> ThinkerError;
    /// 根据 reqwest 的 Error 分类（超时 vs 网络）
    fn classify_http_client_error(
        err: &reqwest::Error, trace_id: &str, timeout: Duration,
    ) -> ThinkerError;
    /// 根据 JSON(JSON格式) 解析失败分类
    fn classify_parse_error(
        raw: &str, trace_id: &str,
    ) -> ThinkerError;
    /// 判断错误是否可重试
    fn is_retryable(&self, error: &ThinkerError) -> bool;
    /// 获取错误的人类可读解决建议
    fn suggestion(&self, error: &ThinkerError) -> &'static str;
}
```

**ThinkerError(思考者错误) 完整定义**：

```rust
pub enum ThinkerError {
    /// API(接口) 返回了非成功 HTTP(超文本传输协议) 状态码
    ApiError {
        provider: String,     // 提供商名称，如 "openai"
        model: String,        // 模型名
        status: Option<u16>,  // HTTP(超文本传输协议) 状态码
        message: String,      // 错误消息体
        trace_id: String,     // 追踪 ID，用于日志关联
        retryable: bool,      // 是否可重试（5xx=true, 4xx=false）
    },
    /// 请求超时
    Timeout {
        trace_id: String,
        timeout: Duration,    // 当前配置的超时值
    },
    /// 网络连接失败
    NetworkError {
        trace_id: String,
        source: reqwest::Error,
    },
    /// 响应 JSON(JSON格式) 解析失败
    ParseError {
        trace_id: String,
        raw_response: String, // 原始响应字符串，用于调试
    },
    /// 流式处理异常
    StreamError {
        trace_id: String,
        message: String,
    },
}
```

**ThinkerError(思考者错误) 分类规则详情**：

| 错误变体 | 触发场景 | 触发逻辑 | `is_retryable()` | `suggestion()` 建议 |
|----------|---------|---------|-----------------|-------------------|
| `ApiError` | HTTP(超文本传输协议) 响应 status >= 300 | 检查 `response.status()` | `status >= 500`（服务端错误） | "请检查 API key(API密钥) 和 base_url(基础URL) 是否正确" |
| `Timeout` | `reqwest::Error::is_timeout()` 返回 true | 检查 `reqwest::Error` 的 timeout(超时) 标记 | **true** | "请增加 timeout(超时) 配置值，当前为 {timeout:?}" |
| `NetworkError` | `reqwest::Error` 不是 timeout(超时) 且不是 status(状态码) 错误 | 连接被拒绝、DNS(域名系统) 解析失败等 | **true** | "请检查 base_url(基础URL) 是否可达，网络是否正常" |
| `ParseError` | JSON(JSON格式) 解码失败或缺少必要字段 | `serde_json::from_str()` 失败，或检查缺失字段 | **false** | "请检查模型是否支持 tool calling(工具调用) 格式" |
| `StreamError` | SSE(服务器推送事件) 流解析异常 | 流数据格式不符合预期 | **false** | "请检查模型是否支持流式输出，或关闭 stream(流式) 配置" |

---

### 3.4 StreamProcessor(流处理器)

**元数据声明**：
```rust
ComponentMeta {
    name: "stream_processor",
    version: "0.2.0",
    priority: 20,
    provides: &["stream_parse"],
    requires: &[],
    config_key: None,
}
```

**业务接口**：
```rust
pub trait StreamProcessingService: Send + Sync {
    /// 解析 OpenAI 兼容格式的 SSE(服务器推送事件) 流
    /// 输入：reqwest::Response(HTTP 响应)（需已获取完整响应体）
    /// 输出：异步 Receiver(接收者) 流，发送解析后的 StreamEvent(流事件)
    fn parse_openai(
        response: Response,
        trace_id: String,
    ) -> UnboundedReceiver<Result<StreamEvent, ThinkerError>>;

    /// 解析 Anthropic 格式的 SSE(服务器推送事件) 流
    fn parse_anthropic(
        response: Response,
        trace_id: String,
    ) -> UnboundedReceiver<Result<StreamEvent, ThinkerError>>;
}
```

**StreamEvent(流事件) 枚举**：
```rust
pub enum StreamEvent {
    /// 文本增量：LLM(大模型) 生成了新的文本片段
    TextDelta(String),
    /// 工具调用增量：LLM(大模型) 正在构建工具调用参数
    ToolCallDelta {
        index: usize,            // 同次响应中第几个工具调用
        delta: serde_json::Value, // tool_call 的部分 JSON(JSON格式) 数据
    },
    /// 流式结束：包含完整的 Thought(思考结果)
    /// 可能来自 finish_reason(完成原因)="tool_calls" 或 "stop"
    End(Thought),
}
```

#### OpenAI SSE(服务器推送事件) 解析算法 `parse_openai()`

```
输入: HTTP Response, trace_id(追踪ID)
输出: tokio::sync::mpsc::UnboundedReceiver<Result<StreamEvent>>

算法步骤：
1. 将 response 整个读为字符串（await response.text()）
   - 如果读取失败，send Err(StreamError("读取响应失败")) → return
2. 按 '\n' 分割为多行
3. 初始化：full_text = "", tool_calls = empty vec
4. 对每一行 line：
   a. 如果不是以 "data: " 开头 → 跳过
   b. 去掉 "data: " 前缀，得到 data
   c. 如果 data.trim() == "[DONE]" → break（流结束）
   d. 尝试 serde_json::from_str(data) 解析 JSON(JSON格式)
      - 失败 → log(warn) + 跳过，继续下一行（容错）
   e. 从 JSON(JSON格式) 提取 choices[0]
      - 如果有 delta.content → 追加到 full_text + send TextDelta
      - 如果有 delta.tool_calls → 遍历数组：
          * 对每个 tool_call，push((index, tool_call)) 到 tool_calls
          * send ToolCallDelta { index, delta }
      - 如果有 finish_reason：
          * "tool_calls" → 调用 parse_tool_calls_to_thought() → send End
          * "stop" → send End(Thought::Final { answer: full_text })
          * 其他 → send End(Thought::Final { answer: full_text })
          * return（退出循环）
5. 如果循环结束仍未遇到 finish_reason(完成原因) 且 full_text 非空：
   → send End(Thought::Final { answer: full_text })
6. 如果空流 → 不发送任何事件，channel(通道) 自动关闭
```

#### Anthropic SSE(服务器推送事件) 解析算法 `parse_anthropic()`

```
输入: HTTP Response, trace_id(追踪ID)
输出: UnboundedReceiver<Result<StreamEvent>>

Anthropic SSE(服务器推送事件) 事件类型：
- content_block_start: 内容块开始，可能包含 tool_use 的 id(标识符) 和 name(名称)
- content_block_delta: 内容块增量，包含 text(文本) 或 partial_json(部分JSON)
- message_delta: 消息层增量，包含 stop_reason(停止原因)
- message_stop: 消息停止，流结束

状态变量：
- full_text: String = ""
- tool_use_id: Option<String> = None
- tool_name: Option<String> = None
- tool_input_parts: Vec<String> = []

算法步骤：
1. 读取完整响应体为字符串
2. 按 '\n' 分割，对每行：
   a. 如果不是 "data: " 开头 → 跳过
   b. 尝试解析 JSON(JSON格式)
   c. 根据 event_type(事件类型) 分支：
      - "content_block_start":
          * block.type == "tool_use" → 记录 id(标识符)+name(名称)，清空 tool_input_parts
          * block.type == "text" → 无操作（文本在 delta 中逐步到达）
      - "content_block_delta":
          * delta.type == "text_delta" → 提取 text，追加到 full_text，send TextDelta
          * delta.type == "input_json_delta" → 提取 partial_json，推入 tool_input_parts
      - "message_delta":
          * delta.stop_reason == "tool_use"：
              - 拼接 tool_input_parts → 解析 JSON(JSON格式)
              - 构造 Thought::Action(动作) → send End
          * delta.stop_reason == "end_turn"：
              - send End(Thought::Final { answer: full_text })
          * return
      - "message_stop" → 如果 full_text 非空，send End(Final)；return
      - 其他事件类型 → 忽略
```

**注意**：两种解析器均在一个 `tokio::spawn` 任务中运行，channel(通道) 的发送端 `tx` 在任务内持有，接收端 `rx` 返回给调用方。

---

### 3.5 RetryManager(重试管理器)

**元数据声明**：
```rust
ComponentMeta {
    name: "retry_manager",
    version: "0.2.0",
    priority: 30,
    provides: &["retry"],
    requires: &["error_classification"],
    config_key: None,
}
```

**业务接口**：
```rust
pub trait RetryService: Send + Sync {
    /// 带重试的异步调用包装器
    /// 按 config.max_retries(最大重试次数) 和 config.retry_backoff(重试退避) 策略自动重试
    async fn call_with_retry<F, Fut, T>(
        &self,
        config: &LlmConfig,
        call_fn: F,
    ) -> Result<T, ThinkerError>
    where
        F: Fn() -> Fut,
        Fut: Future<Output = Result<T, ThinkerError>>;
}
```

**RetryBackoff(重试退避) 策略**：
```rust
pub enum RetryBackoff {
    /// 固定延迟：每次重试等待相同的时长
    Fixed(Duration),
    /// 指数退避：delay(n) = min(initial * 2^n, max)
    Exponential {
        initial: Duration,  // 初始延迟
        max: Duration,      // 最大延迟上限
    },
}
```

**重试行为**：

| 配置 | 行为 |
|------|------|
| `max_retries = 0` | 不重试，首次失败立即返回 |
| `max_retries = 3` + `Fixed(2s)` | 失败后等待 2s、2s、2s，共 4 次尝试 |
| `max_retries = 3` + `Exponential(1s, 10s)` | 失败后等待 1s、2s、4s（封顶 10s），共 4 次尝试 |

**重试状态机**：
```
                     ┌─────────────┐
                     │  attempt=0  │
                     │  call_fn()  │
                     └──────┬──────┘
                            │
                    ┌───────┴───────┐
                    │               │
               Ok(value)       Err(ThinkerError)
                    │               │
              return Ok     is_retryable()?
                                │       │
                              true    false
                                │       │
                         attempt <    return Err
                         max_retries?
                            │       │
                          true    false
                            │       │
                    计算 delay     return Err
                    sleep(delay)   (最后一个错误)
                    attempt += 1
                            │
                    ┌───────┘
                    │
                    ▼
              ┌─────────────┐
              │  attempt=N  │
              │  call_fn()  │
              └─────────────┘
```

**流式请求的重试行为**：`call_with_retry`(带重试调用) 包装的是整个 `chat()` 调用，不区分流式和非流式。流式请求在 HTTP(超文本传输协议) 层面是一个请求，如果中途出错（空闲超时、连接断开），重试的是**整个流式请求**（重新建立 SSE(服务器推送事件) 连接从头开始），**不支持断点续传**。这是因为 SSE(服务器推送事件) 协议无偏移量概念，服务端不保证可以从中断处继续。如果业务上对此有更高要求（如大 Token(令牌) 生成场景），建议关闭 stream(流式) 改为非流式调用。

**重试耗尽后的错误信息**：当所有重试耗尽时，返回的 `ThinkerError`(思考者错误) 是**最后一次尝试的错误**。这意味着如果第一次是 500，第二次是超时，最终用户只会看到"超时"错误。为便于调试，`RetryManager`(重试管理器) 应在返回最终错误之前记录一条 `error!`(错误) 日志，包含所有尝试的摘要（`attempt=0: ApiError(500), attempt=1: Timeout, attempt=2: ApiError(500)`）。这条日志通过 `tracing::error!`(追踪::错误) 发出，关联 `trace_id`(追踪ID)，不修改 `ThinkerError`(思考者错误) 本身——因为 `ThinkerError`(思考者错误) 属于 Provider(提供商) 层的领域，携带重试链会污染其职责边界。

**并发安全**：`RetryService`(重试服务) 的实现必须是**无状态的**——每次 `call_with_retry`(带重试调用) 调用独立，不共享重试计数器。这自然满足，因为 `call_fn`(调用函数) 闭包和 `attempt`(尝试次数) 变量都是函数局部变量。多个 `run()`(运行) 并发调用 `ChatInvoker`(聊天调用器) 时，各自的重试状态互不干扰。

**访问 ErrorClassifier(错误分类器)**：
```
let handle = ap.call("error_classifier")?;
let classifier = handle.as_any().downcast_ref::<dyn ErrorClassificationService>()?;
if classifier.is_retryable(&err) { ... }
```

---

### 3.6 ChatInvoker(聊天调用器)

**元数据声明**：
```rust
ComponentMeta {
    name: "chat_invoker",
    version: "0.2.0",
    priority: 40,
    provides: &["chat"],
    requires: &["multimodal_format", "stream_parse", "retry"],
    config_key: None,
}
```

**业务接口**：
```rust
pub trait ChatInvocationService: Send + Sync {
    /// 执行 LLM(大模型) 聊天调用
    async fn chat(
        &self,
        config: &LlmConfig,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<ChatResponse, ThinkerError>;
}

pub enum ChatResponse {
    /// 非流式完整响应
    Complete(Thought),
    /// 流式响应
    Stream(UnboundedReceiver<Result<StreamEvent, ThinkerError>>),
}
```

#### 3.6.1 ProviderDispatcher(提供商分发器)

`ChatInvoker`(聊天调用器) 内部持有一个 `ProviderDispatcher`(提供商分发器)，根据 `LlmConfig.provider`(提供商类型) 路由到对应执行器：

```rust
impl ProviderDispatcher {
    fn dispatch(&self, provider: &ProviderKind) -> &dyn ProviderExecutor;
}

trait ProviderExecutor: Send + Sync {
    async fn execute(
        &self,
        dispatcher: &ProviderDispatcher,
        config: &LlmConfig,
        messages: &[Message],
        tools: &[ToolDefinition],
        trace_id: &str,
    ) -> Result<ChatResponse, ThinkerError>;
}
```

| ProviderKind(提供商类型) | 执行器实现 | HTTP(超文本传输协议) 端点 |
|------------------------|-----------|----------------------|
| `OpenAi`(OpenAI) | `OpenAiExecutor(OpenAI执行器)` | `{base_url}/chat/completions` |
| `OpenAiCompatible`(OpenAI兼容) | `OpenAiExecutor(OpenAI执行器)` | `{base_url}/chat/completions` |
| `Anthropic`(Anthropic) | `AnthropicExecutor(Anthropic执行器)` | `{base_url}/v1/messages` |
| `Ollama`(Ollama) | `OllamaExecutor(Ollama执行器)` → 委托 `OpenAiExecutor(OpenAI执行器)` | `{base_url}/chat/completions` |

#### 3.6.2 OpenAiExecutor(OpenAI执行器)

**消息格式转换（`build_openai_messages()`）**：

| 内部 MessageRole(消息角色) | OpenAI role(角色) | 特殊处理 |
|------------------------|------------------|---------|
| `System`(系统) | `"system"` | 无 |
| `User`(用户) | `"user"` | content(内容) 经 `MultimodalFormatter(多模态格式化器)->to_openai()` |
| `Assistant`(助手) | `"assistant"` | 无 |
| `Tool`(工具) | `"tool"` | **必须**提供 `tool_call_id`，否则跳过该消息并发出 `warn!`(警告) 日志（含 trace_id(追踪ID) 和 tool_call_id(工具调用ID) 缺失提示） |

**跳过 Tool(工具) 消息的影响**：跳过可能导致 LLM(大模型) 看到的对话历史不完整——它发出 tool_call(工具调用) 后没有得到对应的 tool_result(工具结果)，下一次调用可能重试或报错。常见原因为上游 `ToolExecutorSlot`(工具执行器槽口) 未正确写入 `tool_call_id`(工具调用ID)。目前设计选择跳过+告警而不是中断，以保证 Pipeline(管道) 不断流。如果业务上要求严格一致性，可在 `ConfigProvider`(配置提供器) 中增加一个 `strict_tool_message`(严格工具消息模式) 开关——启用时跳过触发 `SlotError`(槽口错误)。

**工具定义格式转换（`build_openai_tools()`）**：
```json
{
  "type": "function",
  "function": {
    "name": "...",
    "description": "...",
    "parameters": { ... }
  }
}
```

**HTTP(超文本传输协议) 请求体**：
```json
{
  "model": "gpt-4o",
  "messages": [...],
  "max_tokens": 4096,
  "temperature": 0.7,
  "top_p": 0.9,
  "stop": ["\n\n\n"],
  "stream": false,
  "tools": [...]
}
```

- `max_tokens`、`temperature`、`top_p`、`stop`、`stream`、`tools` 均为可选字段，仅当 `Some`(存在值) 或 `true` 时加入 body(请求体)
- 请求头：`Authorization: Bearer {api_key}`；如果 `api_key` 为 `None` 则不设置（适用于无需认证的本地端点）
- 额外请求头：`extra_headers` 全部注入

**非流式响应解析 `parse_openai_response()`**：

```
输入: serde_json::Value (HTTP Response JSON body(响应体JSON))
输出: Result<Thought, ThinkerError>

1. 提取 choices[0]
   - 不存在 → Err(ParseError("响应中没有 choices"))
2. 提取 choices[0].message
   - 不存在 → Err(ParseError("choice 中没有 message"))
3. 提取 choices[0].finish_reason (默认 "")
4. 响应中的 `model` 字段（如 `"model": "gpt-4o-mini"`）**不参与本模块的逻辑**——本模块不依赖 API(应用程序编程接口) 返回的模型名做 Token(令牌) 计算或模型识别。Token(令牌) 预算由上游 `context-assembler`(上下文组装器) Slot(槽口) 基于 `LlmConfig.model`(大模型配置::模型) 查询元数据完成。API(应用程序编程接口) 返回的 `model` 仅在流式日志中记录，不影响业务路径。
5. 如果 finish_reason == "tool_calls":
   a. 提取 message.tool_calls 数组
   b. 取第一个 tool_call 的 function.name + function.arguments（arguments 是 JSON(JSON格式) 字符串，需二次解析）
   c. 遍历所有 tool_call，提取完整信息（id + name + arguments）构造 Vec<ToolCall>
   d. 返回 Thought::Action { action: Action { tool_name, arguments, tool_call_id, tool_calls } }
5. 否则（finish_reason == "stop" 或其他）:
   a. 提取 message.content（字符串）
   b. 如果 content 为空 → Err(ParseError("content 为空"))
   c. 返回 Thought::Final { answer: content }
```

**流式响应处理**：
```
调用 StreamProcessor::parse_openai(response, trace_id) → 返回 Receiver(接收者)
ChatInvoker(聊天调用器) 将 Receiver(接收者) 包装为 ChatResponse::Stream(流式) 返回上层
```

#### 3.6.3 AnthropicExecutor(Anthropic执行器)

**Anthropic 专用处理**：

| 内部 MessageRole(消息角色) | Anthropic mapping(映射) | 特殊处理 |
|------------------------|-----------------------|---------|
| `System`(系统) | 提取到顶层 `system` 字段 | 所有 system(系统) 消息合并为一个字符串 |
| `User`(用户) | `"user"` | content(内容) 经 `MultimodalFormatter(多模态格式化器)->to_anthropic()` |
| `Assistant`(助手) | `"assistant"` | 无 |
| `Tool`(工具) | `"user"` | content(内容) 替换为 `{"type":"tool_result","tool_use_id":"...","content":"..."}` |

**系统提示词提取算法**：
```
fn extract_system_prompt(messages):
    system_parts = []
    for msg in messages:
        if msg.role == System:
            for block in msg.content:
                if block.as_text() is not None:
                    system_parts.push(block.as_text())
    return system_parts.join("\n")
```

**HTTP(超文本传输协议) 请求体**：
```json
{
  "model": "claude-3-5-sonnet-20241022",
  "messages": [{"role": "user", "content": [...]}],
  "system": "You are a helpful assistant.",
  "max_tokens": 4096,
  "temperature": 0.7,
  "stream": false,
  "tools": [{"name": "read_file", "description": "...", "input_schema": {...}}]
}
```

**Anthropic vs OpenAI 工具定义格式差异**：

| 方面 | OpenAI(OpenAI) | Anthropic(Anthropic) |
|------|---------------|---------------------|
| 外层包装 | `{"type":"function","function":{...}}` | 直接使用对象 |
| 参数字段名 | `"parameters"` | `"input_schema"` |
| 参数格式 | JSON Schema(JSON模式) | JSON Schema(JSON模式)（相同） |

**非流式响应解析 `parse_anthropic_response()`**：

```
1. 提取 stop_reason(停止原因)
2. 提取 content 数组
3. 如果 stop_reason == "tool_use":
   a. 在 content 中查找 type == "tool_use" 的块
   b. 提取 name + input + id
   c. 返回 Thought::Action
4. 否则：
   a. 收集 content 中所有 type == "text" 的 text 字段
   b. 用 "\n" 拼接
   c. 如果为空 → Err(ParseError)
   d. 返回 Thought::Final { answer }
```

#### 3.6.4 OllamaExecutor(Ollama执行器)

内部创建 `OpenAiExecutor`(OpenAI执行器) 实例，完全委托调用。仅在日志中标注 `provider = "ollama"`。

**模型名传递**：`LlmConfig.model`(大模型配置::模型) 原样透传到 OpenAI 兼容端点的 `"model"` 字段。Ollama 的服务端会回显该值，但实际使用的模型取决于 Ollama 服务端拉取的模型名。例如 `model = "llama3.2:3b"` 直接传递，无需额外转换。如果用户配的模型名 Ollama 服务端不认识，Ollama 会返回 HTTP(超文本传输协议) 404(未找到) 错误，由 `ErrorClassifier`(错误分类器) 按 `ApiError`(API错误) 处理。

### 3.7 `run()` 方法的完整执行流程

```
LlmThinkerSlot::run(ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, SlotError>
    │
    ├── 1. 生成 trace_id(追踪ID) = Uuid::new_v4()
    │
    ├── 2. 读取 ToolDefinition(工具定义)
    │      ap.read_context_raw("tools")
    │      → 如果没有则使用空 Vec
    │
    ├── 3. 查询会话级模型覆盖（可选）
    │      ap.provider_raw("session-context")
    │      → 如果有覆盖，**创建 LlmConfig(大模型配置) 的临时副本**，应用覆盖字段
    │      → 注意：这是**请求级临时覆盖**，不会修改 ConfigProvider(配置提供器) 的全局状态。
    │         ConfigProvider(配置提供器) 始终持有 Pipeline(管道) 启动时加载的基配置。
    │         各次 run() 调用之间互不影响，无并发冲突。
    │
    ├── 4. 查询模型元数据（可选，供未来 Token(令牌) 预算使用）
    │      ap.provider_raw("model-registry")
    │
    ├── 5. 调用 ChatInvoker(聊天调用器) 的 chat(config, messages, tools)
    │      │
    │      ├── 5a. ap.call("config_provider") → get_config()
    │      ├── 5b. 生成 LlmChatRequest { messages, tools }
    │      ├── 5c. 调用 ap.call("retry_manager") → call_with_retry(config, || {
    │      │         内部:
    │      │         1. ProviderDispatcher(提供商分发器).dispatch(config.provider)
    │      │         2. 执行器.execute(dispatcher, config, messages, tools, trace_id)
    │      │            ├── 调用 MultimodalFormatter(多模态格式化器) 做格式转换
    │      │            ├── 发送 HTTP(超文本传输协议) 请求
    │      │            ├── 收到 response → ErrorClassifier(错误分类器).classify_http_error()
    │      │            ├── 非流式 → parse_*_response() → Thought
    │      │            └── 流式 → StreamProcessor(流处理器).parse_*_stream() → Receiver(接收者)
    │      │       })
    │      └── 得到 ChatResponse
    │
    ├── 6. 处理 ChatResponse(聊天响应)
    │      ├── ChatResponse::Complete(thought):
    │      │   └── 进入步骤 7
    │      │
    │      └── ChatResponse::Stream(rx):
    │          ├── answer = ""
    │          ├── final_thought = None
    │          ├── loop recv() on rx:
    │          │   ├── Ok(TextDelta(text)) → answer.push_str(text)
    │          │   ├── Ok(ToolCallDelta{..}) → 忽略（信息在 End 中）
    │          │   ├── Ok(End(thought)) → final_thought = Some(thought); break
    │          │   ├── Err(ThinkerError) → final_thought = Some(Final{"stream error"}); break
    │          │   └── None(channel(通道) 关闭) → final_thought = Some(Final{answer}); break
    │          └── thought = final_thought.unwrap_or(Final{answer})
    │
    ├── 7. 写入输出
    │      ap.write_context_raw("thought", Box::new(thought))
    │
    ├── [注] 消息同步由 ThoughtSyncSlot 负责
    │      LlmThinkerSlot 不再直接操作 messages。
    │      ThoughtSyncSlot (Phase::think 末尾) 读取 thought，
    │      构建 Assistant 消息并通过 ap.append_message() 追加到对话历史。
    │      
    │      旧流程（v2）:
    │        LlmThinkerSlot → ap.write_context_raw("thought", boxed_thought)  # 由 ThoughtSyncSlot 同步到 messages
    │      新流程（v3）:
    │        LlmThinkerSlot → write_context_raw("thought")
    │        ThoughtSyncSlot → ap.append_message(assistant_msg)
    │
    └── 9. 返回 SlotDirective::Continue(继续)
```

---

## 4. Orchestrator(协调器) 编排逻辑

### 4.1 生命周期映射

| SlotPlugin(槽口插件) 方法 | Orchestrator(协调器) 调用 | 说明 |
|-------------------------|------------------------|------|
| `init(ctx)` | `orch.init_all()` | 按 1→2→3→4 层级顺序 init，层级内并行 |
| `run(ap)` | 不调 `process_all()` | `run()` 业务逻辑直接调用 `ChatInvocationService::chat()`，不经过 `Component::process()` |
| `shutdown()` | `orch.shutdown_all()` | 按 4→3→2→1 反向序 shutdown |

### 4.2 `init_all()` 详细流程

```
Orchestrator::init_all()
    │
    ├── [层级 1] ConfigProvider.init()
    │   └── 从 InitContext(初始化上下文) 读取配置段 "llm"
    │   └── 校验配置（model 必填等）
    │   └── 如果失败 → 返回 Err，Slot(槽口) 不加载
    │
    ├── [层级 2] 并行执行：
    │   ├── ErrorClassifier.init() → 无操作
    │   ├── StreamProcessor.init() → 无操作
    │   └── MultimodalFormatter.init() → 无操作
    │
    ├── [层级 3] RetryManager.init()
    │   └── 通过 ap.call("config_provider") 获取默认 LlmConfig(大模型配置)
    │   └── 读取 max_retries(最大重试次数) 和 retry_backoff(重试退避) 作为默认值
    │
    └── [层级 4] ChatInvoker.init()
        └── 创建 HTTP(超文本传输协议) 客户端（共享连接池）
        └── 初始化 ProviderDispatcher(提供商分发器) 的三个执行器
        └── 通过 ap.call("config_provider") 获取 ProviderKind(提供商类型) 验证
```

### 4.3 `process_all()` 定期维护

由外部框架定时触发（如每 30s 或每个 Step(步骤) 结束时）。`process()` 的职责边界：
- **做**：检查配置热更新、健康检查（ping(连接测试)）、定期清理资源、汇报状态/指标
- **不做**：执行业务请求（`chat()` 调用）、修改 Provider(提供商) 层的 LLM(大模型) 请求、修改消息/工具列表、执行重试逻辑

| 组件 | `process()` 行为 |

| 组件 | `process()` 行为 |
|------|-----------------|
| `ConfigProvider(配置提供器)` | 检查 ConfigReload(配置重载) 信号，如有则重新加载配置 |
| `ErrorClassifier(错误分类器)` | 无操作 |
| `StreamProcessor(流处理器)` | 无操作 |
| `MultimodalFormatter(多模态格式化器)` | 无操作 |
| `RetryManager(重试管理器)` | 无操作 |
| `ChatInvoker(聊天调用器)` | 健康检查：用默认配置 ping(连接测试) LLM(大模型) 端点，失败时记录警告日志 |

### 4.4 `shutdown_all()` 详细流程

```
Orchestrator::shutdown_all()
    │
    ├── [层级 4] ChatInvoker.shutdown()
    │   └── 关闭 HTTP(超文本传输协议) 客户端连接池
    │   └── 重置 ProviderDispatcher(提供商分发器) 状态
    │
├── [层级 3] RetryManager.shutdown()
│   └── 无操作（RetryManager(重试管理器) 无状态，无需清理）
│
├── [层级 2] 并行执行：
    │   ├── ErrorClassifier.shutdown() → 无操作
    │   ├── StreamProcessor.shutdown() → 无操作
    │   └── MultimodalFormatter.shutdown() → 无操作
    │
    └── [层级 1] ConfigProvider.shutdown()
        └── 清除配置缓存
```

---

## 5. 完整数据流

```
┌─────────────────────────────────────────────────────────────────────────────────┐
│ [外部] SlotAccessPoint(槽口访问点)                                                │
│                                                                                 │
│  读取:                                                                          │
│  ├── ap.messages()                    → &[Message]          对话历史             │
│  ├── ap.read_context_raw("tools")     → Arc<Vec<ToolDefinition>>  工具列表       │
│  ├── ap.provider_raw("model-registry")→ ModelRegistryProvider  模型元数据        │
│  └── ap.provider_raw("session-context")→ SessionCtxProvider   会话覆盖配置       │
│                                                                                 │
│  写入:                                                                          │
│  └── ap.write_context_raw("thought", ..)    Thought(思考结果)                    │
│                                                                                 │
│  [消息同步在 Phase::think 末尾由 ThoughtSyncSlot 完成]                            │
│  ThoughtSyncSlot: ap.append_message(assistant_msg)                             │
└───────────────────────────────────┬─────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ [SlotPlugin::run()] LlmThinkerSlot                                               │
│                                                                                 │
│  1. 生成 trace_id(追踪ID)                                                       │
│  2. 读取 tools(工具定义) 和外部能力                                              │
│  3. 通过 ConfigService(配置服务) 获取当前 LlmConfig(大模型配置)                  │
│  4. 调用 ChatInvocationService::chat() — 主业务流程                              │
└───────────────────────────────────┬─────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ [ChatInvoker(聊天调用器)::chat()]                                                  │
│                                                                                 │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │ RetryManager(重试管理器) 包装层                                              │  │
│  │ call_with_retry(config, || { ... }) → 可重试错误自动重试                    │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                   │                                              │
│                                   ▼                                              │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │ ProviderDispatcher(提供商分发器)                                            │  │
│  │                                                                           │  │
│  │  ┌──────────────┐  ┌──────────────────┐  ┌──────────────┐                │  │
│  │  │ OpenAiChat   │  │ AnthropicChat    │  │ OllamaChat   │                │  │
│  │  │ (OpenAI/兼容)│  │ (Claude)         │  │ (委托 OpenAI)│                │  │
│  │  └──────┬───────┘  └───────┬──────────┘  └──────────────┘                │  │
│  └─────────┼──────────────────┼─────────────────────────────────────────────┘  │
│            │                  │                                                 │
│            ▼                  ▼                                                 │
│  ┌──────────────────────┐  ┌──────────────────────────┐                       │
│  │ MultimodalFormatter  │  │ extract_system_prompt    │                       │
│  │ .to_openai()         │  │ MultimodalFormatter      │                       │
│  │ build_openai_msgs()  │  │ .to_anthropic()          │                       │
│  │ build_openai_tools() │  │ build_anthropic_msgs()   │                       │
│  └──────────┬───────────┘  │ build_anthropic_tools()  │                       │
│             │              └─────────────┬────────────┘                       │
│             ▼                            ▼                                     │
│  ┌──────────────────────┐  ┌──────────────────────────┐                       │
│  │ HTTP POST /chat/     │  │ HTTP POST /v1/messages    │                       │
│  │   completions        │  │ 头: x-api-key, version   │                       │
│  │ 头: Bearer {api_key} │  │ 体: Anthropic 格式        │                       │
│  │ 体: OpenAI 格式      │  └─────────────┬────────────┘                       │
│  └──────────┬───────────┘                │                                     │
│             │                            │                                     │
│             ▼                            ▼                                     │
│  ┌──────────────────────┐  ┌──────────────────────────┐                       │
│  │ ErrorClassifier     │  │ ErrorClassifier           │                       │
│  │ .classify_http_     │  │ .classify_http_           │                       │
│  │   error()           │  │   error()                 │                       │
│  └──────────┬───────────┘  └─────────────┬────────────┘                       │
│             │                            │                                     │
│      ┌──────┴──────┐              ┌──────┴──────┐                              │
│      │ 非流式/流式  │              │ 非流式/流式  │                              │
│      ▼             ▼              ▼             ▼                              │
│  ┌────────┐ ┌────────────┐  ┌────────┐ ┌────────────┐                        │
│  │parse_  │ │StreamProc │  │parse_  │ │StreamProc   │                        │
│  │openai_ │ │.parse_    │  │anthrop │ │.parse_      │                        │
│  │response│ │openai()   │  │ic_     │ │anthropic()  │                        │
│  │→Thought│ │→Receiver  │  │response│ │→Receiver    │                        │
│  └────────┘ └────────────┘  └────────┘ └────────────┘                        │
│             │                            │                                     │
│             ▼                            ▼                                     │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │ ChatResponse 返回                                                         │  │
│  │ ├── Complete(Thought)    非流式完整结果                                    │  │
│  │ └── Stream(Receiver)     流式事件流                                        │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
└───────────────────────────────────┬─────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────────────────────┐
│ [SlotPlugin::run() 后处理]                                                        │
│                                                                                 │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │ Streaming(流式) 处理（仅 ChatResponse::Stream 时）                         │  │
│  │                                                                           │  │
│  │  loop {                                                                   │  │
│  │    match rx.recv().await {                                                │  │
│  │      Some(Ok(TextDelta(text)))     → answer += text                       │  │
│  │      Some(Ok(ToolCallDelta{..}))   → 忽略（累积，End 时取完整）           │  │
│  │      Some(Ok(End(thought)))        → final_thought = thought; break       │  │
│  │      Some(Err(e))                  → 构造 Final(最终答案) + 错误消息      │  │
│  │      None                          → 构造 Final(最终答案) + 已有文本      │  │
│  │    }                                                                      │  │
│  │  }                                                                        │  │
│  │  thought = final_thought.unwrap_or(Final{answer})                         │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                                                                 │
│  ┌───────────────────────────────────────────────────────────────────────────┐  │
│  │ Thought(思考结果) 后处理                                                    │  │
│  │                                                                           │  │
│  │  if Thought::Final { answer, reasoning }:                                 │  │
│  │    ap.write_context_raw("thought", boxed_thought)  # 由 ThoughtSyncSlot 同步到 messages   ← 追加到对话历史                        │  │
│  │    设置 step_result = Done(answer, reasoning)                              │  │
│  │                                                                           │  │
│  │  if Thought::Action { action, reasoning }:                                │  │
│  │    不修改 messages(消息)，保持 thought(思考结果)                           │  │
│  │                                                                           │  │
│  │  ap.write_context_raw("thought", boxed_thought)                           │  │
│  └───────────────────────────────────────────────────────────────────────────┘  │
│                                                                                 │
└──────────────────────────────┬──────────────────────────────────────────────────┘
                               │
                               ▼
                    SlotDirective::Continue(继续)
```

---

## 6. 错误处理总策略

### 6.1 错误传播层级

```
[层级 1: HTTP(超文本传输协议) 请求层]
    │
    ├── reqwest::Error::is_timeout() = true
    │   └── ErrorClassifier.classify_http_client_error() → ThinkerError::Timeout
    │
    ├── reqwest::Error::is_timeout() = false (连接被拒、DNS(域名系统) 等)
    │   └── ErrorClassifier.classify_http_client_error() → ThinkerError::NetworkError
    │
    └── HTTP(超文本传输协议) 响应 status >= 300
        └── ErrorClassifier.classify_http_error() → ThinkerError::ApiError
    │
    ▼
[层级 2: Provider(提供商) 解析层]
    │
    ├── JSON(JSON格式) 解析失败
    │   └── ErrorClassifier.classify_parse_error() → ThinkerError::ParseError
    │
    ├── 响应中缺少必要字段 (choices[0] / message / content)
    │   └── ThinkerError::ParseError
    │
    └── SSE(服务器推送事件) 流格式异常
        └── ThinkerError::StreamError
    │
    ▼
[层级 3: RetryManager(重试管理器) 层]
    │
    ├── ThinkerError::is_retryable() = true → attempt < max_retries → 重试
    ├── ThinkerError::is_retryable() = true → attempt >= max_retries → 抛出
    └── ThinkerError::is_retryable() = false → 立即抛出
    │
    ▼
[层级 4: ChatInvoker(聊天调用器)]
    │
    └── 所有 ThinkerError(思考者错误) 包装为 ChatResponse::Complete(完整)(Final(最终答案)+错误消息)
        └── 特殊：StreamError 在流处理循环中捕获，同样转为 Final(最终答案)
    │
    ▼
[层级 5: SlotPlugin::run()]
    │
    └── 所有错误转为 Thought::Final(最终答案)，设置 step_result(步骤结果)
    └── 永不返回 Err，总返回 Ok(SlotDirective::Continue)
```

### 6.2 各错误场景的最终用户可见消息

| 错误场景 | 重试行为 | 最终消息 | 错误消息 |
|----------|---------|---------|---------|
| HTTP(超文本传输协议) 401(未授权) (ApiError, retryable=false) | 不重试 | "LLM API(接口) 错误: ..." | "LLM API(接口) 错误。请检查 API key(API密钥) 和 base_url(基础URL) 是否正确" |
| HTTP(超文本传输协议) 429(限流) (ApiError, retryable=false) | 不重试 | "LLM API(接口) 错误: ..." | "LLM API(接口) 限流。请稍后重试" |
| HTTP(超文本传输协议) 500(服务端错误) (ApiError, retryable=true) | 按策略重试，耗尽后 | "LLM API(接口) 错误: ..." | "LLM API(接口) 返回服务端错误 (500)。已自动重试 {n} 次，仍然失败" |
| 超时 (Timeout, retryable=true) | 按策略重试，耗尽后 | "LLM 请求超时" | "LLM 请求超时 (当前配置: 30s)。已自动重试 {n} 次，仍然失败" |
| 网络错误 (NetworkError, retryable=true) | 按策略重试，耗尽后 | "LLM 网络错误: ..." | "LLM 网络错误。请检查网络连接。已自动重试 {n} 次" |
| JSON(JSON格式) 解析失败 (ParseError, retryable=false) | 不重试 | "LLM 响应解析失败: ..." | "LLM 响应解析失败。请检查模型是否支持 tool calling(工具调用) 格式" |
| 流式错误 (StreamError, retryable=false) | 不重试 | "流式响应错误: ..." | "流式响应错误。请检查模型是否支持流式输出，或关闭 stream(流式) 配置" |

---

## 7. 模块边界声明（本模块不做什么）

| 不做的事情 | 原因 | 正确归属 |
|-----------|------|---------|
| 模型选择决策（"这轮用 GPT-4o 还是 Claude？"） | 非 LLM(大模型) 调用者职责 | Pipeline(管道) 配置层 |
| 会话级模型覆盖的存储和维护 | 跨 Slot(槽口) 的可变状态，llm_thinker(大模型思考者) 只读 | `session-context` Service(服务) |
| API Key(API密钥) 轮转和冷却管理 | 传输层基础设施，HTTP(超文本传输协议) 客户端全局行为 | HTTP(超文本传输协议) Client(客户端) 拦截器 或 `key-manager` Service(服务) |
| Failover(故障转移)（换 Key(密钥)/换模型/降级） | 需要全局视野（可用资源列表），非 LLM(大模型) 调用者决策 | `resilience` Service(服务) |
| OAuth(开放授权) token(令牌) 自动刷新 | 认证基础设施，所有 HTTP(超文本传输协议) 请求共享 | HTTP(超文本传输协议) Client(客户端) 拦截器 |
| Token(令牌) 预算计算和上下文窗口估算 | 需要模型目录元数据，应在 LLM(大模型) 调用之前完成 | `context-assembler` Slot(槽口) |
| 上下文压缩（旧轮次摘要化） | 需要独立的 LLM(大模型) 调用做摘要，是独立的业务逻辑 | `context-compactor` Service(服务) |
| 上下文裁剪（丢弃不重要历史） | 策略性决策，非 LLM(大模型) 协议转换 | `context-assembler` Slot(槽口) |
| 计费记录和用量统计 | 纯业务逻辑，与 LLM(大模型) 调用正交 | `metering` Service(服务) |
| 图片/音频/文件的编解码 | 本模块只做 JSON(JSON格式) 格式编排，不做二进制处理 | Tool(工具) 或其他 Slot(槽口) |
| Prompt Cache(提示词缓存) 策略决策 | 缓存策略（"哪些消息值得缓存"）是独立的决策逻辑 | 传输层缓存策略 |

---

## 8. HTTP(超文本传输协议) 客户端设计

本模块内部使用 `reqwest::Client` 发送 HTTP(超文本传输协议) 请求。

### 8.1 超时模型

本模块区分两类超时，不可混用：

| 配置字段 | 适用场景 | 行为 |
|---------|---------|------|
| `config.timeout` | 所有请求（非流式 + 流式的初始连接） | 从请求开始到收到完整响应（非流式）或收到第一个字节（流式）的时间限制 |
| `config.idle_timeout` | 仅流式（SSE(服务器推送事件)） | 流建立后，相邻两个 `data:` 行之间的最大等待时间。超过此间隔未收到新行，认为流已中断。`None` 表示不限制 |

**设计说明**：单次 `timeout`（如 30s）对于 SSE(服务器推送事件) 场景是不够的——一个流可能持续数分钟甚至更长。所以需要独立的 `idle_timeout` 来控制"接收过程中服务端是否还活着"。`idle_timeout` 的实现方式是：在 `StreamProcessor`(流处理器) 的 SSE(服务器推送事件) 解析循环中，对每次 `rx.recv()` 设置 `tokio::time::timeout(idle_timeout)`。

### 8.2 客户端配置

| 配置项 | 值 | 说明 |
|--------|-----|------|
| 连接超时 | `config.timeout` | 建立 TCP(传输控制协议) 连接的超时时间（reqwest(请求) 的 `.connect_timeout()`） |
| 连接池 | 复用 | 使用 reqwest(请求) 默认连接池，支持 Keep-Alive(长连接) |
| User-Agent(用户代理) | `"aagnet-llm-thinker/0.2.0"` | 可被 extra_headers(额外请求头) 覆盖 |
| 重定向 | 禁止 | 不自动跟随重定向 |

### 8.3 连接生命周期

- `reqwest::Client` 在 `ChatInvoker(聊天调用器).init()` 时创建
- 在 `ChatInvoker(聊天调用器).shutdown()` 时随 drop(释放) 自动关闭
- **不**在每个 `run()` 调用中创建新客户端（重用连接池）

### 8.4 并发安全

本模块支持多个 `run()` 调用并发执行（如果 Pipeline(管道) 同时处理多个 Step(步骤) 或会话）。并发安全由以下设计保证：

| 组件 | 并发策略 |
|------|---------|
| `reqwest::Client`(请求客户端) | 内部使用 `Arc`(原子引用计数) 管理连接池，`ChatInvoker`(聊天调用器) 各次 `chat()`(聊天) 调用共享同一客户端。`reqwest::Client`(请求客户端) 实现了 `Send + Sync`(线程安全)，天然支持并发 |
| `ConfigProvider(配置提供器)` | 配置更新通过 `RwLock`(读写锁) 保护。`get_config()`(获取配置) 用读锁，`update_config()`(更新配置) 用写锁。`run()`(运行) 路径只读，不阻塞 |
| `RetryManager(重试管理器)` | 无状态，每次 `call_with_retry()`(带重试调用) 的 attempt(尝试次数) 计数器是局部变量，互不干扰 |
| `MultimodalFormatter(多模态格式化器)` | 纯函数，无状态，可安全并发 |
| `ErrorClassifier(错误分类器)` | 纯函数，无状态，可安全并发 |
| `StreamProcessor(流处理器)` | 每次 `parse_openai/parse_anthropic()`(解析) 调用创建独立的 `tokio::spawn`(异步任务) 任务和 channel(通道)，互不干扰 |

### 8.5 认证方式

| ProviderKind(提供商类型) | 认证方式 |
|------------------------|---------|
| OpenAi(OpenAI) | `Authorization: Bearer {api_key}` |
| OpenAiCompatible(OpenAI兼容) | `Authorization: Bearer {api_key}`（api_key(API密钥) 可选） |
| Anthropic(Anthropic) | `x-api-key: {api_key}`（必填）+ `anthropic-version: 2023-06-01` |
| Ollama(Ollama) | 无认证 |

---

## 9. 未来扩展

### 9.1 Thinking(思考链) 适配（未实现）

**场景**：DeepSeek R1、Google Gemini、MiniMax 等模型在返回最终答案前输出一段"思考链"（reasoning/thinking content），需要在 `parse_*_response()` 中提取并存入 `Thought::reasoning`(推理过程)。

**改动范围**：
- 在 `parse_openai_response()` 中检测 `choices[0].message` 中是否包含 `reasoning_content`(推理内容) 字段
- 在 `parse_anthropic_response()` 中检测 `content` 数组中是否有 type(类型) 为 `"reasoning"` 的块
- 提取的内容存入 `Thought::Final { reasoning, .. }`(最终答案::推理过程) 或 `Thought::Action { reasoning, .. }`(动作调用::推理过程)

### 9.2 流式事件拦截（未实现）

**场景**：在流式事件 TextDelta(文本增量)/ToolCallDelta(工具调用增量)/End(结束) 经过时插入钩子，用于实时计费增量、Token(令牌) 统计、流式日志。

**改动范围**：
- 在 `ChatInvoker(聊天调用器)` 的流式处理循环中增加可选的 `StreamInterceptor(流拦截器)` trait(特质) 调用点
- `StreamInterceptor`(流拦截器) 可注册多个，顺序执行
- 实现 `CountingInterceptor(计费拦截器)` 作为 `metering`(计量) Service(服务) 的一部分

---

## 10. 测试策略

| 测试层级 | 测试对象 | 方法 | 关键用例 |
|---------|---------|------|---------|
| 单元测试 | `ErrorClassifier(错误分类器)` | 构造不同 HTTP(超文本传输协议) 状态码和错误类型，验证分类结果 | 401→ApiError(retryable=false), 500→ApiError(retryable=true), 超时→Timeout |
| 单元测试 | `MultimodalFormatter(多模态格式化器)` | 构造 ContentBlock(内容块) 数组，验证输出 JSON(JSON格式) 结构 | 纯文本→正确格式, 图片→Base64(图片) 格式, 混合多块→正确数量, multimodal=false 丢弃非文本 |
| 单元测试 | `StreamProcessor(流处理器)` | 构造模拟 SSE(服务器推送事件) 文本，验证 StreamEvent(流事件) 序列 | OpenAI 格式 text delta(文本增量), tool_calls delta(工具调用增量), [DONE], Anthropic 格式各事件类型 |
| 单元测试 | `RetryManager(重试管理器)` | 模拟可重试/不可重试错误，验证重试次数和延迟 | 首次成功→1 次调用, 前 N 次失败→重试 N+1 次, 不可重试→不重试 |
| 单元测试 | `openai.rs` 响应解析 | 构造模拟 JSON(JSON格式) 响应体，验证 `parse_openai_response()` | tool_calls(工具调用)→Action(动作), stop→Final(最终答案), 缺失字段→Err |
| 单元测试 | `anthropic.rs` 响应解析 | 构造模拟 Anthropic JSON(JSON格式) 响应体 | tool_use(工具使用)→Action(动作), end_turn(结束)→Final(最终答案) |
| 集成测试 | `ChatInvoker(聊天调用器)` + 模拟 HTTP(超文本传输协议) 服务器 | 使用 `mockito` 模拟 HTTP(超文本传输协议) 端点 | OpenAI(OpenAI) 非流式调用, OpenAI(OpenAI) 流式调用, Anthropic(Anthropic) 调用, Ollama(Ollama) 委托, HTTP(超文本传输协议) 500→重试→成功 |
| 集成测试 | `LlmThinkerSlot(LLM(大模型)思考者槽口)` 完整流程 | 模拟 `SlotAccessPoint(槽口访问点)`，验证 `run()` 输出 | 正常 Final(最终答案)→StepResponse::Done(完成), Action(动作)→thought(思考结果)写入, 全链路错误→Final(最终答案)+错误消息 |
