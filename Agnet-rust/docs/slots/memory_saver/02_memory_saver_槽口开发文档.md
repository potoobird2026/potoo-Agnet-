# memory_saver 槽口开发文档

> 文档版本：v3.1
> 编写日期：2026-05-30
> 状态：待开发（按三份规范从零设计，旧代码全部废弃）
> 优先级：P1（Memorize 阶段核心 Slot，负责将对话上下文持久化到记忆系统）
> 执行规范：《跨平台与硬编码规范》《protocol-Slot接入协议》《protocol-模块内部组件协议》

---

## 0. 设计约束

### 0.1 规范红线

| 来源 | 红线 | 本设计如何遵守 | 代码位置 |
|------|------|---------------|---------|
| 跨平台规范 §1 | 禁止硬编码 URL/模型名/超时/路径 | 超时值定义为 `DEFAULT_MEMORY_WRITE_TIMEOUT_SECS` 常量，通过 serde default 注入 config；无 URL、无模型名、无硬编码路径 | §2.3 常量定义；§4.1 步骤 1 `self.config.write_timeout_secs` |
| 跨平台规范 §2 | 禁止裸用 `/tmp/`、`~`、相对路径 | 无文件路径操作；记忆写入通过 MemoryProvider trait，路径由 MemoryService 内部处理（MemoryService 遵守跨平台规范 §2） | §4.1 全程无 PathBuf 构造 |
| 跨平台规范 §3 | 测试禁止硬编码路径、禁止访问真实 API | 测试使用 Mock MemoryProvider（§9.5），无网络调用、无文件 I/O | §9.5 MockMemoryProvider |
| 跨平台规范 §4 | 自查清单 10 项全部通过 | §10.1 逐项检查 | §10.1 |
| 跨平台规范 §5 | 生效范围：plugins/ 下所有 Rust 源码 | 本槽口位于 plugins/slots/memory_saver/，完全适用 | — |
| Slot协议 §1 | SlotPlugin 单入口（name/init/run/shutdown） | 严格实现四方法生命周期；name() 返回全局唯一标识 `"memory_saver"` | §2.2 生命周期方法表；§4.1/§4.2/§4.3 |
| Slot协议 §2 | 只通过 SlotAccessPoint 与核心交互 | 不直接访问任何核心状态；所有交互通过 `ap` 参数 | §3.1 Core 内建方法表 |
| Slot协议 §2.2 | Provider 通过 provider_raw + downcast 获取 | `ap.provider_raw("memory")` → `downcast::<Arc<dyn MemoryProvider>>()` | §4.1 步骤 1 |
| Slot协议 §3 | 元数据声明 permissions/requires | 声明 messages:read、context:read、context:write；requires `"memory"` | §3.3 PluginMetadata |
| Slot协议 §4 | 权限 tag 与实际调用一致 | messages:read → `ap.messages()`；context:read → `ap.read_context_raw()`；context:write → `ap.write_context_raw()` | §3.1 权限 tag 表逐行对应 |
| Slot协议 §5 | SlotDirective 所有变体被正确处理（S-R01） | 本槽口所有路径返回 Continue（含失败降级）；无 BreakPhase/AbortStep 等变体 | §4.1 步骤 8；§2.5 返回值说明 |
| Slot协议 §6 | 生命周期：init→run(多次)→shutdown | init 解析配置；run 执行持久化；shutdown 刷新缓冲区 | §4.1/§4.2/§4.3 |
| Slot协议 §7 | Provider 未注册时优雅降级 | provider_raw 返回 None → 记录 warn 日志 → 返回 Continue | §4.1 步骤 1 的 None 分支 |
| Slot协议 §8 | 新增 Slot 需改 2 个文件 | plugins/slots/mod.rs + Pipeline 构建代码 | §8.1/§8.2 |
| Slot协议 S-R01 | 所有 SlotDirective 变体必须被正确处理 | Continue 覆盖所有路径（含失败降级），无未处理变体 | §4.1 步骤 8 |
| Slot协议 S-R02 | init 失败意味着插件不加载 | 配置解析失败返回 PluginError::Config，不退化运行 | §4.2 init() |
| Slot协议 S-R03 | run() 禁止持有跨次调用的可变状态 | last_persisted_count / last_indexed_count 存入 StepContext，不在 Slot 字段中 | §3.1 S-R03 合规关键设计；§4.1 步骤 2/7 |
| 组件协议 §0 | 本协议解决子模块各自为战问题 | 本槽口无子模块，不需要 Orchestrator/Component/AccessPoint；单一 Slot 直接实现 SlotPlugin | §6.1 mod.rs 注释 |
| 组件协议 §6 | mod.rs 只暴露三样东西 | mod.rs 只暴露 MemorySaverSlot（Slot 入口）、MemorySaverConfig（配置）；MemoryProvider trait 定义在 shared_types（见下方说明） | §6.1 mod.rs |
| 组件协议 C-R03 | process() 必须可重入 | 不适用（本槽口无内部组件）；Slot 的 run() 通过 StepContext 传递状态，等价满足 | §4.1 步骤 2 |

### 0.2 设计原则

1. **Provider trait 定义在 shared_types**：`MemoryProvider` trait 定义在 `shared_types` 中（与 `ToolProvider` 并列），memory_saver 和 MemoryService 都从 `shared_types` 引用，**禁止 MemoryService 反向依赖 Slot 类型**。这是 Slot协议 §2.2 Provider 扩展机制的正确依赖方向。
2. **持久化失败不中断 Pipeline**：记忆写入是辅助功能，失败只记录错误日志，返回 Continue（Slot协议 §7 优雅降级）。
3. **增量持久化**：通过 StepContext 中的 `last_persisted_count` 和 `last_indexed_count` 实现增量写入，避免重复持久化（S-R03 合规）。
4. **异步操作不阻塞**：向量索引更新和经验提取通过 `tokio::spawn` fire-and-forget 异步执行，不阻塞 Pipeline。JoinHandle 故意丢弃，错误通过 tracing::error 记录。

---

## 1. 功能概述

### 1.1 功能定位

`memory_saver` 是 Pipeline **Memorize 阶段**的核心槽口，负责在每轮 Step 结束时，将当前对话上下文（消息、工具调用结果、观察）持久化到记忆系统（MemoryService）。

**核心职责**：
1. 从 `SlotAccessPoint` 读取当前轮次的完整对话历史
2. 提取需要持久化的信息（新消息、工具观察）
3. 通过 Provider 调用 MemoryService 的写入接口
4. 更新工作记忆（L2）和向量索引（L3，如已启用）
5. 可选：触发经验提取（异步）

### 1.2 在 Pipeline 中的位置

```
Phase::init()       → InitPhaseSlot（会话初始化）
Phase::context()    → ToolRegistrySlot（收集工具定义）
Phase::think()      → LlmThinkerSlot（生成 Thought）
Phase::audit()      → AuditPhaseSlot（安全审计）
Phase::execute()    → ToolExecutorSlot（执行工具调用）
Phase::loop()       → ReActLoopSlot（决定是否继续迭代）
Phase::memorize()   → ★ MemorySaverSlot（本文档）→ CompressionHookSlot（compression 服务）
```

**执行顺序**：memory_saver 在 CompressionHookSlot 之前注册，确保持久化完成后再触发压缩。

### 1.3 数据流

```
react_loop (loop 阶段)
    │
    ▼
StepContext（包含完整的 messages + observation + step_result）
    │
    ▼
MemorySaverSlot (memorize 阶段)
    │
    ├─ 1. provider_raw("memory") → Arc<dyn MemoryProvider>
    ├─ 2. read_context_raw("last_persisted_count") → 增量起点
    ├─ 3. messages() → 读取完整对话历史
    ├─ 4. MemoryProvider::persist_messages() → 写入工作记忆 L2
    ├─ 5. read_context_raw("observation") → 读取工具执行结果
    ├─ 6. MemoryProvider::persist_observation() → 写入观察结果
    ├─ 7. MemoryProvider::trigger_vector_index() → 异步更新向量索引 L3
    ├─ 8. MemoryProvider::extract_experiences() → 异步经验提取（可选）
    ├─ 9. write_context_raw("last_persisted_count") → 更新进度（S-R03 合规）
    └─ 10. write_context_raw("memory_persisted") → 写入完成标记
    │
    ▼
CompressionHookSlot (memorize 阶段，同阶段后续 Slot）
    │
    ▼
发送 HookEvent::RoundComplete 到 CompressionService
```

---

## 2. 接口契约

### 2.1 实现 trait

```rust
#[async_trait::async_trait]
impl SlotPlugin for MemorySaverSlot
```

### 2.2 生命周期方法

| 方法 | 调用次数 | 职责 |
|------|---------|------|
| `name()` | 多次 | 返回 `"memory_saver"`，全局唯一标识 |
| `init()` | 1 | 校验配置、建立连接、分配资源。失败则插件不被加载（S-R02） |
| `run()` | 每轮 Memorize 阶段 | 读取上下文 → 提取信息 → 调用 MemoryProvider 持久化。不返回 Err（失败降级为 Continue） |
| `shutdown()` | 1 | 刷新缓冲区，确保所有待写入数据落盘 |

### 2.3 配置结构体

> **跨平台规范 §1**：超时值、数字阈值必须定义为常量，禁止在业务逻辑中写死字面量。

```rust
/// 记忆写入默认超时秒数
pub const DEFAULT_MEMORY_WRITE_TIMEOUT_SECS: u64 = 10;
/// 经验提取最小消息数默认值
pub const DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE: usize = 5;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemorySaverConfig {
    /// 是否自动提取并存储用户消息，默认 true
    #[serde(default = "default_true")]
    pub persist_user_messages: bool,

    /// 是否自动存储工具调用结果，默认 true
    #[serde(default = "default_true")]
    pub persist_observations: bool,

    /// 是否触发向量索引更新，默认 true
    #[serde(default = "default_true")]
    pub update_vector_index: bool,

    /// 是否启用经验提取，默认 false
    #[serde(default)]
    pub enable_experience_extract: bool,

    /// 经验提取最小消息数，默认 DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE
    #[serde(default = "default_min_messages_for_experience")]
    pub min_messages_for_experience: usize,

    /// 写入超时秒数，默认 DEFAULT_MEMORY_WRITE_TIMEOUT_SECS
    #[serde(default = "default_write_timeout_secs")]
    pub write_timeout_secs: u64,
}

fn default_true() -> bool { true }
fn default_min_messages_for_experience() -> usize { DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE }
fn default_write_timeout_secs() -> u64 { DEFAULT_MEMORY_WRITE_TIMEOUT_SECS }
```

### 2.4 错误类型

```rust
#[derive(Debug, thiserror::Error)]
pub enum MemorySaverError {
    #[error("Memory Provider 未注册，无法持久化")]
    ProviderUnavailable,

    #[error("记忆写入超时（{timeout_secs} 秒）")]
    WriteTimeout { timeout_secs: u64 },

    #[error("记忆写入失败: {source}")]
    WriteError { source: String },

    #[error("向量索引更新失败: {source}")]
    VectorIndexError { source: String },

    #[error("配置解析错误: {0}")]
    ConfigError(String),

    #[error("序列化错误: {0}")]
    SerializationError(String),
}
```

---

## 3. 依赖接口

### 3.1 Core 内建（通过 SlotAccessPoint）

> **Slot协议 §2.1**：权限 tag 必须与协议定义完全一致。

| 方法 | 权限 tag | 用途 | 调用频率 |
|------|---------|------|---------|
| `messages()` | `messages:read` | 读取当前会话完整对话历史 | 每轮 1 次 |
| `read_context_raw("observation")` | `context:read` | 读取 tool_executor 写入的观察结果 | 每轮 1 次 |
| `read_context_raw("last_persisted_count")` | `context:read` | 读取上次持久化进度（S-R03 合规） | 每轮 1 次 |
| `read_context_raw("last_indexed_count")` | `context:read` | 读取上次索引进度（S-R03 合规） | 每轮 1 次 |
| `session_id()` | 无（总是允许） | 获取当前会话 ID | 每轮 1 次 |
| `write_context_raw("last_persisted_count", ...)` | `context:write` | 写入持久化进度（S-R03 合规） | 每轮 1 次 |
| `write_context_raw("last_indexed_count", ...)` | `context:write` | 写入索引进度（S-R03 合规） | 每轮 1 次 |
| `write_context_raw("memory_persisted", ...)` | `context:write` | 写入持久化完成标记 | 每轮 1 次 |
| `provider_raw("memory")` | 无（总是允许） | 查找记忆服务 Provider | 每轮 1 次 |

> **S-R03 合规关键设计**：`last_persisted_count` 和 `last_indexed_count` 存入 StepContext（通过 `write_context_raw`），而非 `MemorySaverSlot` 结构体字段。这确保 run() 中不持有跨次调用的可变状态。

### 3.2 Provider 扩展

| Provider 名 | 期望类型 | 注册方 | 用途 |
|-------------|---------|--------|------|
| `"memory"` | `Arc<dyn MemoryProvider>` | MemoryService::start() | 写入工作记忆、更新向量索引、提取经验 |

**MemoryProvider trait 定义**：

> **Slot协议 §2.2**：Provider 通过 `provider_raw(name)` 返回类型擦除的 `Arc`，调用方通过 `downcast` 获取具体类型。
>
> **依赖方向规范**：`MemoryProvider` trait 定义在 `shared_types` 中（与 `ToolProvider` 并列），memory_saver（消费方）和 MemoryService（提供方）都从 `shared_types` 引用。**禁止 MemoryService 反向依赖 Slot 类型**（即禁止 `use crate::plugins::slots::memory_saver::provider::MemoryProvider`）。

```rust
/// 记忆服务 Provider——定义在 shared_types 中
///
/// 设计原则：Provider trait 定义在第三方（shared_types），消费方和提供方都从第三方引用，
/// 避免提供方反向依赖消费者。这与 ToolProvider 的放置方式一致。
///
/// 注册方：MemoryService::start() 注册 Arc<dyn MemoryProvider> 到 ProviderRegistry
/// 消费方：MemorySaverSlot::run() 通过 provider_raw("memory") + downcast 获取
#[async_trait::async_trait]
pub trait MemoryProvider: Send + Sync {
    /// 持久化消息到工作记忆（L2）
    async fn persist_messages(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<(), MemoryError>;

    /// 持久化工具观察结果到工作记忆（L2）
    async fn persist_observation(
        &self,
        session_id: &str,
        observation: &Observation,
    ) -> Result<(), MemoryError>;

    /// 触发向量索引更新（L3，异步触发，不等待完成）
    async fn trigger_vector_index(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<(), MemoryError>;

    /// 提取经验（从对话中提取有价值的经验片段）
    async fn extract_experiences(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<Vec<ExperienceEntry>, MemoryError>;

    /// 获取记忆统计信息
    async fn stats(&self) -> Result<MemoryStats, MemoryError>;
}

#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    #[error("写入失败: {0}")]
    WriteError(String),
    #[error("向量索引错误: {0}")]
    VectorIndexError(String),
    #[error("经验提取错误: {0}")]
    ExperienceError(String),
    #[error("未初始化")]
    NotInitialized,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryStats {
    pub l1_count: usize,
    pub l2_count: usize,
    pub l3_count: usize,
    pub experience_count: usize,
}
```

### 3.3 插件元数据声明

> **Slot协议 §3**：每个插件必须附带元数据声明，name 全局唯一。

```rust
impl MemorySaverSlot {
    pub const METADATA: PluginMetadata = PluginMetadata {
        name: "memory_saver",
        category: "slot",
        version: "0.1.0",
        permissions: &[
            "messages:read",
            "context:read",
            "context:write",
        ],
        requires: &[
            "memory",    // 依赖 MemoryService 注册的 Provider
        ],
        conflicts: &[],
    };
}
```

---

## 4. 执行逻辑

### 4.1 run() 完整流程

> **Slot协议 §9 S-R03**：run() 中禁止持有跨次调用的可变状态。所有跨 run() 的状态通过 StepContext 传递。

```rust
async fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
    // ══════════════════════════════════════════════════════════════
    // 步骤 1：获取 Memory Provider
    // ══════════════════════════════════════════════════════════════
    let memory_provider = match ap.provider_raw("memory") {
        Some(raw) => match raw.downcast::<Arc<dyn MemoryProvider>>() {
            Ok(arc) => (*arc).clone(),
            Err(_) => {
                tracing::warn!(
                    "{} Memory Provider 类型不匹配，跳过持久化",
                    LOG_PREFIX
                );
                return Ok(SlotDirective::Continue);
            }
        },
        None => {
            tracing::warn!(
                "{} Memory Provider 未注册，跳过持久化",
                LOG_PREFIX
            );
            return Ok(SlotDirective::Continue);
        }
    };

    let session_id = ap.session_id().to_string();
    let timeout = Duration::from_secs(self.config.write_timeout_secs);

    // ══════════════════════════════════════════════════════════════
    // 步骤 2：从 StepContext 读取上次持久化进度（S-R03 合规）
    // ══════════════════════════════════════════════════════════════
    let last_persisted_count: usize = ap
        .read_context_raw("last_persisted_count")
        .and_then(|any| any.downcast_ref::<usize>().copied())
        .unwrap_or(0);

    let last_indexed_count: usize = ap
        .read_context_raw("last_indexed_count")
        .and_then(|any| any.downcast_ref::<usize>().copied())
        .unwrap_or(0);

    // ══════════════════════════════════════════════════════════════
    // 步骤 3：持久化用户消息（L2 工作记忆）
    // ══════════════════════════════════════════════════════════════
    if self.config.persist_user_messages {
        let messages = ap.messages();
        let new_messages: Vec<Message> = messages
            .iter()
            .skip(last_persisted_count)
            .cloned()
            .collect();

        if !new_messages.is_empty() {
            match tokio::time::timeout(
                timeout,
                memory_provider.persist_messages(&session_id, &new_messages),
            )
            .await
            {
                Ok(Ok(())) => {
                    let new_count = messages.len();
                    ap.write_context_raw("last_persisted_count", Box::new(new_count))
                        .map_err(|e| PluginError::Runtime(
                            format!("{} 写入持久化进度失败: {}", LOG_PREFIX, e)
                        ))?;
                    tracing::debug!(
                        "{} 持久化 {} 条消息（累计 {} 条）",
                        LOG_PREFIX, new_messages.len(), new_count
                    );
                }
                Ok(Err(e)) => {
                    tracing::error!("{} 消息持久化失败: {}", LOG_PREFIX, e);
                }
                Err(_) => {
                    tracing::warn!(
                        "{} 消息持久化超时（{} 秒）",
                        LOG_PREFIX, self.config.write_timeout_secs
                    );
                }
            }
        }
    }

    // ══════════════════════════════════════════════════════════════
    // 步骤 4：持久化工具观察结果
    // ══════════════════════════════════════════════════════════════
    if self.config.persist_observations {
        if let Some(observation_any) = ap.read_context_raw("observation") {
            if let Some(observation) = observation_any.downcast_ref::<Observation>() {
                match tokio::time::timeout(
                    timeout,
                    memory_provider.persist_observation(&session_id, observation),
                )
                .await
                {
                    Ok(Ok(())) => {
                        tracing::debug!("{} 观察结果已持久化", LOG_PREFIX);
                    }
                    Ok(Err(e)) => {
                        tracing::error!("{} 观察结果持久化失败: {}", LOG_PREFIX, e);
                    }
                    Err(_) => {
                        tracing::warn!("{} 观察结果持久化超时", LOG_PREFIX);
                    }
                }
            }
        }
    }

    // ══════════════════════════════════════════════════════════════
    // 步骤 5：触发向量索引更新（L3，异步，不阻塞 Pipeline）
    // ══════════════════════════════════════════════════════════════
    if self.config.update_vector_index {
        let messages = ap.messages();
        if messages.len() > last_indexed_count {
            let new_messages: Vec<Message> = messages
                .iter()
                .skip(last_indexed_count)
                .cloned()
                .collect();

            let provider_clone = memory_provider.clone();
            let session_id_clone = session_id.clone();
            // fire-and-forget：向量索引更新不阻塞 Pipeline，JoinHandle 故意丢弃
            // 错误通过 tracing::error 记录，不影响主流程
            tokio::spawn(async move {
                if let Err(e) = provider_clone
                    .trigger_vector_index(&session_id_clone, &new_messages)
                    .await
                {
                    tracing::error!("{} 向量索引更新失败: {}", LOG_PREFIX, e);
                }
            });
            // 注意：tokio::spawn 返回的 JoinHandle 被故意丢弃（fire-and-forget 模式）
            // 如果需要等待结果或取消任务，应保存 JoinHandle，但本场景不需要

            let new_indexed = messages.len();
            ap.write_context_raw("last_indexed_count", Box::new(new_indexed))
                .map_err(|e| PluginError::Runtime(
                    format!("{} 写入索引进度失败: {}", LOG_PREFIX, e)
                ))?;
        }
    }

    // ══════════════════════════════════════════════════════════════
    // 步骤 6：经验提取（可选，异步，不阻塞 Pipeline）
    // ══════════════════════════════════════════════════════════════
    if self.config.enable_experience_extract {
        let messages = ap.messages();
        if messages.len() >= self.config.min_messages_for_experience {
            let provider_clone = memory_provider.clone();
            let session_id_clone = session_id.clone();
            let messages_clone = messages.to_vec();
            // fire-and-forget：经验提取不阻塞 Pipeline，JoinHandle 故意丢弃
            tokio::spawn(async move {
                match provider_clone
                    .extract_experiences(&session_id_clone, &messages_clone)
                    .await
                {
                    Ok(experiences) if !experiences.is_empty() => {
                        tracing::info!(
                            "{} 提取 {} 条经验",
                            LOG_PREFIX, experiences.len()
                        );
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::error!("{} 经验提取失败: {}", LOG_PREFIX, e);
                    }
                }
            });
        }
    }

    // ══════════════════════════════════════════════════════════════
    // 步骤 7：写入持久化完成标记
    // ══════════════════════════════════════════════════════════════
    let current_persisted_count: usize = ap
        .read_context_raw("last_persisted_count")
        .and_then(|any| any.downcast_ref::<usize>().copied())
        .unwrap_or(0);

    ap.write_context_raw(
        "memory_persisted",
        Box::new(MemoryPersistedMarker {
            session_id: session_id.clone(),
            persisted_count: current_persisted_count,
            timestamp: Timestamp::now(),
        }),
    )
    .map_err(|e| PluginError::Runtime(
        format!("{} 写入持久化标记失败: {}", LOG_PREFIX, e)
    ))?;

    // ══════════════════════════════════════════════════════════════
    // 步骤 8：返回 Continue
    // ══════════════════════════════════════════════════════════════
    Ok(SlotDirective::Continue)
}
```

### 4.2 init() 逻辑

```rust
async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
    let config_value = ctx
        .plugin_config
        .get("memory_saver")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    self.config = serde_json::from_value(config_value)
        .map_err(|e| PluginError::Config(
            format!("{} 解析 memory_saver 配置失败: {}", LOG_PREFIX, e)
        ))?;

    tracing::info!(
        "{} 初始化完成: persist_user_messages={}, persist_observations={}, update_vector_index={}",
        LOG_PREFIX,
        self.config.persist_user_messages,
        self.config.persist_observations,
        self.config.update_vector_index
    );

    Ok(())
}
```

### 4.3 shutdown() 逻辑

```rust
async fn shutdown(&mut self) -> Result<(), PluginError> {
    tracing::info!("{} 关闭，刷新缓冲区", LOG_PREFIX);
    // 无内部缓冲区需要刷新（每次 run() 直接写入）
    Ok(())
}
```

---

## 5. 数据结构

### 5.1 持久化完成标记

```rust
/// 持久化完成标记——写入 StepContext 供下游 Slot 读取
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryPersistedMarker {
    pub session_id: String,
    pub persisted_count: usize,
    pub timestamp: Timestamp,
}
```

### 5.2 工具执行结果

> **类型归属说明**：`Observation` 是 tool_executor 写入、memory_saver 读取的共享类型。统一定义在 `shared_types` 中（与 `Message`、`ToolDefinition` 并列），不在任何 Slot 本地定义。

```rust
/// 工具执行结果——统一定义在 shared_types 中
/// 本文件仅引用，不重复定义
pub use crate::shared_types::Observation;
```

### 5.3 经验条目

> **类型归属说明**：`ExperienceEntry` 由 MemoryService 定义（memory 服务内部类型），memory_saver 通过 MemoryProvider trait 返回值引用。

```rust
/// 经验条目——由 MemoryService 定义，通过 MemoryProvider::extract_experiences 返回
/// 本文件仅引用，不重复定义
pub use crate::plugins::services::memory::ExperienceEntry;
```

---

## 6. 文件结构

```
plugins/slots/memory_saver/
├── mod.rs              # 模块入口，pub struct MemorySaverSlot + re-export
├── plugin.rs           # SlotPlugin 实现（核心逻辑：init/run/shutdown）
├── config.rs           # MemorySaverConfig 定义 + 常量定义（DEFAULT_*）
├── types.rs            # MemoryPersistedMarker 定义（Observation 引用 shared_types）
└── error.rs            # MemorySaverError 定义

// 注意：MemoryProvider trait 定义在 shared_types 中（与 ToolProvider 并列）
// memory_saver 不拥有 provider.rs，避免 MemoryService 反向依赖 Slot
// MemoryService 从 shared_types 引用 MemoryProvider，注册 Arc<dyn MemoryProvider>
```

### 6.1 mod.rs 规范

```rust
// ============================================
// 模块：memory_saver 槽口
//
// 模块职责：
// 在 Pipeline Memorize 阶段将对话上下文持久化到记忆系统
//
// 模块边界：
// - 本模块负责：消息持久化、观察结果存储、向量索引触发、经验提取
// - 本模块不负责：记忆检索（由 llm_thinker 通过 Provider 读取）、
//                 记忆压缩（由 compression 服务处理）
//
// 依赖 Provider：
// - "memory"（由 MemoryService 注册，提供 MemoryProvider trait）
//   注意：MemoryProvider trait 定义在 shared_types 中，不在本模块 provider.rs
//   本模块的 provider.rs 只做 re-export：pub use shared_types::MemoryProvider
//   依赖方向：memory_saver 和 MemoryService 都从 shared_types 引用，无反向依赖
//
// 被依赖模块：
// - compression_hook 在同一 Memorize 阶段运行，依赖本模块完成持久化
//
// 核心层实现：
// - SlotPlugin → MemorySaverSlot（无状态，无内部组件）
//
// 错误类型：见 error.rs
// 数据类型：见 types.rs
// Provider 接口：见 provider.rs（re-export from shared_types）
//
// 协议合规：
// - S-R03 合规：持久化进度（last_persisted_count）存入 StepContext，不在 Slot 字段中
// - 组件协议 §0：本槽口无子模块，不需要 Orchestrator/Component/AccessPoint
// - 组件协议 §6：mod.rs 只暴露 MemorySaverSlot + MemorySaverConfig
// ============================================

pub mod config;
pub mod error;
pub mod plugin;
pub mod provider;
pub mod types;

pub use config::MemorySaverConfig;
pub use plugin::MemorySaverSlot;
pub(crate) use error::MemorySaverError;
```

---

## 7. MemoryService 配套修改

memory_saver 正常工作需要 MemoryService 注册真正的 Provider，而非空 tuple。

> **组件协议 §6**：模块内部组件通过 Orchestrator 和 AccessPoint 间接通信，不直接引用兄弟组件。

**需要修改的文件**：`plugins/services/memory/service.rs`

**当前代码（需替换）**：
```rust
ap.register_provider("memory", Arc::new(()) as Arc<dyn std::any::Any + Send + Sync>);
```

**应改为**：
```rust
// MemoryProviderImpl 实现 MemoryProvider trait（定义在 shared_types 中）
// 注意：MemoryService 从 shared_types 引用 MemoryProvider，不反向依赖 memory_saver Slot
let provider = Arc::new(MemoryProviderImpl {
    inner: Arc::clone(&self.inner),
});
ap.register_provider("memory", provider as Arc<dyn std::any::Any + Send + Sync>);
```

**MemoryProviderImpl 实现骨架**：

```rust
/// MemoryProvider trait 的实现——桥接 MemoryService 内部实现与 memory_saver Slot
/// MemoryProvider trait 定义在 shared_types 中，此处为具体实现
struct MemoryProviderImpl {
    inner: Arc<RwLock<Option<MemoryInner>>>,
}

#[async_trait]
impl MemoryProvider for MemoryProviderImpl {
    async fn persist_messages(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<(), MemoryError> {
        let mut guard = self.inner.write().await;
        let inner = guard.as_mut().ok_or(MemoryError::NotInitialized)?;
        inner
            .working_memory
            .append_messages(session_id, messages)
            .map_err(|e| MemoryError::WriteError(e.to_string()))
    }

    async fn persist_observation(
        &self,
        session_id: &str,
        observation: &Observation,
    ) -> Result<(), MemoryError> {
        let mut guard = self.inner.write().await;
        let inner = guard.as_mut().ok_or(MemoryError::NotInitialized)?;
        inner
            .working_memory
            .append_observation(session_id, observation)
            .map_err(|e| MemoryError::WriteError(e.to_string()))
    }

    async fn trigger_vector_index(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<(), MemoryError> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or(MemoryError::NotInitialized)?;
        inner
            .vector_index
            .index_messages(session_id, messages)
            .map_err(|e| MemoryError::VectorIndexError(e.to_string()))
    }

    async fn extract_experiences(
        &self,
        session_id: &str,
        messages: &[Message],
    ) -> Result<Vec<ExperienceEntry>, MemoryError> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or(MemoryError::NotInitialized)?;
        inner
            .experience_extract
            .extract(session_id, messages)
            .map_err(|e| MemoryError::ExperienceError(e.to_string()))
    }

    async fn stats(&self) -> Result<MemoryStats, MemoryError> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or(MemoryError::NotInitialized)?;
        Ok(MemoryStats {
            l1_count: inner.working_memory.l1_count(),
            l2_count: inner.working_memory.l2_count(),
            l3_count: inner.vector_index.count(),
            experience_count: inner.experience_extract.count(),
        })
    }
}
```

---

## 8. 注册步骤

> **Slot协议 §8**：新增 Slot 标准流程共需改 2 个文件。

### 8.1 修改 `plugins/slots/mod.rs`（第 1 个文件）

```rust
pub mod llm_thinker;
pub mod memory_saver;    // ★ 新增
pub mod react_loop;
pub mod tool_executor;
pub mod tool_registry;
```

### 8.2 修改 Pipeline 构建代码（第 2 个文件）

在 `main.rs`（或等效的 Pipeline 构建位置）中添加：

```rust
pipeline
    .add_slot(Phase::memorize(), Box::new(memory_saver_slot))
    .add_slot(Phase::memorize(), Box::new(compression_hook_slot));
```

**执行顺序**：memory_saver 在 compression_hook 之前注册，确保持久化完成后再触发压缩。

---

## 9. 测试要点

> **跨平台规范 §3**：测试中无 Unix-only 路径，均用 `std::env::temp_dir()`；禁止访问真实 API。

### 9.1 正常路径测试

| # | 测试场景 | 前置条件 | 输入 | 期望 |
|---|---------|---------|------|------|
| T-N01 | 消息持久化 | messages 有 3 条新消息，Provider 已注册 | 3 条用户/助手消息 | persist_messages 被调用 1 次，StepContext 中 last_persisted_count=3 |
| T-N02 | 观察结果持久化 | context 中有 observation，Provider 已注册 | Observation::Success | persist_observation 被调用 1 次 |
| T-N03 | 向量索引触发 | 有新消息，Provider 已注册 | 2 条新消息 | trigger_vector_index 被调用（异步），last_indexed_count 更新 |
| T-N04 | 完整流程 | messages + observation 均存在，Provider 已注册 | 完整 StepContext | 所有写入完成，返回 Continue，memory_persisted 标记存在 |

### 9.2 边界条件测试

| # | 测试场景 | 输入 | 期望 |
|---|---------|------|------|
| T-B01 | 无新消息 | messages 为空 | persist_messages 不被调用，返回 Continue |
| T-B02 | observation 不存在 | context 中无 "observation" key | persist_observation 被跳过，返回 Continue |
| T-B03 | 消息数不足经验提取 | messages.len() < min_messages_for_experience | extract_experiences 不被调用 |
| T-B04 | 全部配置关闭 | persist_user_messages=false, persist_observations=false, update_vector_index=false | 所有写入被跳过，返回 Continue |

### 9.3 异常路径测试

| # | 测试场景 | 输入 | 期望 |
|---|---------|------|------|
| T-E01 | Memory Provider 未注册 | provider_raw("memory") 返回 None | 跳过持久化，返回 Continue，不返回 Err |
| T-E02 | Provider 类型不匹配 | provider_raw("memory") 返回 Arc<()> | downcast 失败，跳过持久化，返回 Continue |
| T-E03 | 写入超时 | persist_messages 超时 | 记录警告，返回 Continue |
| T-E04 | 写入失败 | persist_messages 返回 Err | 记录错误，返回 Continue |
| T-E05 | init 失败 | 配置格式错误 | 返回 Err，插件不被加载（S-R02） |

### 9.4 幂等性测试（S-R03 合规验证）

| # | 测试场景 | 输入 | 期望 |
|---|---------|------|------|
| T-I01 | 重复运行 | 同一 StepContext 运行两次 | 第二次不重复写入（通过 last_persisted_count 去重） |
| T-I02 | Slot 重建后运行 | 新建 Slot 实例，使用同一 StepContext | 从 StepContext 读取进度，正确续接 |

### 9.5 测试 Mock 实现

```rust
/// Mock MemoryProvider——用于测试，不访问真实存储
#[cfg(test)]
pub struct MockMemoryProvider {
    pub persisted_messages: Arc<RwLock<Vec<Message>>>,
    pub persisted_observations: Arc<RwLock<Vec<Observation>>>,
    pub vector_index_called: Arc<AtomicBool>,
    pub extract_called: Arc<AtomicBool>,
}

#[cfg(test)]
#[async_trait]
impl MemoryProvider for MockMemoryProvider {
    async fn persist_messages(
        &self,
        _session_id: &str,
        messages: &[Message],
    ) -> Result<(), MemoryError> {
        self.persisted_messages.write().await.extend(messages.to_vec());
        Ok(())
    }

    async fn persist_observation(
        &self,
        _session_id: &str,
        observation: &Observation,
    ) -> Result<(), MemoryError> {
        self.persisted_observations.write().await.push(observation.clone());
        Ok(())
    }

    async fn trigger_vector_index(
        &self,
        _session_id: &str,
        _messages: &[Message],
    ) -> Result<(), MemoryError> {
        self.vector_index_called.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn extract_experiences(
        &self,
        _session_id: &str,
        _messages: &[Message],
    ) -> Result<Vec<ExperienceEntry>, MemoryError> {
        self.extract_called.store(true, Ordering::SeqCst);
        Ok(vec![])
    }

    async fn stats(&self) -> Result<MemoryStats, MemoryError> {
        Ok(MemoryStats {
            l1_count: 0,
            l2_count: 0,
            l3_count: 0,
            experience_count: 0,
        })
    }
}
```

---

## 10. 规范合规检查清单

### 10.1 《跨平台与硬编码规范》

| # | 检查项 | 状态 | 措施 | 代码位置 |
|---|--------|------|------|---------|
| 1 | URL 端点来自配置或常量，禁止硬编码 | ✅ 通过 | 本槽口无网络调用，不涉及 URL | — |
| 2 | 模型名称来自配置字段，禁止硬编码 | ✅ 通过 | 本槽口无模型调用，不涉及模型名 | — |
| 3 | 超时值来自配置或 `DEFAULT_*` 常量 | ✅ 通过 | `DEFAULT_MEMORY_WRITE_TIMEOUT_SECS` 常量 + serde default | §2.3 常量；§4.1 步骤 1 |
| 4 | API 版本号定义为模块级 `const` | ✅ 通过 | 不涉及 API 版本号 | — |
| 5 | User-Agent 定义为 `const USER_AGENT` | ✅ 通过 | 不涉及 HTTP 请求 | — |
| 6 | 文件路径通过 `dirs` + `PathBuf::join()` 构建，无 `/tmp/`、`~`、相对路径 | ✅ 通过 | 无文件路径操作；MemoryService 内部处理路径时遵守此规则 | §4.1 全程无 PathBuf |
| 7 | 数字阈值默认 `None` 或从配置读取 | ✅ 通过 | `min_messages_for_experience` 有 `DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE` 常量 + serde default | §2.3 |
| 8 | 平台特定指令通过 `OsKind` 枚举分支，不假设 `sh` 或 `cmd` | ✅ 通过 | 不涉及平台指令 | — |
| 9 | 测试中无 Unix-only 路径，均用 `std::env::temp_dir()` | ✅ 通过 | 测试使用 Mock MemoryProvider，无文件 I/O | §9.5 |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 全部通过 | ⬜ 待验证 | 实现后执行 | — |

### 10.2 《protocol-Slot接入协议》

| # | 检查项 | 条款 | 状态 | 措施 | 代码位置 |
|---|--------|------|------|------|---------|
| 1 | 实现 SlotPlugin（name/init/run/shutdown） | §1 | ✅ | 严格实现四方法生命周期 | §2.2；§4.1/§4.2/§4.3 |
| 2 | name() 返回全局唯一标识 | §1 | ✅ | `"memory_saver"` | §3.3 |
| 3 | init 失败返回 Err，不退化运行 | S-R02 | ✅ | 配置解析失败返回 PluginError::Config | §4.2 |
| 4 | run() 不缓存跨次可变状态 | S-R03 | ✅ | last_persisted_count/last_indexed_count 存入 StepContext | §3.1；§4.1 步骤 2/7 |
| 5 | 只通过 SlotAccessPoint 与核心交互 | §2 | ✅ | 所有交互通过 `ap` 参数 | §3.1 |
| 6 | 权限声明与实际调用一致 | §3/§4 | ✅ | messages:read→`ap.messages()`；context:read→`ap.read_context_raw()`；context:write→`ap.write_context_raw()` | §3.1 权限 tag 表 |
| 7 | requires 声明与实际依赖一致 | §3 | ✅ | 声明 `"memory"`，run() 中 `provider_raw("memory")` | §3.2；§4.1 步骤 1 |
| 8 | SlotDirective 所有变体被正确处理 | §5/S-R01 | ✅ | Continue 覆盖所有路径（含失败降级） | §4.1 步骤 8 |
| 9 | Provider 不可用时优雅降级 | §7 | ✅ | provider_raw 返回 None → warn 日志 → Continue | §4.1 步骤 1 None 分支 |
| 10 | 通过 provider_raw + downcast 获取 Provider | §2.2 | ✅ | `ap.provider_raw("memory")` → `downcast::<Arc<dyn MemoryProvider>>()` | §4.1 步骤 1 |
| 11 | 元数据包含 name/category/version/permissions/requires/conflicts | §3 | ✅ | §3.3 PluginMetadata | §3.3 |
| 12 | 生命周期：init→run(多次)→shutdown | §6 | ✅ | init 解析配置；run 执行持久化；shutdown 无操作 | §4.1/§4.2/§4.3 |
| 13 | 新增 Slot 需改 2 个文件 | §8 | ✅ | plugins/slots/mod.rs + Pipeline 构建代码 | §8.1/§8.2 |

### 10.3 《protocol-模块内部组件协议》

| # | 检查项 | 条款 | 状态 | 措施 | 代码位置 |
|---|--------|------|------|------|---------|
| 1 | 内部子模块实现 Component trait | §1 | ✅ 不适用 | 本槽口无子模块，单一 Slot 直接实现 SlotPlugin | §6.1 |
| 2 | clone_box 返回 Box<dyn ComponentHandle> | §2 | ✅ 不适用 | 无内部组件 | — |
| 3 | 组件间通过 AccessPoint 通信，不直接引用 | §3 | ✅ 不适用 | 无内部组件 | — |
| 4 | AccessPoint 由 Orchestrator 统一注入 | §3.2 | ✅ 不适用 | 无内部组件 | — |
| 5 | 处理结果使用 Processing 枚举 | §4 | ✅ 不适用 | 无内部组件 | — |
| 6 | 组件元数据 ComponentMeta 完整 | §5 | ✅ 不适用 | 无内部组件 | — |
| 7 | Orchestrator 只做编排，不含业务代码 | §5.1 | ✅ 不适用 | 无内部组件 | — |
| 8 | Orchestrator 校验 requires/provides | §5.2 | ✅ 不适用 | 无内部组件 | — |
| 9 | mod.rs 只暴露三样东西 | §6.1 | ✅ | 只暴露 MemorySaverSlot + MemorySaverConfig | §6.1 |
| 10 | call() 获取句柄后必须 downcast | C-R01 | ✅ 不适用 | 无内部组件 | — |
| 11 | requires 声明必须真实可验证 | C-R02 | ✅ 不适用 | 无内部组件 | — |
| 12 | process() 必须可重入 | C-R03 | ✅ 等价满足 | Slot 的 run() 通过 StepContext 传递状态，等价可重入 | §4.1 步骤 2 |
| 13 | Provider trait 定义在 shared_types，无反向依赖 | §2.2 | ✅ | MemoryProvider 在 shared_types 中，MemoryService 不反向依赖 Slot | §0.2；§3.2；§6.1 |

---

## 11. 开发清单

| 序号 | 文件 | 操作 | 说明 |
|------|------|------|------|
| 1 | `shared_types` | 添加 `MemoryProvider` trait + `MemoryError` + `MemoryStats` + `ExperienceEntry` | 统一定义，与 ToolProvider 并列；memory_saver 和 MemoryService 都从 shared_types 引用 |
| 2 | `plugins/slots/memory_saver/config.rs` | 新建 | 常量（DEFAULT_MEMORY_WRITE_TIMEOUT_SECS、DEFAULT_MIN_MESSAGES_FOR_EXPERIENCE）+ MemorySaverConfig（**无 batch_size 字段**） |
| 3 | `plugins/slots/memory_saver/error.rs` | 新建 | MemorySaverError + Into<PluginError> |
| 4 | `plugins/slots/memory_saver/types.rs` | 新建 | MemoryPersistedMarker 定义 |
| 5 | `plugins/slots/memory_saver/plugin.rs` | 新建 | MemorySaverSlot 实现（init/run/shutdown）；run() 中 tokio::spawn 必须加 fire-and-forget 注释 |
| 6 | `plugins/slots/memory_saver/mod.rs` | 新建 | 模块入口（组件协议 §6.1：只暴露 MemorySaverSlot + MemorySaverConfig） |
| 7 | `plugins/slots/mod.rs` | 添加 `pub mod memory_saver` | 模块注册（Slot协议 §8 第 1 个文件） |
| 8 | `main.rs`（或 Pipeline 构建代码） | 添加 `.add_slot(Phase::memorize(), ...)` | 注册到 memorize 阶段，在 compression_hook 之前（Slot协议 §8 第 2 个文件） |
| 9 | `plugins/services/memory/service.rs` | 修改 | 替换空 tuple 注册为 `Arc::new(MemoryProviderImpl { ... })`；从 shared_types 引用 MemoryProvider |
| 10 | `plugins/services/memory/` | 新建 `provider.rs` | MemoryProviderImpl 实现 MemoryProvider trait |

## 12. 依赖关系

### 12.1 上游依赖

| 依赖 | 类型 | 说明 |
|------|------|------|
| `MemoryService` | Provider `"memory"` | 注册 `Arc<dyn MemoryProvider>` 到 ProviderRegistry |
| `shared_types::MemoryProvider` | trait | 定义在 shared_types 中，本槽口通过 provider_raw + downcast 获取 |
| `shared_types::Message` | 类型 | 从 `ap.messages()` 读取 |
| `shared_types::Observation` | 类型 | 从 StepContext 读取（tool_executor 写入） |
| `shared_types::ExperienceEntry` | 类型 | MemoryProvider::extract_experiences 返回值 |

### 12.2 下游依赖

| 依赖者 | 说明 |
|--------|------|
| `compression_hook` | 在同一 Memorize 阶段运行，依赖本模块完成持久化（memory_persisted 标记） |

### 12.3 执行顺序

Pipeline 阶段顺序保证 memorize 在 execute 之后，memory_saver 在 compression_hook 之前注册，无需额外同步。

---

> 文档版本：v3.1  
> 最后更新：2026-05-30