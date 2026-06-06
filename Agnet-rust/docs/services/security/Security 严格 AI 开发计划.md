# Security（安全策略引擎）严格 AI 开发计划

本计划用于指导 AI 严格按照 `docs/services/security/security开发文档.md` 完成 memory 模块的全部代码。

---

## 项目背景

- **模块名称**：security（安全策略引擎）
- **模块定位**：ToolRegistry 执行工具之前的安全守护层，Deny → Allow → Guard → Approve 四级决策模型，支持可插拔 Guardian 链和审批流程。
- **外部接口**：
  - `SecurityPolicyEngine` — 核心策略引擎 trait
  - `DefaultSecurityPolicyEngine` — 默认引擎实现
  - `ApprovalService` — 审批服务
  - `Guardian` trait — 可插拔的安全检查器
- **当前状态**：engine.rs（评估流程 ✅）、models.rs ✅、approval.rs（审批+GC ✅）、Guardians（❌ 全部待实现）、SecurityService（❌ 待创建）
- **依赖项**：`tokio::sync::{RwLock, oneshot}`、`uuid`、`serde_json`、`tracing`、`async-trait`

---

## 硬编码分类定义（security 特有）

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 审批超时 | `30` 秒 | 从 `ApprovalConfig.default_timeout_secs` 读取 |
| GC 间隔 | `60` 秒 | 从 `ApprovalConfig.gc_interval_secs` 读取 |
| 已完成记录上限 | `500` | 从 `ApprovalConfig.completed_max_count` 读取 |
| 已完成的保留时间 | `3600` 秒 | 从 `ApprovalConfig.completed_max_age_secs` 读取 |
| 最大待审批 | `200` | 从 `ApprovalConfig.max_pending` 读取 |
| 默认决策 | `SecurityDecision::Deny` | 从 `SecurityPolicyConfig.default_decision` 读取 |
| 审批合并策略 | `"First"` | 从 `SecurityPolicyConfig.approve_merge_strategy` 读取 |
| Guardian 优先级 | `100` | 从 Guardian 实现自身的 `priority()` 方法返回 |
| 路径遍历检测 | 字符串匹配 `..` | 使用 `Path::canonicalize()` 后检查前缀 |
| 允许目录白名单 | 硬编码 `["/tmp"]` | 从配置 `allowed_directories` 读取 |

---

## 项目目录结构

```
src/plugins/services/security/
├── mod.rs              # 模块入口：子模块声明 + SecurityService + 公开类型 re-export
├── models.rs           # Subject / Action / Resource / SecurityDecision / GuardResult / SecurityError
├── engine.rs           # SecurityPolicyEngine trait + DefaultSecurityPolicyEngine
├── approval.rs         # ApprovalService + PendingApproval + CompletedApproval + GC
├── config.rs           # SecurityPolicyConfig + ApprovalConfig + GuardianConfig + 校验
└── guardians/
    ├── mod.rs          # Guardian trait + 注册辅助函数
    ├── path_traversal.rs   # PathTraversalGuardian
    ├── command_injection.rs # ShellInjectionGuardian
    ├── file_permission.rs  # FilePermissionGuardian
    └── network_access.rs   # NetworkAccessGuardian
```

---

## AI 宪法

```
[宪法已生效]

1. **文档唯一真理**：所有类型、签名、默认值、流程步骤与 security开发文档.md 一致。

2. **零幻觉**：Security 只有四级决策模型（Deny/Allow/Guard/Approve），不存在额外的决策级别。Guardian 只有 4 种内置实现（path_traversal / command_injection / file_permission / network_access），不支持虚构的 Guardian 类型。

3. **零硬编码**：
   a. 审批超时、GC 间隔、已完成的保留时间从 ApprovalConfig 读取
   b. 最大待审批数、已完成记录上限从 ApprovalConfig 读取
   c. 默认决策、审批合并策略从 SecurityPolicyConfig 读取
   d. Guardian 的允许目录 / 允许域名从 GuardianConfig 读取

4. **完整实现**：每个 Guardian 的 evaluate() 必须有完整的安全检查逻辑（非 stub），SecurityService 的 init/start/handle_signal/stop/shutdown 齐全。

5. **错误处理**：
   - Guardian evaluate() 内部错误视为跳过（Option<GuardResult>::None），不阻断链
   - ApprovalService 的 pending 满或 channel 关闭返回 SecurityError（不 panic）
   - Config validate() 在 init 阶段执行，不合法则拒绝启动

6. **测试同步生成**：
   - DefaultSecurityPolicyEngine：完整 evaluate 流程（Deny 中断 / Allow 中断 / Guard 累积 / Approve 合并 / 空链默认决策 / 跳过 None）
   - ApprovalService：正常审批流程 / 超时拒绝 / 重复响应 / 满 pending 拒绝 / GC 清理
   - PathTraversalGuardian：`../../etc/passwd` / 绝对路径 / 符号链接 / 白名单路径
   - CommandInjectionGuardian：`;` / `|` / `$()` / 安全参数通过
   - FilePermissionGuardian：允许目录内 / 禁止目录外 / 通配符
   - NetworkAccessGuardian：白名单域名 / 拒绝域名 / IP 格式
   - SecurityService：完整生命周期 / 信号处理 / shutdown 清理
```

---

## 详细开发步骤

### 步骤 0：确认骨架

**操作**：确认现有文件（engine.rs / models.rs / approval.rs）已就位，创建缺失文件（config.rs、service.rs、guardians/mod.rs、各个 guardian 文件）。声明 mod chain。

**验收**：`cargo check` 通过

---

### 步骤 1：Config 层（config.rs）

| 结构体 | 关键字段 | 说明 |
|--------|---------|------|
| `SecurityPolicyConfig` | default_decision, user_confirmation_timeout_secs(30), approve_merge_strategy, guardian_configs, audit_enabled(true) | 顶层安全配置 |
| `ApprovalConfig` | default_timeout_secs(30), max_pending(200), gc_interval_secs(60), completed_max_count(500), completed_max_age_secs(3600) | 审批配置 |
| `GuardianConfig` | enabled(true), priority, allowed_dirs, allowed_hosts, denied_patterns | 各 Guardian 的独立配置 |

`SecurityPolicyConfig::validate()`：`user_confirmation_timeout_secs != 0` 校验。

### 步骤 2：models.rs

现有类型（已存在，按需微调）：

```rust
pub struct Subject { pub session_id: String, pub session_type: SessionType, pub metadata: HashMap<String, String> }
pub struct Action { pub tool_name: String, pub operation: Operation, pub arguments: Value }
pub struct Resource { pub resource_type: ResourceType, pub identifier: String, pub metadata: HashMap<String, String> }
pub enum SecurityDecision { Allow, Deny { reason: String }, Guard { findings: Vec<GuardFinding> }, Approve { timeout: Duration, prompt: String, findings: Vec<GuardFinding> } }
pub enum GuardResult { Deny(String), Allow, Guard(GuardFinding), Approve(Duration, String) }
pub enum Operation { Read, Write, Execute, Delete, NetworkAccess, ConfigModify }
pub enum ResourceType { File, Directory, NetworkHost, Tool, Configuration, MemoryItem }
pub enum SessionType { Interactive, Automated, Debug }
pub enum ApproveMergeStrategy { First, Strictest }
pub struct GuardFinding { pub guardian: String, pub severity: Severity, pub message: String, pub recommendation: Option<String> }
pub struct SecurityError { pub kind: SecurityErrorKind, pub description: String, pub recommendation: Option<String> }
pub enum SecurityErrorKind { Denied, ConfigInvalid, ApprovalCancelled, ApprovalTimeout, Internal }
```

### 步骤 3：Guardian 链（guardians/）

**3.1 mod.rs — Guardian trait + 排序/注册**

```rust
#[async_trait]
pub trait Guardian: Send + Sync {
    fn name(&self) -> &str;
    fn priority(&self) -> i32;
    fn enabled(&self) -> bool;
    async fn evaluate(&self, subject: &Subject, action: &Action, resource: &Resource) -> Option<GuardResult>;
}
fn sort_guardians(guardians: &mut Vec<Box<dyn Guardian>>) { guardians.sort_by_key(|g| -g.priority()); }
```

**3.2 path_traversal.rs — PathTraversalGuardian**

- priority=100，检测 `ResourceType::File` / `ResourceType::Directory`
- 检测项：含 `..` 组件、绝对路径不存在 allowed_dirs 内、符号链接目标出界
- 使用 `Path::canonicalize()` 解析后对比 `allowed_dirs` 前缀
- 配置：guardian_configs["path_traversal"].allowed_dirs

**3.3 command_injection.rs — CommandInjectionGuardian**

- priority=90，检测 `Operation::Execute` 且工具涉及 shell 调用
- 检测 shell 元字符：`;` `|` `$()` `` ` `` `&` `&&` `||` `>` `<`
- 参数白名单对比（已知安全字符通过）
- 配置：guardian_configs["command_injection"].denied_patterns

**3.4 file_permission.rs — FilePermissionGuardian**

- priority=80，检测 `ResourceType::File` / `ResourceType::Directory`
- 路径不在 allowed_dirs 内 → Deny
- 路径在 allowed_dirs 内 → Allow（短路，不被后续 Guardian 重复检查）
- 配置：guardian_configs["file_permission"].allowed_dirs

**3.5 network_access.rs — NetworkAccessGuardian**

- priority=70，检测 `ResourceType::NetworkHost`
- 域名/IP 白名单匹配 → Allow
- 域名/IP 不匹配 → Deny（含具体原因）
- 配置：guardian_configs["network_access"].allowed_hosts

**验收**：每个 Guardian 的 evaluate() 测试（含 None→跳过、Deny→拒绝、Allow→放行场景）

### 步骤 4：DefaultSecurityPolicyEngine（engine.rs）

现有逻辑已存在，需验证：
- evaluate() 按 priority 降序遍历
- Deny → 立即返回
- Allow → 立即返回（短路）
- Guard → 累积 findings
- Approve → 累积 approve_decisions
- 全部遍历完：非空 approve → 按 merge strategy 合并 / 非空 guard → 返回 Guard findings / 无匹配 → default_decision
- audit_enabled → 审计日志

**验收**：完整 evaluate 流程测试（含 5 种决策组合）

### 步骤 5：ApprovalService（approval.rs）

现有逻辑已存在，需验证：
- request_approval()：UUID + oneshot channel + max_pending 检查
- respond()：channel 发送 decision + 记录 completed
- start_gc()：tokio::spawn 定期清理超时 pending + 过期/超量 completed

**验收**：正常流程/超时/满 pending/GC 测试

### 步骤 6：SecurityService（service.rs，新建）

```rust
pub struct SecurityService {
    engine: Arc<DefaultSecurityPolicyEngine>,
    approval: Arc<ApprovalService>,
    config: SecurityPolicyConfig,
}

impl ServicePlugin for SecurityService {
    fn name(&self) -> &str { "security" }
    async fn init(&mut self, ctx: &ServiceInitContext) -> Result<()> {
        1. 加载 SecurityPolicyConfig（ctx.get_config()）
        2. validate() 校验配置
        3. 创建并注册 Guardian：PathTraversal / CommandInjection / FilePermission / NetworkAccess
        4. 根据 guardian_configs 启用/禁用每个 Guardian
        5. 初始化 ApprovalService
    }
    async fn start(&self, ap: &ServiceAccessPoint) -> Result<()>
        1. ap.register_provider("security", SecurityProviderAdapter::new(self.engine.clone()))
        2. 启动审批 GC 后台任务
    }
    async fn handle_signal(&self, signal: ServiceSignal) -> Result<()>
        GracefulShutdown → 拒绝新审批请求
        ImmediateShutdown → 清除 pending 审批
        ConfigReload → 重新加载配置 + 重建 Guardian 列表
        HealthCheck → 5s 内返回 Ok(())（红线 V-R01）
        Suspend → 设置暂停标志，evaluate() 全部 Deny
        Resume → 清除暂停标志
    }
    async fn stop(&self) -> Result<()> { 设置暂停标志 }
    async fn shutdown(&self) -> Result<()> {
        1. 清除所有 pending 审批（channel 关闭）
        2. 停止 GC 任务
        3. 反注册 Provider
    }
}
```

**验收**：SecurityService 完整生命周期测试（init+start+handle_signal+stop+shutdown）

### 步骤 7：mod.rs

```
pub use engine::{SecurityPolicyEngine, DefaultSecurityPolicyEngine};
pub use approval::ApprovalService;
pub use guardians::Guardian;
pub use models::{Subject, Action, Resource, SecurityDecision, GuardResult, GuardFinding, SecurityError, ...};
pub use service::SecurityService;
```

### 步骤 8：终态自检

1. `cargo test --all` 全量通过，`cargo build` 无 error
2. 对照 security开发文档.md §5.3 的 10 项自查清单
3. Guardian 全部有完整实现（非 stub）
