# L3 Vector Memory → Assembler 集成开发计划

> 上位约束：`docs/development/AI开发红线与纪律.md`
> 现有计划：`docs/services/memory/Memory 严格 AI 开发计划.md` 步骤 4（**不替代**）
> 集成方向：Memory L3 → Assembler Slot（via `PROVIDER_VECTOR`）
> 计划日期：2026-06-01

---

## 0. 目标

把 Memory L3（向量知识库）从"内部组件（不暴露）"升级为"通过 `PROVIDER_VECTOR` 对外暴露"，并让 Assembler 的 `VectorMemoryProvider` 真正调用 L3 检索（而非当前的 L2 关键词搜索）。

**附带目标**（用户在决策 2 拍板）：
- 实现 `vector_db` feature flag（按 `docs/services/memory/memory开发文档.md` §6.3 承诺）
- 让 `l3_vector` 模块在 `cargo build`（默认）下完全不编译，按需启用

**完成定义**：
1. `cargo check --no-default-features` 0 errors, 0 warnings（L3 完整屏蔽）
2. `cargo check`（默认） 0 errors, 0 new warnings
3. `cargo build --features vector_db` 0 errors
4. `cargo test`（默认 + `--features vector_db`）都通过
5. 4 协议 grep 守卫全部 0 匹配
6. E2E：跑 aagnet 启用 L3，写入一些文档，Assembler 检索到相关内容

---

## 1. 协议与红线引用

| 红线 | 来源 | 本计划如何遵守 |
|------|------|---------------|
| **K-R01** | shared_types §2 | 注册/查询用 `PROVIDER_VECTOR` 常量 |
| **K-R02** | shared_types §2 | `PROVIDER_VECTOR` 先在 shared_types 定义 |
| **T-R01** | shared_types §3 | `VectorMemoryContract` 放 `shared_types/vector.rs` |
| **T-R02** | shared_types §3 | 第一阶段就定义 trait |
| **D-R01** | shared_types §4 | 用现有 `DynProvider<T>`，不造 `DynVectorProvider` |
| **P-R01** | Service §6 | `MemoryService::start` 不留 `Arc::new(())` |
| **P-R02** | Service §6 | 至少 Assembler 1 个消费者 |
| **V-R01** | Service §8 | `HealthCheck` 5s 内 |
| **V-R02** | Service §8 | 后台任务用 `tokio::spawn` |
| **V-R03** | Service §8 | 插件 metadata YAML 与 `start()` 一致 |
| **C-R04** | 内部组件 | 即使 L3 内部用 Component 模式，主入口必须触发 |
| 跨平台 | `docs/跨平台与硬编码规范.md` | workspace 默认 `dirs::data_dir() + join("potoobird/memory")` |
| Feature flag | `docs/services/memory/memory开发文档.md` §6.3 | 按文档实现 `vector_db` flag |

---

## 2. 架构决策（已与用户拍板）

### 2.1 Feature Flag 形状

**用户决策**：**实现**（不是推迟+加 deviation 注释）

**flag 设计**：
```toml
# Cargo.toml
[features]
default = ["vector_db"]
vector_db = []
```

**`default = ["vector_db"]` 是关键决策**——保证现有用户不破坏，向后兼容。

**模块编译条件**：
- `src/plugins/services/memory/l3_vector/` 整个目录只在 `#[cfg(feature = "vector_db")]` 下编译
- `MemoryConfig::l3` 字段在 `#[cfg(feature = "vector_db")]` 下
- `VectorStoreManager` 等类型在 `#[cfg(feature = "vector_db")]` 下
- `MemoryService::start` 中注册 `PROVIDER_VECTOR` 的代码在 `#[cfg(feature = "vector_db")]` 下
- `VectorMemoryProvider`（Assembler 侧）**总是在编译**——但当 `vector_db` feature 关闭时，`provide()` 降级返回空块

### 2.2 Provider trait 形状

**新文件**：`src/shared_types/vector.rs`

**核心 trait**：
```rust
#[async_trait]
pub trait VectorMemoryContract: Send + Sync {
    async fn search(&self, query: &str, top_k: usize) -> Result<Vec<VectorSearchHit>, VectorError>;
    async fn upsert(&self, id: &str, text: &str, metadata: serde_json::Value) -> Result<(), VectorError>;
    async fn delete(&self, ids: &[String]) -> Result<(), VectorError>;
    async fn stats(&self) -> Result<VectorStats, VectorError>;
}
```

**辅助类型**：
```rust
pub const PROVIDER_VECTOR: &str = "vector";

pub struct VectorSearchHit {
    pub id: String,
    pub score: f32,
    pub text: String,
    pub source: String,
}

pub struct VectorStats {
    pub total_vectors: usize,
    pub dim: usize,
}

pub enum VectorError {
    NotReady(String),  // 当 vector_db feature 关闭时返回这个
    SearchFailed(String),
    UpsertFailed(String),
    DeleteFailed(String),
}
```

### 2.3 Assembler consumer 集成（决策 2.1）

**当前**（`src/plugins/slots/assembler/providers/vector_memory.rs:33-51`）：
- 用 `PROVIDER_MEMORY` 查询
- downcast 到 `DynProvider<dyn MemoryProvider>`
- 调 `provider.search_memory(&query, ...)` → 实际是 L2 关键词搜索

**改造**：
- 用 `PROVIDER_VECTOR` 查询
- downcast 到 `DynProvider<dyn VectorMemoryContract>`
- 调 `provider.search(&query, top_k).await`
- 失败/未就绪时降级回 `PROVIDER_MEMORY` 的 `search_memory`

**降级链**（重要）：PROVIDER_VECTOR 不可用 → 用 PROVIDER_MEMORY.search_memory（兼容老路径）。这样：
- 用户不开 `vector_db` feature：Assembler 仍能从 L2 关键词搜索拿到"接近"的结果
- 用户开了但 VectorStoreManager 没注册：同上降级
- 用户开了且 L3 注册：真实 L3 检索

### 2.4 CleanupService / SyncService / DreamOptimizer

| 组件 | 当前状态 | 本计划处理 |
|------|---------|-----------|
| `CleanupService` | 只清 `is_invalid=true` | 加 weight/age 清理（T-V5） |
| `VectorSyncService` | 只有单次 `sync_document` | 加 start/stop 后台循环（T-V4） |
| `DreamOptimizerService::run_cycle` | no-op | 不在本计划范围，留 TODO 注释 |
| `RetrievalService::hybrid_search` | BM25 是 `to_lowercase().contains` 占位 | 不在本计划范围；**所有 v0.2 TODO 统一放 `docs/services/memory/v0.2-roadmap.md`**，代码内 0 注释 |

理由：这两项是"已有但占位"，不阻塞主流程，留到 v0.2 单独任务做。

---

## 3. 任务清单

### Phase A：定义契约 + Feature Flag 基础设施（5 个任务）

#### A-1. 修改 `Cargo.toml` 加 `[features]` 段

**文件**：`Cargo.toml`

**操作**：
- 末尾加：
  ```toml
  [features]
  default = ["vector_db"]
  vector_db = []
  ```

**禁止**：
- ❌ 不要给 `vector_db` 加额外依赖（l3_memory 是 in-memory，不依赖 rusqlite/lancedb/qdrant）
- ❌ 不要在 `[dependencies]` 段加 `optional = true` 字段（现在用不上）

**验证**：
- `cargo check` 0 errors
- `cargo check --no-default-features` 0 errors（应该不编译 l3_vector）
- `cargo build --features vector_db` 0 errors
- 跑 `rg "cfg\(feature = .vector_db" src/` 至少 5 处（验证 flag 真的被使用）

#### A-2. `memory/l3_vector/` 模块加 cfg 守护

**文件**：`src/plugins/services/memory/l3_vector/mod.rs`

**操作**：
- 把整个文件包在 `#[cfg(feature = "vector_db")]` 下：
  ```rust
  //! L3 向量知识库
  #![cfg(feature = "vector_db")]
  
  pub mod chunker;
  // ... 其他 mod
  ```
- 或者用 `#[cfg(feature = "vector_db")] pub mod chunker;` 一行一行加（**推荐这种**——更显式）

**验证**：
- `cargo check --no-default-features` 0 errors（不应该编译 l3_vector）
- 跑 `rg "#\[cfg\(feature = .vector_db.\)\]" src/plugins/services/memory/l3_vector/` 命中 12 处

#### A-3. `memory/mod.rs` 引用 l3_vector 加 cfg

**文件**：`src/plugins/services/memory/mod.rs:8`

**操作**：
- `pub mod l3_vector;` 改为 `#[cfg(feature = "vector_db")] pub mod l3_vector;`

**禁止**：
- ❌ 不要在 `mod.rs` 内用 `cfg_attr` 条件 re-export（破坏 IDE 索引）

**验证**：
- `cargo check --no-default-features` 0 errors
- 跑 `rg "l3_vector" src/plugins/services/memory/mod.rs` 命中 1（cfg 守护）

#### A-4. 新建 `shared_types/vector.rs`

**文件**：新建 `src/shared_types/vector.rs`

**内容**：
- `pub const PROVIDER_VECTOR: &str = "vector";`（K-R01 + K-R02）
- `pub trait VectorMemoryContract`（T-R01）
- `pub struct VectorSearchHit`
- `pub struct VectorStats`
- `pub enum VectorError`
- 加 `pub mod vector;` 到 `src/shared_types/mod.rs:38` 后面
- re-export：`pub use vector::{PROVIDER_VECTOR, VectorMemoryContract, VectorSearchHit, VectorStats, VectorError};`

**禁止**：
- ❌ 不要在 `VectorMemoryContract` trait 方法里调任何具体 backend（Qdrant/LanceDB）
- ❌ 不要给 trait 方法加 `&self` 之外的引用类型
- ❌ 不要让 `VectorError` 用 `String` 之外的具体错误类型（避免循环依赖）

**验证**：
- 跑 `rg "pub const PROVIDER_VECTOR" src/shared_types/vector.rs` 命中 1
- `cargo check` 0 errors

#### A-5. 修改 `memory/config.rs` 给 `L3Config` 加 cfg 守护

**文件**：`src/plugins/services/memory/config.rs:107-119`

**操作**：
- `L3Config`、`VectorBackend` 整段加 `#[cfg(feature = "vector_db")]`：
  ```rust
  #[cfg(feature = "vector_db")]
  #[derive(Debug, Clone, Serialize, Deserialize)]
  pub struct L3Config { ... }
  ```
- `MemoryConfig` 的 `l3: L3Config` 字段也加 cfg：
  ```rust
  #[cfg(feature = "vector_db")]
  #[serde(default)] pub l3: L3Config,
  ```
- `resolve_paths` 中的 `self.l3.resolve_paths(...)` 也加 cfg

**禁止**：
- ❌ 不要让 `MemoryConfig` 在 `--no-default-features` 下变成"无法 l3 字段"
- ❌ 不要在 `L3Config` 移除时破坏现有 YAML 兼容性

**验证**：
- `cargo check --no-default-features` 0 errors
- `cargo check` 0 errors

---

### Phase B：L3 服务侧自完成（6 个任务）

#### B-1. `impl VectorMemoryContract for VectorStoreManager`

**文件**：`src/plugins/services/memory/l3_vector/manager.rs:12-31`

**操作**：
- 加 `use crate::shared_types::{VectorError, VectorMemoryContract, VectorSearchHit, VectorStats};`
- 加 `#[async_trait] impl VectorMemoryContract for VectorStoreManager`：
  ```rust
  #[async_trait]
  impl VectorMemoryContract for VectorStoreManager {
      async fn search(&self, query: &str, top_k: usize) -> Result<Vec<VectorSearchHit>, VectorError> {
          let filter = crate::l3_vector::metadata::VectorFilter::default();
          match self.retrieval.search(query, top_k, &filter).await {
              Ok(hits) => Ok(hits.into_iter().map(|(id, score, meta)| VectorSearchHit {
                  id, score, text: meta.text, source: meta.source_doc_id,
              }).collect()),
              Err(e) => Err(VectorError::SearchFailed(e)),
          }
      }
      async fn upsert(&self, id: &str, text: &str, metadata: serde_json::Value) -> Result<(), VectorError> {
          let meta = crate::l3_vector::metadata::VectorMetadata {
              source_doc_id: id.to_string(),
              section_title: String::new(),
              text: text.to_string(),
              weight: 1.0,
              tags: vec![],
              doc_type: "default".to_string(),
              created_at: chrono::Utc::now().to_rfc3339(),
              last_accessed: chrono::Utc::now().to_rfc3339(),
              access_count: 0,
              is_invalid: false,
          };
          let embedding = self.embedder.embed(&[text.to_string()]).await
              .map_err(|e| VectorError::UpsertFailed(e))?;
          let vector = embedding.first().cloned().unwrap_or_default();
          self.store.upsert(vec![(id.to_string(), vector, meta)]).await
              .map_err(|e| VectorError::UpsertFailed(e.to_string()))
      }
      async fn delete(&self, ids: &[String]) -> Result<(), VectorError> {
          self.store.delete(ids).await.map_err(|e| VectorError::DeleteFailed(e.to_string()))
      }
      async fn stats(&self) -> Result<VectorStats, VectorError> {
          self.store.stats().await.map(|s| VectorStats { total_vectors: s.total_vectors, dim: s.dim })
              .map_err(|e| VectorError::SearchFailed(e.to_string()))
      }
  }
  ```
- 整个文件加 `#[cfg(feature = "vector_db")]` 顶部守卫

**禁止**：
- ❌ 不要在 `search` 里做 hybrid（只在 semantic）
- ❌ 不要硬编码 `top_k` 默认值（让 caller 决定）
- ❌ 不要在 `upsert` 里 `chunker.chunk()`（那是 sync 的事）

**验证**：
- 跑 `rg "impl VectorMemoryContract for VectorStoreManager" src/` 命中 1
- `cargo check --features vector_db` 0 errors

#### B-2. `VectorStoreManager::init()` 和 `shutdown()` 完善

**文件**：`src/plugins/services/memory/l3_vector/manager.rs:21-30`

**操作**：
- 加 `pub async fn init(&mut self) -> Result<(), VectorStoreError>` 方法
  - 调 `self.store.init().await`
  - 调 `self.sync.start()` 启动后台循环（见 B-3）
  - 调 `self.cleanup.start()` 启动后台循环（见 B-4）
- 加 `pub async fn shutdown(&mut self) -> Result<(), VectorStoreError>` 方法
  - 调 `self.sync.stop()` 停止后台
  - 调 `self.cleanup.stop()` 停止后台
  - 调 `self.store.delete` 清空（可选，按 config 决定）

**禁止**：
- ❌ 不要在 `init`/`shutdown` 同步等待任何 LLM 调用
- ❌ 不要在 `shutdown` 阶段报错就直接 panic

**验证**：
- `cargo test` 通过（如果 `manager.rs` 已有测试）

#### B-3. `VectorSyncService` 加后台循环（最小骨架 + 真实 L2 扫描）

**文件**：`src/plugins/services/memory/l3_vector/sync.rs:13-44`

**反 A-02 偷懒**：
- ❌ 不能留空 `// TODO` 占位
- ✅ 必须做"最小可用骨架"：真实扫描 L2 目录（按 mtime）+ 真实调 `chunk()` + 真实调 `embed()` + 真实 `upsert` 到 store
- v0.2 才加：增量 diff、batch_size 自适应、失败重试

**操作**：
1. 加字段 `running: Arc<AtomicBool>`、`interval_secs: u64`、`l2_path: PathBuf`、`synced_mtime: Arc<Mutex<HashMap<PathBuf, SystemTime>>>`
2. 加方法 `pub fn start(&self)`：
   ```rust
   pub fn start(&self) {
       self.running.store(true, Ordering::SeqCst);
       let store = self.store.clone();
       let chunker = self.chunker.clone();
       let embedder = self.embedder.clone();
       let running = self.running.clone();
       let interval = self.interval_secs;
       let l2_path = self.l2_path.clone();
       let synced_mtime = self.synced_mtime.clone();
       tokio::spawn(async move {
           let mut tick = tokio::time::interval(Duration::from_secs(interval));
           tick.tick().await; // 跳过首次立即触发
           while running.load(Ordering::SeqCst) {
               tick.tick().await;
               // 真实最小扫描：遍历 L2 目录 .md/.txt，按 mtime diff
               match scan_l2_for_changes(&l2_path, &synced_mtime).await {
                   Ok(changed) => {
                       for path in changed {
                           if let Err(e) = sync_one(&store, &chunker, &embedder, &path).await {
                               tracing::warn!("sync {} failed: {}", path.display(), e);
                           }
                       }
                   }
                   Err(e) => tracing::warn!("scan_l2 failed: {}", e),
               }
           }
       });
   }
   ```
3. 加方法 `pub fn stop(&self) { self.running.store(false, Ordering::SeqCst); }`
4. 加 cfg 守护：整个文件 `#[cfg(feature = "vector_db")]`
5. 实现 `async fn scan_l2_for_changes(...)`：用 `tokio::fs::read_dir` 遍历 → 对比 `synced_mtime` 中记录的 mtime → 返回新/修改的文件列表
6. 实现 `async fn sync_one(...)`：读文件 → `chunker.chunk(&text)` → 对每块 `embedder.embed(&chunk)` → `store.upsert(...)` → 更新 `synced_mtime[path] = mtime`
7. v0.2 留 TODO 但**不写入代码注释**（单独写在 `docs/services/memory/v0.2-roadmap.md`）：增量 diff 算法、batch 限流、失败重试

**禁止**：
- ❌ 不要留空 `// TODO` 在代码内（反 A-02 偷懒）
- ❌ 不要让循环永不停（`stop` 必须能停）
- ❌ 不要在 `start` 同步等待 LLM 调用（embedder 内部应是 async）
- ❌ 不要 hardcode 路径（用 `l2_path` 字段）
- ❌ 不要忽略 `scan_l2_for_changes` 错误（要 log warn）

**验证**：
- 加测试 `tests/sync_loop.rs`：
  - 调 `start()` → 等 100ms → 创建测试文件 → 调 `stop()` → 确认 task 在 5s 内退出
  - 验证：写入文件 → 等 2 tick → 查询 store 应能搜到嵌入
- 跑 `rg "tokio::spawn" src/plugins/services/memory/l3_vector/sync.rs` 命中 1
- 跑 `rg "TODO" src/plugins/services/memory/l3_vector/sync.rs` 命中 0
- `cargo test --features vector_db sync_loop` 通过

#### B-4. `CleanupService` 加 weight/age 标准

**文件**：`src/plugins/services/memory/l3_vector/cleanup.rs:6-18`

**操作**：
- 加字段 `min_weight: f64`、`max_age_days: u64`、`interval_secs: u64`
- 加方法 `pub fn start(&self)`（与 B-3 同模式）
- 加方法 `pub fn stop(&self)`
- 改造 `cleanup()`:
  ```rust
  pub async fn cleanup(&self) -> Result<usize, VectorStoreError> {
      let all = self.store.search(&[0.0; 1], 100000, &VectorFilter::default()).await?;
      let now = chrono::Utc::now();
      let invalid_ids: Vec<String> = all.iter().filter(|(_, _, m)| {
          if m.is_invalid { return true; }
          if m.weight < self.min_weight { return true; }
          // age 检查需要 parse m.last_accessed
          if let Ok(last) = chrono::DateTime::parse_from_rfc3339(&m.last_accessed) {
              let age = now.signed_duration_since(last);
              if age.num_days() > self.max_age_days as i64 { return true; }
          }
          false
      }).map(|(id, _, _)| id.clone()).collect();
      let count = invalid_ids.len();
      if !invalid_ids.is_empty() { self.store.delete(&invalid_ids).await?; }
      Ok(count)
  }
  ```
- 加 cfg 守卫

**禁止**：
- ❌ 不要 hardcode `min_weight = 0.05`（从 config 拿）
- ❌ 不要让 `start` 失败导致整个 MemoryService 启动失败

**验证**：
- 加测试：插入 3 个向量（一个 weight=0.01、一个 weight=0.5、一个正常）→ cleanup → 确认只 1 个被删
- 跑 `rg "is_invalid\|m\.weight" src/plugins/services/memory/l3_vector/cleanup.rs` 命中 ≥2

#### B-5. `MemoryService::start` 注册真 VectorProvider

**文件**：`src/plugins/services/memory/service.rs:106-115`

**操作**：
- 整个 106-115 段加 `#[cfg(feature = "vector_db")]` 守卫
- 改 `ap.register_provider("vector", Arc::new(()));` 为：
  ```rust
  #[cfg(feature = "vector_db")]
  if let Some(vsm) = &inner.vector_store {
      let vsm_arc: Arc<dyn VectorMemoryContract> = vsm.clone() as Arc<dyn VectorMemoryContract>;
      ap.register_provider(PROVIDER_VECTOR, Arc::new(DynProvider(vsm_arc)));
  }
  ```
- import: `use crate::shared_types::{DynProvider, PROVIDER_VECTOR, VectorMemoryContract};`
- `MemoryService::start` 后台任务：VectorSyncService 和 CleanupService 的 start 调用：
  ```rust
  #[cfg(feature = "vector_db")]
  if let Some(vsm) = &inner.vector_store {
      vsm.init().await.map_err(|e| PluginError::Runtime(format!("L3 init: {}", e)))?;
  }
  ```
  （在 ForgettingService 之前调，确保 L3 先就绪）

**禁止**：
- ❌ 不要保留 `Arc::new(())` 占位（违反 P-R01）
- ❌ 不要用裸字符串 `"vector"`（违反 K-R01）
- ❌ 不要在 `vector_db` feature 关闭时编译这段

**验证**：
- 跑 `rg "register_provider" src/plugins/services/memory/service.rs` 命中 2（PROVIDER_MEMORY + PROVIDER_VECTOR）
- 跑 `rg '"vector"' src/plugins/services/memory/service.rs` 命中 0
- 跑 `rg "Arc::new\(\(\)\)" src/plugins/services/memory/service.rs` 命中 0

#### B-6. `MemoryService::start` 后台：VectorSync + Cleanup

**文件**：`src/plugins/services/memory/service.rs:117-157`

**操作**：
- 在现有 ForgettingService `tokio::spawn` 块之后、DreamOptimizer 之前，加：
  ```rust
  #[cfg(feature = "vector_db")]
  if let Some(vsm) = &inner.vector_store {
      vsm.sync.start();
      vsm.cleanup.start();
  }
  ```
- DreamOptimizer 块中已有 `result.cleaned_l3` 的打印但实际是 0——保留即可（dream 是 v0.2 任务）

**禁止**：
- ❌ 不要在 `start` 中 `tokio::time::sleep`（会让 start 阻塞）
- ❌ 不要把 L3 后台任务放到 DreamOptimizer 之前（避免时序问题）

---

### Phase C：Assembler 消费侧（3 个任务）

#### C-1. `VectorMemoryProvider` 改用 `PROVIDER_VECTOR`

**文件**：`src/plugins/slots/assembler/providers/vector_memory.rs:1-72`

**操作**：
- 顶部文件头注释更新：明确"通过 `PROVIDER_VECTOR` 检索 L3 真实向量"
- 加 import: `use crate::shared_types::{DynProvider, PROVIDER_VECTOR, VectorMemoryContract};`
- 保留旧 import (`PROVIDER_MEMORY`/`MemoryProvider`) 用于降级
- `provide()` 改造：
  ```rust
  // Step 1: 尝试 L3 向量
  let vector_hits = if let Some(raw) = ap.provider_raw(PROVIDER_VECTOR) {
      match raw.downcast::<DynProvider<dyn VectorMemoryContract>>() {
          Ok(wrapper) => match wrapper.0.search(&query, quota.max_items).await {
              Ok(hits) => Some(hits),
              Err(e) => { tracing::warn!("VectorMemoryProvider: L3 检索失败, 降级: {}", e); None }
          },
          Err(_) => None,
      }
  } else { None };

  // Step 2: 命中
  if let Some(hits) = vector_hits {
      if !hits.is_empty() {
          let content: Vec<String> = hits.iter().map(|h| h.text.clone()).collect();
          let content = content.join("\n");
          let tokens = (content.len() as f64 / 4.0).ceil() as usize;
          let max_tokens = quota.max_tokens.min(tokens);
          return Ok(ProvidedContext {
              blocks: vec![ContextBlock {
                  section_title: "## Related Knowledge".into(),
                  content,
                  source: "vector_memory".into(),
                  token_count: max_tokens,
              }],
              tokens_used: max_tokens,
          });
      }
  }

  // Step 3: 降级到 L2 关键词搜索（保持现有逻辑）
  if let Some(raw) = ap.provider_raw(PROVIDER_MEMORY) {
      if let Ok(wrapper) = raw.downcast::<DynProvider<dyn MemoryProvider>>() {
          let provider = wrapper.0.clone();
          if let Ok(entries) = provider.search_memory(&query, quota.max_items).await {
              // ... 现有 51-71 行逻辑
          }
      }
  }

  // Step 4: 都不可用，返回空
  Ok(ProvidedContext { blocks: vec![], tokens_used: 0 })
  ```

**禁止**：
- ❌ 不要在 L3 失败时**报错**——降级到 L2 是设计预期
- ❌ 不要在 L3 关闭时让编译失败（PROVIDER_VECTOR 常量总在 shared_types）

**验证**：
- 跑 `rg "PROVIDER_VECTOR\|PROVIDER_MEMORY" src/plugins/slots/assembler/providers/vector_memory.rs` 命中 2（一个 L3，一个降级 L2）
- `cargo check` 0 errors
- `cargo check --no-default-features` 0 errors（不应编译 l3_vector，但 PROVIDER_VECTOR 常量在 shared_types 总是存在）

#### C-2. `VectorMemoryProvider` 单元测试更新

**文件**：`src/plugins/slots/assembler/providers/vector_memory.rs`（追加 `#[cfg(test)] mod tests`）

**测试用例**：
- `provide_with_vector_provider_returns_hits`：mock `PROVIDER_VECTOR` 返回 2 个 hits → 1 个 block
- `provide_with_vector_provider_empty_falls_back_to_memory`：L3 返回空 → L2 提供 hits
- `provide_without_vector_provider_falls_back_to_memory`：L3 不存在 → L2 提供
- `provide_with_both_unavailable_returns_empty`：L3 + L2 都不可用 → 0 blocks
- `provide_with_vector_error_falls_back_to_memory`：L3 报错 → L2 兜底

**验证**：
- 跑 `cargo test vector_memory` 5 个测试都过

#### C-3. 端到端 L3 集成测试

**文件**：新建 `tests/integration_l3_vector.rs`（如不存在）

**测试用例**：
- 启动完整 aagnet，启用 memory + vector_db feature
- 通过 `MemoryProvider::persist_messages` 写 5 条记忆
- 触发 L3 索引（`trigger_vector_index`）
- 跑用户消息 "查询：之前的讨论"
- 检查 `assembler_messages` 中 `## Related Knowledge` 块包含相关内容

---

### Phase D：测试 + 收尾（5 个任务）

#### D-1. L3 组件单元测试

**文件**：各 `l3_vector/*.rs` 追加

**测试**：
- `memory_store.rs`: upsert/search/delete/stats 4 个测试
- `retrieval.rs`: search（已存在）和 hybrid_search 测试
- `chunker.rs`: 已有 1 个测试，扩展加 2 个（按字符长度分割、按 ### 分割）
- `rrf.rs`: 已有 1 个测试
- `metadata.rs`: 默认值测试

#### D-2. `MemoryService` L3 相关测试

**文件**：`src/plugins/services/memory/service.rs`（追加）

**测试**：
- `start_registers_vector_provider_when_l3_enabled`（feature 启用）
- `start_skips_vector_provider_when_l3_disabled`（feature 关闭）
- `start_doesnt_register_when_no_l3_backend`（`VectorBackend::Sqlite` 但未实现 backend）

#### D-3. `VectorMemoryContract` 测试

**文件**：`src/plugins/services/memory/l3_vector/manager.rs`（追加）

**测试**：
- `vector_search_returns_empty_on_empty_store`
- `vector_upsert_then_search_round_trip`
- `vector_delete_removes_entry`
- `vector_stats_reflects_count`

#### D-4. Feature flag 矩阵测试

**文件**：新建 `tests/feature_flag_matrix.rs`

**测试**：
- 编译期测试：让 `cargo check` 和 `cargo check --no-default-features` 都过（不在 Rust 测试代码中，但要在 CI 脚本中）
- 文档化在 `docs/development/feature_flag_test.md`

#### D-5. 4 协议 grep 守卫

**操作**：
```bash
# K-R01
rg '"vector"' src/plugins/services/memory/ src/plugins/slots/assembler/providers/  # 0

# T-R01
rg "pub trait.*Provider\|pub trait.*Contract" src/plugins/services/memory/  # 0

# D-R01
rg "DynVector\|DynMcp" src/  # 0

# P-R01
rg "Arc::new\(\(\)\)" src/plugins/services/memory/service.rs  # 0

# Feature flag 使用
rg "cfg\(feature = .vector_db.\)" src/ | wc -l  # 至少 12 处
```

---

## 4. 任务依赖图

```
A-1 (Cargo.toml features)
  ├── A-2 (l3_vector cfg 守护)
  ├── A-3 (memory/mod.rs cfg)
  ├── A-4 (shared_types/vector.rs)
  └── A-5 (L3Config cfg)
          └── B-1 (impl VectorMemoryContract)
                  ├── B-2 (VectorStoreManager init/shutdown)
                  ├── B-3 (VectorSync start/stop)
                  ├── B-4 (CleanupService weight/age)
                  └── B-5 (MemoryService::start 注册)
                          └── B-6 (MemoryService 后台)
                                  └── C-1 (VectorMemoryProvider)
                                          ├── C-2 (测试)
                                          └── C-3 (E2E)
                                                  └── D-1..D-4 (测试)
                                                          └── D-5 (grep)
```

## 5. 汇报节奏

| Phase 完成 | 汇报内容 |
|-----------|---------|
| Phase A 完 | `cargo check`、`cargo check --no-default-features` 输出、新增 shared_types 列表 |
| Phase B 完 | `cargo check --features vector_db` 输出、新增的 `init/shutdown`、后台循环数 |
| Phase C 完 | `cargo check` 输出、VectorMemoryProvider 5 个测试结果、降级链验证 |
| Phase D 完 | `cargo test` 全过、D-5 grep 守卫结果、E2E 输出 |

## 6. 阻塞项汇报清单

下列问题**遇到时立即停手**：

1. **A-2/A-3**：如果 `cargo check --no-default-features` 后还有其他模块 `use l3_vector::...` 编译失败
2. **B-1**：`VectorMetadata` 当前有 `weight: f64`（非 `Option`），如果使用方在 upsert 时不提供 weight 怎么办
3. **B-3**：后台循环的 L2 扫描接口——按新约定**先尝试**用 `tokio::fs::read_dir` 直接扫描 L2 目录（不依赖 `WorkingMemoryManager`）；若 `l2_path` 不可用（用户未配），降级为 `start()` 警告日志 + 循环空转（不报错）
4. **B-5**：`VectorStoreManager` 是否实现 `Clone`？当前 `#[derive(...)]` 没看到——`vsm.clone()` 会编译失败
5. **C-1**：降级逻辑中 `MessageRole` 是否在 `shared_types/message.rs` 中导出
6. **feature flag**：`cargo check --no-default-features` 时 `MemoryService` 也要能编译通过——L3 字段要 cfg 守护

---

## 7. 风险评估

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| Feature flag 改动破坏现有编译 | 高 | 高（动所有 L3 引用） | 先跑 `cargo check` 验证 |
| `VectorStoreManager` 没 `Clone` 派生 | 中 | 中（编译失败） | 加 `#[derive(Clone)]` + 检查字段都可克隆 |
| `provider_raw(PROVIDER_VECTOR)` 在 `--no-default-features` 下不存在 | 中 | 高（编译失败） | shared_types::vector 总是编译 |
| L3 检索性能 | 低 | 中（NoopEmbedding 慢） | MVP 用 Noop 即可，v0.2 加 OpenAI |
| B-3 偷懒（A-02）| 高 | 0 | 必须做"真实最小骨架"：扫描+chunk+embed+upsert；v0.2 TODO 放 `docs/services/memory/v0.2-roadmap.md`，**不在代码注释** |

| BM25 假实现影响正确性 | 低 | 低（只影响 hybrid_search） | 真实最小骨架在 B-3 实现，TODO 改放 `docs/services/memory/v0.2-roadmap.md`（不在代码内） |
| 反 A-02 偷懒执行 | 中 | 中 | 每完成一个 B-3 子任务都跑 `rg "TODO" src/plugins/services/memory/l3_vector/sync.rs` 验证为 0 |
---

**预计总工作量**：
- Phase A：约 2-3 小时（feature flag + cfg 守护）
- Phase B：约 4-5 小时（5 个 L3 文件改造 + Service 接线）
- Phase C：约 2 小时（VectorMemoryProvider + 降级）
- Phase D：约 2-3 小时（测试矩阵 + grep 守卫）
- **总计**：1 个工作日专注开发 + 0.5 天测试与修复
