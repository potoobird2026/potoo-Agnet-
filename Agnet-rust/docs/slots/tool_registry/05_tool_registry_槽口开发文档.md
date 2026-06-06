# tool_registry 槽口开发文档

> 文档版本：v3.0  
> 编写日期：2026-05-30  
> 状态：待开发（按三份规范从零设计，旧代码全部废弃）  
> 优先级：P0（Pipeline CONTEXT 阶段核心 Slot，无此槽口 llm_thinker 无法获取工具定义）  
> 执行规范：《跨平台与硬编码规范》《protocol-Slot接入协议》《protocol-模块内部组件协议》

---

## 0. 设计约束

### 0.1 规范红线

| 来源 | 红线 | 本设计如何遵守 |
|------|------|---------------|
| 跨平台规范 §1 | 禁止硬编码 URL/模型名/超时/路径 | 本槽口无网络调用、无文件 I/O、无硬编码值 |
| 跨平台规范 §2 | 禁止裸用 `/tmp/`、`~`、相对路径 | 本槽口无文件路径操作 |
| 跨平台规范 §3 | 测试禁止硬编码路径、禁止访问真实 API | 测试使用 Mock，无网络、无文件 |
| Slot协议 §1 | SlotPlugin 单入口（init→run→shutdown） | 严格实现三方法生命周期 |
| Slot协议 §2 | 只通过 SlotAccessPoint 与核心交互 | 不直接访问任何核心状态 |
| Slot协议 §3 | 元数据声明 permissions/requires | 声明 context:write + 依赖 "tool" Provider |
| Slot协议 §4 | 权限 tag 与实际调用一致 | 只声明 context:write |
| Slot协议 §5 | SlotDirective 所有变体被正确处理 | 本槽口只返回 Continue |
| Slot协议 §7 | run() 不缓存跨次可变状态 | 无状态设计 |
| 组件协议 §0 | 本协议解决子模块各自为战问题 | 本槽口无子模块，不需要 Orchestrator |
| 组件协议 C-R03 | process() 必须可重入 | 本槽口无 process()，run() 天然可重入 |

### 0.2 设计决策

| 决策 | 选择 | 理由 |
|------|------|------|
| 是否有内部组件 | 否 | 职责单一（获取工具列表→写入上下文），无子模块 |
| 是否需要 Orchestrator | 否 | 组件协议 §0：无子模块不需要编排器 |
| 是否持有跨 run() 状态 | 否 | Slot协议 S-R03：禁止跨 run() 可变状态 |
| Provider 不可用时的行为 | 降级为空列表 | Slot协议 §7：优雅降级，不中断 Pipeline |
| ToolDefinition 定义位置 | shared_types | 跨平台规范：类型归属统一，禁止重复定义 |

---

## 1. 功能概述

### 1.1 功能定位

`tool_registry` 是 Pipeline **CONTEXT 阶段**的核心槽口，职责单一：

1. 从 ProviderRegistry 获取 `"tool"` Provider
2. 调用 Provider 获取工具定义列表
3. 将工具定义列表写入 `StepContext["tools"]`，供 llm_thinker（think 阶段）读取

**这是 LLM 能"看到"哪些工具的唯一来源。**

### 1.2 在 Pipeline 中的位置

```
Phase::init()       → InitPhaseSlot（会话初始化）
Phase::context()    → ★ tool_registry（本文档）
Phase::think()      → llm_thinker（从 StepContext["tools"] 读取工具定义）
Phase::audit()      → AuditPhaseSlot（安全审计）
Phase::execute()    → tool_executor（执行工具调用）
Phase::loop()       → react_loop（决定是否继续迭代）
Phase::memorize()   → memory_saver + compression_hook
```

### 1.3 数据流

```
ToolsService::start()
    │
    ▼
ProviderRegistry["tool"] → Arc<dyn ToolProvider>
    │
    ▼
tool_registry (context 阶段)
    │
    ├─ ap.provider_raw("tool") → Arc<dyn ToolProvider>
    ├─ provider.list() → Vec<ToolDefinition>
    └─ ap.write_context_raw("tools", Arc::new(tools))
    │
    ▼
llm_thinker (think 阶段)
    │
    └─ ap.read_context_raw("tools") → Vec<ToolDefinition> → 序列化传给 LLM
```

---

## 2. 接口契约

### 2.1 SlotPlugin 实现（Slot协议 §1）

```rust
pub struct ToolRegistrySlot;

#[async_trait]
impl SlotPlugin for ToolRegistrySlot {
    fn name(&self) -> &str { "tool_registry" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;
    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError>;
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

### 2.2 SlotAccessPoint 使用（Slot协议 §2）

| 方法 | 权限 tag（Slot协议 §4） | 方向 | 说明 |
|------|------------------------|------|------|
| `provider_raw("tool")` | 无（Provider 扩展） | 读 | 获取工具 Provider |
| `write_context_raw("tools", ...)` | `context:write` | 写 | 写入工具定义列表 |

**不使用的方法**：`messages()`、`write_observation()`、`request_jump()`、`request_abort()` — 此槽口不需要。

### 2.3 插件元数据（Slot协议 §3）

| 字段 | 值 |
|------|-----|
| name | `"tool_registry"` |
| category | `"slot"` |
| version | `"0.1.0"` |
| permissions | `["context:write"]` |
| requires | `["tool"]` |
| conflicts | `[]` |
| config_schema | `None`（无配置） |

### 2.4 依赖的 Provider（Slot协议 §2.2）

| Provider Key | Trait 类型 | 注册者 | 用途 |
|-------------|-----------|--------|------|
| `"tool"` | `Arc<dyn ToolProvider>` | `ToolsService::start()` | 获取工具定义列表 |

**Provider trait 定义**：

```rust
/// 工具 Provider —— 由 ToolsService 实现并注册
///
/// 归属：plugins/services/tools/
/// 注册位置：ToolsService::start() → register_provider("tool", Arc<dyn ToolProvider>)
#[async_trait]
pub trait ToolProvider: Send + Sync {
    /// 返回所有已注册工具的定义列表
    fn list(&self) -> Vec<ToolDefinition>;
}
```

### 2.5 StepContext 数据写入（Slot协议 §2.1）

| Key | 类型 | 方向 | 消费者 |
|-----|------|------|--------|
| `"tools"` | `Arc<Vec<ToolDefinition>>` | 写入 | llm_thinker |

### 2.6 ToolDefinition 类型归属

**统一定义在 `shared_types` 中**，禁止在其他模块重复定义：

```rust
/// 工具定义 —— 跨插件共享类型
///
/// 归属：shared_types
/// 引用者：tool_registry、llm_thinker、ToolsService
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}
```

### 2.7 SlotDirective 返回值（Slot协议 §5）

本槽口**只返回 `SlotDirective::Continue`**。

理由：CONTEXT 阶段必须完成工具收集，不跳转、不中断。即使 Provider 不可用（降级为空列表），也返回 Continue，让 Pipeline 进入 think 阶段。

---

## 3. 文件结构

```
plugins/slots/tool_registry/
├── mod.rs          # 模块声明 + 重新导出
└── plugin.rs       # ToolRegistrySlot 实现
```

**说明**：无 `types.rs`（ToolDefinition 归属 shared_types），无 `components/`（无内部子模块），无 `orchestrator/`（不需要编排器）。

---

## 4. 详细实现

### 4.1 mod.rs

```rust
pub mod plugin;

pub use plugin::ToolRegistrySlot;
```

### 4.2 plugin.rs

```rust
use std::sync::Arc;

use async_trait::async_trait;

use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::ToolDefinition;

/// 工具注册槽口 —— Pipeline CONTEXT 阶段
///
/// 职责：从 ProviderRegistry 获取工具列表，写入 StepContext。
///
/// 设计决策：
/// - 无状态：不持有跨 run() 的可变状态（S-R03）
/// - 无内部组件：职责单一，不需要 Orchestrator（组件协议 §0）
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

impl ToolRegistrySlot {
    pub fn new() -> Self {
        Self
    }
}

impl Default for ToolRegistrySlot {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SlotPlugin for ToolRegistrySlot {
    fn name(&self) -> &str {
        "tool_registry"
    }

    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 无状态 Slot，无需初始化
        // S-R02：init 失败意味着插件不加载——此处不会失败
        Ok(())
    }

    async fn run(
        &mut self,
        ap: &mut dyn SlotAccessPoint,
    ) -> Result<SlotDirective, PluginError> {
        // Step 1: 获取 "tool" Provider
        let tools: Vec<ToolDefinition> = ap
            .provider_raw("tool")
            .and_then(|raw| {
                raw.downcast::<Arc<dyn ToolProvider>>()
                    .ok()
                    .map(|provider| provider.list())
            })
            .unwrap_or_default();

        // Step 2: 写入 StepContext
        ap.write_context_raw("tools", Box::new(Arc::new(tools)))?;

        // Step 3: 返回 Continue（CONTEXT 阶段必须完成）
        Ok(SlotDirective::Continue)
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        // 无状态 Slot，无需清理
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::any::Any;
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::core::access::SlotAccessPoint;
    use crate::core::slot::SlotDirective;
    use crate::core::types::error::PluginError;
    use crate::shared_types::ToolDefinition;

    // ── Mock ToolProvider ───────────────────────────────────────────

    struct MockToolProvider {
        tools: Vec<ToolDefinition>,
    }

    impl ToolProvider for MockToolProvider {
        fn list(&self) -> Vec<ToolDefinition> {
            self.tools.clone()
        }
    }

    // ── Mock SlotAccessPoint ────────────────────────────────────────

    struct MockAccessPoint {
        providers: HashMap<String, Arc<dyn Any + Send + Sync>>,
        context: HashMap<String, Box<dyn Any + Send>>,
    }

    impl MockAccessPoint {
        fn new() -> Self {
            Self {
                providers: HashMap::new(),
                context: HashMap::new(),
            }
        }

        fn with_provider(
            mut self,
            name: &str,
            provider: Arc<dyn Any + Send + Sync>,
        ) -> Self {
            self.providers.insert(name.to_string(), provider);
            self
        }

        fn get_tools(&self) -> Option<Vec<ToolDefinition>> {
            self.context
                .get("tools")
                .and_then(|v| v.downcast_ref::<Vec<ToolDefinition>>())
                .cloned()
        }
    }

    impl SlotAccessPoint for MockAccessPoint {
        fn messages(&self) -> &[crate::shared_types::Message] {
            &[]
        }
        fn session_id(&self) -> &str {
            "test"
        }
        fn phase_name(&self) -> &str {
            "context"
        }
        fn current_iteration(&self) -> usize {
            0
        }
        fn write_observation(
            &mut self,
            _obs: Box<dyn Any + Send>,
        ) -> Result<(), PluginError> {
            Ok(())
        }
        fn write_context_raw(
            &mut self,
            key: &str,
            val: Box<dyn Any + Send>,
        ) -> Result<(), PluginError> {
            self.context.insert(key.to_string(), val);
            Ok(())
        }
        fn read_context_raw(&self, key: &str) -> Option<&dyn Any> {
            self.context.get(key).map(|b| b.as_ref())
        }
        fn request_jump(&self, _phase: &str) -> Result<(), PluginError> {
            Ok(())
        }
        fn request_abort(&self) -> Result<(), PluginError> {
            Ok(())
        }
        fn provider_raw(
            &self,
            name: &str,
        ) -> Option<Arc<dyn Any + Send + Sync>> {
            self.providers.get(name).cloned()
        }
    }

    // ── 测试用例 ───────────────────────────────────────────────────

    /// 测试：正常流程 —— Provider 包含 2 个工具
    #[tokio::test]
    async fn 正常流程_两个工具() {
        let t1 = ToolDefinition {
            name: "read_file".into(),
            description: "读取文件".into(),
            parameters: serde_json::json!({"type":"object"}),
        };
        let t2 = ToolDefinition {
            name: "write_file".into(),
            description: "写入文件".into(),
            parameters: serde_json::json!({"type":"object"}),
        };

        let provider: Arc<dyn ToolProvider> = Arc::new(MockToolProvider { tools: vec![t1, t2] });
        let mut ap = MockAccessPoint::new()
            .with_provider("tool", provider as Arc<dyn Any + Send + Sync>);

        let mut slot = ToolRegistrySlot::new();
        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
        assert_eq!(ap.get_tools().unwrap().len(), 2);
    }

    /// 测试：空工具列表
    #[tokio::test]
    async fn 空工具列表() {
        let provider: Arc<dyn ToolProvider> = Arc::new(MockToolProvider { tools: vec![] });
        let mut ap = MockAccessPoint::new()
            .with_provider("tool", provider as Arc<dyn Any + Send + Sync>);

        let mut slot = ToolRegistrySlot::new();
        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert!(ap.get_tools().unwrap().is_empty());
    }

    /// 测试：Provider 未注册 —— 降级为空列表
    #[tokio::test]
    async fn Provider未注册_降级() {
        let mut ap = MockAccessPoint::new();

        let mut slot = ToolRegistrySlot::new();
        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), SlotDirective::Continue);
    }

    /// 测试：downcast 失败 —— 降级为空列表
    #[tokio::test]
    async fn downcast失败_降级() {
        let wrong: Arc<dyn Any + Send + Sync> = Arc::new(String::from("not a provider"));
        let mut ap = MockAccessPoint::new().with_provider("tool", wrong);

        let mut slot = ToolRegistrySlot::new();
        let result = slot.run(&mut ap).await;

        assert!(result.is_ok());
    }

    /// 测试：init 和 shutdown 不返回 Err
    #[tokio::test]
    async fn init_shutdown() {
        let mut slot = ToolRegistrySlot::new();
        let ctx = PluginInitContext {
            plugin_config: serde_json::Value::Null,
            agent_config: crate::core::types::plugin::AgentConfig::default(),
        };
        assert!(slot.init(&ctx).await.is_ok());
        assert!(slot.shutdown().await.is_ok());
    }
}
```

---

## 5. 错误处理

| 错误场景 | 处理方式 | 依据 |
|---------|---------|------|
| Provider "tool" 未注册 | `unwrap_or_default()` → 空列表 | Slot协议 §7：优雅降级 |
| downcast 失败 | `unwrap_or_default()` → 空列表 | Slot协议 §7：优雅降级 |
| write_context_raw 失败 | 返回 `PluginError` | 唯一可能返回 Err 的路径 |

**不返回 Err 的设计理由**：
- context 阶段失败不应终止整个 Pipeline
- llm_thinker 在 think 阶段处理空工具列表（LLM 直接返回 Final，不调用工具）
- Slot协议 §7："插件应优雅降级或报错"——此处选择降级

---

## 6. 规范检查清单

### 6.1 跨平台与硬编码规范（§4 自查清单）

| # | 检查项 | 结果 |
|---|--------|------|
| 1 | URL 端点来自配置/常量 | ✅ 不涉及（无网络调用） |
| 2 | 模型名称来自配置 | ✅ 不涉及 |
| 3 | 超时值来自配置/常量 | ✅ 不涉及 |
| 4 | API 版本号定义为 const | ✅ 不涉及 |
| 5 | User-Agent 定义为 const | ✅ 不涉及 |
| 6 | 文件路径通过 dirs + PathBuf::join | ✅ 不涉及（无文件 I/O） |
| 7 | 数字阈值来自配置 | ✅ 不涉及 |
| 8 | 平台指令通过 OsKind 分支 | ✅ 不涉及 |
| 9 | 测试无 Unix-only 路径 | ✅ 测试使用 Mock，无路径 |
| 10 | cargo build + test + clippy 通过 | ⬜ 待验证 |

### 6.2 protocol-Slot接入协议

| # | 检查项 | 条款 | 结果 |
|---|--------|------|------|
| 1 | 实现 SlotPlugin（init/run/shutdown） | §1 | ✅ |
| 2 | name() 返回全局唯一标识 | §1 | ✅ `"tool_registry"` |
| 3 | init 失败返回 Err，不退化运行 | S-R02 | ✅ init 不会失败 |
| 4 | run() 不缓存跨次可变状态 | S-R03 | ✅ 无状态 |
| 5 | 只通过 SlotAccessPoint 交互 | §2 | ✅ |
| 6 | 权限声明与实际调用一致 | §3/§4 | ✅ 只声明 context:write |
| 7 | requires 声明与实际依赖一致 | §3 | ✅ 声明 "tool" |
| 8 | SlotDirective 返回值正确 | §5 | ✅ 只返回 Continue |
| 9 | Provider 不可用时优雅降级 | §7 | ✅ 降级为空列表 |
| 10 | 通过 provider_raw + downcast 获取 Provider | §2.2 | ✅ |

### 6.3 protocol-模块内部组件协议

| # | 检查项 | 结果 |
|---|--------|------|
| 1 | 无内部子模块，不需要 Orchestrator | ✅ 职责单一 |
| 2 | 不直接引用兄弟组件具体类型 | ✅ 不涉及 |
| 3 | 不越级直接引用核心状态 | ✅ 只通过 SlotAccessPoint |

---

## 7. 规范合规检查清单

### 《跨平台与硬编码规范》10 项

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| 1 | 所有 URL 端点来自配置或常量 | 不涉及 URL | ✅ 不适用 |
| 2 | 所有模型名称来自配置字段 | 不涉及模型名 | ✅ 不适用 |
| 3 | 所有超时值来自配置或 `DEFAULT_*` 常量 | 不涉及超时 | ✅ 不适用 |
| 4 | API 版本号定义为模块级 `const` | 不涉及 API 版本 | ✅ 不适用 |
| 5 | User-Agent 定义为 `const USER_AGENT` | 不涉及 HTTP 请求 | ✅ 不适用 |
| 6 | 文件路径通过 `dirs` + `PathBuf::join()` 构建 | 不涉及文件路径 | ✅ 不适用 |
| 7 | 数字阈值默认 `None` 或从配置读取 | 不涉及数字阈值 | ✅ 不适用 |
| 8 | 平台特定指令通过 `OsKind` 枚举分支 | 不涉及平台指令 | ✅ 不适用 |
| 9 | 测试中无 Unix-only 路径 | 测试使用 Mock，无文件路径 | ✅ |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 全部通过 | 待实现后验证 | ☐ 待验证 |

### 《protocol-Slot接入协议》红线 3 项

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| S-R01 | 所有 `SlotDirective` 变体必须被正确处理 | 所有路径返回 `Continue` | ✅ |
| S-R02 | `init` 失败意味着插件不加载 | 配置解析失败返回 `Err` | ✅ |
| S-R03 | `run()` 中禁止持有跨次调用的可变状态 | 无状态设计 | ✅ |

### 《protocol-Slot接入协议》权限与依赖

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| 权限声明 | context:write | PluginMetadata.permissions 声明 | ✅ |
| requires | 声明依赖 "tool" Provider | PluginMetadata.requires 声明 | ✅ |
| Provider 查找 | provider_raw("tool") + downcast | run() 中按规范查找 | ✅ |
| 优雅降级 | Provider 未注册时跳过 | 返回 Continue，记录 warn 日志 | ✅ |

### 《protocol-模块内部组件协议》红线 3 项

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| C-R01 | `AccessPoint::call()` 获取句柄后必须 downcast | tool_registry 不使用内部组件协议（单组件） | ✅ 不适用 |
| C-R02 | `meta().requires` 声明必须真实可验证 | 同上 | ✅ 不适用 |
| C-R03 | `process()` 必须可重入 | run() 无隐式跨调用状态，可重入 | ✅ |

---

## 8. 开发清单

| 序号 | 文件 | 操作 | 说明 |
|------|------|------|------|
| 1 | `shared_types` | 添加 `ToolDefinition` | 统一定义，三字段 |
| 2 | `plugins/services/tools/` | 定义 `ToolProvider` trait + 实现 | 注册到 ProviderRegistry |
| 3 | `plugins/slots/tool_registry/plugin.rs` | 新建 | 完整实现 |
| 4 | `plugins/slots/tool_registry/mod.rs` | 新建 | 声明 + 重导出 |
| 5 | `plugins/slots/mod.rs` | 添加 `pub mod tool_registry` | 模块注册 |
| 6 | `main.rs` | Pipeline 添加 `.add_slot(Phase::context(), ...)` | 注册到 context 阶段 |
| 7 | `plugins/slots/llm_thinker/` | 引用 shared_types::ToolDefinition | 类型统一 |

---

## 9. 依赖关系

### 8.1 上游依赖

| 依赖 | 类型 | 说明 |
|------|------|------|
| `ToolsService` | Provider `"tool"` | 注册 Arc<dyn ToolProvider> |
| `shared_types::ToolDefinition` | 类型 | 统一定义 |

### 8.2 下游依赖

| 依赖者 | 说明 |
|--------|------|
| `llm_thinker` | 从 StepContext["tools"] 读取 |

### 8.3 执行顺序

Pipeline 阶段顺序保证 context 在 think 之前，无需额外同步。

---

> 文档版本：v3.0  
> 最后更新：2026-05-30
