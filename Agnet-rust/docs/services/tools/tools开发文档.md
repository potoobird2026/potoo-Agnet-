# Tools（工具注册与执行服务）开发文档

## 0. 协议依据

本文档严格遵循以下三份协议标准，逐条对标：

| 协议 | 应用层级 | 关键条款 |
|------|---------|---------|
| **protocol-Service集成协议** | 模块对框架的接入方式 | §1 ServicePlugin 单入口、§2 ServiceAccessPoint 受控访问句柄、§3 运行时信号、§4 插件元数据、§5 生命周期、§8 协议特有红线 |
| **protocol-模块内部组件协议** | 模块内部子模块组织方式 | §1 Component 单入口、§3 AccessPoint 内部数据共享通道、§6 模块边界规范 |
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义（特别 §1.9 平台指令）、§2 跨平台路径规则、§3 测试代码规范、§4 自查清单 |

---

## 1. 模块定位

### 1.1 一句话描述

**管理所有工具（内置 + MCP + 已安装包）的注册、发现、权限校验、熔断保护和跨平台执行，通过 `ToolRegistry` 为 `ToolExecutorSlot` 提供统一的工具调用入口。**

### 1.2 架构定位

Tools 是框架中**最复杂的 ServicePlugin** 候选模块，包含六个子功能域。其中 `NativePlatform` 被**跨平台与硬编码规范 §1.9 明确引用为正确示例**（`tools/platform.rs:52`）——所有模块如需执行 shell 命令，均应参照此实现：

```
┌──────────────────────────────────────────────────────────────┐
│  ToolsService (impl ServicePlugin) ← 待补齐                     │
│  - init(): 加载内置工具 + ToolDiscover 扫描已安装工具包           │
│  - start(): register_provider("tool", ToolRegistry)           │
│  - handle_signal(): ConfigReload 重扫工具目录                   │
│  - shutdown(): 反注册                                          │
└──────────────────────────────────────────────────────────────┘
          │ 持有
          ▼
┌──────────────────────────────────────────────────────────────┐
│  ToolRegistry ─── 工具执行引擎                                  │
│  - execute(name, args, ctx, cancel) → ToolOutput              │
│  - 内置熔断器 (CircuitBreaker)                                  │
│  - 组件开关 (ComponentSwitch) 检查                              │
│  - 回退链 (fallback) 自动切换                                   │
│  - 防循环调用保护 (visited set + max_attempts=10)               │
└──────────────────────────────────────────────────────────────┘
          │                │                │
          ▼                ▼                ▼
┌──────────────┐  ┌──────────────┐  ┌──────────────────┐
│ CircuitBreaker│  │NativePlatform│  │ ToolInstallManager│
│ - 三态熔断    │  │ - OsKind检测  │  │ - .atp 安装/卸载  │
│ - 半开试探    │  │ - shell适配   │  │ - tools.toml DB  │
│ - 冷却恢复    │  │ - 命令超时    │  │ - 版本管理       │
└──────────────┘  └──────────────┘  └──────────────────┘
          │                │
          ▼                ▼
┌──────────────┐  ┌──────────────────┐
│ ToolDiscover │  │ ToolManifest      │
│ - 扫描目录    │  │ - TOML 解析/校验  │
│ - 发现组件    │  │ - 平台兼容检查    │
└──────────────┘  └──────────────────┘
```

---

## 2. 文件结构

```
src/plugins/services/tools/
├── mod.rs             # 模块入口：子模块声明 + 公开类型 re-export
├── registry.rs        # ToolRegistry — 工具执行引擎（熔断 + 回退 + 开关）
├── circuit_breaker.rs # CircuitBreaker — 三态熔断器（Closed/Open/HalfOpen）
├── platform.rs        # NativePlatform — 跨平台命令执行（impl PlatformContract）
├── manifest.rs        # ToolManifest — .atp 包清单格式（TOML）
├── discover.rs        # ToolDiscover — 扫描 ~/.aagnet/ 发现已安装组件
├── install.rs         # ToolInstallManager — 安装/卸载 + ToolsDatabase
├── package.rs         # ToolPackage — .atp ZIP 归档读写
└── builtins/          # 内置工具
    ├── mod.rs
    ├── read_file.rs       # 文件读取工具
    ├── write_file.rs      # 文件写入工具
    ├── execute_command.rs # 命令执行工具
    └── search_memory.rs   # 记忆搜索工具
```

> **模块边界规范（§6.1）**：`mod.rs` 仅暴露 `CircuitBreaker`、`CircuitBreakerState`、`ToolManifest`、`NativePlatform`、`ToolRegistry` 五个公共类型。`ToolDiscover`、`ToolInstallManager`、`ToolPackage` 等为 `pub(crate)`。

---

## 3. 功能清单

| 功能 | 描述 | 实现状态 | 对应源码 |
|------|------|:---:|---------|
| 工具执行引擎 | 统一 `execute()` 入口，含熔断+回退+开关+防循环 | ✅ | `registry.rs` |
| 三态熔断器 | Closed → Open（连续失败）→ HalfOpen（冷却后试探）→ Closed/Open | ✅ | `circuit_breaker.rs` |
| 跨平台命令执行 | `OsKind` 枚举分支选择 `cmd /C` 或 `sh -c`，含超时 | ✅ | `platform.rs` |
| 工具清单格式 | `.atp` 包的 `aagnet-tool.toml` 定义与校验 | ✅ | `manifest.rs` |
| 工具发现 | 扫描 `~/.aagnet/tools/` / `skills/` / `mcp/` 目录 | ✅ | `discover.rs` |
| 工具安装/卸载 | `.atp` ZIP 包安装到版本目录 + `tools.toml` 数据库 | ✅ | `install.rs` + `package.rs` |
| 内置工具 | `read_file` / `write_file` / `execute_command` / `search_memory` | ✅ | `builtins/` |
| 回退链 | 工具失败后自动切换到 `fallback_tool()` | ✅ | `registry.rs` |
| 组件开关 | `ComponentSwitch` 按名禁用/启用工具 | ✅ | `registry.rs` |
| ServicePlugin | 完整生命周期 | ❌ 待补齐 | — |
| Provider 注册 | 通过 `ServiceAccessPoint::register_provider()` | ❌ 待补齐 | — |

---

## 4. 核心设计

### 4.1 ToolRegistry（工具执行引擎）

**文件**：`registry.rs`

#### 4.1.1 结构

```rust
pub struct ToolRegistry {
    contracts: Arc<ContractRegistry>,           // 工具注册表（框架层）
    breakers: Vec<Mutex<CircuitBreaker>>,      // 每个工具一个熔断器
}
```

- `contracts` 提供 `all_tools()` / `get_tool(name)` / `get_component_switch()` 等查询
- `breakers` 与 `contracts.all_tools()` 一一对应，按索引关联

#### 4.1.2 execute() 执行流程

```
execute(tool_name, args, ctx, cancel)
  │
  ├─ 初始化：visited = HashSet, current_name = tool_name, attempts = 0
  │
  └─ loop (max 10 attempts):
       │
       ├─ 1. attempts >= 10 → CircuitBreakerOpen
       ├─ 2. visited.contains(current_name) → 循环检测 → CircuitBreakerOpen
       ├─ 3. visited.insert(current_name)
       │
       ├─ 4. contracts.get_tool(current_name) → 不存在 → DependencyMissing
       │
       ├─ 5. tool.validate(args, ctx) → 失败 → 返回错误
       │
       ├─ 6. ComponentSwitch 检查 → 被禁用 → ComponentDisabled
       │
       ├─ 7. breaker.is_open()?
       │      ├─ Yes → 有 fallback? → current_name = fallback → continue
       │      └─ Yes → 无 fallback → CircuitBreakerOpen
       │
       ├─ 8. tool.execute(args, ctx, cancel).await
       │
       ├─ 9. 更新熔断器：Ok → record_success(), Err → record_failure()
       │
       ├─ 10. tool.after_execution(&result, ctx).await
       │
       └─ 11. 结果处理：
              ├─ Ok → return output
              └─ Err → 有 fallback? → current_name = fallback → continue
                       └─ 无 fallback → return err
```

#### 4.1.3 防循环调用

- `visited: HashSet<String>` 记录已尝试的工具名
- 如果 `fallback_tool()` 指向已尝试过的工具 → `CircuitBreakerOpen`
- `max_attempts = 10` 硬上限

#### 4.1.4 关键设计决策

| 设计点 | 选择 | 理由 |
|--------|------|------|
| 熔断器索引 | `Vec<Mutex<CircuitBreaker>>` 按 tools 列表顺序 | O(n) 查找但 tools 数量通常 < 100，Mutex 粒度适中 |
| 回退链 | `fallback_tool()` 动态切换 | 允许工具声明降级路径，如 `expensive_api → cached_version` |
| 组件开关 | 每次 execute 检查 `ComponentSwitch` | 支持运行时禁用工具，无需重启 |

### 4.2 CircuitBreaker（熔断器）

**文件**：`circuit_breaker.rs`

#### 4.2.1 三态模型

```
              record_failure() 达到 max_failures
  ┌─────────┐ ─────────────────────────────────→ ┌──────┐
  │ CLOSED  │                                     │ OPEN │
  │ (正常)   │ ←───────────────────────────────── │(熔断) │
  └─────────┘   record_success() 达到             └──┬───┘
       ↑          half_open_max_requests              │
       │                                              │ cooldown_secs 过后
       │          ┌───────────┐                       │ try_half_open()
       └──────────│ HALF_OPEN │ ←─────────────────────┘
                  │  (试探)    │
                  └─────┬─────┘
                        │ record_failure()
                        └────────→ 回到 OPEN
```

#### 4.2.2 状态转换规则

| 当前状态 | 事件 | 新状态 | 条件 |
|---------|------|--------|------|
| Closed | `record_failure()` | Closed | `consecutive_failures < max_failures` |
| Closed | `record_failure()` | **Open** | `consecutive_failures >= max_failures` |
| Open | `is_open()` 检查 | Open | `cooldown_secs == 0` 或冷却未到期 |
| Open | `try_half_open()` | HalfOpen | 冷却到期后手动触发 |
| HalfOpen | `record_success()` | HalfOpen | `half_open_requests < half_open_max_requests` |
| HalfOpen | `record_success()` | **Closed** | `half_open_requests >= half_open_max_requests` |
| HalfOpen | `record_failure()` | **Open** | 试探失败，重新熔断 |

#### 4.2.3 配置（CircuitBreakerConfig）

```rust
pub struct CircuitBreakerConfig {
    pub max_failures: u32,            // 连续失败阈值
    pub window_secs: u64,             // 滑动窗口（秒），预留字段
    pub cooldown_secs: u64,           // 熔断冷却时间（秒）
    pub half_open_max_requests: u32,  // 半开状态最大试探请求数
}
```

> **红线对标**：`record_success()` / `record_failure()` 均为纯同步操作、无阻塞，符合"不可阻塞"要求。

### 4.3 NativePlatform（跨平台抽象）

**文件**：`platform.rs`

**实现 trait**：`PlatformContract` + `Describe`

#### 4.3.1 结构

```rust
pub struct NativePlatform {
    pub os: OsKind,                    // Windows / Linux / MacOS
    pub arch: ArchKind,                // x86_64 / aarch64
    pub command_timeout_secs: u64,     // 命令超时（默认 120）
}
```

#### 4.3.2 平台适配（跨平台与硬编码规范 §1.9 对标）

```rust
fn default_shell(&self) -> (&str, &str) {
    match self.os {
        OsKind::Windows => ("cmd", "/C"),    // ✅ Windows 用 cmd
        _                => ("sh", "-c"),    // ✅ Unix 用 sh
    }
}
```

> **这是跨平台规范 §1.9 的标准实现**：`tools/platform.rs:52` 被规范文档引用为正确示例。平台指令通过 `OsKind` 枚举分支选择，不假设 `sh` 或 `cmd`。

#### 4.3.3 跨平台路径（§2 对标）

| 方法 | 实现 | 合规 |
|------|------|:---:|
| `home_dir()` | `dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))` | ✅ |
| `temp_dir()` | `std::env::temp_dir()` | ✅ |
| `execute_command()` | `Command::new(shell).args([flag, command]).output()` + timeout | ✅ |

### 4.4 ToolManifest（工具清单格式）

**文件**：`manifest.rs`

#### 4.4.1 格式定义

`.atp` 包内必须包含 `aagnet-tool.toml` 文件，格式如下：

```toml
[tool]
name = "aagnet.file.read"
version = "1.0.0"
group = "builtin"
description = "Read file contents with UTF-8 encoding"
kind = "tool"          # "tool" | "skill" | "mcp" | "bundle"
platforms = ["linux-x86_64", "windows-x86_64"]  # 空=全平台

[permissions]
file_read = ["$WORKSPACE/**"]
file_write = []
network = false
shell = false
memory_read = false
memory_write = false

[dependencies]
tools = ["aagnet.file.write"]

# 以下可选，按 kind 决定
[skill]
path = "SKILL.md"

[mcp]
endpoint_type = "stdio"
command = "python"
args = ["-m", "my_mcp_server"]
```

#### 4.4.2 校验规则（`validate()`）

| 校验项 | 规则 |
|--------|------|
| `tool.name` | 非空 |
| `tool.version` | 非空 + 合法的语义版本 |
| `tool.description` | 非空 |
| `tool.kind` | 必须为 `tool` / `skill` / `mcp` / `bundle` 之一 |
| `mcp.endpoint_type` | `kind == "mcp"` 或 `"bundle"` 时非空 |

#### 4.4.3 平台兼容检查

```rust
pub fn is_platform_supported(&self) -> bool {
    if self.tool.platforms.is_empty() { return true; }  // 空=全平台
    let current = platform_tag();  // 如 "windows-x86_64"
    self.tool.platforms.contains(&current)
}
```

### 4.5 ToolDiscover（工具发现）

**文件**：`discover.rs`

#### 4.5.1 目录结构

```
~/.aagnet/
├── tools/
│   └── {tool_name}/
│       └── {version}/
│           └── aagnet-tool.toml
├── skills/
│   └── {skill_name}/
│       └── aagnet-tool.toml
├── mcp/
│   └── {mcp_name}/
│       └── {version}/
│           └── aagnet-tool.toml
└── tools.toml          # 安装数据库（ToolInstallManager 管理）
```

#### 4.5.2 发现流程

```
scan_dir(dir, kind)
  │
  ├─ read_dir(dir) → 遍历 name_dir（工具名目录）
  │
  ├─ read_dir(name_dir) → 遍历 version_dir（版本目录）
  │
  ├─ version_dir.join("aagnet-tool.toml").exists()?
  │    └─ Yes → DiscoveredComponent { name, version, kind, install_dir, manifest_path }
  │
  └─ 返回所有已发现的组件
```

#### 4.5.3 跨平台路径规范（§2）对标

| 规则 | 合规 | 说明 |
|------|:---:|------|
| §2.2 禁止裸用 `~` | ✅ | 使用 `dirs::home_dir().join(".aagnet")` |
| §2.4 路径拼接用 `PathBuf::join()` | ✅ | 全部使用 `.join()` |
| §3.1 测试用 `temp_dir()` | ✅ | `test_discover_empty_dir` / `test_ensure_dirs_exist` 使用 `std::env::temp_dir()` |

### 4.6 ToolInstallManager + ToolPackage（安装管理）

**文件**：`install.rs` + `package.rs`

#### 4.6.1 ToolsDatabase（持久化）

```rust
pub struct ToolsDatabase {
    pub tools: HashMap<String, InstalledEntry>,   // name → { version, installed_at, enabled, install_path }
    pub skills: HashMap<String, InstalledEntry>,
    pub mcp: HashMap<String, InstalledEntry>,
}
```

存储为 `~/.aagnet/tools.toml`。

#### 4.6.2 安装流程

```
install_from_atp(atp_path)
  │
  ├─ 1. ToolPackage::from_atp_file(atp_path) → 解析 ZIP, 校验 manifest
  ├─ 2. package.is_compatible() → 检查平台支持
  ├─ 3. 检查同名同版本冲突
  ├─ 4. 确定安装目录：{tools|skills|mcp}_dir/{name}/{version}
  ├─ 5. package.extract_to(install_dir) → 解压 ZIP
  ├─ 6. 写入 ToolsDatabase
  ├─ 7. save_database() → 持久化 tools.toml
  └─ 8. 返回 manifest
```

#### 4.6.3 .atp ZIP 格式

- 扩展名：`.atp`（aagnet tool package）
- 格式：标准 ZIP 归档
- 必须包含：`aagnet-tool.toml`（清单文件）
- 可选包含：`.so` / `.dll` / `.dylib` / `.wasm`（动态库）、`.skill.md`（技能文件）

> **跨平台规范（§2.6）对标**：`ToolPackage` 的库文件检测列举了所有平台扩展名（`.so` / `.dll` / `.dylib`），不假设单一平台。

### 4.7 内置工具

**目录**：`builtins/`

| 工具 | 文件 | 功能 |
|------|------|------|
| `read_file` | `read_file.rs` | 读取文件内容（UTF-8 编码） |
| `write_file` | `write_file.rs` | 写入文件内容 |
| `execute_command` | `execute_command.rs` | 跨平台执行 shell 命令 |
| `search_memory` | `search_memory.rs` | 搜索记忆库 |

---

## 5. 协议合规性分析

### 5.1 Service 集成协议（protocol-Service集成协议）对标

#### 5.1.1 ServicePlugin 方法职责（协议 §1）

| 方法 | 调用次数 | 用途 | 当前状态 |
|------|---------|------|:---:|
| `name()` | 多次 | 返回全局唯一服务标识 `"tools"` | ❌ 无 ToolsService |
| `init(ctx)` | 1 | 加载内置工具 + ToolDiscover 扫描已安装工具包 | ❌ |
| `start(ap)` | 1 | `ap.register_provider("tool", ToolRegistry)` | ❌ |
| `handle_signal(signal)` | 多次 | 响应运行时信号（见 5.1.2） | ❌ |
| `stop()` | 多次 | 暂停工具注册，已注册工具仍可用 | ❌ |
| `shutdown()` | 1 | 反注册 Provider + 清理 CircuitBreaker 状态 | ❌ |

#### 5.1.2 运行时信号处理（协议 §3）

| 信号 | 说明 | 当前处理 | 合规 |
|------|------|:---:|:---:|
| `GracefulShutdown` | 正常关闭，等待执行中的工具完成 | ❌ 无 | — |
| `ImmediateShutdown` | 强制关闭，立即终止 | ❌ 无 | — |
| `ConfigReload` | 重载配置，ToolDiscover 重扫工具目录 | ❌ 无 | — |
| `HealthCheck` | 健康检查，需在 5s 内返回 `Ok(())`（红线 V-R01） | ❌ 无 | V-R01 ❌ |
| `Suspend` | 暂停新工具注册，已注册工具仍可执行 | ❌ 无 | — |
| `Resume` | 恢复注册和执行 | ❌ 无 | — |

#### 5.1.3 生命周期（协议 §5）

```
PluginLoader 读元数据 → 校验 provides/requires
→ init(ctx) → start(ap) ↔ [handle_signal() ...] → stop() → shutdown()
```

当前状态：**全部未实现**。`ToolRegistry` / `CircuitBreaker` / `NativePlatform` 等作为独立组件存在。

#### 5.1.3.1 计划声明（ServicePlugin 各方法职责与实现要点）

```rust
#[async_trait]
impl ServicePlugin for ToolsService {
    fn name(&self) -> &str { "tools" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 1. 加载内置工具（read_file / write_file / execute_command / search_memory）
        // 2. ToolDiscover::with_default_dir() → 扫描 ~/.aagnet/ 已安装工具包
        // 3. ToolInstallManager::init() → 确保目录结构 + tools.toml 数据库
        // 4. 将内置工具注册到 ContractRegistry
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        // 注册 Provider（协议 §2.2）：
        ap.register_provider("tool", Arc::new(self.registry.clone()));
        // 注意：ToolRegistry 在构造时已为每个工具创建 CircuitBreaker，
        //   不需要额外的后台任务
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::HealthCheck => {
                // 红线 V-R01：5s 内检查 ContractRegistry 中工具数 > 0
                Ok(())
            }
            ServiceSignal::ConfigReload => {
                // ToolDiscover 重扫目录 → 增量更新注册表
                // 新增工具 → register + 创建 CircuitBreaker
                // 已删除工具 → unregister + 清理 CircuitBreaker
                Ok(())
            }
            ServiceSignal::GracefulShutdown => {
                // 等待执行中的工具完成（max_attempts 已提供硬上限保护）
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        // 暂停新工具注册，已注册工具仍可执行
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        // 反注册 Provider + 清空所有 CircuitBreaker 状态
        Ok(())
    }
}
```

> Tools 的 ServicePlugin 与 Memory/Skills 的关键差异：`start()` 中只需注册一个 `tool` Provider，后台不需要定时任务——熔断器是惰性的（在 `execute()` 中按需检查），工具发现是事件驱动的（`ConfigReload` 时触发）。

#### 5.1.4 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 ServicePlugin | 需实现 `ServicePlugin` trait | ❌ | 无 ToolsService（详见 5.1.1） |
| §2.1 ServiceAccessPoint | 通过 `get_config()` / `log()` 与 core 交互 | ❌ | 无 ServiceAccessPoint 注入 |
| §2.2 register_provider() | `start()` 注册 `tool` Provider | ❌ | 无 Service 外壳 |
| §3 运行时信号 | 响应全部 6 个信号 | ❌ | 无 handle_signal()（详见 5.1.2） |
| §4 插件元数据 | YAML 声明 provides/requires/run_mode | ❌ | 元数据已设计（见 §8），未接入 PluginLoader |
| §5 生命周期 | init → start → stop → shutdown | ❌ | 无完整生命周期（详见 5.1.3） |
| §6 补充说明 | ServiceAccessPoint Clone、handle_signal<5s | ❌ | 待实现 |
| §7 标准流程 | 8 步骤从零到运行 | ⚠️ | 步骤 1-4 已完成（registry/circuit_breaker/platform/manifest），步骤 5-8 待完成 |
| §8 V-R01 HealthCheck | 5s 内返回 `Ok(())` | ❌ | 无实现 |
| §8 V-R02 handle_signal 不阻塞 | 超 5s 须 spawn | ❌ | 无实现 |
| §8 V-R03 provides 一致 | 声明 = 实际注册 | ❌ | 无注册 |

### 5.2 模块内部组件协议（protocol-模块内部组件协议）对标

#### 5.2.1 依赖方向（协议 §6.2）

```
┌──────────────────────┐
│  模块 mod.rs          │  （对外暴露的公共 API）
│  CircuitBreaker       │
│  CircuitBreakerState  │
│  ToolManifest         │
│  NativePlatform       │
│  ToolRegistry         │
└──────────┬───────────┘
           │
           ▼
┌──────────────────────────────────────────────┐
│  组件（无 Orchestrator — 组件通过 ContractRegistry 间接通信）│
│                                              │
│  ToolRegistry ──→ ContractRegistry            │
│       │                                      │
│       │ 持有                                  │
│       ▼                                      │
│  Vec<CircuitBreaker>  (每个工具一个熔断器)      │
│                                              │
│  ToolDiscover ──→ ToolManifest (解析)         │
│  ToolInstallManager ──→ ToolDiscover + ToolPackage │
│  NativePlatform (独立，被内置工具引用)          │
│                                              │
│  ✅ Registry 不直接引用 Discover/InstallManager│
│  ✅ Platform 通过 PlatformContract trait 解耦  │
└──────────────────────────────────────────────┘
```

#### 5.2.2 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 Component | 实现 `Component` trait | ❌ | ToolRegistry 等均未实现 Component |
| §3 AccessPoint | 组件间通过 AP 通信 | N/A | 组件间通过 ContractRegistry 间接通信 |
| §5 Orchestrator | 编排器调度 | N/A | 无多组件编排 |
| §6 模块边界 | mod.rs 只暴露入口+配置 | ✅ | 仅 5 个公开类型 |

### 5.3 跨平台与硬编码规范对标（协议 §4 完整 10 项自查清单）

| # | 检查项 | 合规 | 说明 |
|---|--------|:---:|------|
| 1 | 所有 URL 端点来自配置或常量，非字面量写死 | ✅ | 不涉及 HTTP 端点 |
| 2 | 所有模型名称来自配置字段，非硬编码 | ✅ | 不涉及 LLM 模型 |
| 3 | 所有超时值来自配置或 `DEFAULT_*` 常量 | ✅ | `command_timeout_secs` / `cooldown_secs` 可配置 |
| 4 | API 版本号定义为模块级 `const`，不散落 | ✅ | 不涉及 API 版本号 |
| 5 | User-Agent 定义为 `const USER_AGENT` | ✅ | 不涉及 HTTP 请求 |
| 6 | 文件路径通过 `dirs` + `PathBuf::join()` 构建，无 `/tmp/`、`~`、相对路径 | ✅ | `ToolDiscover::with_default_dir()` 使用 `dirs::home_dir()`，测试用 `temp_dir()` |
| 7 | 数字阈值（max_tokens 等）默认 `None` 或从配置读取 | ✅ | `max_failures` / `cooldown_secs` / `half_open_max_requests` 来自 `CircuitBreakerConfig` |
| 8 | 平台特定指令通过 `OsKind` 枚举分支，不假设 `sh` 或 `cmd` | ✅ | `NativePlatform::default_shell()` 是规范引用的正确示例 |
| 9 | 测试中无 Unix-only 路径，均用 `std::env::temp_dir()` | ✅ | `discover.rs` / `install.rs` 测试均用 `temp_dir()` |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 全部通过 | 待验证 | — |

---

## 6. 红线与质量

| 编号 | 来源 | 红线 | 合规 |
|------|------|------|:---:|
| V-R01 | Service集成协议 | 必须响应 HealthCheck | ❌ 待补齐 |
| V-R02 | Service集成协议 | handle_signal 不阻塞超 5s | ❌ 待补齐 |
| V-R03 | Service集成协议 | provides = register_provider 一致 | ❌ 待补齐 |
| — | aagnet-lessons | 异步操作必须有超时 | ✅ `execute_command` 有 timeout 保护；`execute()` 有 `max_attempts=10` 硬上限 |
| — | aagnet-lessons | 外部输入必须校验 | ✅ `ToolManifest::validate()` 校验所有必填字段；路径穿越检查 |
| — | aagnet-lessons | 不可在库代码中 unwrap/expect | ⚠️ `Mutex::lock().expect("mutex poisoned")` 毒锁 panic 可接受 |
| — | 跨平台规范 §1.9 | 平台指令不可假设 sh/cmd | ✅ `NativePlatform::default_shell()` 为标准实现 |

---

## 7. 数据流全景

```
用户请求 "read_file /tmp/test.txt"
  │
  ▼
ToolExecutorSlot
  │ provider_raw("tool") → downcast → ToolRegistry
  ▼
ToolRegistry::execute("read_file", {path: "/tmp/test.txt"}, ctx, cancel)
  │
  ├─ contracts.get_tool("read_file") → ToolContract
  ├─ validate(args) → 路径穿越检查
  ├─ ComponentSwitch 检查 → enabled?
  ├─ CircuitBreaker.is_open()? → No
  ├─ tool.execute(args, ctx, cancel).await
  │    └─ NativePlatform 不做介入（纯文件操作）
  ├─ breaker.record_success()
  └─ return ToolOutput { "file content..." }
```

---

## 8. 插件元数据

```yaml
name: tools
category: service
version: 0.3.0
run_mode: background
provides:
  - tool
requires:
  - storage
conflicts: []
config_schema:
  type: object
  properties:
    tools_dir:
      type: string
      description: 工具安装根目录（默认 ~/.aagnet/，通过 AAGNET_HOME 覆盖）
    command_timeout_secs:
      type: integer
      default: 120
    circuit_breaker:
      type: object
      properties:
        max_failures:
          type: integer
          default: 5
        cooldown_secs:
          type: integer
          default: 30
        half_open_max_requests:
          type: integer
          default: 3
```

---

## 9. 设计决策

### 9.1 为什么熔断器与 Registry 绑定而不是独立服务

**决策**：`CircuitBreaker` 作为 `ToolRegistry` 内部组件，与工具一一对应。

**理由**：
1. **粒度匹配**：每个工具独立熔断，一个工具失败不影响其他工具
2. **无额外通信**：Registry 直接持有 breaker，零开销状态查询
3. **简单可靠**：不引入独立的熔断服务 + RPC 调用链

### 9.2 为什么回退链在 Registry 层实现

**决策**：`ToolRegistry::execute()` 内部处理 `fallback_tool()` 切换，而非在 `ToolContract` 层。

**理由**：
1. **透明性**：工具开发者只需声明 `fallback_tool()`，Registry 自动处理切换
2. **防循环**：`visited` set 在 Registry 层统一管理，各工具无需各自实现
3. **统一熔断**：回退链上的每个工具都有独立的熔断器

---

## 10. 依赖关系

```
ToolRegistry       ──→  ContractRegistry (core::contract)
ToolRegistry       ──→  CircuitBreaker (内部)
ToolRegistry       ──→  ToolContract (core::contract::tool)
NativePlatform     ──→  PlatformContract (core::contract::tool_platform)
ToolDiscover       ──→  ToolManifest (内部)
ToolInstallManager ──→  ToolDiscover + ToolManifest + ToolPackage (内部)
ToolPackage        ──→  ToolManifest + zip crate
```

- 对外依赖：`tokio::process::Command`（子进程）、`zip`（.atp 包）、`chrono`（时间戳）、`toml`（清单解析）、`dirs`（跨平台路径）
- 框架层依赖：`core::contract::tool`（工具契约）、`core::contract::tool_platform`（平台契约）、`core::contract::ContractRegistry`（注册表）

    /// 获取所有工具定义（给 LLM 的 ToolDefinition 列表）
    pub fn list_definitions(&self) -> Vec<ToolDefinition>;

    /// 执行工具调用（含熔断保护 + 超时控制）
    pub async fn call(
        &self,
        name: &str,
        args: serde_json::Value,
        ctx: &ToolContext,
    ) -> Result<ToolOutput, ToolError>;
}
```

**执行流程**：

```
ToolRegistry::call(name, args)
  │
  ├── 1. 按名称查找 ToolContract
  ├── 2. 检查熔断器状态 → Open? 返回 CircuitBreakerOpen 错误
  ├── 3. 校验参数 → args 是否符合 tool.parameters() JSON Schema
  ├── 4. 校验权限 → tool.required_permissions() ⊆ ctx.granted_permissions
  ├── 5. 执行 tool.execute(args, ctx, cancel_token)
  │     ├── 成功 → 熔断器 record_success()
  │     └── 失败 → 熔断器 record_failure()
  └── 6. 返回 ToolOutput
```

### 3.3 CircuitBreaker（Component）

```rust
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitBreakerState,       // Closed / Open / HalfOpen
    consecutive_failures: u32,
    last_failure_time: Option<Instant>,
    opened_at: Option<Instant>,
}

impl CircuitBreaker {
    pub fn record_success(&mut self);
    pub fn record_failure(&mut self) -> bool; // true = 熔断触发
    pub fn allow_request(&mut self) -> bool;  // 请求是否被允许
}
```

**状态转换**：

```
Closed ──连续失败≥阈值──→ Open ──冷却时间到──→ HalfOpen
  ↑                                              │
  └──────── 试探成功 ──────────┘     试探失败 ──→ Open
```

### 3.4 ToolContext

```rust
pub struct ToolContext {
    pub session_id: String,
    pub working_dir: PathBuf,
    pub granted_permissions: Vec<Permission>,
    pub env: HashMap<String, String>,
}
```

### 3.5 ToolOutput / ToolError

```rust
pub struct ToolOutput {
    pub content: String,
    pub exit_code: Option<i32>,
    pub metadata: Option<serde_json::Value>,
}

pub enum ToolError {
    NotFound { tool_name: String },
    Validation { message: String },
    PermissionDenied { required: Permission },
    Timeout { tool_name: String, timeout: Duration },
    Execution { message: String },
    CircuitBreakerOpen { tool_name: String },
    Internal { message: String },
}
```

### 3.6 内置工具

| 工具 | 功能 | 权限 |
|------|------|------|
| `read_file` | 读取文件内容（UTF-8） | `file:read`，禁止 `..` 路径穿越 |
| `write_file` | 写入文件内容 | `file:write` |
| `execute_command` | 执行 shell 命令 | `shell`，通过 NativePlatform 适配 |
| `search_memory` | 搜索本地记忆库 | `memory:read` |

---

## 4. 跨平台与硬编码规范

### 4.1 硬编码值分类（§1）

| # | 类别 | 涉及？ | 合规 |
|---|------|:-----:|:----:|
| 1 | URL/端点 | 不涉及 | ✅ |
| 2 | 模型名 | 不涉及 | ✅ |
| 3 | 超时秒数 | 涉及 | ✅ 默认 120s 定义在 `DEFAULT_COMMAND_TIMEOUT` 常量，可从 ToolContext 覆盖 |
| 4 | API 版本号 | 不涉及 | ✅ |
| 5 | User-Agent | 不涉及 | ✅ |
| 6 | 文件路径 | 涉及 | ✅ 内置工具使用 `ToolContext.working_dir` 解析相对路径 |
| 7 | 数字阈值 | 涉及 | ✅ 熔断阈值从 `CircuitBreakerConfig` 读取 |
| 8 | 字符串模板 | 不涉及 | ✅ |
| 9 | 平台指令 | 涉及 | ✅ `execute_command` 通过 `NativePlatform` 的 `OsKind` 适配 `sh`/`cmd` |

### 4.2 跨平台路径规则（§2）

| # | 规则 | 合规 |
|---|------|:----:|
| 2.1 | 禁止裸用 Unix-only 路径 | ✅ 路径来自 `ToolContext.working_dir` |
| 2.2 | 禁止裸用 `~` | ✅ |
| 2.3 | 禁止相对路径依赖 CWD | ✅ 相对路径基于 `working_dir` 解析 |
| 2.4 | 路径拼接用 `PathBuf::join()` | ✅ |
| 2.5 | 路径分隔符判断 | ✅ 使用 `MAIN_SEPARATOR` |
| 2.6 | 文件扩展名判断 | ✅ 不涉及 |
| 2.7 | 临时文件/目录 | ✅ 使用 `std::env::temp_dir()` |
| 2.8 | 数据目录 | ✅ |

### 4.3 自查清单（§4）

| # | 检查项 | 通过 |
|---|--------|:----:|
| 1 | URL 来自配置或常量 | ✅ |
| 2 | 模型名来自配置 | ✅ |
| 3 | 超时值来自配置或常量 | ✅ `DEFAULT_COMMAND_TIMEOUT` |
| 4 | API 版本号为模块级 const | ✅ |
| 5 | User-Agent 为 const | ✅ |
| 6 | 路径用 `dirs` + `join()` | ✅ |
| 7 | 数字阈值从配置读取 | ✅ |
| 8 | 平台指令用 `OsKind` | ✅ |
| 9 | 测试无硬编码路径 | ✅ |
| 10 | build + test + clippy 通过 | 待验证 |

---

## 5. 红线

| 编号 | 红线 | 合规 |
|------|------|:----:|
| §1 | 不硬编码 URL/模型名/超时/版本号 | ✅ |
| §2 | 路径不使用 `~`、相对路径、Unix-only | ✅ |
| §3 | 测试无硬编码路径 | ✅ |
| — | 所有外部输入必须校验 | ✅ 路径穿越检测 + 参数 JSON Schema 校验 |
| — | 异步操作必须有超时 | ✅ `tokio::time::timeout` |
| — | 不可在库代码中使用 unwrap/expect | ✅ |

---

## 6. 设计决策

### 6.1 为什么工具作为 Service 而不是 Slot

**决策**：工具注册和执行由 `ToolsService`（ServicePlugin）管理。

**理由**：
1. **生命周期独立**：工具在 Agent 启动时加载一次，整个运行期间可用，不需要每次 Pipeline Step 都重新加载
2. **性能**：工具列表和 CircuitBreaker 状态跨 Step 保持，不需要每次重建
3. **符合 Provider 模式**：Service 是能力生产者，Slot 是消费者

### 6.2 为什么每个工具有独立熔断器

**决策**：每个工具独立的 CircuitBreaker。

**理由**：
1. **隔离故障**：`read_file` 连续失败不应影响 `execute_command` 的可用性
2. **粒度控制**：不同工具有不同的故障容忍度（文件读取 3 次熔断 vs 网络工具 5 次）

### 6.3 为什么使用 ToolContract trait 而非闭包

**决策**：工具通过 `ToolContract` trait 注册。

**理由**：
1. **类型安全**：编译期检查参数和返回类型
2. **自描述**：tool 自带 name / description / parameters / permissions
3. **可测试**：每个工具独立单元测试

---

## 7. 文件结构

```
src/plugins/services/tools/
├── mod.rs                  # ToolsService (impl ServicePlugin)
├── types.rs                # ToolContract trait / ToolContext / ToolOutput / ToolError / Permission
├── registry.rs             # ToolRegistry (Component)
├── circuit_breaker.rs      # CircuitBreaker (Component)
├── platform.rs             # NativePlatform (Component) — OsKind / ArchKind / execute_command
├── manifest.rs             # ToolManifest — aagnet-tool.toml 解析
├── discover.rs             # ToolDiscoverer (Component) — 扫描工具目录
├── builtins/
│   ├── mod.rs
│   ├── read_file.rs        # impl ToolContract for ReadFile
│   ├── write_file.rs       # impl ToolContract for WriteFile
│   ├── execute_command.rs  # impl ToolContract for ExecuteCommand
│   └── search_memory.rs    # impl ToolContract for SearchMemory
└── config.rs               # ToolsConfig — 内置工具开关、超时默认值、熔断器配置
```

---

## 8. 插件元数据

```yaml
name: tools
category: service
version: 0.2.0
run_mode: background
provides:
  - tool
requires: []
conflicts: []
config_schema:
  type: object
  properties:
    builtins:
      type: object
      properties:
        read_file: { type: boolean, default: true }
        write_file: { type: boolean, default: true }
        execute_command: { type: boolean, default: true }
        search_memory: { type: boolean, default: true }
    default_timeout_secs: { type: integer, default: 120 }
    circuit_breaker:
      type: object
      properties:
        failure_threshold: { type: integer, default: 3 }
        cooldown_secs: { type: integer, default: 30 }
```

---

## 9. 公开接口

```rust
// ── ToolContract（types.rs）──
pub trait ToolContract: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters(&self) -> Value;
    fn required_permissions(&self) -> Vec<Permission>;
    async fn execute(&self, args: Value, ctx: &ToolContext, cancel: CancellationToken)
        -> Result<ToolOutput, ToolError>;
}

// ── ToolRegistry（registry.rs）──
impl ToolRegistry {
    pub fn new(config: ToolsConfig) -> Self;
    pub fn register(&mut self, tool: Box<dyn ToolContract>);
    pub fn list_definitions(&self) -> Vec<ToolDefinition>;
    pub async fn call(&self, name: &str, args: Value, ctx: &ToolContext)
        -> Result<ToolOutput, ToolError>;
}

// ── NativePlatform（platform.rs）──
impl NativePlatform {
    pub fn new(timeout_secs: u64) -> Self;
    pub fn detect_os() -> OsKind;
    pub fn detect_arch() -> ArchKind;
    pub async fn execute_command(&self, cmd: &str, args: &[&str], ctx: &ToolContext)
        -> Result<ToolOutput, ToolError>;
}
```
