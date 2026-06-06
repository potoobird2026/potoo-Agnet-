# Tools（工具注册与执行服务） 严格 AI 开发计划

本计划用于指导 AI 严格按照 `docs/services/tools/tools开发文档.md` 生成 tools 模块的全部代码，彻底杜绝偷懒、走捷径、幻觉、硬编码、不一致等常见问题。您只需按步骤顺序执行，每一步通过验收后才能进入下一步。

---

## 项目背景

- **模块名称**：tools（工具注册与执行服务）
- **模块定位**：管理所有工具（内置 + MCP + 已安装包）的注册、发现、权限校验、熔断保护和跨平台执行，通过 `ToolRegistry` 为 `ToolExecutorSlot` 提供统一的工具调用入口。是框架中最复杂的 ServicePlugin 候选模块。
- **外部接口**：
  - `ToolRegistry` — 工具执行引擎（核心入口）
  - `CircuitBreaker` / `CircuitBreakerState` — 三态熔断器
  - `ToolManifest` — `.atp` 包清单格式
  - `NativePlatform` — 跨平台命令执行
- **依赖项**：`tokio`、`serde`、`serde_json`、`toml`、`zip`、`chrono`、`dirs`、`tracing`、`async-trait`、`thiserror`
- **框架层依赖**：`core::contract::tool`、`core::contract::tool_platform`、`core::contract::ContractRegistry`

---

## 硬编码专项预防纲领

在所有开发环节中，硬编码是 AI 最容易犯的顽疾。本计划通过以下三层机制彻底根除：

1. **AI 宪法硬编码禁令**（每轮对话生效）
2. **步骤验收中的硬编码检查项**（人工逐项核对）
3. **终态自动化硬编码扫描**（脚本 + 人工复核）

### 硬编码分类定义

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 超时秒数 | `.timeout(Duration::from_secs(30))` | 从 `CircuitBreakerConfig.cooldown_secs` 或 `DEFAULT_COMMAND_TIMEOUT` 常量读取 |
| 数字阈值 | `max_failures = 5` | 从 `CircuitBreakerConfig.max_failures` 读取，默认值在配置 Default 实现中定义 |
| 文件路径 | `"~/.aagnet/tools/"` | 通过 `dirs::home_dir().join(".aagnet")` 构建，测试用 `std::env::temp_dir()` |
| 平台指令 | `"sh"` / `"cmd"` | 通过 `NativePlatform::default_shell()` 的 `OsKind` 枚举分支选择 |
| 工具名 | `"read_file"` 散落在多处 | 在 `builtins/mod.rs` 中定义为 `const BUILTIN_NAMES` |
| 工具有限列表 | 循环中硬编码 4 个内置工具 | 通过 `register_builtins()` 统一注册，新增工具只需加文件 + 改注册函数 |
| 命令超时 | `command_timeout_secs = 120` | 定义为 `DEFAULT_COMMAND_TIMEOUT` 常量，可被 ToolContext 覆盖 |
| 循环上限 | `max_attempts = 10` | 定义为 `MAX_EXECUTE_ATTEMPTS` 常量 |
| 目录名 | `"tools"` / `"skills"` / `"mcp"` | 定义为 `DIR_TOOLS` / `DIR_SKILLS` / `DIR_MCP` 常量 |
| 包文件名 | `"aagnet-tool.toml"` | 定义为 `MANIFEST_FILENAME` 常量 |

---

## 项目目录结构

```
src/plugins/services/tools/
├── mod.rs                    # 模块入口：子模块声明 + 公开类型 re-export
├── registry.rs               # ToolRegistry — 工具执行引擎（熔断 + 回退 + 开关 + 防循环）
├── circuit_breaker.rs        # CircuitBreaker — 三态熔断器（Closed/Open/HalfOpen）
├── platform.rs               # NativePlatform — 跨平台命令执行（impl PlatformContract）
├── manifest.rs               # ToolManifest — .atp 包清单格式（TOML 解析/校验）
├── discover.rs               # ToolDiscover — 扫描 ~/.aagnet/ 发现已安装组件
├── install.rs                # ToolInstallManager — 安装/卸载 + ToolsDatabase
├── package.rs                # ToolPackage — .atp ZIP 归档读写
└── builtins/                 # 内置工具
    ├── mod.rs
    ├── read_file.rs          # 文件读取工具
    ├── write_file.rs         # 文件写入工具
    ├── execute_command.rs    # 命令执行工具
    └── search_memory.rs      # 记忆搜索工具
```

> **模块边界规范**：`mod.rs` 仅暴露 `CircuitBreaker`、`CircuitBreakerState`、`ToolManifest`、`NativePlatform`、`ToolRegistry` 五个公共类型。`ToolDiscover`、`ToolInstallManager`、`ToolPackage`、`ToolsService` 等为 `pub(crate)`。

模块声明链（需确认或补充）：

```
src/lib.rs                        →  pub mod plugins;
src/plugins/mod.rs                 →  pub mod services;
src/plugins/services/mod.rs        →  pub mod tools;
```

---

## AI 宪法（每次对话开始时完整粘贴）

```
[宪法已生效，本次对话必须无条件遵守]

你是一个严格执行设计文档的 Rust 代码生成器。你的代码必须能够直接通过编译、测试，且完全忠实于 `tools开发文档.md`。

1. **文档唯一真理**：所有类型定义、函数签名、默认值、错误变体、转换规则、流程步骤，必须与 `tools开发文档.md` 完全一致，不得自行增删改。

2. **零幻觉**：不允许出现设计文档未提及的字段、方法、枚举值或行为。特别注意：
   - `ToolRegistry` 只有 `contracts` 和 `breakers` 两个字段，不凭空生成第3个
   - `CircuitBreaker` 只有三态（Closed/Open/HalfOpen），不存在第4种状态
   - `NativePlatform` 只有 `os`、`arch`、`command_timeout_secs` 三个字段

3. **零硬编码**：
   a. 所有超时值从 `CircuitBreakerConfig` 读取或使用 `DEFAULT_COMMAND_TIMEOUT` 常量
   b. 所有数字阈值（max_failures/cooldown_secs/half_open_max_requests）从配置读取
   c. 文件路径通过 `dirs::home_dir()` + `PathBuf::join()` 构建，不以 `~` 开头
   d. 平台指令通过 `OsKind` 枚举分支选择，不假设 `sh` 或 `cmd`
   e. 内置工具名通过 `const BUILTIN_NAMES` 集中定义
   f. 循环上限定义为 `const MAX_EXECUTE_ATTEMPTS: usize = 10`
   g. 目录名（tools/skills/mcp）定义为常量
   h. 包清单文件名定义为 `const MANIFEST_FILENAME: &str = "aagnet-tool.toml"`

4. **完整实现**：每个函数必须完整实现，不允许使用 `todo!()`、`unimplemented!()` 或空函数体。特别注意：
   - `ToolRegistry::execute()` 必须完整实现 11 步执行流程（含防循环、熔断检查、回退链、validate、after_execution）
   - `CircuitBreaker` 的三态转换必须完整实现（含半开试探计数、冷却时间检查）
   - `ToolManifest::validate()` 必须实现所有 5 条校验规则
   - `ToolDiscover::scan_dir()` 必须完整实现目录遍历流程
   - `ToolInstallManager::install_from_atp()` 必须完整实现 8 步安装流程
   - 每个内置工具的 `execute()` 必须完整实现功能

5. **错误处理完整**：
   - `ToolRegistry::execute()` 中的每一步失败必须有对应的 `ToolError` 变体
   - `CircuitBreaker::allow_request()` 必须正确处理冷却时间未到期的等待逻辑
   - `NativePlatform::execute_command()` 必须处理子进程启动失败、超时、非零退出码
   - `ToolManifest::validate()` 失败返回描述性错误字符串
   - 不允许 `unwrap()`（测试除外），测试中的 `unwrap()` 必须有注释说明"测试中安全"

6. **一致性**：方法名、字段名、枚举变体名必须与 `tools开发文档.md` 完全一致，大小写敏感。

7. **禁止额外依赖**：只能使用 `std`、`tokio`、`serde`、`serde_json`、`toml`、`zip`、`chrono`、`dirs`、`tracing`、`async-trait`、`thiserror` 以及项目内部模块。严禁引入 `reqwest`、`uuid`、`regex`。

8. **注释规则**：
   - 只允许写"为什么"的注释（解释非显而易见的决策）
   - 不允许写"做什么"的废话注释（如 `// 读取文件`）
   - 引用设计文档时用 `// 设计文档 §X.Y` 格式

9. **测试同时生成**：
   - 为每个 `pub fn` 生成单元测试
   - `CircuitBreaker` 测试覆盖所有 6 种状态转换（文档 §4.2.2 表格）
   - `ToolRegistry::execute()` 测试覆盖全部 11 步流程路径（成功、熔断打开、回退切换、防循环检测、validate 失败、ComponentSwitch 禁用、工具不存在）
   - `NativePlatform` 测试使用 mock 子进程（不调用真实 shell）
   - `ToolManifest` 测试覆盖 5 条校验规则 + 平台兼容检查
   - `ToolDiscover` 测试使用临时目录
   - `ToolInstallManager` 测试使用临时 `.atp` 文件
   - 内置工具每个至少一个成功场景 + 一个错误场景测试
   - 测试名称包含设计文档章节号（如 `test_section_4_2_closed_to_open`）

10. **杜绝捷径**：
    - 不能因为 `CircuitBreaker` 看起来像简单状态机就省略冷却时间检查
    - 不能将 `execute()` 的 11 步合并到几个步骤里，必须严格按文档流程展开
    - 不能跳过 `ToolManifest::validate()` 的任意校验规则
    - 不能跳过 `ToolRegistry::execute()` 的 `after_execution()` 步骤
    - 防循环的 `visited` set 和 `max_attempts` 必须同时实现，缺一不可

11. **模块边界**：
    - `ToolRegistry` 不直接调用 `NativePlatform`（内置工具通过 `PlatformContract` trait 间接使用）
    - `ToolRegistry` 不直接引用 `ToolDiscover` 或 `ToolInstallManager`
    - `NativePlatform` 不涉及工具注册逻辑
    - `CircuitBreaker` 只做熔断状态管理，不做工具执行

12. **日志规范**：
    - 工具执行开始/结束记录 `debug`（携带工具名、参数概览）
    - 熔断器状态变化记录 `warn`（携带工具名、新旧状态）
    - 回退切换记录 `info`（携带原工具名、回退工具名）
    - 工具执行失败记录 `error`（携带工具名、错误信息）
    - 安装/卸载工具记录 `info`
    - ToolDiscover 扫描结果记录 `debug`
    - 禁止打印工具参数中的敏感数据（文件路径可打印，密码/密钥不可）
```

---

## 详细开发步骤

### 步骤 0：确认环境与骨架

**目标**：确保项目可编译，目录就绪，tools 模块已被注册。

**操作**：

1. 确认 Cargo.toml 包含以下依赖：

```toml
[dependencies]
tokio = { version = "1", features = ["sync", "process", "time"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
zip = "2"
chrono = { version = "0.4", features = ["serde"] }
dirs = "5"
tracing = "0.1"
async-trait = "0.1"
thiserror = "1"
```

2. 确认模块声明链完整：
   - `src/lib.rs` → `pub mod plugins;`
   - `src/plugins/mod.rs` → `pub mod services;`
   - `src/plugins/services/mod.rs` → `pub mod tools;`
   - 如 `mod.rs` 不存在则创建，并加入 `pub mod tools;`

3. 确保 `cargo check` 通过（空模块树允许 warning）。

**验收标准**：
- `cargo check` 无 error
- 依赖版本兼容
- 目录结构完整

---

### 步骤 1：生成 CircuitBreaker（circuit_breaker.rs）

**目标**：实现三态熔断器，包含完整的 Closed/Open/HalfOpen 状态转换逻辑。

**文件**：`src/plugins/services/tools/circuit_breaker.rs`

**要求**：

1. `CircuitBreakerState` 枚举：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}
```

2. `CircuitBreakerConfig` 结构体（实现 `Default`）：

```rust
pub struct CircuitBreakerConfig {
    pub max_failures: u32,            // 连续失败阈值，默认 5
    pub window_secs: u64,             // 滑动窗口（秒），默认 60（预留字段，state 中暂不使用）
    pub cooldown_secs: u64,           // 熔断冷却时间（秒），默认 30
    pub half_open_max_requests: u32,  // 半开状态最大试探请求数，默认 3
}
```

3. `CircuitBreaker` 结构体：

```rust
pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    state: CircuitBreakerState,
    consecutive_failures: u32,
    last_failure_time: Option<Instant>,
    opened_at: Option<Instant>,
    half_open_requests: u32,
}
```

4. 方法：

| 方法 | 签名 | 行为 |
|------|------|------|
| `new(config)` | `pub fn new(config: CircuitBreakerConfig) -> Self` | 初始 Closed 状态，所有计数器归零 |
| `state()` | `pub fn state(&self) -> CircuitBreakerState` | 返回当前状态 |
| `record_success()` | `pub fn record_success(&mut self)` | 见下方转换规则表 |
| `record_failure()` | `pub fn record_failure(&mut self) -> bool` | 返回 true 表示熔断触发了 Open |
| `allow_request()` | `pub fn allow_request(&mut self) -> bool` | 请求是否被允许（含冷却到期检测→自动转 HalfOpen） |
| `reset()` | `pub fn reset(&mut self)` | 重置为初始 Closed 状态 |

5. 状态转换规则（严格按文档 §4.2.2 表格）：

| 当前状态 | 事件 | 新状态 | 条件 |
|---------|------|--------|------|
| Closed | `record_failure()` | Closed | `consecutive_failures < max_failures` |
| Closed | `record_failure()` | **Open** | `consecutive_failures >= max_failures` |
| Open | `allow_request()` 检查 | Open | 冷却未到期（`opened_at.elapsed() < cooldown`） |
| Open | `allow_request()` 检查 | **HalfOpen** | 冷却到期后自动转换 |
| HalfOpen | `record_success()` | HalfOpen | `half_open_requests < half_open_max_requests` |
| HalfOpen | `record_success()` | **Closed** | `half_open_requests >= half_open_max_requests` |
| HalfOpen | `record_failure()` | **Open** | 试探失败，重新熔断 |

**验收标准**：
- `cargo test` 通过
- 测试覆盖全部 7 行状态转换表
- 测试覆盖 `reset()` 从任意状态回到 Closed
- 测试覆盖冷却时间边界（刚到期 vs 未到期）
- 所有配置默认值作为常量定义在文件顶部
- `consecutive_failures` 在 Closed 下 `record_success()` 后归零

---

### 步骤 2：生成 NativePlatform（platform.rs）

**目标**：实现跨平台命令执行抽象，通过 `OsKind` 枚举分支适配 Windows/Unix。

**文件**：`src/plugins/services/tools/platform.rs`

**要求**：

1. 枚举定义：

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsKind {
    Windows,
    Linux,
    MacOS,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchKind {
    X8664,
    Aarch64,
}
```

2. `NativePlatform` 结构体：

```rust
pub struct NativePlatform {
    pub os: OsKind,
    pub arch: ArchKind,
    pub command_timeout_secs: u64,
}
```

3. 方法：

| 方法 | 签名 | 行为 |
|------|------|------|
| `new(timeout_secs)` | `pub fn new(timeout_secs: u64) -> Self` | 自动检测当前 OS 和架构 |
| `detect_os()` | `pub fn detect_os() -> OsKind` | 通过 `cfg!(target_os)` 编译时判断 |
| `detect_arch()` | `pub fn detect_arch() -> ArchKind` | 通过 `cfg!(target_arch)` 编译时判断 |
| `default_shell()` | `pub fn default_shell(&self) -> (&str, &str)` | Windows → `("cmd", "/C")`，Unix → `("sh", "-c")` |
| `execute_command()` | `pub async fn execute_command(&self, cmd: &str, args: &[&str], ctx: &ToolContext) -> Result<ToolOutput, ToolError>` | 通过子进程执行命令，含超时 |

4. `execute_command()` 实现要点：
   - 使用 `tokio::process::Command::new(shell).args([flag, command])` 构建命令
   - 设置 `working_dir` 为 `ctx.working_dir`
   - 设置环境变量（注入 `ctx.env`）
   - 使用 `tokio::time::timeout(self.command_timeout_secs)` 包装
   - 超时返回 `ToolError::Timeout`
   - 子进程启动失败返回 `ToolError::Internal`
   - 非零退出码不视为错误，返回 stdout + stderr + exit_code

**验收标准**：
- `cargo test` 通过
- `detect_os()` / `detect_arch()` 测试使用 `#[cfg()]` 条件编译验证
- `default_shell()` 测试验证 Windows → cmd, Linux/MacOS → sh
- `execute_command()` 测试使用 mock 命令（Windows: `cmd /C echo test`，Unix: `echo test`），验证输出正确
- 超时测试使用 `sleep 10`（Unix）或 `timeout /T 10`（Windows）验证超时错误
- `DEFAULT_COMMAND_TIMEOUT` 常量在文件顶部定义

---

### 步骤 3：生成 ToolManifest（manifest.rs）

**目标**：实现 `.atp` 包清单格式的 TOML 解析与校验。

**文件**：`src/plugins/services/tools/manifest.rs`

**要求**：

1. 清单结构（严格按文档 §4.4.1 TOML 格式定义）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolManifest {
    pub tool: ToolMeta,
    pub permissions: PermissionSet,
    pub dependencies: Option<DependencySet>,
    pub skill: Option<SkillConfig>,
    pub mcp: Option<McpEndpointConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolMeta {
    pub name: String,
    pub version: String,
    pub group: String,
    pub description: String,
    pub kind: String,          // "tool" | "skill" | "mcp" | "bundle"
    pub platforms: Vec<String>, // 空=全平台
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionSet {
    pub file_read: Vec<String>,
    pub file_write: Vec<String>,
    pub network: bool,
    pub shell: bool,
    pub memory_read: bool,
    pub memory_write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencySet {
    pub tools: Vec<String>,
}
```

2. `MANIFEST_FILENAME` 常量为 `"aagnet-tool.toml"`，包扩展名 `.atp`

3. 方法：

| 方法 | 签名 | 行为 |
|------|------|------|
| `from_toml(content)` | `pub fn from_toml(content: &str) -> Result<Self, String>` | 反序列化 TOML 字符串 + 调用 `validate()` |
| `validate()` | `pub fn validate(&self) -> Result<(), Vec<String>>` | 执行 5 条校验规则 |
| `is_platform_supported()` | `pub fn is_platform_supported(&self) -> bool` | 检查当前平台是否在 `platforms` 列表中 |
| `platform_tag()` | `pub(crate) fn platform_tag() -> String` | 返回如 `"windows-x86_64"` 的平台标签 |

4. 校验规则（`validate()`）：

| 校验项 | 规则 | 错误信息 |
|--------|------|---------|
| `tool.name` | 非空 | `"tool.name 不能为空"` |
| `tool.version` | 非空 + 合法语义版本 | `"tool.version 不能为空"` 或 `"tool.version 不是合法语义版本"` |
| `tool.description` | 非空 | `"tool.description 不能为空"` |
| `tool.kind` | `tool`/`skill`/`mcp`/`bundle` 之一 | `"tool.kind 必须是 tool/skill/mcp/bundle 之一"` |
| `mcp.endpoint_type` | `kind==mcp` 或 `kind==bundle` 时非空 | `"kind 为 mcp/bundle 时 mcp.endpoint_type 不能为空"` |

5. `platform_tag()` 格式：`"{os}-{arch}"`，其中 `os` 为 `windows`/`linux`/`macos`，`arch` 为 `x86_64`/`aarch64`

**验收标准**：
- `cargo test` 通过
- 5 条校验规则每条至少一个成功 + 一个失败测试
- `is_platform_supported()` 测试空列表返回 true、匹配返回 true、不匹配返回 false
- `platform_tag()` 测试通过 `#[cfg()]` 条件编译验证

---

### 步骤 4：生成 ToolPackage（package.rs）

**目标**：实现 `.atp` ZIP 归档的读写操作。

**文件**：`src/plugins/services/tools/package.rs`

**要求**：

1. `ToolPackage` 结构体：

```rust
pub struct ToolPackage {
    pub manifest: ToolManifest,
    pub entries: Vec<PackageEntry>,
}

pub struct PackageEntry {
    pub path: String,
    pub data: Vec<u8>,
}
```

2. 方法：

| 方法 | 签名 | 行为 |
|------|------|------|
| `from_atp_file(path)` | `pub fn from_atp_file(path: &Path) -> Result<Self, String>` | 读取 `.atp` ZIP，解析 manifest，列出所有文件条目 |
| `is_compatible()` | `pub fn is_compatible(&self) -> bool` | 调用 `self.manifest.is_platform_supported()` |
| `extract_to(dir)` | `pub fn extract_to(&self, dir: &Path) -> Result<(), String>` | 将所有条目解压到目标目录（保持目录结构） |

3. 实现要点：
   - ZIP 读取使用 `zip::ZipArchive`
   - 首先读取 `aagnet-tool.toml` 文件条目中的内容解析为 `ToolManifest`
   - 其余文件条目保留为 `PackageEntry`
   - `extract_to()` 使用 `tokio::fs` 写入文件（原子写入可选）

**验收标准**：
- `cargo test` 通过
- 测试使用临时目录创建 mock `.atp` 文件
- 测试验证 `extract_to()` 后目录结构正确
- 测试验证 `is_compatible()` 正确调用 manifest 检查

---

### 步骤 5：生成 ToolDiscover（discover.rs）

**目标**：实现工具发现服务，扫描 `~/.aagnet/` 下的已安装组件。

**文件**：`src/plugins/services/tools/discover.rs`

**要求**：

1. `DiscoveredComponent` 结构体：

```rust
pub struct DiscoveredComponent {
    pub name: String,
    pub version: String,
    pub kind: String,
    pub install_dir: PathBuf,
    pub manifest_path: PathBuf,
}
```

2. `ToolDiscover` 结构体：

```rust
pub struct ToolDiscover {
    search_dirs: Vec<PathBuf>,
}
```

3. 方法：

| 方法 | 签名 | 行为 |
|------|------|------|
| `new()` | `pub fn new() -> Self` | 空的搜索目录列表 |
| `with_default_dir()` | `pub fn with_default_dir() -> Self` | 添加 `~/.aagnet/tools/`、`~/.aagnet/skills/`、`~/.aagnet/mcp/` |
| `add_search_dir(dir)` | `pub fn add_search_dir(&mut self, dir: PathBuf)` | 添加自定义搜索目录 |
| `scan_all()` | `pub async fn scan_all(&self) -> Vec<DiscoveredComponent>` | 扫描所有搜索目录，返回发现的所有组件 |
| `scan_dir(dir, kind)` | `pub async fn scan_dir(&self, dir: &Path, kind: &str) -> Vec<DiscoveredComponent>` | 扫描单个目录 |

4. `scan_dir()` 流程：

```
scan_dir(dir, kind)
  │
  ├─ read_dir(dir) → 遍历 name_dir（工具名目录）
  │
  ├─ read_dir(name_dir) → 遍历 version_dir（版本目录）
  │
  ├─ 检查 version_dir.join(MANIFEST_FILENAME).exists()?
  │    └─ Yes → DiscoveredComponent { name, version, kind, install_dir: version_dir, manifest_path }
  │
  └─ 返回所有已发现的组件
```

5. `with_default_dir()` 路径构建：
   - 使用 `dirs::home_dir().join(".aagnet")`，不以 `~` 或硬编码路径开头
   - 子目录名 `tools`/`skills`/`mcp` 定义为 `DIR_TOOLS`/`DIR_SKILLS`/`DIR_MCP` 常量

**验收标准**：
- `cargo test` 通过
- 测试使用 `std::env::temp_dir()` 创建临时目录结构（name/version/manifest）
- 测试空目录返回空列表
- 测试缺少 manifest 文件的目录被跳过
- 测试 `with_default_dir()` 使用 `dirs::home_dir()` 而非硬编码路径

---

### 步骤 6：生成 ToolInstallManager + ToolsDatabase（install.rs）

**目标**：实现工具安装/卸载管理 + `tools.toml` 数据库持久化。

**文件**：`src/plugins/services/tools/install.rs`

**要求**：

1. `InstalledEntry` 结构体：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledEntry {
    pub version: String,
    pub installed_at: String,  // RFC3339 时间戳
    pub enabled: bool,
    pub install_path: PathBuf,
}
```

2. `ToolsDatabase` 结构体（持久化为 `~/.aagnet/tools.toml`）：

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsDatabase {
    pub tools: HashMap<String, InstalledEntry>,
    pub skills: HashMap<String, InstalledEntry>,
    pub mcp: HashMap<String, InstalledEntry>,
}
```

3. `ToolInstallManager` 结构体：

```rust
pub struct ToolInstallManager {
    discover: ToolDiscover,
    database_path: PathBuf,
    database: ToolsDatabase,
}
```

4. 方法：

| 方法 | 签名 | 行为 |
|------|------|------|
| `new(database_path)` | `pub fn new(database_path: PathBuf) -> Self` | 加载数据库 |
| `load_database()` | `pub fn load_database(path: &Path) -> ToolsDatabase` | 从 TOML 加载，文件不存在返回空数据库 |
| `save_database()` | `pub fn save_database(&self) -> Result<(), String>` | 持久化为 TOML |
| `install_from_atp(atp_path)` | `pub async fn install_from_atp(&mut self, atp_path: &Path) -> Result<ToolManifest, String>` | 完整安装流程（8 步） |
| `uninstall(name)` | `pub async fn uninstall(&mut self, name: &str) -> Result<(), String>` | 卸载工具 |
| `list_installed(kind)` | `pub fn list_installed(&self, kind: &str) -> Vec<(&String, &InstalledEntry)>` | 列出已安装的某类组件 |

5. `install_from_atp()` 8 步流程（严格按设计文档 §4.6.2）：

```
install_from_atp(atp_path)
  │
  ├─ 1. ToolPackage::from_atp_file(atp_path) → 解析 ZIP，校验 manifest
  ├─ 2. package.is_compatible() → 检查平台支持
  ├─ 3. 检查同名同版本冲突（已安装则返回错误）
  ├─ 4. 确定安装目录：{tools|skills|mcp}_dir/{name}/{version}
  ├─ 5. package.extract_to(install_dir) → 解压 ZIP
  ├─ 6. 写入 ToolsDatabase（新增 InstalledEntry）
  ├─ 7. save_database() → 持久化 tools.toml
  └─ 8. 返回 manifest
```

6. `install_from_atp()` 错误处理：
   - 步骤 1 失败：返回 `"解析 .atp 文件失败: {原因}"`
   - 步骤 2 不兼容：返回 `"当前平台不支持该工具包"`
   - 步骤 3 冲突：返回 `"工具 {name} 版本 {version} 已安装"`
   - 步骤 5 失败：清理已创建的目录，返回 `"解压失败: {原因}"`
   - 步骤 7 失败：返回 `"保存数据库失败: {原因}"`

**验收标准**：
- `cargo test` 通过
- 测试使用 `std::env::temp_dir()` 创建临时安装目录
- 测试 mock `.atp` 文件安装流程
- 测试重复安装返回冲突错误
- 测试 `load_database()` 从 TOML 恢复

---

### 步骤 7：生成 ToolRegistry（registry.rs）

**目标**：实现工具执行引擎，包含熔断、回退、组件开关、防循环调用。

**文件**：`src/plugins/services/tools/registry.rs`

**要求**：

1. `ToolRegistry` 结构体：

```rust
pub struct ToolRegistry {
    contracts: Arc<ContractRegistry>,
    breakers: Vec<Mutex<CircuitBreaker>>,
}
```

2. 方法：

| 方法 | 签名 | 行为 |
|------|------|------|
| `new(contracts)` | `pub fn new(contracts: Arc<ContractRegistry>) -> Self` | 从 contracts 读取工具列表，为每个工具创建 CircuitBreaker |
| `execute(name, args, ctx, cancel)` | `pub async fn execute(&self, name: &str, args: Value, ctx: &ToolContext, cancel: CancellationToken) -> Result<ToolOutput, ToolError>` | 完整 11 步执行流程 |

3. `execute()` 11 步流程（严格按文档 §4.1.2）：

```
execute(tool_name, args, ctx, cancel)
  │
  ├─ 初始化：visited = HashSet, current_name = tool_name, attempts = 0
  │
  └─ loop (max MAX_EXECUTE_ATTEMPTS 次):
       │
       ├─ 1. attempts >= MAX_EXECUTE_ATTEMPTS → ToolError::CircuitBreakerOpen
       ├─ 2. visited.contains(current_name) → 循环检测 → ToolError::CircuitBreakerOpen
       ├─ 3. visited.insert(current_name)
       │
       ├─ 4. contracts.get_tool(current_name) → 不存在 → ToolError::NotFound
       │
       ├─ 5. tool.validate(args, ctx) → 失败 → 返回 ToolError::Validation
       │
       ├─ 6. ComponentSwitch 检查 → 被禁用 → ToolError::Internal("ComponentDisabled")
       │
       ├─ 7. breaker.allow_request()?
       │      ├─ false → 有 fallback? → current_name = fallback → continue
       │      └─ false → 无 fallback → ToolError::CircuitBreakerOpen
       │
       ├─ 8. tool.execute(args, ctx, cancel).await
       │
       ├─ 9. 更新熔断器：Ok → record_success(), Err → record_failure()
       │
       ├─ 10. tool.after_execution(&result, ctx).await（设计文档有，需确认 ToolContract 是否有此方法）
       │
       └─ 11. 结果处理：
              ├─ Ok → return ToolOutput
              └─ Err → 有 fallback? → current_name = fallback, attempts += 1 → continue
                       └─ 无 fallback → return err
```

4. `MAX_EXECUTE_ATTEMPTS` 常量为 `10`，定义为文件顶部 `const`。

5. 熔断器索引：`breakers` 与 `contracts.all_tools()` 一一对应。
   - `get_breaker_index(name)` 查找工具名在 `all_tools()` 中的位置
   - 未找到对应熔断器时视为熔断通过（安全降级）

**验收标准**：
- `cargo test` 通过
- 测试使用 mock ContractRegistry（提供少量 mock ToolContract）
- 测试覆盖：
  - 正常执行成功路径
  - 工具不存在（NotFound）
  - validate 失败
  - ComponentSwitch 禁用
  - 熔断器打开 + 有回退
  - 熔断器打开 + 无回退
  - 执行失败 + 有回退
  - 执行失败 + 无回退
  - 回退链循环检测（fallback 指向已访问过的工具）
  - max_attempts 上限

---

### 步骤 8：生成内置工具（builtins/）

**目标**：实现 4 个内置工具：read_file、write_file、execute_command、search_memory。

**目录**：`src/plugins/services/tools/builtins/`

**通用要求**：每个内置工具实现 `ToolContract` trait，包含 `name()`、`description()`、`parameters()`、`required_permissions()`、`execute()`。

**8.1 builtins/mod.rs**

- 声明 4 个子模块
- `pub fn register_all(registry: &mut ToolRegistry, platform: Arc<NativePlatform>)` — 注册所有内置工具
- `pub const BUILTIN_NAMES: &[&str]` — 内置工具名列表

**8.2 read_file.rs**

- `name()` → `"read_file"`
- `description()` → `"读取文件内容（UTF-8 编码）"`
- `parameters()` → JSON Schema: `{ path: string }`
- `required_permissions()` → `[Permission::FileRead(vec!["$WORKSPACE/**"])]`
- `execute()`:
  - 参数校验：path 非空
  - 路径穿越检查：`path.canonicalize()` 必须在 `ctx.working_dir.canonicalize()` 下
  - 使用 `tokio::fs::read_to_string(path)` 读取文件
  - 成功返回 `ToolOutput { content: 文件内容, exit_code: Some(0), metadata: None }`
  - 失败返回 `ToolError::Execution`

**8.3 write_file.rs**

- `name()` → `"write_file"`
- `description()` → `"写入文件内容"`
- `parameters()` → JSON Schema: `{ path: string, content: string }`
- `required_permissions()` → `[Permission::FileWrite(vec!["$WORKSPACE/**"])]`
- `execute()`:
  - 参数校验：path 非空
  - 路径穿越检查同 read_file
  - 使用 `tokio::fs::write(path, content)` 写入文件
  - 自动创建父目录

**8.4 execute_command.rs**

- `name()` → `"execute_command"`
- `description()` → `"跨平台执行 shell 命令"`
- `parameters()` → JSON Schema: `{ command: string, args: string[] }`
- `required_permissions()` → `[Permission::Shell]`
- `execute()`:
  - 委托给 `NativePlatform::execute_command()`
  - 不直接调用 `Command::new`，保持与平台解耦

**8.5 search_memory.rs**

- `name()` → `"search_memory"`
- `description()` → `"搜索本地记忆库"`
- `parameters()` → JSON Schema: `{ query: string, top_k: number }`
- `required_permissions()` → `[Permission::MemoryRead]`
- `execute()`:
  - 通过 `ContractRegistry` 获取记忆服务
  - 调用记忆服务搜索（简化实现：返回空结果 + 提示连接记忆服务）

**验收标准**：
- 每个内置工具独立单元测试（成功 + 错误场景）
- `register_all()` 验证 4 个工具全部注册
- 路径穿越测试（`../../etc/passwd` 被拒绝）
- `execute_command` 测试使用 `NativePlatform` mock

---

### 步骤 9：生成 Services 入口 + mod.rs

**目标**：实现 `ToolsService`（ServicePlugin）和模块入口。

**文件**：
- `src/plugins/services/tools/mod.rs` — 模块入口（声明 + re-export）
- `src/plugins/services/tools/service.rs` — ToolsService 实现

**9.1 service.rs — ToolsService**

```rust
pub struct ToolsService {
    registry: Arc<ToolRegistry>,
    config: ToolsConfig,
}

pub struct ToolsConfig {
    pub builtins_enabled: HashMap<String, bool>,
    pub default_timeout_secs: u64,
    pub circuit_breaker: CircuitBreakerConfig,
}
```

实现 `ServicePlugin` trait：

| 方法 | 职责 |
|------|------|
| `name()` | 返回 `"tools"` |
| `init(ctx)` | 1. 加载 ToolsConfig（从 ctx.plugin_config 读取）<br>2. 调用 `register_builtins()` 注册 4 个内置工具到 ContractRegistry<br>3. 创建 `ToolDiscover::with_default_dir()` 扫描已安装工具<br>4. 创建 `ToolInstallManager` 加载工具数据库<br>5. 构造 `ToolRegistry` |
| `start(ap)` | `ap.register_provider("tool", self.registry.clone())` |
| `handle_signal(signal)` | `HealthCheck` → 5s 内返回 Ok<br>`ConfigReload` → ToolDiscover 重扫目录<br>`GracefulShutdown` → 等待执行完成 |
| `stop()` | 暂停新工具注册 |
| `shutdown()` | 反注册 Provider + 清理熔断器 |

**9.2 mod.rs**

```rust
pub mod circuit_breaker;
pub mod discover;
pub mod install;
pub mod manifest;
pub mod package;
pub mod platform;
pub mod registry;
pub mod service;

mod builtins;

pub use circuit_breaker::{CircuitBreaker, CircuitBreakerState};
pub use manifest::ToolManifest;
pub use platform::NativePlatform;
pub use registry::ToolRegistry;
pub use service::ToolsService;
```

**验收标准**：
- `cargo check` 通过
- 外部通过 `use crate::plugins::services::tools::ToolsService` 可访问
- ServicePlugin 生命周期方法完整实现（init → start → stop → shutdown）
- `mod.rs` 仅暴露 5 个公共类型

---

### 步骤 10：终态自检与硬编码审计

1. 全量编译与测试：

```bash
cargo test --all
cargo check
```

2. 硬编码扫描：

```bash
# 查找可能硬编码的超时数字
rg 'from_secs\(\d+\)' src/plugins/services/tools/
# 查找可能硬编码的文件路径
rg '"~/' src/plugins/services/tools/
# 查找可能硬编码的平台指令字符串
rg '"/[a-z]+"' src/plugins/services/tools/ --type rust
```

3. 对照 `tools开发文档.md` §5.3 的 10 项自查清单逐项核对。

---

## 持续纪律

- 每轮对话都先发送完整宪法
- 不接受任何包含 `todo!()` 或占位符的代码
- 发现 AI 幻觉立即打断并要求改正
- 每步人工校验不得跳过
- 提交代码时，commit message 标明完成了哪一步骤
