# Storage（统一数据目录管理）设计文档

## 0. 协议依据

| 协议 | 应用层 | 关键条款 |
|------|--------|---------|
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 新增插件自查清单 |
| **protocol-模块内部组件协议** | 内部架构 | §1 Component 单入口、§3 InternalAccessPoint 数据共享 |
| **protocol-Service集成协议** | 接入方式 | §2 受控访问句柄、§2.2 Provider 注册 |

---

## 1. 模块定位

### 1.1 一句话

**所有运行时数据统一存放在单一根目录下，通过环境变量逐层覆盖，提供跨平台的路径解析和会话持久化。**

### 1.2 架构定位

Storage 不是 ServicePlugin，是**全局工具库 + 后台工作器**：

```
┌──────────────────────────────────────────────┐
│  storage::（路径工具函数）                        │
│  - home() / sessions_dir() / logs_dir() 等      │
│  - 环境变量覆盖：AAGNET_XXX_DIR > 默认路径         │
│  - 跨平台：dirs::data_dir() + PathBuf::join()   │
└──────────────────────────────────────────────┘
          │
          │ 被所有需要读写文件的模块调用
          ▼
┌──────────────────────────────────────────────┐
│  PersistenceWorker（后台任务）                    │
│  - 通过 mpsc 通道接收 core::PersistenceCommand  │
│  - 将 Session 消息序列化为 JSON 文件              │
│  - 原子写入：先写 .tmp，再 rename                 │
│  - 接收 Shutdown 信号后 flush 并退出             │
└──────────────────────────────────────────────┘
```

**设计决策**：Storage 不实现 ServicePlugin。
- 理由 1：路径工具是纯函数，无状态，不需要生命周期管理
- 理由 2：PersistenceWorker 由 `AgentRuntime` 直接管理（通过 `persistence_tx` 通道），不通过 Service 框架
- 理由 3：零侵入——任何模块 `use storage::logs_dir()` 即可获取路径

---

## 2. 功能清单

| 功能 | 描述 | 优先级 |
|------|------|--------|
| 根目录解析 | `home()` — 从 `AAGNET_HOME` 环境变量或平台标准目录获取根路径 | P0 |
| 子目录解析 | `sessions_dir()` / `logs_dir()` / `memory_dir()` / `chronos_dir()` / `compressed_dir()` | P0 |
| 向量库目录 | `vector_db_dir()` / `vector_db_enabled()` | P1 |
| 会话持久化 | `PersistenceWorker` — 异步写入 Session JSON 文件，原子 rename | P0 |
| 会话恢复 | `load_sessions_from_disk()` — 扫描目录恢复所有会话到 SharedMessageStore | P0 |
| 跨平台合规 | 所有路径通过 `dirs` + `PathBuf::join()` 构建，不使用裸 `/tmp/`、`~`、相对路径 | P0 |

---

## 3. 核心设计

### 3.1 路径解析优先级

```
AAGNET_XXX_DIR 环境变量（最高优先级）
    │ 未设置？
    ▼
AAGNET_HOME/xxx/（次优先级，通过 home() 解析）
    │ 未设置？
    ▼
平台默认目录（dirs::data_dir()/potoobird/xxx）
```

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

### 3.2 子目录一览

| 函数 | 环境变量 | 默认路径 | 用途 |
|------|---------|---------|------|
| `home()` | `AAGNET_HOME` | `<data_dir>/potoobird` | 数据根目录 |
| `sessions_dir()` | `AAGNET_SESSIONS_DIR` | `<home>/sessions` | 会话持久化 |
| `logs_dir()` | `AAGNET_LOGS_DIR` | `<home>/logs` | 日志输出 |
| `compressed_dir()` | `AAGNET_COMPRESSED_DIR` | `<home>/compressed` | 压缩快照 |
| `chronos_dir()` | `AAGNET_CHRONOS_DIR` | `<home>/chronos` | Chronos 决策记录 |
| `memory_dir()` | `AAGNET_MEMORY_DIR` | `<home>/memory` | 记忆存储 |
| `vector_db_dir()` | `AAGNET_VECTOR_DB_DIR` | `<memory>/vector_db` | 向量数据库 |

### 3.3 PersistenceWorker 设计

```rust
pub struct PersistenceWorker {
    receiver: mpsc::UnboundedReceiver<PersistenceCommand>,
    base_path: PathBuf,
}
```

**主循环**：

```
PersistenceWorker.run()
  │
  ├── 1. 从 channel 接收 PersistenceCommand
  │     ├── SaveSession { session_id, messages, ack_tx }
  │     │   ├── 序列化 messages → JSON 字符串
  │     │   ├── 写入 {session_id}.json.tmp
  │     │   ├── rename → {session_id}.json（原子操作）
  │     │   └── 通过 ack_tx 发送 PersistenceAck
  │     └── Shutdown → flush 并退出
  │
  └── channel 关闭时自动退出
```

**关键设计决策**：
1. **原子写入**：先写 `.tmp` 再 `rename`，防止写入中途崩溃导致文件损坏
2. **会话 ID 清理**：文件名中的特殊字符替换为 `_`，防止路径注入
3. **无界通道**：`PersistenceCommand` 体积小（< 1KB），写入频率低，无界通道不会堆积

### 3.4 会话恢复

```rust
pub async fn load_sessions_from_disk(
    base_path: &Path,
    store: SharedMessageStore,
) -> Result<usize, std::io::Error>
```

启动时调用，扫描 `sessions_dir()` 下所有 `.json` 文件，反序列化后写入 `SharedMessageStore`。

---

## 4. 跨平台与硬编码规范

### 4.1 硬编码值分类（§1，9 类逐条对照）

| # | 类别 | 涉及？ | 合规 |
|---|------|:-----:|:----:|
| 1 | URL/端点 | 不涉及 | ✅ |
| 2 | 模型名 | 不涉及 | ✅ |
| 3 | 超时秒数 | 不涉及 | ✅ |
| 4 | API 版本号 | 不涉及 | ✅ |
| 5 | User-Agent | 不涉及 | ✅ |
| 6 | 文件路径 | 涉及 | ✅ 通过 `AAGNET_HOME` + `dirs::data_dir()` 解析，无裸路径 |
| 7 | 数字阈值 | 不涉及 | ✅ |
| 8 | 字符串模板 | 不涉及 | ✅ |
| 9 | 平台指令 | 不涉及 | ✅ |

### 4.2 跨平台路径规则（§2，8 条逐条对照）

| # | 规则 | 合规 |
|---|------|:----:|
| 2.1 | 禁止裸用 Unix-only 路径（`/tmp/`、`/var/log/`） | ✅ 使用 `dirs::data_dir()` |
| 2.2 | 禁止裸用 `~` | ✅ `AAGNET_HOME` 环境变量展开由调用方处理 |
| 2.3 | 禁止相对路径依赖 CWD | ✅ 相对路径仅用于环境变量值，且通过 `current_dir()` 展开 |
| 2.4 | 路径拼接用 `PathBuf::join()` | ✅ |
| 2.5 | 路径分隔符判断 | ✅ 不涉及 |
| 2.6 | 文件扩展名判断 | ✅ 使用 `.json` / `.tmp` |
| 2.7 | 临时文件/目录 | ✅ 原子写入使用 `.tmp` 后缀，在目标目录内操作 |
| 2.8 | 数据目录 | ✅ `home()` 使用 `dirs::data_dir()` |

### 4.3 自查清单（§4，10 项逐项）

| # | 检查项 | 通过 |
|---|--------|:----:|
| 1 | URL 来自配置或常量 | ✅ 不涉及 |
| 2 | 模型名来自配置 | ✅ 不涉及 |
| 3 | 超时值来自配置或常量 | ✅ 不涉及 |
| 4 | API 版本号为模块级 const | ✅ 不涉及 |
| 5 | User-Agent 为 const | ✅ 不涉及 |
| 6 | 路径用 `dirs` + `join()` | ✅ |
| 7 | 数字阈值从配置读取 | ✅ 不涉及 |
| 8 | 平台指令用 `OsKind` | ✅ 不涉及 |
| 9 | 测试无硬编码路径 | ✅ 使用 `std::env::temp_dir()` |
| 10 | build + test + clippy 通过 | 待验证 |

---

## 5. 红线

| 编号 | 红线 | 合规 |
|------|------|:----:|
| §1 | URL/模型名/超时/版本号/User-Agent 不硬编码 | ✅ |
| §2 | 文件路径不使用 `~`、相对路径、Unix-only 路径 | ✅ |
| §3 | 测试中不使用硬编码路径 | ✅ |

---

## 6. 设计决策

### 6.1 为什么不是 ServicePlugin

**决策**：Storage 不实现 `ServicePlugin` trait。

**理由**：
1. **路径工具是纯函数**：`home()` / `logs_dir()` 等无状态，不需要 `init/start/stop/shutdown`
2. **PersistenceWorker 由 Runtime 管理**：`AgentRuntime` 通过 `persistence_tx` 直接发送 `PersistenceCommand`，不走 Provider 注册
3. **零侵入**：任何模块只需 `use storage::logs_dir()` 即可获取路径，无需通过 `ap.provider_raw("storage")`

### 6.2 为什么用环境变量而不是配置文件

**决策**：目录路径通过环境变量覆盖，不走 `config.toml`。

**理由**：
1. **部署灵活性**：容器化部署时通过 `docker run -e AAGNET_HOME=/data` 即可指定，无需修改配置文件
2. **分层覆盖**：全局根目录（`AAGNET_HOME`）→ 子目录逐个覆盖（`AAGNET_LOGS_DIR`），粒度可控
3. **约定优于配置**：95% 场景不需要设置任何环境变量，默认路径即合理

### 6.3 为什么原子写入用 rename 而不是 fsync

**决策**：使用 `write(.tmp) → rename(.json)` 而非 `write(.json) → fsync`。

**理由**：
1. **rename 是原子操作**：POSIX 保证 rename 的原子性，不会出现半个文件
2. **跨平台一致**：Windows 和 Unix 都支持 rename 原子性（Windows 需要同卷）
3. **性能更好**：不需要显式 fsync（OS 会在合适时机刷盘）

---

## 7. 文件结构

```
src/plugins/services/storage/
├── storage.rs              # 路径工具（home/logs_dir/sessions_dir 等）
└── store_persistence.rs    # PersistenceWorker + load_sessions_from_disk
```

---

## 8. 公开接口

```rust
// ── 路径工具（storage.rs）──

/// 数据根目录
pub fn home() -> PathBuf;

/// 各子系统目录
pub fn sessions_dir() -> PathBuf;
pub fn logs_dir() -> PathBuf;
pub fn compressed_dir() -> PathBuf;
pub fn chronos_dir() -> PathBuf;
pub fn memory_dir() -> PathBuf;
pub fn vector_db_dir() -> PathBuf;
pub fn vector_db_enabled() -> bool;

// ── 持久化（store_persistence.rs）──

/// 启动后台持久化循环（阻塞当前任务直到 Shutdown 或 channel 关闭）
pub async fn run(&mut self);

/// 从磁盘恢复所有会话到 SharedMessageStore
pub async fn load_sessions_from_disk(
    base_path: &Path,
    store: SharedMessageStore,
) -> Result<usize, std::io::Error>;
```
