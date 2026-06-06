# Security（安全策略引擎）开发文档

## 0. 协议依据

本文档严格遵循以下三份协议标准，逐条对标：

| 协议 | 应用层级 | 关键条款 |
|------|---------|---------|
| **protocol-Service集成协议** | 模块对框架的接入方式 | §1 ServicePlugin 单入口、§2 ServiceAccessPoint 受控访问句柄、§3 运行时信号、§4 插件元数据、§5 生命周期、§8 协议特有红线 |
| **protocol-模块内部组件协议** | 模块内部子模块组织方式 | §1 Component 单入口、§3 AccessPoint 内部数据共享通道、§6 模块边界规范 |
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§4 自查清单 |

---

## 1. 模块定位

### 1.1 一句话描述

**在每个工具调用前执行多层安全守护（Guardian）链，按 Deny → Allow → Guard → Approve 四级决策模型决定操作是否放行，支持审批流程和审计日志。**

### 1.2 架构定位

Security 模块是框架的**安全策略执行点（Policy Enforcement Point）**，应在 ToolRegistry 执行工具之前被调用：

```
ToolRegistry::execute(tool_name, args, ctx, cancel)
  │
  ├── 1. 查找 ToolContract
  ├── 2. 熔断器检查
  ├── 3. 参数校验
  ├── 4. ★ SecurityPolicyEngine::evaluate(subject, action, resource)
  │     │
  │     ├── Guardian 1 (priority=100): PathTraversalGuardian
  │     ├── Guardian 2 (priority=90):  ShellInjectionGuardian
  │     ├── Guardian 3 (priority=80):  FilePermissionGuardian
  │     ├── Guardian 4 (priority=70):  NetworkAccessGuardian
  │     └── Guardian N (priority=50):  RuleBasedGuardian (自定义规则)
  │     │
  │     └── 四级决策：
  │           Deny（拒绝） > Allow（放行） > Guard（标记） > Approve（需审批）
  │
  ├── 5. 执行工具（如未被拒绝）
  └── 6. 审计日志记录（audit_enabled）
```

### 1.3 四级决策模型

```
优先级：Deny > Allow > Guard > Approve（前两者为立即终止）

Guardian A → Deny    ─┐
Guardian B → Allow   ─┤ 任一 Deny → 立即返回 Deny
Guardian C → Guard   ─┤ 任一 Allow → 立即返回 Allow
Guardian D → Approve ─┘ 全部 Guard + Approve → 合并处理

全部 Approve → 按 ApproveMergeStrategy 合并：
  - First: 取第一个审批要求
  - Strictest: 取超时最短的审批要求

全部 Guard → 聚合所有 findings → Guard { findings }
无匹配 Guardian → 返回 default_decision
```

---

## 2. 文件结构

```
src/plugins/services/security/
├── mod.rs         # 模块入口：子模块声明 + Engine + Models 公开类型 re-export
├── engine.rs      # SecurityPolicyEngine trait + DefaultSecurityPolicyEngine
├── models.rs      # Subject / Action / Resource / SecurityDecision / GuardResult 等
├── approval.rs    # ApprovalService — 审批管理（pending + completed + GC）
└── guardians/
    ├── mod.rs             # (空，预留)
    ├── file_path.rs       # 路径穿越检测 Guardian（待实现）
    ├── shell_injection.rs # 命令注入检测 Guardian（待实现）
    ├── network_access.rs  # 网络访问控制 Guardian（待实现）
    └── rule_based.rs      # 自定义规则 Guardian（待实现）
```

> **模块边界规范（§6.1）**：`mod.rs` 暴露 `SecurityPolicyEngine`、`DefaultSecurityPolicyEngine` 以及所有 Models 类型。Guardian 实现细节为 `pub(crate)`。

---

## 3. 功能清单

| 功能 | 描述 | 实现状态 | 对应源码 |
|------|------|:---:|---------|
| 安全策略引擎 | Guardian 链式评估，四级决策模型 | ✅ | `engine.rs:DefaultSecurityPolicyEngine` |
| 决策审计 | 每次评估结果写入 EventLogger（可开关） | ✅ | `engine.rs:audit()` |
| 审批服务 | oneshot 通道管理待审批操作，支持超时自动拒绝 | ✅ | `approval.rs:ApprovalService` |
| 审批 GC | 定期清理过期 pending + 超量 completed | ✅ | `approval.rs:start_gc()` |
| 路径穿越 Guardian | 检测 `..` / 绝对路径 / 符号链接 | ❌ 待实现 | `guardians/file_path.rs` |
| 命令注入 Guardian | 检测 shell 元字符（`;` `\|` `$()` 等） | ❌ 待实现 | `guardians/shell_injection.rs` |
| 文件权限 Guardian | 检测越权访问禁止目录 | ❌ 待实现 | `guardians/file_path.rs` |
| 网络访问 Guardian | 域名/IP 白名单控制 | ❌ 待实现 | `guardians/network_access.rs` |
| 自定义规则 Guardian | 用户定义的安全规则 | ❌ 待实现 | `guardians/rule_based.rs` |
| ServicePlugin | 完整生命周期 | ❌ 待补齐 | — |

---

## 4. 核心设计

### 4.1 安全模型（Subject + Action + Resource）

**文件**：`models.rs`

```
┌─────────────────┐    ┌──────────────────┐    ┌─────────────────┐
│    Subject       │    │     Action        │    │    Resource      │
│    (谁在操作)     │    │    (做什么操作)     │    │   (操作什么资源)   │
├─────────────────┤    ├──────────────────┤    ├─────────────────┤
│ session_id       │    │ tool_name         │    │ resource_type    │
│ session_type     │    │ operation         │    │ identifier       │
│   - Interactive  │    │   - Read          │    │ metadata         │
│   - Automated    │    │   - Write         │    │                  │
│   - Debug        │    │   - Execute       │    │ ResourceType:    │
│ metadata         │    │   - Delete        │    │   - File         │
│                  │    │   - NetworkAccess │    │   - Directory    │
│                  │    │   - ConfigModify  │    │   - NetworkHost  │
│                  │    │ arguments         │    │   - Tool         │
│                  │    │                   │    │   - Configuration│
│                  │    │                   │    │   - MemoryItem   │
└─────────────────┘    └──────────────────┘    └─────────────────┘
```

### 4.2 SecurityPolicyEngine（策略引擎）

**文件**：`engine.rs`

#### 4.2.1 Trait

```rust
#[async_trait]
pub trait SecurityPolicyEngine: Send + Sync {
    async fn evaluate(
        &self,
        subject: &Subject,
        action: &Action,
        resource: &Resource,
    ) -> Result<SecurityDecision, SecurityError>;

    fn register_guardian(&self, guardian: Box<dyn Guardian>) -> Result<(), SecurityError>;
    fn list_guardians(&self) -> Vec<String>;
}
```

#### 4.2.2 DefaultSecurityPolicyEngine

```rust
pub struct DefaultSecurityPolicyEngine {
    guardians: RwLock<Vec<Box<dyn Guardian>>>,  // 按 priority 降序排列
    default_decision: SecurityDecision,          // 无 Guardian 匹配时的默认决策
    config: SecurityPolicyConfig,
}
```

#### 4.2.3 evaluate() 流程

```
evaluate(subject, action, resource)
  │
  ├─ 1. 获取 guardians 列表（RwLock read）
  │
  ├─ 2. 按 priority 降序排列
  │
  ├─ 3. 遍历每个 Guardian（跳过 disabled）：
  │      │
  │      ├─ guardian.evaluate(s, a, r) → GuardResult
  │      │
  │      ├─ Deny → audit() + 立即返回 SecurityDecision::Deny
  │      ├─ Allow → audit() + 立即返回 SecurityDecision::Allow
  │      ├─ Guard → 累积 findings 到 guard_findings
  │      └─ Approve → 累积到 approve_decisions 列表
  │
  ├─ 4. 后处理（全部 Guardian 遍历完毕）：
  │      │
  │      ├─ approve_decisions 非空？
  │      │    └─ First 策略 → 取第一个
  │      │       Strictest 策略 → 取 timeout 最短的
  │      │    → audit() + 返回 SecurityDecision::Approve
  │      │
  │      ├─ guard_findings 非空？
  │      │    → audit() + 返回 SecurityDecision::Guard { findings }
  │      │
  │      └─ 否则 → audit() + 返回 default_decision
  │
  └─ 5. 审计日志：SystemEvent::SecurityDecided（含 session_id, tool_name, resource, decision）
```

#### 4.2.4 Guardian trait

```rust
pub trait Guardian: Send + Sync {
    fn name(&self) -> &str;
    fn priority(&self) -> i32;       // 越高越先执行
    fn enabled(&self) -> bool;
    async fn evaluate(
        &self,
        subject: &Subject,
        action: &Action,
        resource: &Resource,
    ) -> Option<GuardResult>;
}
```

- `evaluate()` 返回 `Option`：`None` = 该 Guardian 不适用于当前操作（跳过）
- `priority()` 越大越先执行（`sort_by_key(|g| -g.priority())`）

### 4.3 SecurityDecision（四级决策）

```rust
pub enum SecurityDecision {
    Allow,                           // 立即放行
    Deny { reason: String },         // 立即拒绝
    Guard { findings: Vec<GuardFinding> },  // 标记问题（不阻断，仅记录）
    Approve {                        // 需要用户审批
        timeout: Duration,
        prompt: String,
        findings: Vec<GuardFinding>,
    },
}
```

| 级别 | 行为 | 谁触发 |
|------|------|--------|
| `Deny` | 立即拒绝，返回错误 | Guardian 检测到明确危险 |
| `Allow` | 立即放行，跳过剩余 Guardian | Guardian 确认安全（如白名单匹配） |
| `Guard` | 不阻断，累积 findings 传给上层 | 多个 Guardian 标记了需要关注的问题 |
| `Approve` | 挂起等待用户确认（超时可配） | Guardian 认为需要人工判断 |

### 4.4 ApprovalService（审批服务）

**文件**：`approval.rs`

#### 4.4.1 架构

```rust
pub struct ApprovalService {
    pending: Arc<RwLock<HashMap<String, PendingApproval>>>,    // 待审批
    completed: Arc<RwLock<Vec<CompletedApproval>>>,            // 已完成
    config: ApprovalConfig,
}
```

#### 4.4.2 审批流程

```
request_approval(tool_name, prompt, timeout)
  │
  ├─ 1. 生成 UUID 审批 ID
  ├─ 2. 创建 oneshot channel (tx → PendingApproval, rx → ApprovalReceiver)
  ├─ 3. 检查 pending 数量 < max_pending（默认 200）
  ├─ 4. 存入 pending
  └─ 5. 返回 ApprovalReceiver

调用方持有 ApprovalReceiver:
  receiver.wait().await
    ├─ channel 收到响应 → Ok(AllowOnce / AllowAlways / Deny)
    ├─ sender 已 drop → ChannelClosed
    └─ 超时（timeout）→ Timeout

管理员/CLI 响应:
  ApprovalService::respond(approval_id, decision)
    ├─ 从 pending 移除
    ├─ 通过 oneshot tx 发送 decision
    └─ 记录到 completed 列表
```

#### 4.4.3 GC 机制

```rust
fn start_gc(&self) {
    tokio::spawn(async move {
        loop {
            interval.tick().await;  // 默认每 60 秒
            // 1. 清理超时的 pending
            // 2. 清理过期的 completed（> completed_max_age_secs，默认 3600s）
            // 3. 截断超量的 completed（> completed_max_count，默认 500）
        }
    });
}
```

#### 4.4.4 ApprovalConfig

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `default_timeout_secs` | 30 | 默认审批超时 |
| `max_pending` | 200 | 最大待审批数 |
| `gc_interval_secs` | 60 | GC 间隔 |
| `completed_max_count` | 500 | 最大已完成记录数 |
| `completed_max_age_secs` | 3600 | 已完成记录保留时间 |

### 4.5 SecurityPolicyConfig（安全策略配置）

```rust
pub struct SecurityPolicyConfig {
    pub default_decision: SecurityDecision,              // 默认 Deny（安全优先）
    pub user_confirmation_timeout_secs: u64,              // 用户确认超时（默认 30）
    pub approve_merge_strategy: ApproveMergeStrategy,     // First / Strictest
    pub guardian_configs: HashMap<String, GuardianConfig>, // 每个 Guardian 的独立配置
    pub audit_enabled: bool,                              // 审计开关（默认 true）
}
```

> **配置校验（红线对标）**：`validate()` 确保 `user_confirmation_timeout_secs != 0`，无效配置立即报错。

---

## 5. 协议合规性分析

### 5.1 Service 集成协议（protocol-Service集成协议）对标

#### 5.1.1 ServicePlugin 方法职责（协议 §1）

| 方法 | 调用次数 | 用途 | 当前状态 |
|------|---------|------|:---:|
| `name()` | 多次 | 返回全局唯一服务标识 `"security"` | ❌ 无 SecurityService |
| `init(ctx)` | 1 | 加载 SecurityPolicyConfig，注册默认 Guardian | ❌ |
| `start(ap)` | 1 | `ap.register_provider("security", engine)` | ❌ |
| `handle_signal(signal)` | 多次 | 响应运行时信号（见 5.1.2） | ❌ |
| `stop()` | 多次 | 暂停策略评估，已发出的审批仍有效 | ❌ |
| `shutdown()` | 1 | 反注册 Provider + 清理审批 pending 列表 | ❌ |

#### 5.1.2 运行时信号处理（协议 §3）

| 信号 | 说明 | 当前处理 | 合规 |
|------|------|:---:|:---:|
| `GracefulShutdown` | 正常关闭，拒绝所有新审批请求 | ❌ 无 | — |
| `ImmediateShutdown` | 强制关闭，立即清除所有 pending 审批 | ❌ 无 | — |
| `ConfigReload` | 重载 SecurityPolicyConfig，更新 Guardian 列表 | ❌ 无 | — |
| `HealthCheck` | 健康检查，需在 5s 内返回 `Ok(())`（红线 V-R01） | ❌ 无 | V-R01 ❌ |
| `Suspend` | 暂停策略评估，默认拒绝所有操作 | ❌ 无 | — |
| `Resume` | 恢复策略评估 | ❌ 无 | — |

#### 5.1.3 生命周期（协议 §5）

```
PluginLoader 读元数据 → 校验 provides/requires
→ init(ctx) → start(ap) ↔ [handle_signal() ...] → stop() → shutdown()
```

当前状态：**全部未实现**。`DefaultSecurityPolicyEngine` 和 `ApprovalService` 作为独立组件存在。

#### 5.1.4 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 ServicePlugin | 需实现 `ServicePlugin` trait | ❌ | 无 SecurityService（详见 5.1.1） |
| §2.1 ServiceAccessPoint | 通过 `get_config()` / `log()` 与 core 交互 | ❌ | 无 ServiceAccessPoint 注入 |
| §2.2 register_provider() | 注册 `security` Provider | ❌ | 无 Provider 注册 |
| §3 运行时信号 | 响应全部 6 个信号 | ❌ | 无 handle_signal()（详见 5.1.2） |
| §4 插件元数据 | YAML 声明 provides/requires/run_mode | ❌ | 元数据已设计（见 §7），未接入 PluginLoader |
| §5 生命周期 | init → start → stop → shutdown | ❌ | 无完整生命周期（详见 5.1.3） |
| §6 补充说明 | ServiceAccessPoint Clone、handle_signal<5s、Provider 自行鉴权 | ❌ | 待实现；Security 本身即为鉴权层 |
| §7 标准流程 | 8 步骤从零到运行 | ⚠️ | 步骤 1-4 已完成（engine/models/approval），步骤 5-8 待完成 |
| §8 V-R01 HealthCheck | 5s 内返回 `Ok(())` | ❌ | 无实现 |
| §8 V-R02 handle_signal 不阻塞 | 超 5s 须 spawn | ❌ | 无实现 |
| §8 V-R03 provides 一致 | 声明 = 实际注册 | ❌ | 无注册 |

### 5.2 模块内部组件协议（protocol-模块内部组件协议）对标

#### 5.2.1 依赖方向（协议 §6.2）

```
┌──────────────────────────────┐
│  模块 mod.rs                  │  （对外暴露约 15 个公共类型）⚠️ 超出协议建议
│  SecurityPolicyEngine         │
│  DefaultSecurityPolicyEngine  │
│  SecurityDecision / GuardResult│
│  ... (models 全部公开)        │
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────────────────────┐
│  组件（无 Orchestrator — Engine 自编排 Guardian 链）│
│                                              │
│  DefaultSecurityPolicyEngine                 │
│       │                                      │
│       ├── RwLock<Vec<Guardian>>              │
│       │    ├── PathTraversalGuardian (待实现) │
│       │    ├── ShellInjectionGuardian (待实现)│
│       │    ├── NetworkAccessGuardian (待实现) │
│       │    └── RuleBasedGuardian (待实现)     │
│       │                                      │
│       └── evaluate() 流程：                   │
│            按 priority 降序遍历 Guardian       │
│            Deny/Allow → 立即返回               │
│            Guard/Approve → 累积后合并           │
│                                              │
│  ApprovalService (独立)                       │
│       ├── pending: HashMap<id, Pending>       │
│       ├── completed: Vec<Completed>           │
│       └── GC: tokio::spawn 定期清理            │
│                                              │
│  ⚠️ Engine ↔ ApprovalService 直接引用         │
│  ⚠️ 应为: 通过 AccessPoint 间接通信            │
└──────────────────────────────────────────────┘
```

#### 5.2.2 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 Component | 实现 `Component` trait | ❌ | Engine 实现 SecurityPolicyEngine，非 Component |
| §3 AccessPoint | 通过 AP 间接通信 | ⚠️ | Engine ↔ ApprovalService 直接引用 |
| §5 Orchestrator | 编排器调度 Guardian 链 | ❌ | Engine 自身承担编排，未使用 Orchestrator |
| §6 模块边界 | mod.rs 只暴露入口+配置 | ⚠️ | 暴露约 15 个公共类型 |

### 5.3 跨平台与硬编码规范对标（协议 §4 完整 10 项自查清单）

| # | 检查项 | 合规 | 说明 |
|---|--------|:---:|------|
| 1 | 所有 URL 端点来自配置或常量，非字面量写死 | ✅ | 网络访问控制白名单从配置读取 |
| 2 | 所有模型名称来自配置字段，非硬编码 | ✅ | 不涉及 LLM 模型 |
| 3 | 所有超时值来自配置或 `DEFAULT_*` 常量 | ✅ | `user_confirmation_timeout_secs` / `gc_interval_secs` 可配 |
| 4 | API 版本号定义为模块级 `const`，不散落 | ✅ | 不涉及 API 版本号 |
| 5 | User-Agent 定义为 `const USER_AGENT` | ✅ | 不涉及 HTTP 请求 |
| 6 | 文件路径通过 `dirs` + `PathBuf::join()` 构建 | ✅ | Security 不直接操作文件路径 |
| 7 | 数字阈值默认 `None` 或从配置读取 | ✅ | `max_pending` / `completed_max_count` / `completed_max_age_secs` 从 ApprovalConfig 读取 |
| 8 | 平台特定指令通过 `OsKind` 枚举分支 | ✅ | 不涉及 shell 命令 |
| 9 | 测试中无 Unix-only 路径，均用 `std::env::temp_dir()` | ✅ | 测试无文件路径依赖 |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 全部通过 | 待验证 | — |

---

## 6. 红线与质量

| 编号 | 来源 | 红线 | 合规 |
|------|------|------|:---:|
| V-R01~V-R03 | Service集成协议 | — | ❌ 待补齐 |
| — | aagnet-lessons | 配置必须有校验 | ✅ `SecurityPolicyConfig::validate()` 校验超时非零 |
| — | aagnet-lessons | 错误信息含错误类型+描述+建议 | ⚠️ `SecurityError` 含类型和描述，但无解决建议 |
| — | aagnet-lessons | 异步操作必须有超时 | ✅ ApprovalService 有 timeout；GC 有独立间隔 |

---

## 7. 插件元数据

```yaml
name: security
category: service
version: 0.2.0
run_mode: background
provides:
  - security
requires:
  - tools
conflicts: []
config_schema:
  type: object
  properties:
    default_decision:
      type: string
      default: "Deny"
      description: 默认安全决策
    user_confirmation_timeout_secs:
      type: integer
      default: 30
    approve_merge_strategy:
      type: string
      enum: ["First", "Strictest"]
      default: "First"
    audit_enabled:
      type: boolean
      default: true
```

---

## 8. 设计决策

### 8.1 为什么 Default 是 Deny（默认拒绝）

**决策**：`default_decision = SecurityDecision::Deny`，无匹配 Guardian 时拒绝所有操作。

**理由**：
1. **安全优先**：零信任模型——不明确允许的操作一律拒绝
2. **显式授权**：迫使开发者为每个工具/资源显式配置 Guardian 规则
3. **防遗漏**：新增工具默认不可执行，必须经过安全审核

### 8.2 为什么 Guardian 优先级用降序

**决策**：Guardian 按 `priority` 降序排列，高优先级先执行。

**理由**：
1. **性能优化**：高优先级的拒绝 Guardian 先执行，可提前终止链（短路求值）
2. **语义清晰**：Critical > High > Medium > Low 对应的 priority 从高到低

### 8.3 为什么 Deny/Allow 立即终止而 Guard/Approve 不终止

**决策**：`Deny` 和 `Allow` 是**终结性决策**——立即返回不继续遍历；`Guard` 和 `Approve` 是**累积性决策**——继续遍历后续 Guardian。

**理由**：
1. **Deny 不可逆**：一个 Guardian 拒绝就足够了
2. **Allow 信任放行**：明确允许时无需检查更低的 Guardian
3. **Guard/Approve 需聚合**：多个 Guardian 可能各自发现不同问题，需要全部收集后统一处理

---

## 9. Guardian 接入待办

| Guardian | 文件 | 检测内容 | 实现要点 |
|----------|------|---------|---------|
| `PathTraversalGuardian` | `guardians/file_path.rs` | `..` / 绝对路径 / 符号链接 | 使用 `Path::canonicalize()` 后检查前缀 |
| `ShellInjectionGuardian` | `guardians/shell_injection.rs` | `;` `\|` `$()` `` ` `` `&` `&&` `\|\|` | 正则匹配后对 shell 上下文做语义分析 |
| `FilePermissionGuardian` | `guardians/file_path.rs` | 操作路径是否在允许目录内 | 读取配置的 `allowed_dirs` 白名单 |
| `NetworkAccessGuardian` | `guardians/network_access.rs` | 目标域名/IP 是否在白名单 | 读取配置的 `allowed_hosts` |
| `RuleBasedGuardian` | `guardians/rule_based.rs` | 用户自定义 JSON 规则 | 支持 `{subject, action, resource}` 条件匹配 |

---

## 10. 依赖关系

```
DefaultSecurityPolicyEngine ──→  Guardian trait (guardians/)
DefaultSecurityPolicyEngine ──→  EventLogger (core::logger)
ApprovalService             ──→  tokio::sync::oneshot
ApprovalService             ──→  uuid (审批 ID 生成)
```

- 对外依赖：`tokio::sync::RwLock` + `oneshot`（并发控制）、`uuid`（审批 ID）、`serde_json`（参数序列化）
- 框架层依赖：`core::logger::event`（审计日志）、`core::config::Validate`（配置校验）
|---|--------|:----:|
| 1 | URL 来自配置或常量 | ✅ |
| 6 | 路径用 `dirs` + `join()` | ✅ |
| 7 | 数字阈值从配置读取 | ✅ 不涉及 |
| 10 | build + test + clippy 通过 | 待验证 |

---

## 5. 红线

| 编号 | 红线 | 合规 |
|------|------|:----:|
| — | 外部输入必须校验 | ✅ 所有工具参数经 Guardian 链 |
| — | 安全策略拒绝不可绕过 | ✅ Guardian 链任一拒绝即阻止 |
| — | 不可在库代码中 unwrap | ✅ |

---

## 6. 设计决策

### 6.1 为什么用 Guardian 链模式

**决策**：多个 Guardian 组成可插拔的检查链。

**理由**：
1. **单一职责**：每个 Guardian 只检查一种攻击类型
2. **可扩展**：新增 Guardian 不影响现有逻辑
3. **可配置**：用户可在 config.toml 中启用/禁用特定 Guardian
4. **快速失败**：任一 Guardian 拒绝即停止后续检查

### 6.2 为什么在 ToolRegistry 层做安全检查

**决策**：安全检查在 `ToolRegistry::call()` 中执行，而不是在每个 `ToolContract::execute()` 中。

**理由**：
1. **统一入口**：所有工具调用必经 ToolRegistry
2. **零侵入**：工具实现者不需要关心安全
3. **不可绕过**：没有工具可以跳过安全检查

---

## 7. 文件结构

```
src/plugins/services/security/
├── mod.rs                  # SecurityService (impl ServicePlugin)
├── models.rs               # Subject / Action / Resource / SecurityDecision / SecurityError
├── engine.rs               # DefaultSecurityPolicyEngine
├── approval.rs             # 审批流程管理
└── guardians/
    ├── mod.rs              # Guardian trait + 注册
    ├── path_traversal.rs   # PathTraversalGuardian
    ├── command_injection.rs # CommandInjectionGuardian
    ├── file_permission.rs  # FilePermissionGuardian
    └── network_access.rs   # NetworkAccessGuardian
```

---

## 8. 插件元数据

```yaml
name: security
category: service
version: 0.2.0
run_mode: background
provides:
  - security
requires: []
conflicts: []
config_schema:
  type: object
  properties:
    enabled_guardians:
      type: array
      items: { type: string }
      default: ["path_traversal", "command_injection"]
    allowed_directories:
      type: array
      items: { type: string }
    network_whitelist:
      type: array
      items: { type: string }
```

---

## 9. 公开接口

```rust
// ── SecurityProvider ──
pub trait SecurityProvider: Send + Sync {
    async fn evaluate(&self, subject: &Subject, action: &Action, resource: &Resource)
        -> Result<SecurityDecision, SecurityError>;
}

// ── Guardian ──
pub trait Guardian: Send + Sync {
    fn name(&self) -> &str;
    fn category(&self) -> GuardCategory;
    async fn check(&self, subject: &Subject, action: &Action, resource: &Resource)
        -> GuardResult;
}

// ── SecurityDecision ──
pub enum SecurityDecision {
    Allow,
    Deny { reason: String, findings: Vec<GuardFinding> },
    ApprovalRequired { reason: String, approval_id: String },
}
```
