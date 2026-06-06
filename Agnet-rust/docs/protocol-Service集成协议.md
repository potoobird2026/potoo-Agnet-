# Service 集成协议（Service Integration Protocol）

## 0. 协议范围

本协议定义 **Service 插件如何接入 aagnet 框架**。Service 是独立于 Pipeline 的
后台服务，负责提供可以被 Slot 和其他 Service 使用的**业务能力**。

Service 通过两个通道与外界交互：
1. **`ServiceAccessPoint`** —— 与 core 交互（配置、日志、Provider 注册）
2. **`register_provider()`** —— 将自己的能力暴露给其他插件

---

## 1. 插件单入口

插件只需要实现 `ServicePlugin`：

```rust
#[async_trait::async_trait]
pub trait ServicePlugin: Send + Sync {
    /// 服务名称
    fn name(&self) -> &str;

    /// 初始化（只调用一次）
    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError>;

    /// 启动后台服务
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError>;

    /// 处理运行时信号
    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError>;

    /// 停止服务（暂停，不销毁）
    async fn stop(&mut self) -> Result<(), PluginError>;

    /// 销毁（只调用一次）
    async fn shutdown(&mut self) -> Result<(), PluginError>;
}
```

### 各方法职责

| 方法 | 调用次数 | 用途 |
|------|---------|------|
| `name` | 多次 | 返回全局唯一服务标识 |
| `init` | 1 | 校验配置、建立连接。失败则服务不被加载 |
| `start` | 1 | 通过 `ServiceAccessPoint` 注册 Provider、启动后台循环 |
| `handle_signal` | 多次 | 响应运行时信号（关闭、重载、健康检查等） |
| `stop` | 多次 | 暂停服务，不销毁资源 |
| `shutdown` | 1 | 释放所有资源、反注册 Provider |

---

## 2. 受控访问句柄

`ServiceAccessPoint` 是 Service 与 core 交互的**唯一通道**，支持 Clone 以便在
多个异步任务间共享。

```rust
#[derive(Clone)]
pub struct ServiceAccessPoint {
    inner: Arc<dyn ServiceAccessImpl>,
}

pub trait ServiceAccessImpl: Send + Sync {
    // ── Core 内建 ──
    fn get_config(&self) -> AgentConfig;
    fn log(&self, level: &str, message: &str);

    // ── Provider 注册 ──
    /// 将本服务的 Provider 注册到运行时，供其他插件通过 `SlotAccessPoint::provider_raw()` 查找
    fn register_provider(&self, name: &str, provider: Arc<dyn Any + Send + Sync>);
}
```

### 2.1 Core 内建方法

| 方法 | 说明 |
|------|------|
| `get_config()` | 读取 Agent 配置（结构化） |
| `log()` | 向框架日志系统写入日志 |

### 2.2 Provider 注册

`register_provider` 是本协议的核心设计——Service 通过它将业务能力暴露给外界：

```rust
// MemoryService 启动时注册记忆 Provider
impl ServicePlugin for MemoryService {
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        let memory_provider: Arc<dyn MemoryProvider> = Arc::new(L2Memory { /* ... */ });
        ap.register_provider("memory", memory_provider);

        let vector_provider: Arc<dyn VectorProvider> = Arc::new(VectorMemory { /* ... */ });
        ap.register_provider("vector", vector_provider);

        // 启动后台循环
        self.run_loop(ap.clone()).await;
        Ok(())
    }
}

// ToolService 启动时注册工具 Provider
impl ServicePlugin for ToolService {
    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        let tool_provider: Arc<dyn ToolProvider> = Arc::new(ToolRegistry::new());
        ap.register_provider("tool", tool_provider);
        Ok(())
    }
}

// Slot 在使用时通过 SlotAccessPoint::provider_raw() 查找：
fn run(&mut self, ap: &mut dyn SlotAccessPoint) -> Result<SlotDirective, PluginError> {
    let raw = ap.provider_raw("tool")
        .ok_or(PluginError::NotFound("tool provider unavailable".into()))?;
    let tools = raw.downcast::<dyn ToolProvider>()
        .map_err(|_| PluginError::Internal("type mismatch".into()))?;
    let result = tools.call("read_file", args).await?;
    Ok(SlotDirective::Continue)
}
```

**Provider 接口由 Service 自行定义**——core 不知道也不关心 `MemoryProvider`、
`ToolProvider` 有什么方法。

---

## 3. 运行时信号

```rust
pub enum ServiceSignal {
    GracefulShutdown,   // 优雅关闭
    ImmediateShutdown,  // 强制关闭
    ConfigReload,       // 重载配置
    HealthCheck,        // 健康检查
    Suspend,            // 暂停运行
    Resume,             // 恢复运行
}
```

| 信号 | 说明 |
|------|------|
| `GracefulShutdown` | 正常关闭，完成后台任务再退出 |
| `ImmediateShutdown` | 强制关闭，立即停止 |
| `ConfigReload` | 重载配置（重新读取配置并应用） |
| `HealthCheck` | 健康检查，需在 5s 内返回 `Ok(())`（红线 V-R01） |
| `Suspend` | 暂停服务，释放临时资源 |
| `Resume` | 从暂停中恢复 |

---

## 4. 插件元数据

每个插件必须附带一份元数据声明，用于启动时校验。

| 字段 | 类型 | 必需 | 说明 |
|------|------|------|------|
| `name` | `String` | 是 | 全局唯一标识，必须与 `ServicePlugin::name()` 一致 |
| `category` | `"service"` | 是 | 固定值 |
| `version` | `String` | 是 | 语义版本 |
| `run_mode` | `enum` | 是 | `background` / `on_demand` / `cron` |
| `provides` | `Vec<String>` | 是 | 本服务注册的 Provider 名称列表 |
| `requires` | `Vec<String>` | 否 | 依赖的其他 Service/Provider 名 |
| `conflicts` | `Vec<String>` | 否 | 冲突的插件名 |
| `config_schema` | `Option<JsonSchema>` | 否 | JSON Schema 配置格式 |

### YAML 示例

```yaml
name: memory
category: service
version: 0.2.0
run_mode: background
provides:
  - memory
  - vector
requires:
  - config-loader
```

---

## 5. 生命周期

```
PluginLoader 读元数据 → 校验 provides/requires
→ init(ctx) → start(ap) ↔ [handle_signal() ...] → stop() → shutdown()
```

- `init`：只调用一次，失败则服务不被加载
- `start`：在此方法中调用 `register_provider()` 注册能力，然后启动后台循环
- `handle_signal`：运行时通过信号驱动服务行为变更
- `stop`：暂停服务，不销毁资源。Provider 仍然可用但不更新
- `shutdown`：只调用一次，应反注册 Provider 并释放所有资源

---

## 6. 补充说明

- `ServiceAccessPoint` 可 Clone，便于在多个异步任务中共享
- `handle_signal` 不得阻塞超过 5 秒（红线 V-R02），长时间操作应通过 `tokio::spawn` 处理
- 服务不应假设 `start`/`stop` 的调用次数或配对关系
- Provider 一经注册即对所有插件可见。如需要访问控制，Provider 接口自行实现鉴权

### 红线

| 编号 | 红线 |
|:----|:------|
| P-R01 | **禁止用 `Arc::new(())` 作为 Provider 注册**——空元组不是合法的业务能力。如果当前没有能力可注册，应省略 `register_provider()` 调用，或保留注册行并附加 `// TODO` 注释注明预期的 Provider trait 和实现计划 |
| P-R02 | **已注册的 Provider key 应能被至少一个消费者查找**。如果当前没有任何 Slot 通过 `provider_raw(key)` 查找该 key，应附加 `// TODO` 注释说明预期的消费者。仅注册而无消费者的 Provider 属于"幽灵 Provider"，消耗注意力但无业务价值 |

---

## 7. 新增/替换 Service 标准流程

### 新增（从零到运行）

| 步骤 | 做什么 | 涉及文件 |
|------|--------|---------|
| 1 | 创建插件目录 | `plugins/services/my-service/` |
| 2 | 实现 `ServicePlugin` | `plugin.rs` |
| 3 | 定义配置结构体 + 默认值 | `config.rs` |
| 4 | 定义 Provider trait + 实现 | `provider.rs` |
| 5 | 编写 `mod.rs` 重新导出 | `mod.rs` |
| 6 | 在 `plugins/services/mod.rs` 注册 | 加一行 `pub mod my-service;` |
| 7 | 编写 `PluginMetadata` YAML | 声明 provides / requires |
| 8 | 运行 `cargo check` 验证 | — |

**共需改 2 个文件**：新建 `plugin.rs` + 修改 `plugins/services/mod.rs`



---

## 8. 协议特有红线

| 编号 | 红线 | 说明 |
|------|------|------|
| V-R01 | **必须响应 `HealthCheck` 信号** | `handle_signal(HealthCheck)` 须在 5 秒内返回 `Ok(())` |
| V-R02 | **`handle_signal` 不得阻塞超过 5 秒** | 长时间操作应 `tokio::spawn` 异步处理 |
| V-R03 | **`provides` 必须与代码中 `register_provider` 调用一致** | YAML 声明的 Provider 名必须全部在 `start()` 中实际注册，防止接口声明与实际不符 |

---


