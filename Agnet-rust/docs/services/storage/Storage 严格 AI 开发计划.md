# Storage（统一数据目录管理）严格 AI 开发计划

本计划用于指导 AI 严格按照 `docs/services/storage/storage开发文档.md` 生成 storage 模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：storage（统一数据目录管理）
- **模块定位**：**不是 ServicePlugin**，而是全局工具库 + 后台工作器。所有运行时数据统一存放在单一根目录下，通过环境变量逐层覆盖，提供跨平台的路径解析和会话持久化。
- **内部组件**：
  - `storage.rs` — 路径工具函数（`home()`、`sessions_dir()`、`logs_dir()` 等），纯函数无状态
  - `store_persistence.rs` — `PersistenceWorker`（异步 mpsc 通道 + 原子写入）+ `load_sessions_from_disk()`
- **关键设计决策**：
  1. 不实现 ServicePlugin（路径工具是纯函数，PersistenceWorker 由 AgentRuntime 直接管理）
  2. 路径通过环境变量覆盖（`AAGNET_HOME`、`AAGNET_SESSIONS_DIR` 等），不走 config.toml
  3. 原子写入使用 `write(.tmp) → rename(.json)` 而非 fsync
- **代码目录**：`src/plugins/services/storage/`
- **依赖项**：`tokio`、`serde`、`serde_json`、`tracing`、`dirs`

---

## 硬编码专项预防纲领

在所有开发环节中，硬编码是 AI 最容易犯的顽疾。本计划通过以下三层机制彻底根除：

1. **AI 宪法硬编码禁令**（每轮对话生效）
2. **步骤验收中的硬编码检查项**（人工逐项核对）
3. **终态自动化硬编码扫描**（脚本 + 人工复核）

### 硬编码分类定义

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 文件路径 | `PathBuf::from("~/.aagnet")` | 通过 `dirs::data_dir()` + `PathBuf::join("potoobird")` 构建，或以 `AAGNET_HOME` 环境变量覆盖 |
| 目录名 | `"potoobird"` 散落在多处 | 定义为 `const DATA_ROOT_DIR: &str`，只能出现一次 |
| 环境变量名 | `"AAGNET_HOME"` 硬编码在业务逻辑中 | 定义为 `const ENV_HOME: &str`，每个环境变量有独立常量 |
| 文件后缀 | `".json"` / `".tmp"` 散落在代码中 | 定义为 `const SESSION_EXT: &str` / `const TMP_EXT: &str` |
| 日志前缀 | `"[persistence]"` 散落在文件中 | 定义为 `const LOG_PREFIX: &str` |
| 会话 ID 清理规则 | 特殊字符替换逻辑散落 | 定义为 `const FILENAME_ILLEGAL_CHARS: &[char]` |
| 平台默认目录 | `dirs::data_dir()` 直接使用 | 通过 `resolve()` 函数统一封装，调用方不直接使用 dirs |

---

## 项目目录结构

```
src/plugins/services/storage/
├── mod.rs                    # 模块声明（当前不存在，需创建）
├── storage.rs                # 路径工具函数（home/logs_dir/sessions_dir 等）
└── store_persistence.rs      # PersistenceWorker + load_sessions_from_disk
```

模块声明链（需补充）：

```
src/lib.rs                        →  pub mod plugins;
src/plugins/mod.rs                 →  pub mod services;
src/plugins/services/mod.rs        →  pub mod storage;
```

> **注意**：当前 `src/plugins/services/storage/` 目录下 **无 `mod.rs`**，模块未被编译。步骤 0 必须创建 `mod.rs` 并在上层 `services/mod.rs` 中添加 `pub mod storage;`（如该文件不存在则创建）。

---

## AI 宪法（每次对话开始时完整粘贴）

```
[宪法已生效，本次对话必须无条件遵守]

你是一个严格执行设计文档的 Rust 代码生成器。你的代码必须能够直接通过编译、测试，且完全忠实于 `storage开发文档.md`。

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、转换规则、流程步骤，必须与 `storage开发文档.md` 完全一致，不得自行增删改。

2. **零幻觉**：不允许出现设计文档未提及的字段、方法、枚举值或行为。特别注意：
   - Storage 只有两个需要维护的实体（`PersistenceWorker`、`PersistenceCommand`），不凭空生成第 3 个结构体
   - Storage 不做会话管理、不做消息索引、不做向量数据库查询
   - 只提供"放哪里"和"存进去"的能力，"存什么"和"怎么查"由上层模块决定

3. **零硬编码**：
   a. 根目录名 `"potoobird"` 定义为 `const DATA_ROOT_DIR: &str = "potoobird"`，只能出现一次
   b. 所有环境变量名定义为 `const ENV_XXX` 常量（`ENV_HOME`、`ENV_SESSIONS_DIR` 等）
   c. 文件后缀 `.json` / `.tmp` 定义为常量
   d. 日志前缀定义为 `const LOG_PREFIX: &str`
   e. 所有路径通过 `resolve()` 函数解析，优先环境变量 → 拼接 → 平台默认
   f. 拒绝使用 `"~/"`、`"/tmp/"`、`"./"` 等裸路径

4. **完整实现**：每个函数必须完整实现，不允许使用 `todo!()`、`unimplemented!()` 或空函数体。

5. **错误处理完整**：
   - `PersistenceWorker::run()` 循环中遇到 I/O 错误必须记录 warn 日志并继续，不能 panic 退出
   - `handle_command()` 中的错误必须通过 `ack_tx` 回传 `PersistenceAck::Failed`
   - `load_sessions_from_disk()` 遇到无法解析的 `.json` 文件必须跳过并记录 warn，不能终止整个恢复过程
   - 不允许 `unwrap()`（测试除外），测试中的 `unwrap()` 必须有注释说明"测试中安全"

6. **一致性**：方法名、字段名、枚举变体名必须与文档完全一致，大小写敏感。

7. **禁止额外依赖**：只能使用 `std`、`tokio`、`serde`、`serde_json`、`tracing`、`dirs` 以及项目内部模块。严禁引入 `chrono`、`uuid`、`regex`、`tempfile`。

8. **注释规则**：
   - 只允许写"为什么"的注释（解释非显而易见的决策，如"为什么用 rename 而不是 fsync"）
   - 不允许写"做什么"的废话注释（如 `// 解析路径`）
   - 引用设计文档时用 `// 设计文档 §X.Y` 格式

9. **测试同时生成**：
   - 路径解析测试：环境变量存在/不存在、相对路径拼接、绝对路径直接使用
   - 目录函数测试：每个目录函数至少一个测试
   - 持久化测试：发送 SaveSession 命令后收到 ack、Shutdown 优雅退出、写入失败返回 Failed
   - 会话恢复测试：扫描目录中有效/无效 JSON 文件
   - 所有路径测试使用 `std::env::temp_dir()` 隔离，不操作真实文件系统
   - 环境变量测试使用 `std::env::set_var` + `std::env::remove_var`（注意测试隔离，建议使用 `#[serial_test::serial]` 或互斥锁）

10. **杜绝捷径**：
    - `resolve()` 函数必须正确处理相对路径（拼接到 `current_dir()`），不能假设环境变量值总是绝对路径
    - 不能因为路径解析看起来简单就省略环境变量的回退链：ENV > 拼接 > 平台默认
    - `PersistenceWorker` 的 `run()` 循环必须处理 `Shutdown` 优雅退出
    - 原子写入必须先写 `.tmp` 再 `rename`，不能直接写入 `.json`
    - 会话 ID 清理必须替换所有非法文件名字符，不能只替换空格

11. **模块边界**：
    - storage 不做会话管理、不做消息索引、不做向量数据库查询
    - 只提供"放哪里"和"存进去"的能力，"存什么"和"怎么查"由上层模块决定
    - 严禁引入记忆、Chronos、压缩等模块的业务逻辑

12. **日志规范**：
    - `PersistenceWorker` 启动记录 `info`（携带 base_path）
    - 每次 `SaveSession` 完成记录 `debug`（携带 session_id 和 message_count）
    - 持久化失败记录 `warn`（携带 session_id 和 error）
    - 工作器收到 `Shutdown` 记录 `info`
    - 使用 `const LOG_PREFIX` 常量统一日志前缀
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架

**目标**：确保 storage 模块被正确注册，项目可编译。

**操作**：

1. 确认 Cargo.toml 包含以下依赖：

```toml
[dependencies]
tokio = { version = "1", features = ["sync", "fs"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
dirs = "5"
```

2. 创建 `src/plugins/services/storage/mod.rs`：

```rust
pub mod storage;
pub mod store_persistence;

pub use storage::{
    chronos_dir, compressed_dir, home, logs_dir, memory_dir, sessions_dir, vector_db_dir,
    vector_db_enabled,
};
pub use store_persistence::{load_sessions_from_disk, PersistenceAck, PersistenceCommand, PersistenceWorker};
```

3. 确保 `src/plugins/services/mod.rs` 存在且包含 `pub mod storage;`。如果该文件不存在则创建。

4. `cargo check` 通过（允许 warning）。

**验收标准**：
- `cargo check` 无 error
- 外部可通过 `use crate::plugins::services::storage::home` 调用路径函数
- 目录结构完整

---

### 步骤 1：实现路径工具（storage.rs）

**目标**：实现所有路径解析函数和环境变量封装。

**文件**：`src/plugins/services/storage/storage.rs`

**要求**：

1. 常量定义（文件顶部）：

```rust
const DATA_ROOT_DIR: &str = "potoobird";
const ENV_HOME: &str = "AAGNET_HOME";
const ENV_SESSIONS_DIR: &str = "AAGNET_SESSIONS_DIR";
const ENV_COMPRESSED_DIR: &str = "AAGNET_COMPRESSED_DIR";
const ENV_LOGS_DIR: &str = "AAGNET_LOGS_DIR";
const ENV_CHRONOS_DIR: &str = "AAGNET_CHRONOS_DIR";
const ENV_MEMORY_DIR: &str = "AAGNET_MEMORY_DIR";
const ENV_VECTOR_DB_DIR: &str = "AAGNET_VECTOR_DB_DIR";
const ENV_VECTOR_DB_ENABLED: &str = "AAGNET_VECTOR_DB_ENABLED";
```

2. `resolve()` 函数：

```rust
fn resolve(env_key: &str, default: impl FnOnce() -> PathBuf) -> PathBuf {
    if let Ok(val) = std::env::var(env_key) {
        let p = PathBuf::from(val);
        if p.is_relative() {
            std::env::current_dir().unwrap_or_default().join(p)
        } else {
            p
        }
    } else {
        default()
    }
}
```

3. 目录函数（每个函数使用 `resolve(ENV_XXX, || base.join("subdir"))` 模式）：

| 函数 | 签名 | 行为 |
|------|------|------|
| `home()` | `pub fn home() -> PathBuf` | `resolve(ENV_HOME, \|\| dirs::data_dir().unwrap_or_default().join(DATA_ROOT_DIR))` |
| `sessions_dir()` | `pub fn sessions_dir() -> PathBuf` | `resolve(ENV_SESSIONS_DIR, \|\| home().join("sessions"))` |
| `compressed_dir()` | `pub fn compressed_dir() -> PathBuf` | `resolve(ENV_COMPRESSED_DIR, \|\| home().join("compressed"))` |
| `logs_dir()` | `pub fn logs_dir() -> PathBuf` | `resolve(ENV_LOGS_DIR, \|\| home().join("logs"))` |
| `chronos_dir()` | `pub fn chronos_dir() -> PathBuf` | `resolve(ENV_CHRONOS_DIR, \|\| home().join("chronos"))` |
| `memory_dir()` | `pub fn memory_dir() -> PathBuf` | `resolve(ENV_MEMORY_DIR, \|\| home().join("memory"))` |
| `vector_db_dir()` | `pub fn vector_db_dir() -> PathBuf` | `resolve(ENV_VECTOR_DB_DIR, \|\| memory_dir().join("vector_db"))` |

4. `vector_db_enabled()` 函数：

```rust
pub fn vector_db_enabled() -> bool {
    matches!(
        std::env::var(ENV_VECTOR_DB_ENABLED)
            .as_deref(),
        Ok("true") | Ok("1")
    )
}
```

**验收标准**：
- `cargo test` 通过
- 测试覆盖：
  - 环境变量不存在时使用默认路径
  - 环境变量为绝对路径时直接使用
  - 环境变量为相对路径时拼接到 `current_dir()`
  - `DATA_ROOT_DIR` 只出现一次（`const` 定义行）
  - 所有环境变量名定义为常量
  - `vector_db_enabled()` 对 `"true"`/`"1"`/`"false"`/`"0"`/未设置全部覆盖

---

### 步骤 2：实现 PersistenceCommand 与 PersistenceAck

**目标**：定义持久化命令和应答的枚举类型。

**文件**：`src/plugins/services/storage/store_persistence.rs`

**要求**：

1. `PersistenceCommand` 枚举：

```rust
pub enum PersistenceCommand {
    SaveSession {
        session_id: String,
        messages: Vec<Message>,
        ack_tx: Option<oneshot::Sender<PersistenceAck>>,
    },
    Shutdown,
}
```

2. `PersistenceAck` 枚举：

```rust
pub enum PersistenceAck {
    Ok { message_count: usize },
    Failed { reason: String },
}
```

3. `Message` 类型引用自 `crate::core::Message`（需确认路径，可能需要 `use` 引入）。

**验收标准**：
- `cargo check` 通过
- 变体名称与文档完全一致
- 字段类型与文档一致（`Option<oneshot::Sender<PersistenceAck>>` 而非 `Option<Sender<PersistenceAck>>`）

---

### 步骤 3：实现 PersistenceWorker

**目标**：实现异步持久化工作器的完整功能。

**文件**：`src/plugins/services/storage/store_persistence.rs`

**要求**：

1. `PersistenceWorker` 结构体：

```rust
pub struct PersistenceWorker {
    receiver: mpsc::UnboundedReceiver<PersistenceCommand>,
    base_path: PathBuf,
}
```

2. 方法：

| 方法 | 签名 | 行为 |
|------|------|------|
| `new(receiver, base_path)` | `pub fn new(receiver: mpsc::UnboundedReceiver<PersistenceCommand>, base_path: PathBuf) -> Self` | 构造工作器 |
| `run()` | `pub async fn run(&mut self)` | 主事件循环 |

3. `run()` 主循环（严格按文档 §3.3 流程）：

```
run()
  │
  ├── tracing::info!("{LOG_PREFIX} PersistenceWorker started, base_path={}", self.base_path.display())
  │
  └── while let Some(cmd) = self.receiver.recv().await {
       │
       ├── match cmd {
       │    ├── PersistenceCommand::SaveSession { session_id, messages, ack_tx } => {
       │    │   ├── let result = self.save_session(&session_id, &messages).await;
       │    │   ├── if let Some(tx) = ack_tx { let _ = tx.send(result); }
       │    │   }
       │    └── PersistenceCommand::Shutdown => {
       │        ├── tracing::info!("{LOG_PREFIX} received Shutdown, exiting");
       │        └── break;
       │    }
       │   }
      }
  │
  └── tracing::info!("{LOG_PREFIX} PersistenceWorker stopped")
```

4. `save_session()` 方法：

| 方法 | 签名 | 行为 |
|------|------|------|
| `save_session(session_id, messages)` | `async fn save_session(&self, session_id: &str, messages: &[Message]) -> PersistenceAck` | 序列化 → 写入 `.tmp` → rename → 返回 ack |

5. `save_session()` 实现细节：
   - 使用 `sanitize_filename(session_id)` 清理文件名（特殊字符替换为 `_`）
   - 构建路径：`self.base_path.join("sessions").join("{sanitized_id}.json")`
   - 序列化 `messages` 为 JSON 字符串（`serde_json::to_string_pretty` 或 `to_string`）
   - 先写入 `{path}.tmp`（使用 `tokio::fs::write`）
   - 再 `tokio::fs::rename` 为 `{path}`
   - 成功返回 `PersistenceAck::Ok { message_count: messages.len() }`
   - 失败返回 `PersistenceAck::Failed { reason: error.to_string() }`
   - 确保会话目录存在（`tokio::fs::create_dir_all`）

6. `sanitize_filename()` 辅助函数：

```rust
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}
```

**验收标准**：
- `cargo test` 通过
- 测试覆盖：
  - 发送 `SaveSession` 命令后收到 `Ok` ack
  - `Shutdown` 命令后工作器退出
  - 写入失败时返回 `Failed` ack（可通过只读目录模拟）
  - 文件名清理（特殊字符 → `_`）
  - `channel` 关闭时工作器自动退出（所有 sender 释放后 recv 返回 None）

---

### 步骤 4：实现会话恢复

**目标**：实现 `load_sessions_from_disk()` 函数，从磁盘恢复会话。

**文件**：`src/plugins/services/storage/store_persistence.rs`

**要求**：

1. `load_sessions_from_disk()` 函数：

```rust
pub async fn load_sessions_from_disk(
    base_path: &Path,
    store: SharedMessageStore,
) -> Result<usize, std::io::Error>
```

2. 实现流程：
   - 扫描 `base_path.join("sessions")` 目录下所有 `.json` 文件
   - 对每个文件：
     - 读取文件内容
     - 反序列化为 `Vec<Message>`
     - 写入 `SharedMessageStore`
     - 记录 debug 日志（session_id, message_count）
   - 遇到无法解析的 `.json` 文件：
     - 记录 warn 日志（文件名、错误原因）
     - 跳过该文件继续处理下一个
   - 返回成功恢复的会话数
   - 如果 `sessions` 目录不存在，返回 `Ok(0)`（不视为错误）

**验收标准**：
- `cargo test` 通过
- 测试使用 `std::env::temp_dir()` 创建包含多个 `.json` 文件的临时目录
- 测试覆盖：有效文件全部恢复、部分损坏文件跳过、空目录返回 0

---

### 步骤 5：终态自检与硬编码审计

1. 全量编译与测试：

```bash
cargo test --all
cargo check
```

2. 硬编码扫描：

```bash
# 检查 DATA_ROOT_DIR 是否仅定义一次
# 检查每个环境变量名是否定义为 ENV_XXX 常量
# 检查 resolve() 是否正确处理相对路径
# 检查日志前缀是否为 LOG_PREFIX 常量
# 检查文件路径构建是否全部通过路径函数，无硬编码字面量
```

3. 对照 `storage开发文档.md` §4.3 的 10 项自查清单逐项核对。

---

## 持续纪律

- 每轮对话都先发送完整宪法
- 不接受任何包含 `todo!()` 或占位符的代码
- 发现 AI 幻觉立即打断并要求改正
- 每步人工校验不得跳过
- 提交代码时，commit message 标明完成了哪一步骤
