# EventLogger(统一业务事件日志系统) 设计文档

## 0. 协议依据

| 协议 | 应用层 | 关键条款 |
|------|--------|---------|
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 新增插件自查清单 |

---

## 0.5 功能清单

| 功能 | 描述 | 对应 Component | 优先级 |
|------|------|---------------|--------|
| 事件记录 | 通过全局注册表记录业务事件 | `EventLogger` | P0 |
| 异步写入 | 后台 tokio 任务将事件写入 JSONL 文件 | `AsyncWriter` | P0 |
| 文件滚动 | 按时间/大小自动滚动日志文件 | `AsyncWriter` | P0 |
| 事件级别过滤 | 只记录高于 min_level 的事件 | `EventLogger` | P0 |
| 周期汇总 | 定期调用模块统计接口生成汇总事件 | `spawn_aggregator` | P1 |
| 保留策略 | 定期清理过期文件、控制磁盘用量 | `spawn_retention` | P1 |
| 16 种事件类型 | 覆盖压缩、持久化、Chronos、工具执行、认证、配置、组件、安全 | `SystemEvent` | P0 |

EventLogger 是基础设施模块，不是插件。它不实现 ServicePlugin 或 SlotPlugin，不通过 PluginLoader 加载。

---

## 1. 模块定位

### 1.1 架构定位

EventLogger 不是 ServicePlugin，而是**全局工具库**：

```
┌──────────────────────────────────────────────────────────────┐
│  EventLogger (EventRecorder trait)                              │
│  - 全局注册表：OnceLock<Arc<dyn EventRecorder>>               │
│  - 通过 mpsc channel 将事件传递给 AsyncWriter                   │
│  - 事件级别过滤：只记录高于 min_level 的事件                     │
└──────────────────────────────────────────────────────────────┘
          │ mpsc channel
          ▼
┌──────────────────────────────────────────────────────────────┐
│  AsyncWriter (后台 tokio 任务)                                  │
│  - 从 channel 接收 LogEntry                                   │
│  - 序列化为 JSON 行写入文件                                    │
│  - 按时间/大小自动滚动文件                                      │
└──────────────────────────────────────────────────────────────┘
```

### 1.2 公开接口

```rust
// 初始化（启动时调用一次）
pub fn init(config: LoggerConfig);

// 用自定义 recorder 初始化（测试用）
pub fn init_with(recorder: Arc<dyn EventRecorder>);

// 记录事件（任何地方可调用）
pub fn record_event(event: SystemEvent);

// 带上下文的事件记录
pub fn record_event_with_ctx(event: SystemEvent, session_id: Option<String>, trace_id: Option<String>);

// 启动周期汇总（可选）
pub fn spawn_aggregator(interval_secs: u64, enabled: Vec<AggregatorType>, fetch_stats: impl Fn() -> AggregatedStats + Send + 'static);

// 启动保留策略（可选）
pub fn spawn_retention(output_dir: PathBuf, policy: RetentionPolicy);
```

---

## 2. 核心设计

### 2.1 EventRecorder trait

遵循模块内部组件协议 §6——模块 `mod.rs` 只暴露必要接口：

```rust
pub trait EventRecorder: Send + Sync + Debug {
    /// 记录事件（必须非阻塞）
    fn record(&self, event: SystemEvent);

    /// 带上下文的事件记录（默认实现忽略上下文，直接调用 record()）
    fn record_with_ctx(
        &self,
        event: SystemEvent,
        session_id: Option<String>,
        trace_id: Option<String>,
    ) {
        self.record(event);
    }
}
```

**§10 设计决策**：`record()` 要求非阻塞——EventLogger 通过 mpsc channel 发送事件，实际写入由 AsyncWriter 异步完成。

### 2.2 EventLogger 实现

```rust
pub struct EventLogger {
    tx: mpsc::UnboundedSender<LogEntry>,
    config: LoggerConfig,
}

impl EventRecorder for EventLogger {
    fn record(&self, event: SystemEvent) {
        if !self.config.enabled { return; }
        if event.level() < self.config.min_level { return; }
        let meta = event.into_meta();
        let entry = LogEntry::from_meta(meta, None);
        let _ = self.tx.send(entry);  // 无界通道，永不阻塞
    }
}
```

**§10 设计决策**：`tx.send()` 使用 `let _ =` 忽略发送失败。如果 AsyncWriter 已关闭（如 shutdown），事件丢弃而不是阻塞调用方。

### 2.3 AsyncWriter 异步写入

```rust
pub struct AsyncWriter {
    config: LoggerConfig,
    current_file: Option<File>,
    current_file_path: Option<PathBuf>,
    current_file_size: u64,
    current_date_hint: String,
    current_hour_hint: u32,
    sequence: u32,
}
```

**主循环**：

```
AsyncWriter.run(rx)
  │
  ├── 1. 创建输出目录（create_dir_all）
  │
  ├── 2. 循环接收 LogEntry
  │     ├── 检查是否需要滚动文件
  │     │   ├── Hourly: 日期变化 或 小时变化
  │     │   ├── Daily: 日期变化
  │     │   ├── SizeBased: 文件大小超过限制
  │     │   └── Never: 不滚动
  │     ├── 滚动时：flush → 创建新文件
  │     └── 写入 JSON 行 + 换行
  │
  └── 3. channel 关闭时：flush 并退出
```

### 2.4 文件滚动策略

| 策略 | 触发条件 | 文件名格式 |
|------|---------|-----------|
| Hourly | 日期变化或小时变化 | `{prefix}_{YYYY-MM-DD}_{HH}.jsonl` |
| Daily | 日期变化 | `{prefix}_{YYYY-MM-DD}.jsonl` |
| SizeBased | 文件大小超过 `max_size` 字节 | `{prefix}_{YYYY-MM-DD}_{seq:04}.jsonl` |
| Never | 不滚动 | `{prefix}.jsonl` |

### 2.5 SystemEvent 事件类型

16 种事件类型，覆盖框架所有关键路径：

| 事件类型 | 模块 | 级别 | 用途 |
|---------|------|------|------|
| `CompressionStarted` | compression | Info | 压缩开始 |
| `CompressionCompleted` | compression | Info | 压缩完成 |
| `CompressionFailed` | compression | Error | 压缩失败 |
| `CompressionCasConflict` | compression | Warning | CAS 写冲突 |
| `PersistenceSnapshot` | persistence | Debug | 持久化快照 |
| `PersistenceError` | persistence | Error | 持久化错误 |
| `ChronosDecision` | chronos | Info | Chronos 决策 |
| `ChronosFeedback` | chronos | Info | Chronos 反馈 |
| `SystemStartup` | system | Info | 系统启动 |
| `SystemShutdown` | system | Info | 系统关闭 |
| `AggregatedStats` | aggregator | Info | 周期汇总 |
| `ToolCallStarted` | tool_executor | Debug | 工具调用开始 |
| `ToolCallCompleted` | tool_executor | Info | 工具调用完成 |
| `AuthDecision` | auth | Info | 认证决策 |
| `ConfigChanged` | config_loader | Info | 配置变更 |
| `ComponentToggled` | component_switch | Info | 组件开关 |
| `SecurityDecided` | security_policy | Info | 安全策略决策 |

### 2.6 LogEntry 结构

```rust
pub struct LogEntry {
    pub timestamp: String,      // RFC3339 格式
    pub event_id: String,       // UUID v4
    pub session_id: Option<String>,
    pub trace_id: Option<String>,
    pub module: &'static str,
    pub level: &'static str,
    pub event_type: &'static str,
    pub payload: serde_json::Value,
}
```

### 2.7 周期汇总

```rust
pub fn spawn_aggregator(
    interval_secs: u64,
    _enabled: Vec<AggregatorType>,
    fetch_stats: impl Fn() -> AggregatedStats + Send + 'static,
)
```

独立 tokio 任务，按配置间隔调用 `fetch_stats()` 闭包获取统计信息，生成 `AggregatedStats` 事件。

### 2.8 保留策略

```rust
pub fn spawn_retention(output_dir: PathBuf, policy: RetentionPolicy)
```

独立 tokio 任务，每小时扫描日志目录：

1. **按天数删除**：文件修改时间超过 `policy.days` 天 → 删除
2. **按大小删除**：总文件大小超过 `policy.max_disk_mb` MB → 从最旧的开始删除直到满足限制

---

## 3. 运行时信号

EventLogger 不是 ServicePlugin，不直接响应 `ServiceSignal`。但可以通过以下方式间接响应：

| 信号 | 间接处理方式 |
|------|------------|
| `GracefulShutdown` | AsyncWriter 在 channel 关闭时自动 flush 并退出 |
| `ConfigReload` | 调用 `init_with()` 替换全局 recorder |
| `HealthCheck` | 无直接响应（EventLogger 是同步的，不存在"未运行"状态） |

---

## 4. 跨平台与硬编码规范视角

### 4.1 硬编码值分类（§1，9 类逐条对照）

| # | 类别 | 涉及？ | 合规 |
|---|------|:-----:|:----:|
| 1 | URL/端点 | 不涉及 | ✅ |
| 2 | 模型名 | 不涉及 | ✅ |
| 3 | 超时秒数 | 不涉及 | ✅ |
| 4 | API 版本号 | 不涉及 | ✅ |
| 5 | User-Agent | 不涉及 | ✅ |
| 6 | 文件路径 | 涉及 | ✅ `output_dir` 从配置读取，默认值使用 `crate::core::storage::logs_dir()` |
| 7 | 数字阈值 | 涉及 | ✅ `min_level`、`retention.days`、`retention.max_disk_mb` 均从配置读取 |
| 8 | 字符串模板 | 涉及 | ✅ `file_prefix` 从配置读取 |
| 9 | 平台指令 | 不涉及 | ✅ |

### 4.2 跨平台路径规则（§2，8 条逐条对照）

| # | 规则 | 合规 |
|---|------|:----:|
| 2.1 | 禁止裸用 Unix-only 路径 | ✅ |
| 2.2 | 禁止裸用 `~` | ✅ |
| 2.3 | 禁止相对路径依赖 CWD | ✅ |
| 2.4 | 路径拼接用 `PathBuf::join()` | ✅ |
| 2.5 | 路径分隔符判断 | ✅ 不涉及 |
| 2.6 | 文件扩展名判断 | ✅ 使用 `.jsonl` 跨平台扩展名 |
| 2.7 | 临时文件/目录 | ✅ 不涉及 |
| 2.8 | 数据目录 | ✅ 使用 `crate::core::storage::logs_dir()` |

### 4.3 测试代码规范（§3，3 条逐条对照）

| # | 规则 | 合规 |
|---|------|:----:|
| 3.1 | 临时路径用 `std::env::temp_dir()` | ✅ |
| 3.2 | 平台特定测试用 `#[cfg()]` | ✅ 不涉及 |
| 3.3 | 网络测试用 mock 或 `#[ignore]` | ✅ 不涉及 |

### 4.4 自查清单（§4，10 项逐项）

| # | 检查项 | 通过 |
|---|--------|:----:|
| 1 | URL 来自配置或常量 | ✅ |
| 2 | 模型名来自配置 | ✅ |
| 3 | 超时值来自配置或常量 | ✅ |
| 4 | API 版本号为模块级 const | ✅ 不涉及 |
| 5 | User-Agent 为 const | ✅ 不涉及 |
| 6 | 路径用 `dirs` + `join()` | ✅ 使用 `storage::logs_dir()` |
| 7 | 数字阈值从配置读取 | ✅ |
| 8 | 平台指令用 `OsKind` | ✅ 不涉及 |
| 9 | 测试无硬编码路径 | ✅ |
| 10 | build + test + clippy 通过 | 待验证 |

---

## 5. 红线

### 跨平台与硬编码规范红线

| 编号 | 红线 | 合规 |
|------|------|:----:|
| §1 | URL/模型名/超时/版本号/User-Agent 不硬编码 | ✅ 不涉及 |
| §2 | 文件路径不使用 `~`、相对路径、Unix-only 路径 | ✅ |
| §3 | 测试中不使用硬编码路径 | ✅ |

---

## 6. 设计决策

### 6.1 为什么作为基础设施而非插件

**决策**：EventLogger 不实现 `ServicePlugin` trait，而是全局工具库 + `OnceLock`。

**理由**：
1. **零侵入**：任何模块只需 `use logging::record_event;` 即可调用，无需构造函数注入
2. **无后台循环**：写入任务由 `tokio::spawn` 启动，不通过 `start()` 管理
3. **无 Provider**：EventLogger 不提供业务能力，只记录日志
4. **简化调用**：如果改为 ServicePlugin，每个模块调用前都需要先获取 Provider，违背"零侵入"目标

### 6.2 无界通道用于事件传递

**决策**：EventLogger 到 AsyncWriter 之间使用 `mpsc::unbounded_channel`。

**理由**：
1. **不阻塞调用方**：`record_event()` 必须是同步且非阻塞的
2. **事件体积小**：每条 LogEntry < 1KB（JSON 序列化后）
3. **产生速率有限**：业务事件不会高频产生
4. **堆积可控**：即使 AsyncWriter 写入慢，内存占用可忽略

### 6.3 事件级别过滤在发送前

**决策**：EventLogger 在 `record()` 中先检查 `min_level`，再发送到 channel。

**理由**：避免不需要的事件进入 channel 和 AsyncWriter，减少 I/O 开销。过滤逻辑在同步代码中完成，不消耗异步任务资源。

### 6.4 AsyncWriter 在 channel 关闭时 flush

**决策**：AsyncWriter 在 `rx.recv()` 返回 `None` 时（channel 关闭）执行 `flush()` 并退出。

**理由**：确保最后一批事件不会丢失。`flush()` 将文件缓冲区写入磁盘，保证数据持久化。

### 6.5 保留策略独立于主写入任务

**决策**：保留策略由独立的 `spawn_retention()` 任务执行，不嵌入 AsyncWriter。

**理由**：
1. **关注点分离**：写入和清理是两个独立职责
2. **清理频率不同**：写入是实时的，清理是每小时一次
3. **清理可能耗时**：遍历目录、删除文件可能较慢，不应阻塞写入任务

---

## 7. 新增/替换流程

### 新增事件类型

| 步骤 | 做什么 | 涉及文件 |
|------|--------|---------|
| 1 | 定义 payload 结构体 | `event.rs` |
| 2 | 使用 `impl_payload!` 宏实现 `event_type()`、`level()`、`module()` | 同上 |
| 3 | 在 `SystemEvent` 枚举中添加变体 | 同上 |
| 4 | 在 `SystemEvent::into_meta()` 中添加 match 分支 | 同上 |
| 5 | 在 `SystemEvent::level()` 中添加 match 分支 | 同上 |
| 6 | `cargo check` | — |

**共需改 1 个文件**：`event.rs`

### 替换 EventRecorder 实现

| 步骤 | 做什么 |
|------|--------|
| 1 | 实现 `EventRecorder` trait |
| 2 | 在启动时调用 `init_with(Arc::new(MyRecorder))` |
| 3 | `cargo check` |
