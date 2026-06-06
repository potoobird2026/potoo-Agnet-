# LLM Thinker Slot 严格 AI 开发计划（修正版）

本计划用于指导 AI 严格按照设计文档生成 llm-thinker 槽口模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：llm-thinker（LLM 思考者）
- **模块定位**：一个实现 `SlotPlugin` trait 的插件，对外仅通过 `SlotAccessPoint` 交互
- **内部组件**：ConfigProvider、ErrorClassifier、MultimodalFormatter、StreamProcessor、RetryManager、ChatInvoker（含三个执行器）
- **代码目录**：所有源码位于 `src/plugins/slots/llm_thinker/` 下
- **依赖项**：详见步骤 0

---

## 硬编码专项预防纲领

在所有开发环节中，硬编码是 AI 最容易犯的顽疾。本计划通过以下三层机制彻底根除：

1. **AI 宪法硬编码禁令**（每轮对话生效）
2. **步骤验收中的硬编码检查项**（人工逐项核对）
3. **终态自动化硬编码扫描**（脚本 + 人工复核）

### 硬编码分类定义

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| URL/端点 | `"https://api.openai.com/v1"` | 从 `LlmConfig.base_url` 读取，默认值在 `ProviderKind::default_base_url()` 中定义 |
| 模型名 | `"gpt-4o"` | 从 `LlmConfig.model` 读取 |
| 超时秒数 | `.timeout(Duration::from_secs(30))` | 使用常量 `DEFAULT_TIMEOUT` 或 `config.timeout` |
| API 版本号 | `"2023-06-01"` | 定义为 `ANTHROPIC_API_VERSION` 常量 |
| User-Agent | `"aagnet-llm-thinker/0.2.0"` | 定义为 `USER_AGENT` 常量 |
| 文件路径 | `"/etc/config.toml"` | 禁止（本模块不涉及文件操作，如有则必须通过环境变量+dirs） |
| 数字阈值 | `max_tokens = 4096` | 从 `LlmConfig.max_tokens` 读取，`None` 则不发送字段 |
| 字符串模板 | `"Bearer " + api_key` | 可接受（api_key 来自配置，非硬编码） |

---

## 项目目录结构

请手动创建以下目录结构（已有文件保留不动）：

```
src/plugins/
├── mod.rs                            # 已有 pub mod slots;
└── slots/
    ├── mod.rs                        # 已有 pub mod llm_thinker;
    └── llm_thinker/
        ├── mod.rs                    # 模块声明（仅 pub mod 重导出）
        ├── llm_thinker_slot.rs       # LlmThinkerSlot 定义（SlotPlugin 实现）
        ├── types.rs                  # 公共数据类型（§3.1–§3.4）
        ├── component.rs              # Component trait + 元数据
        ├── orchestrator.rs           # Orchestrator
        ├── services/                 # 业务 trait 定义
        │   ├── mod.rs
        │   ├── config_service.rs
        │   ├── error_classification_service.rs
        │   ├── multimodal_service.rs
        │   ├── stream_processing_service.rs
        │   ├── retry_service.rs
        │   └── chat_invocation_service.rs
        ├── components/               # 组件实现
        │   ├── mod.rs
        │   ├── config_provider.rs
        │   ├── error_classifier.rs
        │   ├── multimodal_formatter.rs
        │   ├── stream_processor.rs
        │   ├── retry_manager.rs
        │   └── chat_invoker.rs
        └── executors/                # 提供商执行器
            ├── mod.rs
            ├── provider_executor.rs
            ├── openai.rs
            ├── anthropic.rs
            └── ollama.rs
```

模块声明链：

```
src/lib.rs               →  pub mod plugins;
src/plugins/mod.rs        →  pub mod slots;
src/plugins/slots/mod.rs  →  pub mod llm_thinker;
```

---

## AI 宪法（每次对话开始时完整粘贴）

```
[宪法已生效，本次对话必须无条件遵守]

你是一个严格执行设计文档的 Rust 代码生成器。你的代码必须能够直接通过编译、测试，且完全忠实于设计文档。

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、转换规则、流程步骤，必须与设计文档完全一致，不得自行增删改。
2. **零幻觉**：不允许出现设计文档未提及的字段、方法、枚举值或行为。
3. **零硬编码**：
   a. 禁止在代码中直接出现 URL 字符串（如 "https://api.openai.com/v1"）、超时数值（如 30）、API 版本号（如 "2023-06-01"）、模型名、文件路径，除非你已将其定义为具名常量并放在文件顶部。
   b. 所有可配置的值必须来自 LlmConfig 或具名常量，不得凭空出现数字或字符串字面量。
   c. 所有常量必须添加注释说明其来源（设计文档章节号）。
4. **完整实现**：每个函数必须完整实现，不允许使用 `todo!()`、`unimplemented!()` 或空函数体，除非该功能在设计文档中明确标记为"未实现"且你只保留扩展点。
5. **错误处理完整**：所有可能失败的操作必须有错误处理，必须使用文档定义的 `ThinkerError` 或其子变体，不得使用 `unwrap()`（测试除外）。
6. **一致性**：组件名称、方法名、字段名必须与文档完全一致，大小写敏感，单词拼写无误。
7. **禁止额外依赖**：只能使用 `std`、`tokio`、`reqwest`、`serde`、`serde_json`、`uuid`、`tracing`、`async-trait`、`futures` 以及项目内部模块。
8. **注释规则**：只允许写"为什么"的注释（解释非显而易见的决策），不允许写"做什么"的废话注释。如需引用文档，请用 `/// See design doc §X.Y` 格式。
9. **测试同时生成**：为每个公开函数生成单元测试，测试名称必须包含设计文档条款编号（如 `test_section_3_3_classify_401_error`）。
10. **杜绝捷径**：
    - 不能因为类型看起来相似就复用不正确的结构体。
    - 不能将几个功能合并到一个大函数里，必须按文档的组件划分严格分离。
    - 不能跳过输入校验步骤，校验规则必须与文档逐条匹配。
11. **模块边界**：严禁引入计费、Token计数、模型选择、上下文裁剪、API Key轮转等文档明确声明"不做"的功能。
12. **日志规范**：关键路径记录 info，错误记录 error（携带 trace_id），警告记录 warn。禁止打印 api_key 或完整 base64 图片数据。
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架（手动操作）

**目标**：确保项目可编译，目录就绪。

**操作**：

1. 确认 Cargo.toml 中的依赖包含以下清单（缺少的需添加）：

```toml
[dependencies]
tokio = { version = "1", features = ["sync"] }
reqwest = { version = "0.12", features = ["json", "stream"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4", "serde"] }
tracing = "0.1"
thiserror = "1"
async-trait = "0.1"
futures = "0.3"
```

2. 按上面的目录结构创建所有空文件夹和 `mod.rs` 文件（只需建立模块树，暂时留空）。

3. 确保模块声明链完整（已有，无需重复添加）：
   - `src/lib.rs` 已有 `pub mod plugins;`
   - `src/plugins/mod.rs` 已有 `pub mod slots;`
   - `src/plugins/slots/mod.rs` 已有 `pub mod llm_thinker;`

4. 确保 `cargo check` 通过。

---

### 步骤 1：生成所有公共数据类型

**目标**：生成 `types.rs`，包含 LlmConfig、ProviderKind、ContentBlock、ThinkerError、StreamEvent、Thought、Action、ToolCall、ToolDefinition、Message 等基础类型，保证后续所有组件依赖无误。

**文件**：`src/plugins/slots/llm_thinker/types.rs`

**要求**：
1. `ProviderKind` 枚举（OpenAi, OpenAiCompatible, Anthropic, Ollama），实现 `default_base_url()` 返回对应默认值
2. `LlmConfig` 结构体，包含设计文档表格中的所有字段，提供 `Default` 实现
3. `ContentBlock` 枚举（Text, Image, Audio, File），含 `as_text()` 方法
4. `ThinkerError` 枚举（ApiError, Timeout, NetworkError, ParseError, StreamError），含 `is_retryable()` 和 `suggestion()`
5. `StreamEvent` 枚举（TextDelta, ToolCallDelta, End(Thought)）— 详见设计文档 §3.4
6. 辅助类型：Thought、Action、ToolCall、ToolDefinition、Message、StepResponse

**验收标准**：
- `cargo check` 通过
- `cargo test -- types` 所有测试通过
- ProviderKind 变体名无拼写错误
- LlmConfig 字段数量与文档表格一致
- ThinkerError::suggestion() 返回字符串与文档逐字比对
- 所有数字默认值均为常量或 Default 实现

---

### 步骤 2：生成 Component 框架和 Orchestrator

**目标**：建立组件生命周期管理骨架，为后续组件接入做准备。

**文件**：
- `src/plugins/slots/llm_thinker/component.rs` — Component trait + ComponentMeta + ComponentHandle
- `src/plugins/slots/llm_thinker/orchestrator.rs` — Orchestrator

**协议依据**：严格按 `docs/protocol-模块内部组件协议.md` 第 1-5 节定义

**`component.rs` 要求**：

1. **`ComponentMeta` 结构体**（协议 §5）：
```rust
pub struct ComponentMeta {
    pub name: &'static str,
    pub version: &'static str,
    pub priority: u8,          // 越小越先执行
    pub provides: &'static [&'static str],
    pub requires: &'static [&'static str],
    pub config_key: Option<&'static str>,
}
```

2. **`Component` trait**（协议 §1），所有方法为 `async fn`：
```rust
#[async_trait::async_trait]
pub trait Component: Send + Sync {
    fn meta(&self) -> &ComponentMeta;
    fn clone_box(&self) -> Box<dyn Component>;
    async fn init(&mut self, ctx: &InitContext) -> Result<(), ComponentError>;
    // NOTE: 使用 &dyn 而非 &mut dyn AccessPoint。因为 process_all()
    // 用 join_all 组内并行，多个 future 需要共享同一个 AccessPoint；
    // &mut dyn 无法被多个 future 共享，改为 &dyn + interior mutability。
    async fn process(&mut self, ap: &dyn AccessPoint) -> Result<Processing, ComponentError>;
    async fn shutdown(&mut self) -> Result<(), ComponentError>;
}
```

3. **`ComponentHandle` trait**（协议 §2），用于跨组件类型安全调用：
```rust
pub trait ComponentHandle: Send + Sync {
    fn name(&self) -> &str;
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn clone_box(&self) -> Box<dyn ComponentHandle>;
}

// 自动 blanket impl：所有 Component 自动实现 ComponentHandle
impl<T: Component + 'static> ComponentHandle for T { ... }
```

注：`clone_box()` 不是协议原生要求。由于 Component 的 `populate_access_point()` 需要将 `Box<dyn Component>` 复制为 `Box<dyn ComponentHandle>` 并存进 `InternalAccessPointImpl`，而 `Box<dyn Component>` 和 `Box<dyn ComponentHandle>` 是不同 vtable，无法 safe cast。解决方案：Component 和 ComponentHandle 各自提供 clone_box()，用 `unsafe { std::mem::transmute }` 在二者之间做 vtable 指针转换。Soundness 论证：因果式类型（所有 ComponentHandle 同时也是 Component）保证转换后 vtable 兼容。

4. **`AccessPoint` trait**（协议 §3），组件通过它读写共享数据和调用兄弟组件：
```rust
pub trait AccessPoint: Send + Sync {
    // NOTE: 使用类型擦除 read_any/write_any 而非泛型 read<T>/write<T>。
    // 泛型方法使 trait 非 object-safe，但 Component::process 接收 &dyn AccessPoint
    // 要求 trait 是 object-safe。Arc 保证 read_any 返回值的共享所有权。
    fn read_any(&self, key: &str) -> Option<Arc<dyn Any + Send>>;
    // write_any 用 &self 而非 &mut self，原因同上（&dyn AccessPoint 下的并发访问）。
    // interior mutability 由内部 std::sync::RwLock 提供。
    fn write_any(&self, key: &str, val: Arc<dyn Any + Send>) -> Result<(), ComponentError>;
    fn call(&self, name: &str) -> Result<Box<dyn ComponentHandle>, ComponentError>;
    fn config(&self) -> &ModuleConfig;
    fn metrics(&self) -> &MetricsHandle;
    fn log(&self) -> &dyn ModuleLogger;
}
```

5. **`InitContext` 结构体**（协议 §1），从模块配置构造：
```rust
pub struct InitContext {
    pub config: ModuleConfig,
}
```

6. **`ComponentError` 枚举**：
```rust
pub enum ComponentError {
    Config(String),
    Internal(String),
    NotFound(String),
}
```

7. **`Processing` 枚举**（协议 §4）：
```rust
pub enum Processing {
    Continue,
    BreakChain,
    Restart,
    Warn { message: String },
}
```

8. **`ModuleConfig` 结构体** — 轻量包装，当前仅持有 llm 配置段：
```rust
pub struct ModuleConfig {
    pub raw: serde_json::Value,  // 原始配置树，各组件自行解析所需字段
}
```

9. **可见性规则**（协议 §6.1）：`Component`、`ComponentHandle`、`AccessPoint`、`Processing`、`ComponentMeta`、`InitContext`、`ComponentError`、`ModuleConfig` 为 `pub`。`InternalAccessPointImpl` 等内部实现为 `pub(crate)`。

**`orchestrator.rs` 要求**（协议 §5.2）：

```rust
pub struct Orchestrator {
    components: Vec<Box<dyn Component>>,
    parallel_groups: Vec<Vec<usize>>,          // 拓扑排序后可并行执行的组
    access_point: Arc<RwLock<InternalAccessPointImpl>>,
    config: ModuleConfig,
}
```

1. **`register()`** — 注册组件，自动：
   - 校验 `requires` 是否被已有组件的 `provides` 覆盖（同时检查组件名和能力名）
   - 拓扑排序，同层无依赖的归入同一 `parallel_group`
   - 注：未实现全量环检测，因为当前 6 组件的 DAG 是静态已知无环的。如果将来增加动态注册，需补充环检测

2. **`init_all()`** — 按组件注册顺序依次调用 `init()`

3. **`process_all()`** — 按 `parallel_groups` 分层执行，同组内用 `futures::future::join_all` 并发，层间串行等待。支持 `Processing::BreakChain` 提前终止、`Processing::Restart` 重入

4. **`process_all_serial()`** — 串行版，用于组件间有隐式顺序依赖的场景

5. **`shutdown_all()`** — 按注册顺序反向调用 `shutdown()`

**`InternalAccessPointImpl` 结构体**（协议 §3.2）：
```rust
struct InternalAccessPointImpl {
    components: HashMap<String, Box<dyn ComponentHandle>>,
    // NOTE: 用 std::sync::RwLock 包裹是因为 write_any 走 &self（interior mutability），
    // 这是 &dyn AccessPoint 下并发访问的必要条件。
    // 用 Arc 而非 Box，是因为 read_any 返回 Arc<dyn Any + Send> 需要 clone 语义。
    data_share: std::sync::RwLock<HashMap<String, Arc<dyn Any + Send>>>,
    module_config: Arc<ModuleConfig>,
    logger: Box<dyn ModuleLogger>,
    metrics: MetricsHandle,
}

// Safety: Arc<dyn Any + Send> 不是 Send/Sync（Arc<T>: Send 需要 T: Send+Sync），
// 但 InternalAccessPointImpl 所有数据变异均通过 RwLock 同步，跨线程安全。
unsafe impl Send for InternalAccessPointImpl {}
unsafe impl Sync for InternalAccessPointImpl {}

impl AccessPoint for InternalAccessPointImpl { }
```

`InternalAccessPointImpl` 由 `Orchestrator` 统一构造并注入，组件无权自行构造或修改。

**验收标准**：
- `cargo check` 通过
- 测试创建一个虚拟 Component（实现 Component trait，元数据自定义），加入 Orchestrator，验证 init_all / process_all / shutdown_all 顺序
- 测试验证 Processing::BreakChain 能正确阻断后续组件
- 没有硬编码组件名称字符串（测试除外）

---

### 步骤 3：实现 ConfigProvider

**文件**：
- `src/plugins/slots/llm_thinker/services/config_service.rs`
- `src/plugins/slots/llm_thinker/components/config_provider.rs`

**要求**：
1. 实现 Component trait，元数据与文档一致（priority: 10, provides: ["config"]）
2. `init(ctx: &InitContext)` 从 `ctx.config.raw` 读取配置段 `"llm"`，反序列化为 `LlmConfig`，并执行 5 条校验：
   - model 非空
   - OpenAiCompatible 必须有 base_url
   - Anthropic 必须有 api_key
   - temperature 在 0.0..=2.0 范围内（若提供）
   - timeout 不小于 1s，否则提升到 1s
   校验失败返回 `ComponentError::Config(String)`
3. 配置使用 `Arc<RwLock<LlmConfig>>`，保证并发读取
4. `process()` 检查重载标记（用 `AtomicBool` 示意）
5. 测试覆盖所有校验规则（成功和失败情况）

---

### 步骤 4：实现 ErrorClassifier

**文件**：
- `src/plugins/slots/llm_thinker/services/error_classification_service.rs`
- `src/plugins/slots/llm_thinker/components/error_classifier.rs`

**要求**：严格按文档表格分类 HTTP 错误，suggestion 字符串与文档一致。

---

### 步骤 5：实现 MultimodalFormatter

**文件**：
- `src/plugins/slots/llm_thinker/services/multimodal_service.rs`
- `src/plugins/slots/llm_thinker/components/multimodal_formatter.rs`

**要求**：严格按文档转换规则表，multimodal=false 时过滤非文本，Anthropic 下 Audio/File 发出 warn。

---

### 步骤 6：实现 StreamProcessor

**文件**：
- `src/plugins/slots/llm_thinker/services/stream_processing_service.rs`
- `src/plugins/slots/llm_thinker/components/stream_processor.rs`

**要求**：严格按文档 SSE 解析算法，tokio::spawn + mpsc 通道，测试用真实样例验证。

---

### 步骤 7：实现 RetryManager

**文件**：
- `src/plugins/slots/llm_thinker/services/retry_service.rs`
- `src/plugins/slots/llm_thinker/components/retry_manager.rs`

**要求**：从 config 读取 max_retries 和 retry_backoff，实现指数退避，测试覆盖各种重试场景。

---

### 步骤 8a：实现 Provider Executors

**文件**：
- `src/plugins/slots/llm_thinker/executors/provider_executor.rs`
- `src/plugins/slots/llm_thinker/executors/openai.rs`
- `src/plugins/slots/llm_thinker/executors/anthropic.rs`
- `src/plugins/slots/llm_thinker/executors/ollama.rs`

**要求**：消息格式转换按文档，HTTP 用 reqwest，端点从 config.base_url 读取，版本号/User-Agent 定义为常量，测试 mock JSON 验证响应解析。

---

### 步骤 8b：实现 ChatInvoker

**文件**：
- `src/plugins/slots/llm_thinker/services/chat_invocation_service.rs`
- `src/plugins/slots/llm_thinker/components/chat_invoker.rs`

**要求**：持有 ProviderDispatcher，根据 config.provider 分发，用 RetryManager 包装，集成测试 mock 执行器。

---

### 步骤 9：实现 LlmThinkerSlot

**文件**：`src/plugins/slots/llm_thinker/llm_thinker_slot.rs`（入口）<br>
`mod.rs` 负责 `pub mod llm_thinker_slot;` 重导出

**要求**：
1. `init(ctx: &PluginInitContext)`（SlotPlugin 级别）：
   a. 从 `ctx.plugin_config` 读取 LLM 配置段
   b. 构造 `ModuleConfig { raw: ... }`
   c. 创建 `Orchestrator`，逐一 `register()` 6 个 Component
   d. 调用 `orch.init_all()`（内部调用各 Component 的 `init(&InitContext)`）
2. `run(ap: &mut dyn SlotAccessPoint)` 严格按设计文档 9 步实现：
    - trace_id 生成
    - 通过 `ap.read_context_raw("tools")` 获取 tools 列表（设计文档 §3.7 step 2）
   - 调用 ChatInvoker 的 `chat()`
   - 流式事件循环 / 非流式直接返回
   - 后处理：Final 追加消息，Action 不追加
   - 写入 thought 到 ap
   - 始终返回 `SlotDirective::Continue`
3. 错误处理：任何错误转为 Final 错误消息，绝不返回 Err
4. 集成测试模拟 `SlotAccessPoint`，覆盖 Final、Action、错误场景

---

### 步骤 10：终态自检与硬编码审计

1. 全量编译与测试：
```bash
cargo test --all
cargo clippy -- -D warnings
cargo fmt --check
```

2. 硬编码辅助扫描：
```bash
# 查找可能硬编码的 URL
grep -rE '"(https?://|api\.|/v[0-9]+)' src/plugins/slots/llm_thinker/
# 查找可能硬编码的版本号
grep -rE '"[0-9]{4}-[0-9]{2}-[0-9]{2}"' src/plugins/slots/llm_thinker/
# 查找裸超时数字
grep -rE 'from_secs\([0-9]+\)' src/plugins/slots/llm_thinker/
```

3. 人工确认每一条匹配项已合理定义为常量。

---

## 持续纪律

- 每轮对话都先发送完整宪法
- 不接受任何包含 `todo!()` 或占位符的代码
- 发现 AI 幻觉立即打断并要求改正
- 每步人工校验不得跳过
- 提交代码时，commit message 标明完成了哪一步骤
