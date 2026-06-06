# Slot 接入协议（Slot Integration Protocol）

## 0. 协议范围

本协议只定义 **Slot 插件如何接入 aagnet 框架**，不关心 Slot 内部如何实现。
Slot 插件只与 `core` 的 `SlotPlugin` trait 和 `SlotAccessPoint` 交互。

Slot 所需的其他能力（记忆、工具、事件等）通过 **Provider 扩展机制** 获得，
由具体的 Service 插件注册到运行时，core 只负责路由，不定义业务接口。

---

## 1. 插件单入口

插件只需要实现 `SlotPlugin`：

```rust
#[async_trait::async_trait]
pub trait SlotPlugin: Send + Sync {
    /// 插件名称
    fn name(&self) -> &str;

    /// 初始化（只调用一次）
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;

    /// 每次 Phase 触发时调用
    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError>;

    /// 清理（只调用一次）
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

### 各方法职责

| 方法 | 调用次数 | 用途 |
|------|---------|------|
| `name` | 多次 | 返回全局唯一标识，用于日志/监控/依赖声明 |
| `init` | 1 | 校验配置、建立连接、分配资源。失败则插件不加载 |
| `run` | 多次 | 执行业务逻辑，通过 `SlotAccessPoint` 与核心和其他服务交互 |
| `shutdown` | 1 | 释放资源、关闭连接 |

---

## 2. 受控访问接口

`SlotAccessPoint` 是插件能与核心交互的**唯一通道**。

### 接口定义

```rust
pub trait SlotAccessPoint {
    // ── Core 内建：与 Pipeline 和会话直接相关 ──
    fn messages(&self) -> &[Message];
    fn session_id(&self) -> &str;
    fn phase_name(&self) -> &str;
    fn current_iteration(&self) -> usize;
    /// 写入观察结果（类型擦除，由 Slot 自行装箱具体 Observation 类型）
    fn write_observation(&mut self, obs: Box<dyn Any + Send>) -> Result<(), PluginError>;

    /// 写入上下文数据（类型擦除，调用方自行装箱）
    fn write_context_raw(&mut self, key: &str, val: Box<dyn Any + Send>) -> Result<(), PluginError>;

    /// 读取上下文数据（类型擦除，调用方自行向下转型）
    fn read_context_raw(&self, key: &str) -> Option<&dyn Any>;

    fn request_jump(&self, phase: &str) -> Result<(), PluginError>;
    fn request_abort(&self) -> Result<(), PluginError>;

    // ── Provider 扩展：获取其他 Service 注册的能力 ──

    /// 按名称查找业务 Provider（返回类型擦除的 Arc，调用方通过 downcast 获取具体类型）
    fn provider_raw(&self, name: &str) -> Option<Arc<dyn Any + Send + Sync>>;
}

> **技术说明**：`SlotAccessPoint` 作为 `dyn` trait 使用，不能有泛型方法。`write_context`/`read_context`/`provider` 均采用类型擦除方案——调用方自行装箱/向下转型。
```

### 2.1 Core 内建方法

| 方法 | 权限 tag | 说明 |
|------|---------|------|
| `messages()` | `messages:read` | 读取当前会话对话历史 |
| `session_id()` | 无（总是允许） | 当前会话 ID |
| `phase_name()` | 无（总是允许） | 当前 Phase 名称 |
| `current_iteration()` | 无（总是允许） | 当前迭代次数 |
| `write_observation()` | `observation:write` | 写入工具观察结果 |
| `write_context_raw()` | `context:write` | 写入上下文数据（类型擦除） |
| `read_context_raw()` | `context:read` | 读取上下文数据（类型擦除） |
| `request_jump()` | `phase:jump` | 请求跳转到指定 Phase |
| `request_abort()` | `phase:abort` | 请求终止当前 Pipeline |

### 2.2 Provider 扩展机制

业务级能力（记忆检索、工具调用、事件订阅等）不写在 `SlotAccessPoint` 上，
而是由各 Service 在启动时以 Provider 形式注册到运行时：

```rust
// MemoryService 在 start() 中注册 Provider：
runtime.register_provider("memory", Arc::new(MyMemoryProvider { ... }));

// ToolService 在 start() 中注册 Provider：
runtime.register_provider("tool", Arc::new(MyToolRegistry { ... }));

// Slot 在 run() 中按需查找：
fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
    let raw = ap.provider_raw("memory")
        .ok_or(PluginError::NotFound("memory provider unavailable".into()))?;
    let mem = raw.downcast::<dyn MemoryProvider>()
        .map_err(|_| PluginError::Internal("type mismatch".into()))?;
    let ctx = mem.query("user preferences").await?;

    let raw = ap.provider_raw("tool")
        .ok_or(PluginError::NotFound("tool provider unavailable".into()))?;
    let tools = raw.downcast::<dyn ToolProvider>()
        .map_err(|_| PluginError::Internal("type mismatch".into()))?;
    let result = tools.call("read_file", args).await?;

    Ok(SlotDirective::Continue)
}
```

**设计要点**：
- Core **不定义** `MemoryProvider`、`ToolProvider` 等业务接口——这些由注册方自行定义
- Core 只负责 `provider_raw(name)` 返回类型擦除的 `Arc`，调用方通过 `downcast` 获取具体类型
- Provider 是否鉴权是其自身职责，core 不插手

---

## 3. 插件元数据

每个插件必须附带一份元数据声明，用于启动时校验。

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `name` | `String` | 是 | 全局唯一标识，必须与 `SlotPlugin::name()` 返回一致 |
| `category` | `"slot"` | 是 | 固定值 |
| `version` | `String` | 是 | 语义版本 |
| `permissions` | `Vec<String>` | 是 | 声明的权限列表 |
| `requires` | `Vec<String>` | 否 | 依赖的其他服务/Provider 名 |
| `conflicts` | `Vec<String>` | 否 | 冲突的插件名 |
| `config_schema` | `Option<JsonSchema>` | 否 | JSON Schema 配置格式 |

### YAML 示例

```yaml
name: llm-thinker
category: slot
version: 0.1.0
permissions:
  - messages:read
  - observation:write
  - context:read
  - context:write
  - phase:jump
requires:
  - identity-context
  - memory
```

---

## 4. 权限枚举

权限是 Slot 声明"我需要调用核心内建的哪些方法"。

| 权限 tag | 对应方法 | 说明 |
|---------|---------|------|
| `messages:read` | `messages()` | 读取对话历史 |
| `observation:write` | `write_observation()` | 写入工具观察结果 |
| `context:read` | `read_context_raw()` | 读取 StepContext 数据 |
| `context:write` | `write_context_raw()` | 写入 StepContext 数据 |
| `phase:jump` | `request_jump()` | 流程跳转 |
| `phase:abort` | `request_abort()` | 终止当前 Pipeline |

Provider 级鉴权由 Provider 自身接口设计决定，core 不过问。

---

## 5. 返回值

```rust
pub enum SlotDirective {
    Continue,        // 继续下一个 Slot
    BreakPhase,      // 跳出当前 Phase
    BreakStep,       // 跳出整个 Step
    RestartStep,     // 重启本 Step
    AbortStep,       // 中止本 Step（错误）
    AbortPipeline,   // 中止整个 Pipeline
    JumpTo(Phase),  // 跳转到指定 Phase
}
```

所有变体必须被正确处理（红线 S-R01）。

---

## 6. 生命周期

```
PluginLoader 读元数据 → 校验依赖与权限
→ init(ctx) → [run(ap) → SlotDirective → ...] → shutdown()
```

- `init`：只调用一次，失败则插件不被加载（红线 S-R02）
- `run`：每次 Phase 触发时调用，通过 `SlotAccessPoint` 与外界交互
- `shutdown`：只调用一次，用于资源清理

---

## 7. 补充说明

- 权限校验在加载时完成，运行时不再校验以保持性能
- `SlotDirective` 中各变体的处理由 Pipeline 自行决定
- 插件不应假设执行顺序，也不应缓存可变内部状态跨 `run` 调用（红线 S-R03）
- 插件通过 Provider 获取的能力是运行时动态的——如果依赖的 Provider 未注册，
  `provider_raw(name)` 返回 `None`，插件应优雅降级或报错

---

## 8. 新增/替换 Slot 标准流程

### 新增（从零到运行）

| 步骤 | 做什么 | 涉及文件 |
|------|--------|---------|
| 1 | 创建插件目录 | `plugins/slots/llm-thinker/` |
| 2 | 实现 `SlotPlugin` | `plugin.rs` |
| 3 | 定义配置结构体 + 默认值 | `config.rs` |
| 4 | 编写 `mod.rs` 重新导出 | `mod.rs` |
| 5 | 在 `plugins/slots/mod.rs` 注册 | 加一行 `pub mod llm-thinker;` |
| 6 | 编写 `PluginMetadata` YAML | 声明 permissions / requires |
| 7 | 运行 `cargo check` 验证 | — |

**共需改 2 个文件**：新建 `plugin.rs` + 修改 `plugins/slots/mod.rs`



---

## 9. 协议特有红线

| 编号 | 红线 | 说明 |
|------|------|------|
| S-R01 | **所有 `SlotDirective` 变体必须被正确处理** | Continue、BreakPhase、BreakStep、RestartStep、AbortStep、AbortPipeline、JumpTo 都不能漏掉 |
| S-R02 | **`init` 失败意味着插件不加载** | `init()` 返回 `Err` 后不允许以退化状态运行 |
| S-R03 | **`run()` 中禁止持有跨次调用的可变状态** | 两次 `run()` 之间不应有隐式状态依赖 |

---

