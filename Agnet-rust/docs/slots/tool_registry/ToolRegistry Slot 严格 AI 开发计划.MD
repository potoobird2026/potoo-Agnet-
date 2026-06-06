# ToolRegistry Slot 严格 AI 开发计划

本计划用于指导 AI 严格按照设计文档生成 tool-registry 槽口模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：tool-registry（工具注册槽口）
- **模块定位**：Pipeline CONTEXT 阶段核心 Slot，职责单一：获取工具列表 → 写入 StepContext。**无内部组件、无 Orchestrator、无配置、无状态**。
- **内部组件**：无（单 SlotPlugin，无需 Orchestrator/Component/AccessPoint 体系）
- **代码目录**：`src/plugins/slots/tool_registry/`
- **文件数**：2（`mod.rs` + `plugin.rs`），无 types.rs / config.rs / error.rs / components/
- **外部类型**：`ToolDefinition` → `crate::shared_types::ToolDefinition`；`ToolProvider` → `crate::shared_types::ToolProvider`（由 ToolsService 实现）
- **设计文档**：`docs/slots/tool_registry/05_tool_registry_槽口开发文档.md`
- **关键约束**：`write_context_raw("tools", Box::new(Arc::new(tools)))` — 写入类型为 `Arc<Vec<ToolDefinition>>`，不是直接 `Vec<ToolDefinition>`

---

## 硬编码专项预防纲领

在所有开发环节中，硬编码是 AI 最容易犯的顽疾。本计划通过以下三层机制彻底根除：

1. **AI 宪法硬编码禁令**（每轮对话生效）
2. **步骤验收中的硬编码检查项**（人工逐项核对）
3. **终态自动化硬编码扫描**（脚本 + 人工复核）

### 硬编码分类定义

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| URL/端点 | — | 不涉及（tool_registry 无 HTTP 请求） |
| 模型名 | — | 不涉及 |
| 超时秒数 | — | 不涉及 |
| API 版本号 | — | 不涉及 |
| User-Agent | — | 不涉及 |
| 文件路径 | — | 不涉及（工具定义由 ToolsService 注册） |
| 数字阈值 | — | 不涉及 |
| 字符串模板 | — | 不涉及（无字符串格式化） |
| Provider key | `"tool"` 写在 provider_raw 调用处 | 本模块仅调用一次，字符串可接受（Provider key 是协议约定，非可配置值） |

---

## 项目目录结构

```
src/plugins/slots/tool_registry/
├── mod.rs          # pub mod plugin + pub use plugin::ToolRegistrySlot
└── plugin.rs       # ToolRegistrySlot 实现 + 测试
```

模块声明链：

```
src/lib.rs                →  pub mod plugins;
src/plugins/mod.rs         →  pub mod slots;
src/plugins/slots/mod.rs   →  pub mod tool_registry;
```

---

## AI 宪法（每次对话开始时完整粘贴）

```
[宪法已生效，本次对话必须无条件遵守]

你是一个严格执行设计文档的 Rust 代码生成器。你的代码必须能够直接通过编译、测试，且完全忠实于 `05_tool_registry_槽口开发文档.md`。

1. **文档唯一真理**：严格按设计文档 §4.2 的完整代码实现，不增删改。ToolRegistrySlot 是单元结构体（pub struct ToolRegistrySlot;），没有任何字段。

2. **零幻觉**：
   - tool_registry 只有 2 个文件：mod.rs + plugin.rs。没有 config.rs、error.rs、types.rs、components/、orchestrator.rs。
   - 没有 Config 结构体、没有错误枚举、没有内部组件、没有 Orchestrator、没有 Component trait。
   - 没有 batch_size、no_timeout、max_retries 等不存在的东西。
   - `run()` 只返回 `Continue`（S-R01），不返回其他变体。

3. **零硬编码**：
   a. 不涉及任何硬编码值——本模块无 URL、无模型名、无超时、无路径、无数字阈值
   b. Provider key `"tool"` 是协议约定的固定标识符，可接受字符串字面量

4. **完整实现**：不允许 `todo!()`、`unimplemented!()` 或空函数体。init 和 shutdown 虽然无操作，但必须完整实现。

5. **错误处理**：
   a. `provider_raw("tool")` 返回 None → `unwrap_or_default()` 降级为空列表
   b. `downcast::<Arc<dyn ToolProvider>>()` 失败 → `unwrap_or_default()` 降级为空列表
   c. `write_context_raw("tools", ...)` 失败 → 传播 Err（唯一可能 Err 的路径）
   d. 不允许 `unwrap()`（测试除外）

6. **类型约束**：
   a. `write_context_raw("tools", Box::new(Arc::new(tools)))` — 类型为 `Arc<Vec<ToolDefinition>>`
   b. `ToolDefinition` 引用 `crate::shared_types::ToolDefinition`，不重复定义
   c. `ToolProvider` 引用 `crate::shared_types::ToolProvider`，不重复定义

7. **注释规则**：
   - 只允许写"为什么"的注释（如降级策略的理由）
   - struct 文档注释必须包含职责说明和规范遵守声明（按设计文档 §4.2 的完整注释）
   - 红线引用用 `// 遵循 S-R0X` 格式

8. **测试同时生成**：
   - 设计文档 §4.2 已提供 4 组完整测试代码，直接使用
   - 测试名称使用中文（与设计文档一致）：正常流程_两个工具、空工具列表、Provider未注册_降级、downcast失败_降级、init_shutdown
   - MockAccessPoint 必须实现 SlotAccessPoint 的所有方法（含未使用的）

9. **模块边界**：
   - 不涉及工具执行（tool_executor）、安全审计（audit_phase）、会话初始化（init_phase）
   - 不引用其他 Slot 的具体类型
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架（手动操作）

**目标**：确保项目可编译，依赖齐全，模块注册。

**操作**：

1. 确认 `shared_types` 中已定义 `ToolDefinition`（三字段：name/description/parameters）和 `ToolProvider` trait（含 `fn list(&self) -> Vec<ToolDefinition>`）

2. 创建 `src/plugins/slots/tool_registry/` 目录及以下空文件：

```
src/plugins/slots/tool_registry/
├── mod.rs           （空文件）
└── plugin.rs        （空文件）
```

3. 在 `src/plugins/slots/mod.rs` 中添加：

```rust
pub mod tool_registry;    // ★ 新增
```

4. 确保 `cargo check` 通过（空模块可能产生未使用警告，可接受）。

**验收标准**：
- `cargo check` 无 error
- 目录结构与上述一致
- `src/plugins/slots/mod.rs` 包含 `pub mod tool_registry;`

---

### 步骤 1：mod.rs

**目标**：按协议 §6.1（虽然本模块无内部组件，仍遵循"暴露最少"原则）。

**文件**：`src/plugins/slots/tool_registry/mod.rs`

```rust
pub mod plugin;

pub use plugin::ToolRegistrySlot;
```

**验收标准**：
- 只暴露 `ToolRegistrySlot`
- 无多余的 `pub use`

---

### 步骤 2：plugin.rs

**目标**：实现 ToolRegistrySlot 的完整生命周期。

**文件**：`src/plugins/slots/tool_registry/plugin.rs`

**要求**：

严格按设计文档 §4.2 的完整实现，包含：

1. **ToolRegistrySlot 结构体**——单元结构体：

```rust
/// 工具注册槽口 —— Pipeline CONTEXT 阶段
///
/// 职责：从 ProviderRegistry 获取工具列表，写入 StepContext。
///
/// 设计决策：
/// - 无状态：不持有跨 run() 的可变状态（S-R03）
/// - 无内部组件：职责单一，不需要 Orchestrator
/// - 降级策略：Provider 不可用时返回空列表，不中断 Pipeline（Slot协议 §7）
///
/// 规范遵守：
/// - 跨平台规范：无硬编码值、无文件路径、无网络调用
/// - Slot协议 §1：SlotPlugin 单入口
/// - Slot协议 §2：只通过 SlotAccessPoint 交互
/// - Slot协议 §3：元数据 permissions=["context:write"], requires=["tool"]
/// - Slot协议 §4：权限 tag 与实际调用一致
/// - Slot协议 §5：返回 Continue
/// - Slot协议 §7：不缓存跨 run() 状态
pub struct ToolRegistrySlot;
```

2. **`new()` + `Default` 实现**：

```rust
impl ToolRegistrySlot {
    pub fn new() -> Self { Self }
}
impl Default for ToolRegistrySlot { fn default() -> Self { Self::new() } }
```

3. **`SlotPlugin` trait 实现**：

**`name()`**：返回 `"tool_registry"`

**`init()`**：
```rust
async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
    // 无状态 Slot，无需初始化
    // S-R02：init 失败意味着插件不加载——此处不会失败
    Ok(())
}
```

**`run()`**（3 段式流程，设计文档 §1.3）：
```
[Step 1] 获取 Provider：provider_raw("tool") → downcast::<Arc<dyn ToolProvider>>()
         → provider.list() → unwrap_or_default() 空列表
         None 或 downcast 失败 → 空列表（Slot协议 §7 优雅降级）

[Step 2] 写入 StepContext：write_context_raw("tools", Box::new(Arc::new(tools)))
         → 唯一可能 Err 的路径

[Step 3] 返回 Continue
```

**`shutdown()`**：
```rust
async fn shutdown(&mut self) -> Result<(), PluginError> {
    // 无状态 Slot，无需清理
    Ok(())
}
```

4. **测试（直接使用设计文档 §4.2 的完整测试代码）**：

| 测试名称 | 场景 | 断言 |
|---------|------|------|
| `正常流程_两个工具` | Provider 包含 2 个工具 | Continue，tools.len()=2 |
| `空工具列表` | Provider 含空列表 | Continue，tools 为空 |
| `Provider未注册_降级` | provider_raw 返回 None | Continue，不 panic，空列表 |
| `downcast失败_降级` | provider_raw 返回 String | Continue，不 panic |
| `init_shutdown` | 无前置条件 | init 和 shutdown 返回 Ok |

**MockAccessPoint 实现要求**：必须实现 `SlotAccessPoint` 的所有 9 个方法（`messages`、`session_id`、`phase_name`、`current_iteration`、`write_observation`、`write_context_raw`、`read_context_raw`、`request_jump`、`request_abort`、`provider_raw`），未使用的方法返回默认值。

**验收标准**：
- `cargo test` 全部通过
- `cargo clippy -- -D warnings` 通过
- `ToolProvider` 引用路径正确（`crate::shared_types::ToolProvider` 或 `crate::plugins::services::tools::ToolProvider`，以实际位置为准）
- 写入类型确认为 `Arc<Vec<ToolDefinition>>`

---

### 步骤 3：注册到 Pipeline

**目标**：将 tool_registry 注册到 Pipeline 的 context 阶段。

**涉及文件**：

1. `src/plugins/slots/mod.rs` —— 已有 `pub mod tool_registry;`（步骤 0 已完成）

2. **Pipeline 构建代码**（`main.rs` 或等效位置）：

```rust
pipeline.add_slot(
    Phase::context(),
    Box::new(ToolRegistrySlot::new()),
);
```

**验收标准**：
- `cargo check` 通过
- `Phase::context()` 阶段已注册 `ToolRegistrySlot`

---

### 步骤 4：终态自检

1. **全量编译与测试**：

```bash
cargo test
cargo clippy -- -D warnings
```

2. **硬编码扫描**：

```bash
# 检查无 todo/unimplemented
rg 'todo!|unimplemented!' src/plugins/slots/tool_registry/ --type rust --glob '!*test*'
# 检查无 panic/unwrap（测试除外）
rg 'unwrap\(\)|expect\(\)|panic!' src/plugins/slots/tool_registry/ --type rust --glob '!*test*'
```

3. **合规检查清单**：

**跨平台规范 10 项**（设计文档 §6.1）：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| 1 | 无硬编码 URL | ✅ 不涉及 |
| 2 | 无硬编码模型名 | ✅ 不涉及 |
| 3 | 无硬编码超时值 | ✅ 不涉及 |
| 4 | 无硬编码 API 版本 | ✅ 不涉及 |
| 5 | 无硬编码 User-Agent | ✅ 不涉及 |
| 6 | 无文件路径操作 | ✅ 不涉及 |
| 7 | 无数字阈值 | ✅ 不涉及 |
| 8 | 无平台指令 | ✅ 不涉及 |
| 9 | 测试使用 Mock，无路径 | ✅ |
| 10 | cargo build + test + clippy 通过 | ☐ 待验证 |

**Slot 协议红线 3 项**（设计文档 §6.2）：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| S-R01 | 所有路径返回 Continue | ✅ |
| S-R02 | init 失败返回 Err（此处不会失败） | ✅ |
| S-R03 | 无状态设计 | ✅ |

**模块边界检查**：

| # | 检查项 | 状态 |
|:-:|--------|:----:|
| mod.rs 只暴露 ToolRegistrySlot | ✅ |
| 无多余文件（仅 2 文件） | ✅ |
| 不引用其他 Slot 具体类型 | ✅ |

---

## 持续纪律

- 每轮对话都先发送完整宪法
- 不接受任何包含 `todo!()` 或占位符的代码
- 发现 AI 幻觉立即打断并要求改正
- 每步人工校验不得跳过
- 提交代码时，commit message 标明完成了哪一步骤
- 如果某步骤产生多余的 warning，必须先解决再进入下一步
- 如果发现设计文档有疏漏，记录到设计文档的 TODO 中，不在代码中猜测
