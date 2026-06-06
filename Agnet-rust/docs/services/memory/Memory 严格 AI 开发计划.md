# Memory（三层记忆系统）严格 AI 开发计划

> 集成补全版本：2026-06-01（v0.2.0 集成）—— L3→Assembler 集成计划已完成

本计划用于指导 AI 严格按照 `docs/services/memory/memory开发文档.md` 生成 memory 模块的全部代码。

---

## 项目背景

- **模块名称**：memory（三层记忆系统）
- **模块定位**：Agent 的长期记忆系统，分 L1（身份）/ L2（工作记忆）/ L3（向量知识库）三层存储，配套经验提炼、PID 遗忘控制、隐式反馈权重闭环和梦优化定期维护。
- **外部接口**：
  - `MemoryService` — Service 入口（当前未实现）
  - `IdentityManager` — L1 身份管理
  - `WorkingMemoryManager` — L2 工作记忆管理
  - `VectorStoreManager` — L3 向量知识库管理
- **依赖项**：`tokio`、`serde`、`serde_json`、`tracing`、`async-trait`、`dirs`、`chrono`
- **Feature flags**：`l3-memory`（默认）、`l3-sqlite`、`l3-lancedb`、`l3-qdrant`

---

## 硬编码分类定义（memory 特有）

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 文件路径 | `"~/.aagnet/memory/"` | 默认使用 `storage::memory_dir()`，通过 `MemoryConfig.workspace_dir` 覆盖 |
| `~` 展开 | 直接替换 `~` 字符串 | 通过 `resolve_paths()` 使用 `dirs::home_dir()` 展开 |
| PID 参数 | `Kp = 0.02` 硬编码在算法中 | 从 `ForgettingConfig.pid_kp` 读取 |
| 权重阈值 | `threshold_min = 0.2` | 从 `ForgettingConfig.threshold_min` 读取 |
| 遗忘间隔 | `86400` 秒 | 从 `MemoryConfig.forgetting_interval_seconds` 读取 |
| 反馈乘数 | `1.05` / `0.9` | 从 `ForgettingConfig.feedback_success_multiplier` / `feedback_failure_multiplier` 读取 |
| 权重地板 | `0.01` | 从 `ForgettingConfig.weight_floor` 读取 |
| 保护天数 | `7` 天 | 从 `ForgettingConfig.access_protection_days` 读取 |
| 备份目录 | `".backup"` | 从 `MemoryConfig.backup_dir` 读取 |
| 嵌入端点 | `"https://api.openai.com/v1/embeddings"` | 从 `EmbeddingConfig.base_url` 读取 |

---

## 项目目录结构

```
src/plugins/services/memory/
├── mod.rs              # 模块入口：全部子模块声明 + 公开类型 re-export
├── config.rs           # MemoryConfig + ForgettingConfig + L1/L2/L3 各层配置
├── service.rs          # MemoryService（ServicePlugin 实现，当前不存在，需创建）
├── l1_identity/
│   ├── mod.rs          # 导出 IdentityManager / IdentityMetadata / IdentitySection
│   └── manager.rs      # 身份文件加载/解析/注入/原子写入
├── l2_working/
│   ├── mod.rs          # 导出 WorkingMemoryManager / ForgettingService / 等
│   ├── manager.rs      # L2 文件管理器：读写/索引/检索
│   ├── forgetting.rs   # PID 遗忘服务：权重评估→退役→深度删除
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
│   └── metadata.rs     # VectorMetadata / VectorFilter
├── experience_extract/ # 经验提炼服务（从压缩输出→L2 Entry）
├── feedback/           # 隐式反馈监控（权重闭环）
└── dream/              # 梦优化服务（定期 L2 合并+L1 更新+L3 GC）
```

---

## AI 宪法

```
[宪法已生效]

1. **文档唯一真理**：所有类型、签名、默认值、流程步骤与 memory开发文档.md 一致。

2. **零幻觉**：Memory 只有三层（L1/L2/L3），不存在第 4 层。ForgettingService 只有 PID 遗忘一种机制（无 LRU/LFU）。

3. **零硬编码**：
   a. PID 参数（Kp/Ki/Kd）、阈值（threshold_min/max）、乘数（feedback_success/failure_multiplier）从 ForgettingConfig 读取
   b. 遗忘间隔从 MemoryConfig.forgetting_interval_seconds 读取
   c. 嵌入端点/模型名从 EmbeddingConfig 读取
   d. `~` 展开通过 resolve_paths() 使用 dirs::home_dir()
   e. 路径默认值使用 storage::memory_dir()

4. **完整实现**：每个 Component 的 init/process/shutdown 必须有完整实现。

5. **错误处理**：
   - L2 单条写入失败不影响其他条目
   - L3 嵌入 API 失败返回向量为空（不 panic）
   - 遗忘扫描中单个文件处理失败记录 warn 并继续

6. **测试同步生成**：
   - IdentityManager：加载/解析/变更检测/原子写入
   - WorkingMemoryManager：CRUD/索引重建/检索
   - ForgettingService：PID 公式/退役决策/深度删除
   - VectorStore：upsert/search/delete/stats
   - TextChunker：标题分割/重叠/边界条件
   - RRFFusion：融合公式正确性

7. **模块边界**：L3 通过 feature flags 可选编译。MemoryService 只是各层的串联外壳，不重写各层内部逻辑。
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架

**操作**：确认 Cargo.toml 依赖、创建 `service.rs`、确认模块声明链完整

**验收**：`cargo check` 通过（L3 可选 feature 允许 warning）

---

### 步骤 1：Config 层（config.rs）

**要求**：所有配置结构体实现 `Default` + `Serialize`/`Deserialize`

| 结构体 | 关键字段 |
|--------|---------|
| `MemoryConfig` | workspace_dir, l1, l2, l3, forgetting_enabled(true), forgetting_interval_seconds(86400), max_active_files(100), max_file_age_days(None), backup_enabled(false), backup_dir(None), forgetting |
| `ForgettingConfig` | pid_kp(0.02), pid_ki(0.005), pid_kd(0.01), threshold_min(0.2), threshold_max(0.8), access_protection_days(7), deep_delete_age_days(365), deep_delete_weight(0.05), feedback_success_multiplier(1.05), feedback_failure_multiplier(0.9), weight_floor(0.01) |
| `L1Config` | identity_path, auto_update, inject_prefix |
| `L2Config` | base_dir, max_files, index_path |
| `L3Config` | backend(VectorBackend), chunking(ChunkingConfig), embedding(EmbeddingConfig) |

`MemoryConfig::resolve_paths()` 展开 `~` 并递归处理 L1/L2/L3 子路径。

---

### 步骤 2：L1 身份层（l1_identity/）

**2.1 manager.rs — IdentityManager**

| 方法 | 行为 |
|------|------|
| `load(path)` | 读取 IDENTITY.md，解析 YAML frontmatter + Markdown 段落 |
| `inject_to_prompt()` | 格式化为 System Prompt 前缀 |
| `check_modified()` | 对比 inode/mtime 检测外部修改 |
| `update(content, reason)` | 原子写入（.tmp + rename），更新内存缓存 |

IDENTITY.md 格式：`---` frontmatter（name/version/updated）+ `# Identity` / `# Preferences` / `# Goals` 段落。

**验收**：加载/解析/变更检测/原子写入测试

---

### 步骤 3：L2 工作记忆层（l2_working/）

**3.1 manager.rs — WorkingMemoryManager**

| 功能 | 方法 |
|------|------|
| 目录结构 | `experiences/` / `projects/` / `corrections/` / `archive/` + `INDEX.md` + `.forgetting_score.json` |
| 初始化 | `init()` 创建目录 + 加载 INDEX.md |
| 写入 | `write_entry(entry)` 创建 Markdown 文件 + 更新索引 |
| 检索 | `search(tags, query, top_k)` 按标签/关键词/时间排序 |
| 索引 | `rebuild_index()` 重扫目录 |

MemoryFile frontmatter：weight, tags, created_at, last_accessed, access_count, source, status。

**3.2 forgetting.rs — ForgettingService**

PID 遗忘流程：
```
1. 加载 .forgetting_score.json 缓存
2. 遍历所有活跃 L2 文件：
   - 计算 error = target_weight - current_weight
   - pid = Kp*error + Ki*integral + Kd*derivative
   - new_weight = weight - pid (受 feedback_multiplier 调整)
3. 决策：
   - weight < threshold_min → 标记退役 → 移入 archive/
   - age > deep_delete_age_days && weight < deep_delete_weight → 深度删除
   - 否则保持活跃
4. 持久化 .forgetting_score.json
```

保护机制：`access_protection_days=7` 不退役、`weight_floor=0.01` 地板、`no_tag_heat_score=0.5` 标签补偿。

**3.3 slot.rs — ActiveMemoryHookSlot**

实现 SlotPlugin，在 Memorize 阶段将活跃 L2 记忆注入 LLM 上下文（按权重+时效性排序，按 token 预算截断）。

**验收**：PID 公式正确性、退役决策测试、深度删除边界测试、Slot 注入测试

---

### 步骤 4：L3 向量知识库（l3_vector/）

**4.1 store.rs — VectorStore trait**

```rust
#[async_trait]
pub trait VectorStore: Send + Sync {
    async fn init(&self) -> Result<(), VectorStoreError>;
    async fn upsert(&self, items: Vec<(String, Vec<f32>, VectorMetadata)>) -> Result<(), VectorStoreError>;
    async fn search(&self, query: &[f32], top_k: usize, filter: &VectorFilter) -> Result<Vec<(String, f32, VectorMetadata)>, VectorStoreError>;
    async fn delete(&self, ids: &[String]) -> Result<(), VectorStoreError>;
    async fn stats(&self) -> Result<VectorStoreStats>;
}
```

**4.2 memory_store.rs** — 默认 HashMap 内存实现（feature `l3-memory`）

**4.3 embedding.rs** — `EmbeddingService` trait + `NoopEmbeddingModel`（确定性 hash mock）+ `EmbeddingBackend` 枚举（Noop/OpenAI/LocalONNX）

**4.4 chunker.rs** — `TextChunker` 按标题（`###`）分割 Markdown，相邻 chunk 重叠

**4.5 metadata.rs** — `VectorMetadata`（source_doc_id, section_title, text, weight, tags, doc_type, created_at, last_accessed, access_count, is_invalid）+ `VectorFilter` + `VectorStoreError`

**4.6 manager.rs** — `VectorStoreManager`：持有 store + embedder + chunker + retrieval_svc，统一管理入口

**4.7 retrieval.rs** — 三路检索：A（语义）+ B（关键词 BM25）+ C（精确 ID），RRF 融合排序

**4.8 rrf.rs** — `RRFFusion`：`score = Σ 1/(k + rank_i)`，k 默认 60

**4.9 sync.rs** — `VectorSyncService`：定期扫描 L2 变更 → 增量更新 L3

**4.10 cleanup.rs** — `CleanupService`：清理 `is_invalid=true` + 低权重 + 过期向量

**验收**：VectorStore CRUD 测试、Embedding mock 测试、Chunker 分块测试、RRF 融合计算测试

---

### 步骤 5：辅助服务

**5.1 experience_extract/ — ExperienceExtractService**

从 `CompressionService` 的输出提取结构化经验，生成 `ExperienceEntry`（exp_type, content, trigger_condition, error_type, weight）写入 L2 experiences/。

**5.2 feedback/ — FeedbackMonitor**

隐式反馈信号检测：
- 引用记忆（正向）→ `weight *= 1.05`
- 忽略建议（负向）→ `weight *= 0.9`
- 覆盖输出（负向）→ `weight *= 0.9`
- 中性 → 不变

实现 `ServicePlugin` 或事件驱动接口。

**5.3 dream/ — DreamOptimizerService**

定期触发（每日）：
1. L2 合并：相似标签 + 高语义相似度 → 合并
2. L1 更新：从高权重 L2 提炼 → 更新 IDENTITY.md
3. L3 GC：触发 CleanupService

**验收**：提取流程 mock 测试、反馈乘数正确性测试、梦优化定时器测试

---

### 步骤 6：MemoryService + mod.rs

**service.rs — MemoryService**

实现 `ServicePlugin`：
- `name()` → `"memory"`
- `init()` → 串联 L1→L2→L3 初始化 + `resolve_paths()`
- `start()` → `register_provider("memory", ...)` + `register_provider("vector", ...)` + 启动后台任务（ForgettingService / VectorSyncService / DreamOptimizer / CleanupService）
- `handle_signal()` → 6 种信号处理
- `shutdown()` → L3 flush → L2 保存评分缓存 → L1 no-op

**mod.rs**

暴露所有公共类型（约 35 个，涵盖 L1/L2/L3 + 辅助服务）。

**验收**：`cargo test --all` 通过，ServicePlugin 生命周期完整

---

### 步骤 7：终态自检

1. `cargo test --all` 全量通过，`cargo build` 无 error
2. `cargo build --no-default-features`（禁用 L3）通过
3. 对照 `memory开发文档.md` §5.3 的 10 项自查清单
