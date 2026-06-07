# Memory（三层记忆系统）开发文档

## 0. 协议依据

本文档严格遵循以下三份协议标准，逐条对标：

| 协议 | 应用层级 | 关键条款 |
|------|---------|---------|
| **protocol-Service集成协议** | 模块对框架的接入方式 | §1 ServicePlugin 单入口、§2 ServiceAccessPoint 受控访问句柄、§3 运行时信号、§4 插件元数据、§5 生命周期、§8 协议特有红线 |
| **protocol-模块内部组件协议** | 模块内部子模块组织方式 | §1 Component 单入口、§3 AccessPoint 内部数据共享通道、§4 Processing 处理结果、§5 Orchestrator 模块协调器、§6 模块边界规范 |
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 自查清单 |

---

## 1. 模块定位

### 1.1 一句话描述

**Agent 的长期记忆系统，分 L1（身份）/ L2（工作记忆）/ L3（向量知识库）三层存储，配套经验提炼、PID 遗忘控制、隐式反馈权重闭环和梦优化定期维护。**

### 1.2 三层架构总览

```
┌──────────────────────────────────────────────────────────────┐
│  L1 身份层 — IdentityManager                                   │
│  - 单文件 IDENTITY.md（Markdown + YAML frontmatter）            │
│  - 存储 Agent 核心身份、偏好、长期目标                            │
│  - 启动时加载，LLM 每次调用时注入 System Prompt 头部              │
│  - inode/mtime 校验冲突，支持 DreamOptimizer 定期自动更新         │
└──────────────────────────┬───────────────────────────────────┘
                           │ 注入上下文
                           ▼
┌──────────────────────────────────────────────────────────────┐
│  L2 工作记忆层 — WorkingMemoryManager + ForgettingService       │
│  - Markdown 文件集合（experiences/ projects/ corrections/）     │
│  - 每条记忆有 frontmatter（权重/标签/时间戳/来源类型）            │
│  - PID 控制器驱动的遗忘机制（权重衰减 → 退役 → 深度删除）         │
│  - ExperienceExtractService：压缩结果 → L2 新记忆条              │
│  - FeedbackMonitor：隐式反馈（引用/忽略/覆盖）调整权重            │
│  - ActiveMemoryHookSlot：将活跃记忆注入 LLM 上下文               │
└──────────────────────────┬───────────────────────────────────┘
                           │ VectorSyncService 同步
                           ▼
┌──────────────────────────────────────────────────────────────┐
│  L3 向量知识库 — VectorStoreManager（可选 feature flag）         │
│  - 可替换后端：内存（默认）/ SQLite / LanceDB / Qdrant            │
│  - TextChunker：L2 文件 → 语义分块                               │
│  - EmbeddingService：文本块 → 向量（支持 Noop / OpenAI / 本地）   │
│  - RetrievalService + RRFFusion：混合检索（语义+关键词）          │
│  - CleanupService：低权重向量垃圾回收                             │
└──────────────────────────────────────────────────────────────┘
```

### 1.3 辅助服务

```
┌──────────────────────────────────────────────────────────────┐
│  ExperienceExtractService                                     │
│  - 从 CompressionService 的压缩输出提取结构化记忆条              │
│  - 产出的 ExperienceEntry 写入 L2 experiences/ 目录             │
└──────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────┐
│  FeedbackMonitor（隐式反馈）                                    │
│  - 监控用户行为信号：引用记忆 / 忽略建议 / 覆盖输出               │
│  - 正向反馈 → 记忆权重 × 1.05，负向反馈 → 权重 × 0.9             │
│  - 与 ForgettingService 形成权重闭环                            │
└──────────────────────────────────────────────────────────────┘
┌──────────────────────────────────────────────────────────────┐
│  DreamOptimizerService（梦优化）                                │
│  - 定期（如每日）运行，执行三个任务：                              │
│    1. L2 合并：相似记忆条去重合并                                 │
│    2. L1 更新：根据重要 L2 记忆提炼身份要点                        │
│    3. L3 清理：触发 CleanupService GC                            │
└──────────────────────────────────────────────────────────────┘
```

---

## 2. 文件结构

```
src/plugins/services/memory/
├── mod.rs              # 模块入口：全部子模块声明 + 公开类型 re-export
├── config.rs           # MemoryConfig + L1/L2/L3/Forgetting 各层配置
├── l1_identity/
│   ├── mod.rs          # 导出 IdentityManager / IdentityMetadata / IdentitySection
│   └── manager.rs      # 身份文件加载、解析、注入、原子写入
├── l2_working/
│   ├── mod.rs          # 导出 WorkingMemoryManager / ForgettingService / 等
│   ├── manager.rs      # L2 文件管理器：读写、索引、检索
│   ├── forgetting.rs   # PID 遗忘服务：权重评估 → 退役 → 深度删除
│   └── slot.rs         # ActiveMemoryHookSlot：活跃记忆注入
├── l3_vector/
│   ├── mod.rs          # 导出 VectorStoreManager / RetrievalService / 等
│   ├── manager.rs      # L3 统一管理入口
│   ├── store.rs        # VectorStore trait（可替换后端）
│   ├── memory_store.rs # 默认内存实现
│   ├── embedding.rs    # EmbeddingService + EmbeddingBackend
│   ├── chunker.rs      # TextChunker：文本分块
│   ├── retrieval.rs    # 混合检索服务
│   ├── rrf.rs          # RRFFusion：倒数排名融合
│   ├── sync.rs         # VectorSyncService：L2→L3 同步
│   ├── cleanup.rs      # 向量垃圾回收
│   ├── metadata.rs     # VectorMetadata / VectorFilter
│   ├── sqlite.rs       # #[cfg(feature = "l3-sqlite")]
│   ├── lancedb.rs      # #[cfg(feature = "l3-lancedb")]
│   └── qdrant.rs       # #[cfg(feature = "l3-qdrant")]
├── experience_extract/ # 经验提炼服务
├── feedback/           # 隐式反馈监控
├── dream/              # 梦优化服务
└── components/         # (空，预留)
```

> **模块边界规范（§6.1）**：`mod.rs` 暴露全部公共类型（约 35 个），涵盖 L1/L2/L3 及辅助服务。内部实现细节（forgetting 算法、embedding 后端选择等）为 `pub(crate)`。

---

## 3. 功能清单

| 功能 | 描述 | 所属层 | 实现状态 |
|------|------|:---:|:---:|
| 身份加载 | 读取 IDENTITY.md，inode/mtime 校验，注入 System Prompt | L1 | ✅ |
| 身份更新 | DreamOptimizer 根据反馈自动更新身份文件 | L1 | ✅ |
| 记忆写入 | 结构化 Entry 写入 L2 Markdown（含 frontmatter） | L2 | ✅ |
| 记忆检索 | 按标签/时间/关键词检索 L2 文件 | L2 | ✅ |
| PID 遗忘控制 | Kp/Ki/Kd 三参数 PID 控制器，权重衰减→退役→深度删除 | L2 | ✅ |
| 经验提炼 | 从 CompressionService 输出提取 ExperienceEntry→L2 | L2 | ✅ |
| 隐式反馈 | 用户行为→权重调整（成功×1.05，失败×0.9，地板 0.01） | L2 | ✅ |
| 向量嵌入 | 文本分块→嵌入向量（支持多后端） | L3 | ✅ |
| 语义检索 | RRFFusion 混合检索 Top-K | L3 | ✅ |
| L2→L3 同步 | VectorSyncService 定期同步 | L3 | ✅ |
| 向量 GC | CleanupService 清理低权重/过期向量 | L3 | ✅ |
| 梦优化 | 定期 L2 合并+L1 更新+L3 GC | Dream | ✅ |
| ServicePlugin | 完整生命周期 | ✅ 已实现（含 init/start/handle_signal/stop/shutdown） | service.rs |

---

## 4. 核心设计

### 4.1 MemoryConfig（配置）

**文件**：`config.rs`

```rust
pub struct MemoryConfig {
    pub workspace_dir: PathBuf,              // 默认 = storage::memory_dir()
    pub l1: L1Config,                        // 身份层配置
    pub l2: L2Config,                        // 工作记忆层配置
    pub l3: L3Config,                        // 向量层配置
    pub forgetting_enabled: bool,            // 遗忘服务开关
    pub forgetting_interval_seconds: u64,    // 遗忘检查间隔（默认 86400）
    pub max_active_files: usize,             // 最大活跃文件数（默认 100）
    pub max_file_age_days: Option<u64>,       // 最大文件年龄
    pub backup_enabled: bool,                // 备份开关
    pub backup_dir: Option<PathBuf>,         // 备份目录
    pub forgetting: ForgettingConfig,        // PID 遗忘参数
}
```

#### 4.1.1 路径解析（跨平台规范 §2 对标）

`MemoryConfig::resolve_paths()` 处理 `~` 展开和相对路径转换：

```rust
pub fn resolve_paths(&mut self) {
    if let Some(home) = dirs::home_dir() {
        // 展开 ~ 前缀
        if workspace_str.starts_with('~') {
            self.workspace_dir = home.join(&workspace_str[2..]);
        }
        // 递归处理 L1/L2/L3 子路径
        self.l1.resolve_paths(&home);
        self.l2.resolve_paths(&home);
        self.l3.resolve_paths(&home);
        // 备份目录默认 = workspace_dir/.backup
    }
}
```

| 规则 | 合规 | 说明 |
|------|:---:|------|
| §2.2 禁止裸用 `~` | ✅ | `resolve_paths()` 通过 `dirs::home_dir()` 展开 |
| §2.4 路径拼接用 `join()` | ✅ | 全部使用 `PathBuf::join()` |
| §2.8 数据目录通过 dirs | ✅ | `workspace_dir` 默认使用 `storage::memory_dir()` |

#### 4.1.2 ForgettingConfig（PID 遗忘参数）

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `pid_kp` | 0.02 | PID 比例系数 |
| `pid_ki` | 0.005 | PID 积分系数 |
| `pid_kd` | 0.01 | PID 微分系数 |
| `threshold_min` | 0.2 | 权重低于此值 → 标记退役 |
| `threshold_max` | 0.8 | 权重高于此值 → 受保护 |
| `access_protection_days` | 7 | 近期访问保护天数 |
| `deep_delete_age_days` | 365 | 深度删除年龄阈值 |
| `deep_delete_weight` | 0.05 | 深度删除权重阈值 |
| `feedback_success_multiplier` | 1.05 | 正向反馈权重乘数 |
| `feedback_failure_multiplier` | 0.9 | 负向反馈权重乘数 |
| `weight_floor` | 0.01 | 权重地板 |

> **跨平台规范（§1.7）对标**：所有 PID 参数、阈值、乘数均从配置读取，非硬编码在算法函数中。

### 4.2 L1 身份层

**文件**：`l1_identity/manager.rs`

#### 4.2.1 IdentityManager

```rust
pub struct IdentityManager {
    config: L1Config,
    metadata: IdentityMetadata,     // 内存缓存
    sections: Vec<IdentitySection>, // 解析后的身份段落
}
```

| 功能 | 方法 |
|------|------|
| 加载身份文件 | `load()` → 解析 YAML frontmatter + Markdown 正文 |
| 注入上下文 | `inject_to_prompt()` → 格式化为 System Prompt 前缀 |
| 校验变更 | 对比 inode + mtime，检测外部修改冲突 |
| 原子写入 | 先写临时文件 → `rename()` → 更新内存缓存 |

#### 4.2.2 文件格式

```markdown
---
name: "Assistant"
version: "1.0"
updated: "2026-01-15"
---

# Core Identity
I am a helpful coding assistant.

# Preferences
- Prefer Rust over Python
- Use async/await patterns
```

### 4.3 L2 工作记忆层

**文件**：`l2_working/`

#### 4.3.1 WorkingMemoryManager

管理 Markdown 文件集合，按类型分目录：

| 目录 | 内容 | MemoryFileType |
|------|------|:---:|
| `experiences/` | 经验记忆 | `Experience` |
| `projects/` | 项目上下文 | `Project` |
| `corrections/` | 纠错记录 | `Correction` |
| `archive/` | 已退役记忆 | `Archive` |
| `INDEX.md` | 全局索引文件 | — |
| `.forgetting_score.json` | 遗忘评分缓存 | — |

每条记忆文件包含 YAML frontmatter：
```yaml
---
weight: 0.75
tags: [rust, async]
created: "2026-01-15T10:00:00Z"
last_accessed: "2026-01-20T14:00:00Z"
access_count: 5
source: experience
---
```

#### 4.3.2 ForgettingService（PID 遗忘）

基于 PID 控制器的遗忘机制：

```
ForgettingService 定期运行（默认每 86400 秒）
  │
  ├─ 1. 加载 .forgetting_score.json 缓存（含历史误差积分）
  │
  ├─ 2. 遍历所有活跃 L2 文件：
  │      计算当前权重 = f(access_count, last_accessed, age, source_importance, tags)
  │      error = target_weight - current_weight
  │      pid_output = Kp*error + Ki*integral + Kd*derivative
  │
  ├─ 3. 决策：
  │      weight < threshold_min → 标记退役 → 移入 archive/
  │      age > deep_delete_age_days && weight < deep_delete_weight → 深度删除
  │      otherwise → 保持活跃
  │
  └─ 4. 持久化 .forgetting_score.json（积分值 + 决策日志）
```

**PID 关键保护**：
- `access_protection_days = 7`：最近 7 天访问过的文件不退役
- `weight_floor = 0.01`：权重不会降到地板以下
- 标签热度补偿：无标签文件获得 `no_tag_heat_score`（0.5）的额外权重

#### 4.3.3 ActiveMemoryHookSlot

将活跃的 L2 记忆注入 LLM 上下文（通过 `SlotPlugin` 接口），按权重和时效性排序，截断到 token 预算。

### 4.4 L3 向量知识库

**文件**：`l3_vector/`

#### 4.4.1 可替换后端

```rust
pub trait VectorStore: Send + Sync {
    async fn upsert(&self, items: Vec<(String, Vec<f32>, VectorMetadata)>) -> Result<()>;
    async fn search(&self, query: &[f32], top_k: usize, filter: &VectorFilter) -> Result<Vec<...>>;
    async fn delete(&self, ids: &[String]) -> Result<()>;
    async fn stats(&self) -> Result<VectorStoreStats>;
}
```

| Feature Flag | 后端 | 适用场景 |
|-------------|------|---------|
| `l3-memory`（默认） | `MemoryVectorStore` | 开发/测试 |
| `l3-sqlite` | SQLite + 向量扩展 | 单机部署 |
| `l3-lancedb` | LanceDB | 大规模本地 |
| `l3-qdrant` | Qdrant | 分布式生产 |

#### 4.4.2 核心组件

| 组件 | 职责 |
|------|------|
| `VectorStoreManager` | L3 统一管理入口，持有 store + embedding + chunker |
| `TextChunker` | L2 Markdown → 语义段落（按标题分割） |
| `EmbeddingService` | 文本块 → 向量（OpenAI API / 本地模型 / Noop） |
| `VectorSyncService` | 定期扫描 L2 变更 → 增量更新 L3 向量 |
| `RetrievalService` | 混合检索：语义相似度 + 关键词匹配 |
| `RRFFusion` | 倒数排名融合（RRF）：合并多个检索结果列表 |
| `CleanupService` | 清理低权重、过期、孤立向量 |

#### 4.4.3 检索流程

```
用户查询 "how to handle async errors in Rust"
  │
  ├─ 1. EmbeddingService.embed(query) → 查询向量
  │
  ├─ 2. VectorStore.search(query_vec, top_k=20) → 语义候选
  │
  ├─ 3. 关键词 BM25 检索 → 关键词候选
  │
  ├─ 4. RRFFusion.merge(语义候选, 关键词候选)
  │      score = Σ 1/(k + rank_i)   // k=60 默认
  │
  └─ 5. 返回 Top-K 融合结果（附向量元数据）
```

### 4.5 辅助服务

#### 4.5.1 ExperienceExtractService

从 `CompressionService` 的压缩输出（对话摘要）中提取结构化记忆：
- 识别实体、决策、教训
- 生成 `ExperienceEntry`（含标题、标签、权重初值）
- 写入 `L2 experiences/` 目录

#### 4.5.2 FeedbackMonitor

监控 LLM 交互中的隐式反馈信号：
- **正向**：用户引用记忆内容、确认建议、无覆盖 → `weight *= 1.05`
- **负向**：用户忽略、覆盖、反驳 → `weight *= 0.9`
- **中性**：无明确信号 → 权重不变

#### 4.5.3 DreamOptimizerService

定期维护（如每日凌晨）：
1. **L2 合并**：相似记忆条（同标签+高语义相似度）→ 合并为一条，权重取 max
2. **L1 更新**：从高权重 L2 记忆中提取关键信息 → 更新 IDENTITY.md 对应段落
3. **L3 GC**：触发 `CleanupService` 清理

---

## 5. 协议合规性分析

### 5.1 Service 集成协议（protocol-Service集成协议）对标

#### 5.1.1 ServicePlugin 方法职责（协议 §1）

| 方法 | 调用次数 | 用途 | 当前状态 |
|------|---------|------|:---:|
| `name()` | 多次 | 返回全局唯一服务标识 "memory" | ✅ `service.rs` |
| `init(ctx)` | 1 | 初始化 L1/L2/L3 各层，建立文件目录结构 | ✅ `service.rs` |
| `start(ap)` | 1 | `ap.register_provider("memory", ...)` + `ap.register_provider("vector", ...)` | ✅ `service.rs` |
| `handle_signal(signal)` | 多次 | 响应运行时信号（见 5.1.2） | ✅ `service.rs` |
| `stop()` | 多次 | 暂停写入，Provider 仍可读取 | ✅ `service.rs` |
| `shutdown()` | 1 | 持久化状态 + 反注册 Provider | ✅ `service.rs` |

#### 5.1.2 运行时信号处理（协议 §3）

| 信号 | 说明 | 当前处理 | 合规 |
|------|------|:---:|:---:|
| `GracefulShutdown` | 正常关闭，刷新 L2 缓存 + 持久化 L3 向量 | ❌ 无 | — |
| `ImmediateShutdown` | 强制关闭，立即停止 | ❌ 无 | — |
| `ConfigReload` | 重载 MemoryConfig，触发 resolve_paths() | ❌ 无 | — |
| `HealthCheck` | 健康检查，需在 5s 内返回 `Ok(())`（红线 V-R01） | ❌ 无 | V-R01 ❌ |
| `Suspend` | 暂停 ForgettingService + VectorSyncService | ❌ 无 | — |
| `Resume` | 恢复后台服务 | ❌ 无 | — |

#### 5.1.3 生命周期（协议 §5）

```
PluginLoader 读元数据 → 校验 provides/requires
→ init(ctx) → start(ap) ↔ [handle_signal() ...] → stop() → shutdown()
```

当前状态：**全部已实现**。MemoryService (impl ServicePlugin) 管理 L1/L2/L3 生命周期，通过 register_provider 注册 memory/vector Provider。

#### 5.1.3.1 计划声明（ServicePlugin 各方法职责与实现要点）

```rust
#[async_trait]
impl ServicePlugin for MemoryService {
    fn name(&self) -> &str { "memory" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 1. 解析 MemoryConfig.resolve_paths() → 展开 ~ / 相对路径
        // 2. L1: IdentityManager.load() → 加载 IDENTITY.md
        // 3. L2: WorkingMemoryManager.init() → 加载 INDEX.md + .forgetting_score.json
        // 4. L3: VectorStoreManager.init() → 连接后端（内存/SQLite/LanceDB/Qdrant）
        // 5. 各子服务的 init（ForgettingService / FeedbackMonitor / DreamOptimizer）
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        // 注册 Provider（协议 §2.2）：
        ap.register_provider("memory", Arc::new(self.memory_provider.clone()));
        ap.register_provider("vector", Arc::new(self.vector_provider.clone()));
        // 启动后台任务（协议 §6 不阻塞）：
        //   - ForgettingService: 定期扫描（间隔 forgetting_interval_seconds）
        //   - VectorSyncService: L2→L3 增量同步
        //   - DreamOptimizer: 定时器（合并+更新+清理）
        //   - CleanupService: L3 低权重向量 GC
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::HealthCheck => {
                // 红线 V-R01：5s 内检查各层是否正常运行
                // L1: identity 文件可读？L2: 索引文件可写？L3: 后端连接正常？
                Ok(())
            }
            ServiceSignal::GracefulShutdown => {
                // L3 flush 向量 → L2 保存 .forgetting_score.json → L1 不变
                Ok(())
            }
            ServiceSignal::ConfigReload => {
                // 重新 resolve_paths() → 检查目录结构 → 更新各层配置引用
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        // 暂停后台任务，Provider 仍可读取
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        // L3: VectorStoreManager.shutdown() → 断开连接 + flush
        // L2: ForgettingService.shutdown() → 保存评分缓存
        // L1: IdentityManager.shutdown() → no-op（内存中缓存不需主动释放）
        Ok(())
    }
}
```

> 以上声明不要求三层内部逻辑重构——各层已有 `init()`/`shutdown()` 方法，MemoryService 只需将它们按拓扑序串联（L1→L2→L3 初始化，L3→L2→L1 销毁）。

#### 5.1.4 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 ServicePlugin | 需实现 `ServicePlugin` | ✅ | `service.rs` 完整实现 |（详见 5.1.1） |
| §2.1 ServiceAccessPoint | 通过 `get_config()` / `log()` 与 core 交互 | ✅ | `start(ap)` 接收 ServiceAccessPoint | |
| §2.2 register_provider() | 注册 `memory` + `vector` Provider | ✅ | `start()` 中注册 PROVIDER_MEMORY + PROVIDER_VECTOR |
| §3 运行时信号 | 响应全部 6 个信号 | ✅ | `handle_signal()` 在 service.rs 中实现 |（详见 5.1.2） |
| §4 插件元数据 | YAML 声明 provides/requires/run_mode | ❌ | 元数据已设计（见 §7），未接入 PluginLoader |
| §5 生命周期 | init → start → stop → shutdown | ❌ | 无完整生命周期（详见 5.1.3） |
| §6 补充说明 | ServiceAccessPoint Clone、handle_signal<5s | ❌ | 待实现 |
| §7 标准流程 | 8 步骤从零到运行 | ⚠️ | 步骤 1-4 已完成（config/L1/L2/L3），步骤 5-8 待完成 |
| §8 V-R01 HealthCheck | 5s 内返回 `Ok(())` | ❌ | 无实现 |
| §8 V-R02 handle_signal 不阻塞 | 超 5s 须 spawn | ❌ | 无实现 |
| §8 V-R03 provides 一致 | 声明 = 实际注册 | ❌ | 无注册 |

### 5.2 模块内部组件协议（protocol-模块内部组件协议）对标

Memory 是 **模块内部组件协议最适用的模块**——L1/L2/L3 三层子模块正是协议设计的典型场景。

#### 5.2.1.1 Component trait 映射表（协议 §1 要求：统一实现 `Component` trait）

当前 Memory 各组件使用独立 trait（IdentityManager、WorkingMemoryManager 等），协议要求统一为 `Component`。下表说明各组件的 `Component` 方法应如何映射：

| 组件 | `init()` | `process()` | `shutdown()` | 依赖 | 优先级 |
|------|---------|------------|-------------|------|:---:|
| **IdentityManager** (L1) | `load()` 身份文件到内存 | 检查 inode/mtime 变更 → 重新加载 | no-op | — | 10 |
| **WorkingMemoryManager** (L2) | 加载 INDEX.md + 评分缓存 | no-op（查询/写入由外部调用驱动） | 保存 .forgetting_score.json | L1 | 20 |
| **ForgettingService** | 加载评分缓存 | 执行一次遗忘扫描（PID 评估→退役→深度删除） | 保存评分缓存 + 决策日志 | L2 | 30 |
| **VectorStoreManager** (L3) | 连接后端（内存/SQLite/LanceDB/Qdrant） | no-op（查询由 RetrievalService 驱动） | 断开连接 + flush | L2 | 40 |
| **VectorSyncService** | no-op | 检查 L2 变更 → 增量更新 L3 向量 | 完成当前批次 | L2, L3 | 35 |
| **ExperienceExtractService** | no-op | no-op（事件驱动：CompressionService 输出时被调用） | no-op | L2 | 25 |
| **FeedbackMonitor** | no-op | no-op（事件驱动：用户行为信号触发权重调整） | no-op | L2 | 15 |
| **DreamOptimizerService** | 加载定时器配置 | 检查是否到期 → 执行 L2合并+L1更新+L3 GC | 取消定时器 | L1, L2, L3 | 50 |

> **编排顺序**：`Orchestrator::init_all()` 按优先级升序初始化（L1→L2→L3），`shutdown_all()` 反向销毁（L3→L2→L1）。ForgettingService 依赖 WorkingMemoryManager 必须先注册；VectorSyncService 应在 ForgettingService 之后执行（遗忘后再同步，避免同步即将删除的条目）。

#### 5.2.1.2 依赖方向（协议 §6.2）

```
┌──────────────────────┐
│  模块 mod.rs          │  （对外暴露约 35 个公共类型）⚠️ 超出协议建议
│  IdentityManager      │
│  WorkingMemoryManager │
│  VectorStoreManager   │
│  ForgettingService    │
│  ExperienceExtract... │
│  FeedbackMonitor      │
│  DreamOptimizerService│
└──────────┬───────────┘
           │
           ▼
┌──────────────────────────────────────────────┐
│  组件（无标准 Orchestrator — DreamOptimizer 承担部分编排）│
│                                              │
│  L1 IdentityManager                          │
│       ↑ 读取                                  │
│  L2 WorkingMemoryManager ←→ ForgettingService │
│       │ (VectorSyncService)                   │
│       ↓                                      │
│  L3 VectorStoreManager                       │
│       ├── TextChunker                         │
│       ├── EmbeddingService                    │
│       ├── RetrievalService + RRFFusion        │
│       └── CleanupService                      │
│                                              │
│  ExperienceExtractService ──→ L2              │
│  FeedbackMonitor ──→ L2 (权重调整)            │
│  DreamOptimizerService ──→ L1 + L2 + L3       │
│                                              │
│  ⚠️ L1/L2/L3 间存在直接类型引用               │
│  ⚠️ 应为: L2 通过 ap.call("l3") + downcast     │
└──────────────────────────────────────────────┘
```

#### 5.2.2 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 Component | 各层实现 `Component` trait | ❌ | L1/L2/L3 均有独立 trait，未统一为 Component |
| §3 AccessPoint | 层间通过 AP 通信 | ⚠️ | L2↔L3 通过 VectorSyncService 中转，但存在直接类型引用 |
| §5 Orchestrator | 编排器调度 L1→L2→L3 | ❌ | DreamOptimizer 承担部分编排，但未用标准 Orchestrator |
| §6 模块边界 | mod.rs 只暴露入口+配置 | ⚠️ | 导出约 35 个公共类型，超出"只暴露入口+配置"建议 |

### 5.3 跨平台与硬编码规范对标（协议 §4 完整 10 项自查清单）

| # | 检查项 | 合规 | 说明 |
|---|--------|:---:|------|
| 1 | 所有 URL 端点来自配置或常量，非字面量写死 | ✅ | L3 embedding API URL 从 EmbeddingConfig 读取 |
| 2 | 所有模型名称来自配置字段，非硬编码 | ✅ | L3 embedding model 从 EmbeddingConfig 读取 |
| 3 | 所有超时值来自配置或 `DEFAULT_*` 常量 | ✅ | 遗忘间隔 / embedding 超时可配 |
| 4 | API 版本号定义为模块级 `const`，不散落 | ✅ | 不涉及外部 API 版本号 |
| 5 | User-Agent 定义为 `const USER_AGENT` | ✅ | 不涉及 HTTP 请求（embedding 由独立 service 处理） |
| 6 | 文件路径通过 `dirs` + `PathBuf::join()` 构建 | ✅ | `resolve_paths()` 展开 `~`，`workspace_dir` 默认使用 `storage::memory_dir()` |
| 7 | 数字阈值默认 `None` 或从配置读取 | ✅ | PID 参数、权重阈值、间隔秒数均在 ForgettingConfig / MemoryConfig 中 |
| 8 | 平台特定指令通过 `OsKind` 枚举分支 | ✅ | 不涉及 shell 命令 |
| 9 | 测试中无 Unix-only 路径，均用 `std::env::temp_dir()` | ✅ | 测试使用平台无关路径 |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 全部通过 | 待验证 | — |

---

## 6. 红线与质量

| 编号 | 来源 | 红线 | 合规 |
|------|------|------|:---:|
| V-R01~V-R03 | Service集成协议 | HealthCheck/超时/一致性 | ❌ 待补齐 |
| — | aagnet-lessons | 外部输入必须校验 | ✅ frontmatter 解析校验必填字段，路径穿越检查 |
| — | aagnet-lessons | 异步操作必须有超时 | ✅ L3 embedding API 调用有超时配置 |

---

## 7. 插件元数据

```yaml
name: memory
category: service
version: 0.3.0
run_mode: background
provides:
  - memory
  - vector
requires:
  - storage
  - compression
conflicts: []
config_schema:
  type: object
  properties:
    workspace_dir:
      type: string
      description: 记忆工作目录（默认 ~/.aagnet/memory/）
    forgetting_enabled:
      type: boolean
      default: true
    forgetting_interval_seconds:
      type: integer
      default: 86400
    max_active_files:
      type: integer
      default: 100
```

---

## 8. 设计决策

### 8.1 为什么分三层

**决策**：L1 身份 + L2 文件 + L3 向量，三层各司其职。

**理由**：
1. **L1 小而精**：身份信息量小（<2KB），但每条 LLM 调用都需要，必须零延迟
2. **L2 结构化**：Markdown + frontmatter，人可读、可编辑、可版本控制
3. **L3 可伸缩**：向量检索不受条数限制，编译时 feature flag 按需启用后端

### 8.2 为什么用 PID 控制器做遗忘

**决策**：ForgettingService 使用 PID 控制器（非固定阈值）。

**理由**：
1. **自适应性**：PID 根据历史误差自动调节遗忘力度，避免过度遗忘或记忆膨胀
2. **可调优**：Kp/Ki/Kd 三个参数独立可配，适应不同规模和使用场景
3. **反馈闭环**：FeedbackMonitor 调整权重 → PID 感知变化 → 动态决策

---

## 9. 依赖关系

```
IdentityManager         ──→  L1Config
WorkingMemoryManager    ──→  L2Config + MemoryFile
ForgettingService       ──→  ForgettingConfig + WorkingMemoryManager
VectorStoreManager      ──→  L3Config + VectorStore + EmbeddingService
VectorSyncService       ──→  WorkingMemoryManager + VectorStoreManager
ExperienceExtractService──→  CompressionService (外部) + WorkingMemoryManager
FeedbackMonitor         ──→  WorkingMemoryManager
DreamOptimizerService   ──→  IdentityManager + WorkingMemoryManager + CleanupService
```

- 对外依赖：`tokio::fs`（异步文件 IO）、`serde` / `serde_json`（序列化）、`dirs`（跨平台路径）
- 框架层依赖：`core::storage`（数据目录）、`core::Slot` / `core::Phase`（Slot 集成）
    path: PathBuf,
    metadata: IdentityMetadata,
}

impl Component for IdentityManager {
    fn name(&self) -> &str { "identity_manager" }
    async fn init(&mut self, ctx: &ComponentInitContext) -> Result<(), ComponentError>;
    async fn process(&mut self, ap: &mut dyn InternalAccessPoint) -> Result<Processing, ComponentError>;
      // 读取 IDENTITY.md → 解析 sections → 写入 ap
    async fn shutdown(&mut self) -> Result<(), ComponentError>;
}
```

**IDENTITY.md 格式**：

```markdown
---
name: assistant
version: 1
updated: 2026-05-28T10:00:00Z
---

# Identity
You are a helpful assistant...

# Preferences
- Be concise
- Use Rust idioms

# Goals
- Help user build aagnet framework
```

### 3.3 L2 WorkingMemoryManager（Component）

```rust
pub struct WorkingMemoryManager {
    base_path: PathBuf,
    entries: Vec<MemoryEntry>,
    forgetting: ForgettingService,
}

pub struct MemoryEntry {
    pub id: String,
    pub content: String,
    pub tags: Vec<String>,
    pub weight: f64,          // PID 控制器维护的动态权重
    pub created_at: Timestamp,
    pub last_accessed: Timestamp,
    pub access_count: u64,
}
```

### 3.4 ForgettingService（Component）— PID 遗忘控制

```
权重衰减公式（PID 控制器）：

  error = current_weight - target_weight
  P = Kp × error
  I = clamp(Ki × ∫error, -clamp, +clamp)
  D = Kd × Δerror

  new_weight = weight - (P + I + D)
              × feedback_success_multiplier (成功时) / failure_multiplier (失败时)

当 new_weight < threshold_min：
  → 标记删除（软删除，保留 access_protection_days 天缓冲）
当 new_weight > threshold_max：
  → 权重饱和，不再增长
当 weight < deep_delete_weight 且 age > deep_delete_age_days：
  → 物理删除
```

### 3.5 FeedbackMonitor（Component）— 隐式反馈

```rust
pub struct FeedbackMonitor;

impl FeedbackMonitor {
    /// 用户引用了某条记忆 → 权重上升
    pub fn record_citation(&mut self, entry_id: &str);

    /// 用户覆盖了某条记忆 → 可能是错误信息，权重下降
    pub fn record_overwrite(&mut self, entry_id: &str);

    /// 用户忽略了检索结果 → 权重微降
    pub fn record_ignore(&mut self, entry_id: &str);
}
```

### 3.6 L3 VectorStoreManager（Component，可选 feature）

```rust
#[cfg(feature = "vector_db")]
pub struct VectorStoreManager {
    db_path: PathBuf,
    embedding_config: EmbeddingConfig,
}
```

---

## 4. 跨平台与硬编码规范

### 4.1 硬编码值分类（§1）

| # | 类别 | 涉及？ | 合规 |
|---|------|:-----:|:----:|
| 1 | URL/端点 | 涉及（L3 Embedding API） | ✅ 从 `EmbeddingConfig.base_url` 读取 |
| 2 | 模型名 | 涉及（L3 Embedding 模型） | ✅ 从 `EmbeddingConfig.model` 读取 |
| 3 | 超时秒数 | 涉及 | ✅ 从配置读取 |
| 6 | 文件路径 | 涉及 | ✅ 使用 `storage::memory_dir()` |
| 7 | 数字阈值 | 涉及 | ✅ PID 参数从 `ForgettingConfig` 读取 |

### 4.2 跨平台路径（§2）

| # | 规则 | 合规 |
|---|------|:----:|
| 2.1 | 禁止裸用 Unix-only 路径 | ✅ 使用 `storage::memory_dir()` |
| 2.2 | 禁止裸用 `~` | ✅ |
| 2.4 | 路径拼接用 `PathBuf::join()` | ✅ |
| 2.8 | 数据目录 | ✅ |

### 4.3 自查清单（§4）

| # | 检查项 | 通过 |
|---|--------|:----:|
| 1 | URL 来自配置或常量 | ✅ |
| 2 | 模型名来自配置 | ✅ |
| 3 | 超时值来自配置或常量 | ✅ |
| 6 | 路径用 `dirs` + `join()` | ✅ |
| 7 | 数字阈值从配置读取 | ✅ |
| 10 | build + test + clippy 通过 | 待验证 |

---

## 5. 红线

| 编号 | 红线 | 合规 |
|------|------|:----:|
| — | 外部输入必须校验 | ✅ 文件内容解析前校验 frontmatter 格式 |
| — | 错误隔离 | ✅ 单条记忆写入失败不影响其他记忆 |
| — | 不可在库代码中 unwrap | ✅ |

---

## 6. 设计决策

### 6.1 为什么三层架构

**决策**：L1（身份）/ L2（工作记忆）/ L3（向量库）分层。

**理由**：
1. **访问频率不同**：L1 每次调用必读（极高频）→ 单文件最简单；L2 偶尔读写（中频）→ 文件集合 + 索引；L3 按需检索（低频）→ 向量数据库
2. **生命周期不同**：L1 长期稳定（DreamOptimizer 才更新）；L2 动态变化（每条对话可能产生新记忆）；L3 批量操作（定期嵌入 + GC）
3. **故障隔离**：L3 不可用不影响 L1/L2 正常工作

### 6.2 为什么用 PID 控制器做遗忘

**决策**：使用 PID 控制器动态调整记忆权重，而非固定时间衰减。

**理由**：
1. **自适应**：高频访问的记忆权重自然上升，冷门记忆自然下降
2. **可调节**：Kp/Ki/Kd 参数可配置，不同场景调不同策略
3. **反馈闭环**：隐式反馈（引用/忽略）直接影响权重变化速率

### 6.3 为什么 L3 是可选的

**决策**：向量数据库通过 Cargo feature flag 控制。

**理由**：
1. **依赖重**：向量数据库（如 Qdrant/lancedb）引入大量依赖和编译时间
2. **大多数 Agent 不需要**：简单的文件记忆已覆盖 80% 场景
3. **按需启用**：`cargo build --features vector_db` 即可开启

> [集成补全 2026-06-01] feature flag 已实现：`Cargo.toml` 中 `default = ["vector_db"]`，l3_vector 全模块 `#[cfg(feature = "vector_db")]` 守护，`cargo check --no-default-features` 0 errors。

---

## 7. 文件结构

```
src/plugins/services/memory/
├── mod.rs                  # MemoryService (impl ServicePlugin) + 公开接口
├── config.rs               # MemoryConfig / L1Config / L2Config / L3Config / ForgettingConfig
├── l1_identity/
│   ├── mod.rs
│   ├── manager.rs          # IdentityManager (Component)
│   └── types.rs            # IdentityMetadata / IdentitySection
├── l2_working/
│   ├── mod.rs
│   ├── manager.rs          # WorkingMemoryManager (Component)
│   ├── forgetting.rs       # ForgettingService (Component)
│   └── types.rs            # MemoryEntry / MemoryFile / MemoryFileType
├── l3_vector/
│   ├── mod.rs              # #[cfg(feature = "vector_db")]
│   ├── manager.rs          # VectorStoreManager (Component)
│   ├── retrieval.rs        # RetrievalService (Component)
│   ├── gc.rs               # GCService (Component)
│   └── embedding.rs        # EmbeddingClient
├── experience_extract/
│   ├── mod.rs
│   └── service.rs          # ExperienceExtractService (Component)
├── feedback/
│   ├── mod.rs
│   └── monitor.rs          # FeedbackMonitor (Component)
├── dream/
│   ├── mod.rs
│   └── optimizer.rs        # DreamOptimizerService (Component)
└── components/
    └── mod.rs              # 统一 Component 注册
```

---

## 8. 插件元数据

```yaml
name: memory
category: service
version: 0.3.0
run_mode: background
provides:
  - memory
  - vector
requires:
  - storage
conflicts: []
```

---

## 9. 公开接口

```rust
// ── MemoryProvider ──
pub trait MemoryProvider: Send + Sync {
    async fn query(&self, query: &str, top_k: usize) -> Vec<MemoryEntry>;
    async fn write(&self, entry: MemoryEntry) -> Result<(), MemoryError>;
    async fn search_by_tag(&self, tag: &str) -> Vec<MemoryEntry>;
    fn identity(&self) -> &IdentityMetadata;
}

// ── IdentityManager（L1）──
impl IdentityManager {
    pub fn load(path: &Path) -> Result<Self, MemoryError>;
    pub fn sections(&self) -> &[IdentitySection];
    pub fn to_system_prompt(&self) -> String;
}

// ── WorkingMemoryManager（L2）──
impl WorkingMemoryManager {
    pub fn query(&self, query: &str) -> Vec<&MemoryEntry>;
    pub fn write(&mut self, entry: MemoryEntry) -> Result<(), MemoryError>;
    pub fn search_by_tag(&self, tag: &str) -> Vec<&MemoryEntry>;
    pub fn gc(&mut self);  // 触发遗忘扫描
}
```

