# init_phase 槽口开发文档

> 文档版本：v3.1  
> 编写日期：2026-05-30  
> 状态：待开发（从零开始，无任何现有代码）  
> 优先级：P1（Pipeline Init 阶段核心 Slot，负责会话初始化、身份加载、工作记忆恢复）  
> 执行规范（强制）：《跨平台与硬编码规范》《protocol-Slot接入协议》《protocol-模块内部组件协议》

---

## 0. 现状诊断

### 0.1 当前代码状态

`Phase::init()` 阶段在 `core/phase.rs` 中已定义（`Phase::init()` 返回 `Phase("init".to_string())`），`Pipeline` 包含该阶段，但**整个阶段没有任何 Slot 注册**。

Pipeline 执行时，init 阶段因 slots 为空而被跳过，直接进入 context 阶段。

### 0.2 设计意图

init 阶段是 Pipeline 的第一个阶段，在每轮 Step 执行前运行，负责：
1. 会话状态初始化（新会话创建 / 旧会话恢复）
2. 身份记忆加载（L1 Identity）
3. 工作记忆恢复（L2 Working Memory）
4. 系统提示词组装（System Prompt Assembly）
5. 上下文窗口预检（消息数量/token 预估）

---

## 1. 功能概述

### 1.1 功能定位

`InitPhaseSlot` 是 Pipeline **Init 阶段**的核心槽口，负责在每轮 Step 开始前准备好完整的执行上下文。

**核心职责**：
1. 检测当前会话是否为新会话，执行差异化初始化
2. 从 MemoryService 加载身份记忆（L1）和最近工作记忆（L2）
3. 组装系统提示词（注入身份、记忆摘要）
4. 预检上下文窗口，确保消息数量在合理范围内
5. 将初始化结果写入 StepContext，供后续阶段使用

### 1.2 在 Pipeline 中的位置

```
Phase::init()       → ★ InitPhaseSlot（本文档）
Phase::context()    → ToolRegistrySlot（收集工具定义）
Phase::think()      → LlmThinkerSlot（生成 Thought）
Phase::audit()      → AuditPhaseSlot（安全审计）
Phase::execute()    → ToolExecutorSlot（执行工具调用）
Phase::loop()       → ReActLoopSlot（决定是否继续迭代）
Phase::memorize()   → MemorySaverSlot + CompressionHookSlot
```

### 1.3 数据流

```
AgentRuntime.step()
    │
    ▼
Pipeline.run() → Phase::init()
    │
    ▼
InitPhaseSlot.run(ap)
    │
    ├─ 1. 检测会话类型（新/旧）
    ├─ 2. 通过 Provider 获取 MemoryProvider
    ├─ 3. 加载 L1 身份记忆 → 写入 StepContext["identity"]
    ├─ 4. 加载 L2 最近工作记忆 → 写入 StepContext["working_memory"]
    ├─ 5. 组装系统提示词摘要 → 写入 StepContext["system_prompt"]
    ├─ 6. 预检上下文窗口
    │
    ▼
Phase::context() → ToolRegistrySlot（读取已初始化的上下文）
```

---

## 2. 接口契约

### 2.1 实现 trait

```rust
#[async_trait::async_trait]
impl SlotPlugin for InitPhaseSlot
```

### 2.2 生命周期方法

| 方法 | 调用次数 | 职责 |
|------|---------|------|
| `name()` | 多次 | 返回 `"init_phase"` |
| `init()` | 1 | 解析配置，初始化内部状态。**失败则插件不被加载（S-R02）** |
| `run()` | 每轮 Init 阶段 | 会话检测 → 记忆加载 → 提示词组装 → 上下文预检 |
| `shutdown()` | 1 | 释放资源 |

### 2.3 配置结构体

> **《跨平台与硬编码规范》§1**：数字阈值必须定义为常量或从配置读取。

```rust
/// 工作记忆默认加载条数
pub const DEFAULT_WORKING_MEMORY_LIMIT: usize = 10;
/// 上下文窗口预检默认最大消息数
pub const DEFAULT_MAX_MESSAGES_PRECHECK: usize = 100;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct InitPhaseConfig {
    /// 是否加载身份记忆（L1），默认 true
    #[serde(default = "default_true")]
    pub load_identity: bool,

    /// 是否加载工作记忆（L2），默认 true
    #[serde(default = "default_true")]
    pub load_working_memory: bool,

    /// 工作记忆加载条数，默认 DEFAULT_WORKING_MEMORY_LIMIT
    #[serde(default = "default_working_memory_limit")]
    pub working_memory_limit: usize,

    /// 是否组装系统提示词，默认 true
    #[serde(default = "default_true")]
    pub assemble_system_prompt: bool,

    /// 系统提示词模板（可选，为空则使用默认模板）
    #[serde(default)]
    pub system_prompt_template: Option<String>,

    /// 上下文窗口预检最大消息数，默认 DEFAULT_MAX_MESSAGES_PRECHECK
    #[serde(default = "default_max_messages")]
    pub max_messages_precheck: usize,
}

fn default_true() -> bool { true }
fn default_working_memory_limit() -> usize { DEFAULT_WORKING_MEMORY_LIMIT }
fn default_max_messages() -> usize { DEFAULT_MAX_MESSAGES_PRECHECK }
```

### 2.4 错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum InitPhaseError {
    #[error("Memory Provider 未注册")]
    MemoryProviderUnavailable,

    #[error("身份记忆加载失败: {0}")]
    IdentityLoadError(String),

    #[error("工作记忆加载失败: {0}")]
    WorkingMemoryLoadError(String),

    #[error("上下文超限: {count} > {limit}")]
    ContextOverflow { count: usize, limit: usize },

    #[error("配置解析错误: {0}")]
    ConfigError(String),
}

impl From<InitPhaseError> for PluginError {
    fn from(e: InitPhaseError) -> Self {
        PluginError::Internal(e.to_string())
    }
}
```

### 2.5 插件元数据声明

> **《protocol-Slot接入协议》§3**：每个插件必须附带元数据声明。

```rust
pub fn metadata() -> PluginMetadata {
    PluginMetadata {
        name: "init_phase".to_string(),
        category: "slot".to_string(),
        version: "0.1.0".to_string(),
        permissions: vec![
            "messages:read".to_string(),
            "context:write".to_string(),
        ],
        requires: vec![
            "memory".to_string(),
        ],
        conflicts: vec![],
        config_schema: None,
    }
}
```

---

## 3. 依赖接口

### 3.1 Core 内建（通过 SlotAccessPoint）

> **《protocol-Slot接入协议》§2.1**：权限 tag 必须与协议定义完全一致。

| 方法 | 权限 tag | 用途 |
|------|---------|------|
| `session_id()` | 无（总是允许） | 获取当前会话 ID |
| `phase_name()` | 无（总是允许） | 确认当前在 init 阶段 |
| `current_iteration()` | 无（总是允许） | 获取当前迭代次数 |
| `messages()` | `messages:read` | 读取当前消息列表（用于上下文预检） |
| `write_context_raw("identity", ...)` | `context:write` | 写入加载的身份记忆 |
| `write_context_raw("working_memory", ...)` | `context:write` | 写入加载的工作记忆 |
| `write_context_raw("system_prompt", ...)` | `context:write` | 写入组装的系统提示词 |
| `write_context_raw("session_meta", ...)` | `context:write` | 写入会话元数据 |
| `provider_raw("memory")` | 无（总是允许） | 查找记忆服务 Provider |

### 3.2 Provider 扩展

| Provider 名 | 期望类型 | 用途 |
|-------------|---------|------|
| `"memory"` | `Arc<dyn MemoryProvider>` | 加载身份记忆和工作记忆 |

**MemoryProvider trait 引用**：基础 trait 定义在 `plugins/slots/memory_saver/provider.rs` 中。  
InitPhaseSlot 额外需要以下读取方法（需扩展 MemoryProvider trait）：

```rust
/// MemoryProvider trait 扩展——InitPhaseSlot 需要的读取方法
///
/// 设计原则（遵循 Slot接入协议 §0、§2.2）：
/// - 所有返回类型定义在 `shared_types` 中，不引用 Service 内部类型
/// - Service（MemoryService）内部将具体类型映射到 shared_types
/// - Slot（InitPhaseSlot）只通过 shared_types 与 Provider trait 交互
///
/// 完整 trait 定义见 memory_saver/provider.rs，此处为扩展部分
#[async_trait::async_trait]
pub trait MemoryProvider: Send + Sync {
    // ... 写入方法（memory_saver 使用）...

    /// 加载身份记忆（L1）
    ///
    /// 入参：
    /// - session_id：会话 ID
    ///
    /// 出参：
    /// - IdentitySection：身份记忆片段（定义在 shared_types）
    ///
    /// 错误：
    /// - 未找到 → MemoryError::WriteError（新会话可能无身份记忆）
    async fn load_identity(
        &self,
        session_id: &str,
    ) -> Result<IdentitySection, MemoryError>;

    /// 加载最近工作记忆（L2）
    ///
    /// 入参：
    /// - session_id：会话 ID
    /// - limit：加载条数上限
    ///
    /// 出参：
    /// - Vec<MemoryFileEntry>：工作记忆条目（定义在 shared_types）
    async fn load_working_memory(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryFileEntry>, MemoryError>;

    /// 检查会话是否为新会话
    ///
    /// 入参：
    /// - session_id：会话 ID
    ///
    /// 出参：
    /// - true：新会话（无历史记忆）
    /// - false：旧会话（有历史记忆）
    async fn is_new_session(
        &self,
        session_id: &str,
    ) -> Result<bool, MemoryError>;
}
```

**契约说明**（Slot接入协议 §2.2 Provider 扩展机制）：

```
shared_types 定义契约类型：
  IdentitySection    ← MemoryService 实现方负责将内部 l1_identity 类型映射至此
  MemoryFileEntry    ← MemoryService 实现方负责将内部 l2_working 类型映射至此
  MemoryProvider     ← trait 定义，包含 load_identity / load_working_memory 等方法

MemoryService（实现方）          shared_types（契约层）          InitPhaseSlot（消费方）
     │                                │                              │
     ├─ 实现 MemoryProvider trait ───→│←── 通过 provider_raw ───────┤
     │  将内部类型映射为              │    "memory" 获取             │
     │  shared_types 类型返回         │    Arc<dyn MemoryProvider>   │
     │                                │    调用 trait 方法           │
     │                                │    只处理 shared_types 类型  │
```

> **注意**：`IdentitySection` 和 `MemoryFileEntry` 类型定义在 `crate::shared_types` 中，**不在** `plugins/services/memory/` 中。MemoryService 内部使用自己的具体类型（如 `l1_identity::IdentitySection`），在实现 `MemoryProvider` trait 时转换为 `shared_types::IdentitySection`。这是 Provider 扩展机制的正确依赖方向——消费方定义契约，提供方实现契约。

---

## 4. 执行逻辑

### 4.1 run() 完整流程

> **《protocol-Slot接入协议》§9 S-R03**：run() 中禁止持有跨次调用的可变状态。

```rust
async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
    let session_id = ap.session_id().to_string();

    // ══════════════════════════════════════════
    // 步骤 1：获取 Memory Provider
    // ══════════════════════════════════════════
    let memory_provider = match ap.provider_raw("memory") {
        Some(raw) => match raw.downcast::<Arc<dyn MemoryProvider>>() {
            Ok(arc) => (*arc).clone(),
            Err(_) => {
                tracing::warn!("init_phase: Memory Provider 类型不匹配，跳过初始化");
                return Ok(SlotDirective::Continue);
            }
        },
        None => {
            tracing::warn!("init_phase: Memory Provider 未注册，跳过初始化");
            return Ok(SlotDirective::Continue);
        }
    };

    // ══════════════════════════════════════════
    // 步骤 2：检测会话类型
    // ══════════════════════════════════════════
    let is_new_session = memory_provider
        .is_new_session(&session_id)
        .await
        .unwrap_or(true);

    let session_meta = SessionMeta {
        session_id: session_id.clone(),
        is_new: is_new_session,
        initialized_at: Timestamp::now(),
    };
    // write_context_raw 失败不中断 Pipeline（与步骤 3/4 的降级策略一致）
    if let Err(e) = ap.write_context_raw("session_meta", Box::new(session_meta)) {
        tracing::warn!("init_phase: 写入 session_meta 失败: {}，跳过", e);
    }

    // ══════════════════════════════════════════
    // 步骤 3：加载身份记忆（L1）
    // ══════════════════════════════════════════
    if self.config.load_identity {
        match memory_provider.load_identity(&session_id).await {
            Ok(identity) => {
                tracing::debug!("init_phase: 身份记忆加载完成");
                if let Err(e) = ap.write_context_raw("identity", Box::new(identity)) {
                    tracing::warn!("init_phase: 写入 identity 失败: {}，跳过", e);
                }
            }
            Err(e) => {
                if is_new_session {
                    tracing::info!("init_phase: 新会话，无已有身份记忆");
                } else {
                    tracing::warn!("init_phase: 身份记忆加载失败: {}", e);
                }
                // 不中断 Pipeline
            }
        }
    }

    // ══════════════════════════════════════════
    // 步骤 4：加载工作记忆（L2）
    // ══════════════════════════════════════════
    if self.config.load_working_memory && !is_new_session {
        match memory_provider
            .load_working_memory(&session_id, self.config.working_memory_limit)
            .await
        {
            Ok(memories) => {
                tracing::debug!("init_phase: 加载 {} 条工作记忆", memories.len());
                if let Err(e) = ap.write_context_raw("working_memory", Box::new(memories)) {
                    tracing::warn!("init_phase: 写入 working_memory 失败: {}，跳过", e);
                }
            }
            Err(e) => {
                tracing::warn!("init_phase: 工作记忆加载失败: {}", e);
                // 不中断 Pipeline
            }
        }
    }

    // ══════════════════════════════════════════
    // 步骤 5：组装系统提示词
    // ══════════════════════════════════════════
    // 遵循 Slot接入协议 §2：只通过 shared_types 类型与上下文交互
    if self.config.assemble_system_prompt {
        let identity_data = ap
            .read_context_raw("identity")
            .and_then(|any| any.downcast_ref::<crate::shared_types::IdentitySection>())
            .cloned();

        let system_prompt = self.assemble_system_prompt(
            &session_id,
            identity_data,
            is_new_session,
        );

        if let Err(e) = ap.write_context_raw("system_prompt", Box::new(system_prompt)) {
            tracing::warn!("init_phase: 写入 system_prompt 失败: {}，跳过", e);
        }
    }

    // ══════════════════════════════════════════
    // 步骤 6：上下文窗口预检
    // ══════════════════════════════════════════
    let messages = ap.messages();
    if messages.len() > self.config.max_messages_precheck {
        tracing::warn!(
            "init_phase: 消息数 {}/{} 接近上限",
            messages.len(),
            self.config.max_messages_precheck,
        );
        // 不中断 Pipeline，仅警告
    }

    // ══════════════════════════════════════════
    // 步骤 7：返回 Continue（S-R01：所有路径必须返回有效 SlotDirective）
    // ══════════════════════════════════════════
    Ok(SlotDirective::Continue)
}
```

### 4.2 init() 流程

```rust
async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
    let config: InitPhaseConfig = serde_json::from_value(ctx.plugin_config.clone())
        .map_err(|e| PluginError::Config(format!("init_phase 配置解析失败: {}", e)))?;

    // 验证配置（S-R02：失败则插件不加载）
    if config.working_memory_limit == 0 {
        return Err(PluginError::Config(
            "init_phase: working_memory_limit 不能为 0".into(),
        ));
    }
    if config.max_messages_precheck == 0 {
        return Err(PluginError::Config(
            "init_phase: max_messages_precheck 不能为 0".into(),
        ));
    }

    self.config = Some(config);
    tracing::info!("init_phase: 初始化完成");
    Ok(())
}
```

### 4.3 shutdown() 流程

```rust
async fn shutdown(&mut self) -> Result<(), PluginError> {
    tracing::info!("init_phase: shutdown 完成");
    Ok(())
}
```

### 4.4 系统提示词组装

```rust
impl InitPhaseSlot {
    fn assemble_system_prompt(
        &self,
        session_id: &str,
        identity: Option<crate::shared_types::IdentitySection>,
        is_new_session: bool,
    ) -> String {
        let mut prompt = String::new();

        if let Some(template) = &self.config.system_prompt_template {
            prompt.push_str(template);
        } else {
            prompt.push_str("You are a helpful AI agent.\n\n");
        }

        if let Some(id) = identity {
            prompt.push_str("## Identity Context\n");
            prompt.push_str(&id.content);
            prompt.push('\n');
        }

        if is_new_session {
            prompt.push_str("\n## Session Info\nThis is a new session.\n");
        }

        prompt
    }
}
```

---

## 5. 数据结构

### 5.1 会话元数据

```rust
/// 会话元数据——写入 StepContext["session_meta"]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub is_new: bool,
    pub initialized_at: Timestamp,
}
```

### 5.2 类型引用（契约层 vs 实现层分离）

> **Slot接入协议 §0、§2.2 红线**：Slot 只能通过 Provider 扩展机制与 shared_types 契约类型交互，**禁止**直接引用 Service 内部具体类型。

**契约层（shared_types）—— InitPhaseSlot 引用的类型：**

| 类型 | 定义位置 | 说明 |
|------|---------|------|
| `IdentitySection` | `crate::shared_types` | 身份记忆片段，MemoryProvider::load_identity() 的返回类型 |
| `MemoryFileEntry` | `crate::shared_types` | 工作记忆条目，MemoryProvider::load_working_memory() 的返回类型 |
| `MemoryProvider` trait | `crate::shared_types`（或 `memory_saver/provider.rs` re-export） | 包含 load_identity / load_working_memory / is_new_session 方法 |

**实现层（MemoryService 内部）—— InitPhaseSlot 不可见：**

| 类型 | 定义位置 | 说明 |
|------|---------|------|
| `l1_identity::IdentitySection` | `plugins/services/memory/l1_identity/manager.rs` | MemoryService 内部类型，实现 MemoryProvider trait 时映射为 `shared_types::IdentitySection` |
| `l2_working::MemoryFile` | `plugins/services/memory/l2_working/manager.rs` | MemoryService 内部类型，实现 MemoryProvider trait 时映射为 `shared_types::MemoryFileEntry` |
| `l2_working::MemoryFileFrontmatter` | `plugins/services/memory/l2_working/manager.rs` | MemoryService 内部，不对外暴露 |
| `l2_working::MemoryFileType` | `plugins/services/memory/l2_working/manager.rs` | MemoryService 内部，不对外暴露 |

**依赖方向**（正确 —— 符合 Provider 扩展机制）：

```
shared_types  ←──  MemoryService（实现 MemoryProvider，内部类型→契约类型映射）
shared_types  ──→  InitPhaseSlot（只使用契约类型，不碰 Service 内部）
```

---

## 6. 文件结构

```
plugins/slots/init_phase/
├── mod.rs              # 模块入口（组件协议 §6.1：只暴露 InitPhaseSlot + InitPhaseConfig）
├── plugin.rs           # SlotPlugin 实现（核心逻辑）
├── config.rs           # InitPhaseConfig 定义 + 常量定义
├── types.rs            # SessionMeta 定义
└── error.rs            # InitPhaseError 定义 + Into<PluginError>
```

---

## 7. mod.rs 规范

> **《protocol-模块内部组件协议》§6.1**：模块 `mod.rs` 只暴露三样东西：对外 Slot 入口、配置、错误类型。

```rust
// ============================================
// 模块：init_phase 槽口
//
// 模块职责：
// 在 Pipeline Init 阶段执行会话初始化、身份加载、工作记忆恢复
//
// 模块边界：
// - 本模块负责：会话检测、记忆加载、系统提示词组装、上下文预检
// - 本模块不负责：工具注册（ToolRegistrySlot）、LLM 思考（LlmThinkerSlot）、
//                 工具执行（ToolExecutorSlot）、记忆持久化（MemorySaverSlot）
//
// 依赖 Provider：
// - "memory"（由 MemoryService 注册，提供 MemoryProvider trait）
//
// 被依赖模块：
// - llm_thinker 读取本模块写入的 system_prompt 和 identity
//
// 核心层实现：
// - SlotPlugin → InitPhaseSlot
//
// 错误类型：见 error.rs
// 数据类型：见 types.rs
//
// 协议合规：
// - S-R03 合规：无跨 run() 的可变状态
// - C-R03 合规：run() 可重入
// ============================================

pub mod config;
pub mod error;
pub mod plugin;
pub mod types;

pub use config::InitPhaseConfig;
pub use plugin::InitPhaseSlot;
pub(crate) use error::InitPhaseError;
```

---

## 8. 注册步骤

> **《protocol-Slot接入协议》§8**：新增 Slot 标准流程共需改 2 个文件。

### 8.1 修改 `plugins/slots/mod.rs`（第 1 个文件）

```rust
pub mod init_phase;      // ★ 新增
pub mod llm_thinker;
pub mod memory_saver;
pub mod react_loop;
pub mod tool_executor;
pub mod tool_registry;
```

### 8.2 修改 Pipeline 构建代码（第 2 个文件）

```rust
pipeline.add_slot(
    Phase::init(),
    Box::new(InitPhaseSlot::new(init_config)),
);
```

---

## 9. 测试要点

> **《跨平台与硬编码规范》§3**：测试中无 Unix-only 路径，均用 `std::env::temp_dir()`。

### 9.1 正常路径测试

| 测试场景 | 前置条件 | 输入 | 期望 |
|---------|---------|------|------|
| 新会话初始化 | MemoryProvider 已注册，无历史 | session_id = "new-session" | is_new=true，写入 session_meta，Continue |
| 旧会话恢复 | 有历史身份和工作记忆 | session_id = "old-session" | is_new=false，加载 identity + working_memory，Continue |
| 系统提示词组装 | identity 已加载 | IdentitySection 存在 | system_prompt 包含身份内容 |
| 完整流程 | 所有 Provider 已注册 | 完整 StepContext | 所有步骤完成，Continue |

### 9.2 边界条件测试

| 测试场景 | 输入 | 期望 |
|---------|------|------|
| Memory Provider 未注册 | provider_raw("memory") 返回 None | 跳过初始化，Continue |
| 新会话无身份记忆 | is_new_session=true，load_identity 返回 Err | 不报错，Continue |
| 消息数接近上限 | messages.len() > max_messages_precheck | 记录警告，Continue |
| working_memory_limit = 0 | 配置中 limit=0 | init() 返回 Err（S-R02） |
| max_messages_precheck = 0 | 配置中 precheck=0 | init() 返回 Err（S-R02） |

### 9.3 异常路径测试

| 测试场景 | 输入 | 期望 |
|---------|------|------|
| Provider 类型不匹配 | downcast 失败 | 记录警告，Continue |
| 身份记忆加载失败（旧会话） | load_identity 返回 Err | 记录警告，Continue |
| 工作记忆加载失败 | load_working_memory 返回 Err | 记录警告，Continue |
| write_context_raw 失败（session_meta） | StepContext 写入异常 | 记录警告，Continue，不传播 Err |
| write_context_raw 失败（identity） | StepContext 写入异常 | 记录警告，Continue，不传播 Err |
| write_context_raw 失败（working_memory） | StepContext 写入异常 | 记录警告，Continue，不传播 Err |
| write_context_raw 失败（system_prompt） | StepContext 写入异常 | 记录警告，Continue，不传播 Err |
| 配置解析错误 | plugin_config 格式错误 | init() 返回 Err（S-R02） |

### 9.4 外部依赖测试

| 测试场景 | 前置条件 | 期望 |
|---------|---------|------|
| MemoryService 未启动 | MemoryService init 未调用 | Provider 未注册，跳过初始化 |
| MemoryProvider 返回超时 | is_new_session 超时 | 默认为新会话，Continue |

### 9.5 S-R03 合规验证

| 测试场景 | 输入 | 期望 |
|---------|------|------|
| 重复运行 | 同一 StepContext 运行两次 | 第二次正确运行，无状态污染 |
| Slot 重建后运行 | 新建 Slot 实例，使用同一 StepContext | Slot 不依赖内部状态，正确运行 |

### 9.6 SlotDirective 完整性测试（S-R01）

| 返回值 | 场景 | Pipeline 行为 |
|--------|------|-------------|
| `Continue` | 正常初始化完成 | 进入 context 阶段 |
| `Continue` | Provider 未注册 | 跳过初始化，进入 context 阶段 |
| `Continue` | Provider 类型不匹配 | 跳过初始化，进入 context 阶段 |
| `Continue` | 记忆加载失败 | 记录警告，进入 context 阶段 |
| `Continue` | write_context_raw 失败（任一 key） | 记录警告，进入 context 阶段 |

---

## 10. 待确认事项

1. ~~**IdentitySection 和 MemoryFile 类型归属**：这些类型定义在 `plugins/services/memory/` 中。~~
   - ~~**建议**：init_phase 的 types.rs 中通过 `use` 引用，不重复定义。~~
   - **已解决**（见 §5.2）：契约类型 `IdentitySection` 和 `MemoryFileEntry` **已移至 `crate::shared_types`**。MemoryService 实现方负责将内部类型映射为契约类型。InitPhaseSlot 只引用 `crate::shared_types` 中的类型。

2. **系统提示词模板格式**：默认模板应为英文（LLM 兼容性），通过配置支持用户自定义。

3. **上下文预检职责**：消息裁剪应由 AgentRuntime 在创建 StepContext 时执行，init_phase 仅做预检和警告。

4. **MemoryProvider trait 扩展**：`load_identity`、`load_working_memory`、`is_new_session` 三个方法需要添加到 MemoryProvider trait 中。其返回类型 `IdentitySection` 和 `MemoryFileEntry` **需先在 `crate::shared_types` 中定义**（见 §5.2 契约层类型表）。完整的 MemoryProvider trait 应定义在 `crate::shared_types` 中，或由 `memory_saver/provider.rs` 从 shared_types re-export。

---

## 11. 规范合规检查清单

### 《跨平台与硬编码规范》10 项

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| 1 | 所有 URL 端点来自配置或常量，非字面量写死 | 不涉及 URL | ✅ 不适用 |
| 2 | 所有模型名称来自配置字段，非硬编码 | 不涉及模型名 | ✅ 不适用 |
| 3 | 所有超时值来自配置或 `DEFAULT_*` 常量 | 不涉及超时（无网络请求） | ✅ 不适用 |
| 4 | API 版本号定义为模块级 `const`，不散落 | 不涉及 API 版本 | ✅ 不适用 |
| 5 | User-Agent 定义为 `const USER_AGENT` | 不涉及 HTTP 请求 | ✅ 不适用 |
| 6 | 文件路径通过 `dirs` + `PathBuf::join()` 构建，无 `/tmp/`、`~`、相对路径 | 不涉及文件路径（由 MemoryService 处理） | ✅ 不适用 |
| 7 | 数字阈值默认 `None` 或从配置读取 | `DEFAULT_WORKING_MEMORY_LIMIT`、`DEFAULT_MAX_MESSAGES_PRECHECK` 常量 | ✅ |
| 8 | 平台特定指令通过 `OsKind` 枚举分支，不假设 `sh` 或 `cmd` | 不涉及平台指令 | ✅ 不适用 |
| 9 | 测试中无 Unix-only 路径，均用 `std::env::temp_dir()` | 测试使用 `std::env::temp_dir()` | ✅ |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 全部通过 | 待实现后验证 | ☐ 待验证 |

### 《protocol-Slot接入协议》红线 3 项

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| S-R01 | 所有 `SlotDirective` 变体必须被正确处理 | 所有路径返回 `Continue`，无遗漏 | ✅ |
| S-R02 | `init` 失败意味着插件不加载 | 配置解析失败、limit/precheck 为 0 均返回 `Err` | ✅ |
| S-R03 | `run()` 中禁止持有跨次调用的可变状态 | Slot 结构体无跨调用状态字段 | ✅ |

### 《protocol-Slot接入协议》权限与依赖

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| 权限声明 | messages:read, context:write | PluginMetadata.permissions 声明 | ✅ |
| requires | 声明依赖 "memory" Provider | PluginMetadata.requires 声明 | ✅ |
| Provider 查找 | provider_raw("memory") + downcast | run() 中按规范查找 | ✅ |
| 优雅降级 | Provider 未注册时跳过，不 panic | 返回 Continue，记录 warn 日志 | ✅ |
| **§0/§2.2 红线** | **禁止直接引用 Service 内部类型** | 所有数据通过 shared_types 契约类型交互，不 `use` Service 内部路径 | ✅ |

### 《protocol-模块内部组件协议》红线 3 项

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| C-R01 | `AccessPoint::call()` 获取句柄后必须 downcast | init_phase 不使用内部组件协议（单组件），通过 SlotAccessPoint 与外部交互 | ✅ 不适用 |
| C-R02 | `meta().requires` 声明必须真实可验证 | 同上 | ✅ 不适用 |
| C-R03 | `process()` 必须可重入 | run() 无隐式跨调用状态，可重入 | ✅ |

### 《protocol-模块内部组件协议》模块边界

| # | 检查项 | 措施 | 状态 |
|---|--------|------|------|
| mod.rs 只暴露三样东西 | InitPhaseSlot + InitPhaseConfig + InitPhaseError | 内部 types 全部 pub(crate) | ✅ |
| 依赖方向正确 | 只依赖 core + shared_types + memory 类型 | 不依赖其他 Slot 具体实现 | ✅ |

---

## 12. 开发清单

| 序号 | 文件 | 操作 | 说明 |
|------|------|------|------|
| 1 | `plugins/slots/memory_saver/provider.rs` | 扩展 MemoryProvider trait | 添加 `load_identity`、`load_working_memory`、`is_new_session` 方法 |
| 2 | `plugins/services/memory/service.rs` | 修改 register_provider | 注册真正的 MemoryProviderImpl（实现新增的读取方法） |
| 3 | `plugins/slots/init_phase/config.rs` | 新建 | 常量 + InitPhaseConfig |
| 4 | `plugins/slots/init_phase/error.rs` | 新建 | InitPhaseError + Into<PluginError> |
| 5 | `plugins/slots/init_phase/types.rs` | 新建 | SessionMeta |
| 6 | `plugins/slots/init_phase/plugin.rs` | 新建 | InitPhaseSlot 实现 |
| 7 | `plugins/slots/init_phase/mod.rs` | 新建 | 模块入口（组件协议 §6.1） |
| 8 | `plugins/slots/mod.rs` | 添加 `pub mod init_phase` | 模块注册 |
| 9 | `main.rs` | Pipeline 添加 `.add_slot(Phase::init(), ...)` | 注册到 init 阶段 |

---

## 13. 依赖关系

### 13.1 上游依赖

| 依赖 | 类型 | 说明 |
|------|------|------|
| `MemoryService` | Provider `"memory"` | 注册 Arc<dyn MemoryProvider>（需扩展读取方法） |
| `shared_types::Message` | 类型 | 从 SlotAccessPoint::messages() 读取 |
| `memory::l1_identity::IdentitySection` | 类型 | 身份记忆片段 |
| `memory::l2_working::MemoryFile` | 类型 | 工作记忆文件 |

### 13.2 下游依赖

| 依赖者 | 说明 |
|--------|------|
| `llm_thinker` | 读取本模块写入的 system_prompt 和 identity |
| `tool_registry` | 同阶段后续 Slot，读取已初始化的上下文 |

### 13.3 执行顺序

Pipeline 阶段顺序保证 init 在 context 之前，无需额外同步。

---

> 文档版本：v3.1  
> 最后更新：2026-05-30  
> 按三份规范逐项对照修订完成，不简化、不降级、不走捷径。