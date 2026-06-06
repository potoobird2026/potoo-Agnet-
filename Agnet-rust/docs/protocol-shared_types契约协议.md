# shared_types 契约协议（Shared Types Contract Protocol）

## 0. 协议范围

本协议定义 **跨插件共享的数据类型、Provider trait 和 Provider key 的定义规则**。

一个插件如果要向其他插件暴露能力（注册 Provider），或消费其他插件的能力（查找 Provider），
所需的共享契约**必须**定义在此处，不能在任一插件的内部模块中定义。

### 为什么需要本协议

```
无 shared_types 时的依赖纠缠：

plugins/services/tools/
  └── trait ToolProvider { ... }   ← 服务方定义
              ↑
plugins/slots/tool_registry/
  └── 引用 services/tools::ToolProvider  ← 消费方直接依赖 Service 内部模块（违反红线）

有 shared_types 时的中立契约：

src/shared_types/
  └── trait ToolProvider { ... }   ← 中立层，不归属于任何一方
              ↗           ↘
  services/tools/ 实现它     slots/tool_registry/ 调用它
```

---

## 1. 契约内容

一个完整的 Provider 对接需要三种契约：

| 契约 | 用途 | 例子 |
|------|------|------|
| **Provider key 常量** | `register_provider()` / `provider_raw()` 双方用同一个字符串 | `pub const PROVIDER_TOOL: &str = "tool";` |
| **Provider trait** | 定义业务接口，服务方 impl，消费方调用 | `pub trait ToolProvider: Send + Sync { fn list(&self) -> ... }` |
| **跨插件数据结构** | Slot ↔ Service 之间传递的数据类型 | `pub struct ToolDefinition { ... }` |

---

## 2. Provider key 常量

### 2.1 定义规则

每个 Provider 必须有且只有一个 key 常量，集中定义在对应的 `shared_types/*.rs` 文件中：

```rust
/// 工具注册 Provider——tool_registry slot 和 tool_executor slot 通过此 key 查找
pub const PROVIDER_TOOL: &str = "tool";

/// 记忆 Provider——memory_saver slot 和 init_phase slot 通过此 key 查找
pub const PROVIDER_MEMORY: &str = "memory";

/// 安全策略 Provider——audit_phase slot 通过此 key 查找
pub const PROVIDER_SECURITY: &str = "security";
```

### 2.2 使用规则

```rust
// Service 端注册（在 ServicePlugin::start() 中）：
use crate::shared_types::PROVIDER_TOOL;
ap.register_provider(PROVIDER_TOOL, Arc::new(...));

// Slot 端查找（在 SlotPlugin::run() 中）：
use crate::shared_types::PROVIDER_TOOL;
let raw = ap.provider_raw(PROVIDER_TOOL)?;
```

### 红线

| # | 红线 |
|:-:|------|
| K-R01 | **禁止在 `register_provider()` 或 `provider_raw()` 中使用裸字符串**——必须使用 `shared_types` 中的 `PROVIDER_*` 常量 |
| K-R02 | 如果 Provider key 尚不存在，**必须先添加到 shared_types，再被双方引用**。禁止在插件内部自己定义一个 key |

---

## 3. Provider trait

### 3.1 定义规则

```rust
// shared_types/tool.rs

/// Provider trait——由谁先开发谁定义，放 shared_types 中
#[async_trait]
pub trait ToolProvider: Send + Sync {
    fn list(&self) -> Vec<ToolDefinition>;
    async fn execute(&self, tool_name: &str, arguments: Value, timeout: Duration) -> Result<String, ToolError>;
}
```

### 3.2 实现规则

```rust
// plugins/services/tools/registry.rs

impl ToolProvider for ToolRegistry {
    fn list(&self) -> Vec<ToolDefinition> { /* ... */ }
    async fn execute(&self, ...) -> Result<String, ToolError> { /* ... */ }
}
```

### 3.3 调用规则

```rust
// plugins/slots/tool_executor/plugin.rs

fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> ... {
    let raw = ap.provider_raw(PROVIDER_TOOL)?;
    let wrapper = raw.downcast::<DynProvider<dyn ToolProvider>>()
        .map_err(|_| PluginError::Internal("类型不匹配".into()))?;
    let result = wrapper.0.execute(...).await?;
}
```

### 红线

| # | 红线 |
|:-:|------|
| T-R01 | **Provider trait 禁止定义在 `services/*` 或 `slots/*` 内部模块中**——必须放在 `shared_types` |
| T-R02 | 谁先开始开发谁负责把 trait 定义好放进 shared_types，不能留给对方去猜接口 |
| T-R03 | Provider trait 不归属于任何一方（不写 `// Service 实现，Slot 消费` 这样的归属注释） |

---

## 4. `DynProvider<T>` 通用包装结构体

### 4.1 为什么要包装

Rust 的 `Arc::downcast::<T>()` 检查的是**具体类型**（`TypeId`），不是 trait 实现。因此：

```rust
// 注册时：Arc<ToolRegistry> 类型 ID = ToolRegistry
register_provider("tool", Arc::new(tool_registry));

// 消费时：请求 Arc<dyn ToolProvider>，但 TypeId::of::<dyn ToolProvider> ≠ TypeId::of::<ToolRegistry>
raw.downcast::<Arc<dyn ToolProvider>>()  // ❌ 运行时失败
```

**解决方案**：用一个通用具体类型做中转，避免为每个 Provider 写一个包装结构体。

### 4.2 `DynProvider<T>` 定义

```rust
// shared_types/mod.rs —— 统一包装结构体，适用于所有 Provider

/// 通用 Provider 包装结构体——用于跨 Arc<dyn Any> 的类型安全传递
///
/// 注册方：register_provider(KEY, Arc::new(DynProvider(Arc::new(impl) as Arc<dyn Trait>)))
/// 消费方：raw.downcast::<DynProvider<dyn Trait>>()?.0.method()
pub struct DynProvider<T: ?Sized + Send + Sync + 'static>(pub Arc<T>);
```

### 4.3 使用方式

```rust
// 服务方注册（在 ServicePlugin::start() 中）：
use crate::shared_types::DynProvider;
let provider: Arc<dyn ToolProvider> = Arc::new(my_impl);
ap.register_provider(PROVIDER_TOOL, Arc::new(DynProvider(provider)));

// 消费方查找（在 SlotPlugin::run() 中）：
let raw = ap.provider_raw(PROVIDER_TOOL)?;
let wrapper = raw.downcast::<DynProvider<dyn ToolProvider>>()
    .map_err(|_| PluginError::Internal("类型不匹配".into()))?;
// 通过 wrapper.0 拿到 Arc<dyn ToolProvider>，然后调用 trait 方法
let result = wrapper.0.list();
```

### 4.4 原理说明

```
注册时：Arc::new(DynProvider(Arc::new(tool_registry) as Arc<dyn ToolProvider>))
         ↑ 外层 Arc 以类型擦除形式存入 ProviderRegistry
         ↑ 内部的具体类型是 DynProvider<dyn ToolProvider>

消费时：raw.downcast::<DynProvider<dyn ToolProvider>>()
         ↑ 检查内部类型是否是 DynProvider<dyn ToolProvider>
           匹配 → 返回 Ok(Arc<DynProvider<dyn ToolProvider>>)
           通过 .0 取出 Arc<dyn ToolProvider>
```

### 红线

| # | 红线 |
|:-:|------|
| D-R01 | 禁止为每个 Provider 单独定义 `DynXxxProvider`——统一使用 `DynProvider<T>` |
| D-R02 | `DynProvider<T>` 定义在 `shared_types/mod.rs` 中，不在任一插件内部 |

---

## 5. 完整对接示例

### 5.1 定义契约（`shared_types/foo.rs`）

```rust
use std::sync::Arc;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

// 1. Provider key 常量
pub const PROVIDER_FOO: &str = "foo";

// 2. Provider trait
#[async_trait]
pub trait FooProvider: Send + Sync {
    async fn bar(&self, input: &str) -> Result<String, FooError>;
}

// 注意：不需要定义 DynFooProvider——统一使用 shared_types 中的 DynProvider<T>
// pub struct DynFooProvider(...)  ← 禁止

// 3. 跨插件数据结构
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FooResult {
    pub data: String,
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum FooError {
    #[error("操作失败: {0}")]
    Failed(String),
}
```

### 5.2 注册契约（`plugins/services/foo/service.rs`）

```rust
use async_trait::async_trait;
use crate::core::access::ServiceAccessPoint;
use crate::core::service::{ServicePlugin, ServiceSignal};
use crate::core::types::error::PluginError;
use crate::core::types::plugin::PluginInitContext;
use crate::shared_types::{DynProvider, FooProvider, PROVIDER_FOO};
use std::sync::Arc;

pub struct FooService {
    engine: Option<Arc<FooEngine>>,
}

#[async_trait]
impl ServicePlugin for FooService {
    fn name(&self) -> &str { "foo" }
    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> { Ok(()) }
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        // 用 PROVIDER_FOO 常量 + DynProvider 包装
        let provider: Arc<dyn FooProvider> = self.engine.clone().unwrap();
        ap.register_provider(PROVIDER_FOO, Arc::new(DynProvider(provider)));
        Ok(())
    }
    // ... handle_signal, stop, shutdown
}

// FooEngine 实现 FooProvider trait
impl FooProvider for FooEngine {
    async fn bar(&self, input: &str) -> Result<String, FooError> {
        Ok(format!("Hello, {}!", input))
    }
}
```

### 5.3 消费契约（`plugins/slots/bar/plugin.rs`）

```rust
use async_trait::async_trait;
use crate::core::access::SlotAccessPoint;
use crate::core::slot::{SlotDirective, SlotPlugin};
use crate::core::types::error::PluginError;
use crate::shared_types::{DynProvider, FooProvider, PROVIDER_FOO};

pub struct BarSlot;

#[async_trait]
impl SlotPlugin for BarSlot {
    fn name(&self) -> &str { "bar" }
    async fn init(&mut self, _ctx: &PluginInitContext) -> Result<(), PluginError> { Ok(()) }
    async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
        let raw = ap.provider_raw(PROVIDER_FOO)
            .ok_or(PluginError::NotFound("foo provider 不可用".into()))?;
        // 用 DynProvider<dyn FooProvider> 进行 downcast
        let wrapper = raw.downcast::<DynProvider<dyn FooProvider>>()
            .map_err(|_| PluginError::Internal("foo provider 类型不匹配".into()))?;
        let result = wrapper.0.bar("world").await
            .map_err(|e| PluginError::Internal(e.to_string()))?;
        println!("{}", result);
        Ok(SlotDirective::Continue)
    }
    async fn shutdown(&mut self) -> Result<(), PluginError> { Ok(()) }
}
```

---

## 6. 现有 Provider 契约一览

| Provider key 常量 | Provider trait | 注册方 | 消费方 | 状态 |
|:-----------------|:---------------|:------|:------|:----:|
| `PROVIDER_TOOL` | `ToolProvider` | `ToolsService` | `ToolRegistrySlot`、`ToolExecutorSlot` | ✅ |
| `PROVIDER_MEMORY` | `MemoryProvider` | `MemoryService` | `MemorySaverSlot`、`InitPhaseSlot` | ✅ |
| `PROVIDER_SECURITY` | `SecurityPolicyProvider` | `SecurityService` | `AuditPhaseSlot` | ✅ |

---

## 7. 合规检查清单

完成 Provider 对接后，逐项检查（编译器不检查这些）：

```
□ key 一致性
   grep -n "PROVIDER_" src/shared_types/*.rs
   grep -rn "register_provider\|provider_raw" src/plugins/ | grep "PROVIDER_"
   → 所有 register_provider 和 provider_raw 都使用 PROVIDER_* 常量，无裸字符串

□ trait 定义位置
   grep -rn "pub trait.*Provider" src/ | grep -v "shared_types"
   → 除 shared_types 外，不应有任何 pub Provider trait 定义

□ 使用 DynProvider 而非 DynXxxProvider
   grep -rn "Dyn[A-Z]" src/shared_types/ | grep -v "DynProvider" | grep -v "//"
   → shared_types 中不应存在 DynToolProvider、DynMemoryProvider 等独立包装结构体

□ impl 存在
   grep -rn "impl.*Provider for" src/plugins/services/*/
   → 每个 Provider 都有具体的实现
```
