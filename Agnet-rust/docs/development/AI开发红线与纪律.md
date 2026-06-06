# AI 开发红线与纪律（AI Development Red Lines & Discipline）

> 适用范围：本项目的所有 AI 编码任务。
> 制定日期：2026-06-01。
> 读者：所有被授权修改 `src/` 的 AI 编码 Agent。

本文档是**所有集成开发计划的上位约束**。任何集成计划、任何子任务，凡是与本文档冲突的，**以本文档为准**。

集成计划清单：
- `docs/development/integration_plans/Skills→Assembler.md`
- `docs/development/integration_plans/MCP→ToolExecutor.md`
- `docs/development/integration_plans/L3→Assembler.md`

---

## 1. 四个外部协议（不可违反）

按权威性递减排序：

| # | 协议 | 文档 | 红线编号前缀 |
|---|------|------|------------|
| 1 | **Slot 接入协议** | `docs/protocol-Slot接入协议.md` | S-R0X |
| 2 | **Service 集成协议** | `docs/protocol-Service集成协议.md` | P-R0X, V-R0X |
| 3 | **shared_types 契约协议** | `docs/protocol-shared_types契约协议.md` | K-R0X, T-R0X, D-R0X |
| 4 | **模块内部组件协议** | `docs/protocol-模块内部组件协议.md` | C-R0X |

补充规范：
- `docs/跨平台与硬编码规范.md`（跨平台路径、禁止 `/tmp/`/`~`、禁止裸路径）
- `docs/开发插件完整流程.md`（含第八步 Provider 对接检查清单）

### 1.1 协议适用性

- **外部协议（1-3）是硬性约束**——所有 Plugin 必须严格遵守
- **模块内部组件协议（4）是有建议性的**——若模块有自己设计文档说"我们不用 Orchestrator"，则以该模块文档为准
- **C-R04 红线**（已注册组件必须被主入口调用）**对所有模块都生效**——这是 D-R04 的"硬核心"

### 1.2 完整红线清单（按协议）

| 协议 | 编号 | 内容 |
|------|------|------|
| shared_types | **K-R01** | 禁止 `register_provider()`/`provider_raw()` 用裸字符串——必须用 `shared_types` 中的 `PROVIDER_*` 常量 |
| shared_types | **K-R02** | 跨插件 key 必须先定义在 shared_types 再被引用 |
| shared_types | **T-R01** | Provider trait 禁止在 `services/*` 或 `slots/*` 内部定义——必须放 `shared_types` |
| shared_types | **T-R02** | 谁先开发谁负责把 trait 定义好放进 shared_types |
| shared_types | **T-R03** | Provider trait 不归属任何一方 |
| shared_types | **D-R01** | 禁止定义 `DynXxxProvider`——统一用 `shared_types::DynProvider<T>` |
| shared_types | **D-R02** | `DynProvider<T>` 只能在 `shared_types/mod.rs` |
| Service | **P-R01** | 禁止 `Arc::new(())` 当 Provider 注册（除非带 TODO 注释+明确计划） |
| Service | **P-R02** | 已注册 Provider key 应有至少一个消费者（否则是"幽灵 Provider"） |
| Service | **V-R01** | `handle_signal(HealthCheck)` 须在 5 秒内返回 `Ok(())` |
| Service | **V-R02** | `handle_signal` 不得阻塞超过 5 秒（长操作 `tokio::spawn`） |
| Service | **V-R03** | YAML 的 `provides` 必须与 `start()` 中的 `register_provider` 一致 |
| Slot | **S-R01** | 所有 `SlotDirective` 变体必须被正确处理 |
| Slot | **S-R02** | `init` 失败意味着插件不加载——不允许退化运行 |
| Slot | **S-R03** | `run()` 中禁止持有跨次调用的可变状态 |
| 内部组件 | **C-R01** | `AccessPoint::call()` 拿到 `ComponentHandle` 后必须 downcast |
| 内部组件 | **C-R02** | `meta().requires` 声明必须真实可验证 |
| 内部组件 | **C-R03** | `process()` 必须可重入 |
| 内部组件 | **C-R04** | **模块入口必须触发已注册组件**——仅注册到 Orchestrator 但不在主循环调用 = 未完成 |
| 全局 | 8 步清单 | `docs/开发插件完整流程.md` §8 的 5 项 Provider 对接检查 |

---

## 2. 6 个反模式（绝对禁止）

| 编号 | 反模式 | 典型症状 | 后果 |
|------|--------|----------|------|
| **A-01** | **乱开发**——超出文档范围自由发挥 | 给模块新增"我觉得应该有的"功能、新增文档没要求的接口、改动无关文件 | 文档与代码脱节，下次开发无据可依 |
| **A-02** | **偷懒**——在 `// TODO`/`unimplemented!()`/`Ok(())` 上蒙混 | 注册 `Arc::new(())`、`Ok(())` 当 HealthCheck、`executed += 1` 当占位 | 整个模块"看起来能跑"但实际空转 |
| **A-03** | **走捷径**——绕过协议 | 在 Service 内部定义 Provider trait、不用 `DynProvider<T>` 自己造 `DynXxxProvider`、直接 `use services::xxx::ConcreteType` | 编译能过但耦合度爆炸 |
| **A-04** | **幻觉**——写代码前没看文档/代码，凭"印象"写 | 编造不存在的 trait、编造不存在的配置字段、编造不存在的常量、引用不存在的函数 | 编译报错或运行时静默失败 |
| **A-05** | **偏离目标**——只完成文档要求的一部分 | 跳过集成验证、跳过测试、跳过文档同步、留 "TODO: 实际实现" | 任务表面完成实际未完成 |
| **A-06** | **伪完成**——不跑完整 CI 就提交 | 只跑 `cargo check` 就认为完成，实际 `cargo fmt`/`clippy`/`test` 未通过 | 反复返工，CI 红灯 |

---

## 3. 每个任务必须满足的 6 条最低要求

每个集成计划的子任务（如 S-1、M-3、T-V2）在执行时**必须**满足：

1. **读了相关文档和代码**（不是凭印象写）
2. **改动定位到文件:行**（不允许"在某处加一段"）
3. **遵守了对应的红线**（K-R01/T-R01/C-R04 等）
4. **跑过 `cargo check`**（零错误零警告）
5. **没引入新的 `field is never read`**（除非是预留字段且加 `#[allow(dead_code)]` + 注释）
6. **同步更新了对应文档**（如改动了 API，同步在 `docs/` 加注）
7. **跑过完整 CI 流程**——`cargo fmt --check` + `cargo clippy -- -D warnings` + `cargo test` 全部通过（仅 `cargo check` 不够）

---

## 4. 执行流程的"必读-必做-必查"三段式

### 4.1 必读（写代码前）

```
□ 读了对应模块的"严格 AI 开发计划"
□ 读了对应的 4 个协议中适用于本任务的红线
□ grep 过 src/ 确认要用的 trait/常量不存在（避免幻觉）
□ grep 过 src/ 确认要注册/查询的 PROVIDER_* 常量在 shared_types 已存在或确认要在本任务新建
```

### 4.2 必做（写代码时）

```
□ Provider key 一律用 shared_types 常量（K-R01）
□ Provider trait 一律定义在 shared_types（T-R01）
□ 跨 Plugin 类型传递用 DynProvider<T>（D-R01）
□ 不用 Arc::new(()) 占位（P-R01，例外：带 TODO 注释）
□ 不用 println!（统一用 tracing::info!/warn!/error!）
□ 不用 unwrap/expect 在生产路径（用 ? + PluginError）
□ 异步 I/O 用 tokio::fs / tokio::net（不用 std::fs）
□ 路径用 dirs::data_dir() + PathBuf::join()（跨平台与硬编码规范）
□ 改完跑 cargo fmt --check + cargo clippy -- -D warnings + cargo test 确认全通过
```

### 4.3 必查（写完代码后）

```
□ 跑 cargo fmt --check → 0 diff（格式统一）
□ 跑 cargo clippy -- -D warnings → 0 errors（lint 清洁）
□ 跑 cargo test → 无回归
□ 跑 rg "Arc::new\(\(\)\)" src/plugins/services/
  → 0 个匹配（除非带 TODO 注释+明确计划）
□ 跑 grep "register_provider\|provider_raw" src/plugins/ | grep '"[a-z_]\+"'
  → 0 个裸字符串（K-R01）
□ 跑 grep "pub trait" src/plugins/services/ src/plugins/slots/
  → 0 个 pub Provider trait（必须在 shared_types）
□ 跑 grep "DynTool\|DynMem\|DynSkill\|DynMcp\|DynVec" src/
  → 0 个自定义包装结构体（D-R01）
```

---

## 5. 文档与代码同步原则

```
文档说"X 应当 Y"  →  代码必须 Y
文档没说         →  可以在 issue 中讨论后补文档，再写代码
代码做了 Y       →  文档要同步加一条"X 应当 Y"
代码与文档冲突   →  视为 bug，文档优先
```

**例外**：当文档明确说过时（>3 个月），且当前发现文档与现实严重脱节：
1. 在对应 issue / `docs/问题根因分析与修复方案.md` 中记录"文档偏差"
2. 给出**要么改文档要么改代码**的方案
3. 等用户裁决，**不要在没授权的情况下自作主张改**

---

## 6. 已知偏差（截止 2026-06-01）

| 偏差 | 文档 | 现状 | 解决方式 |
|------|------|------|---------|
| `vector_db` feature flag 未实现 | `docs/services/memory/memory开发文档.md` §6.3 承诺可选 | Cargo.toml 无 `[features]`，l3_vector 总是编译 | 本次集成任务一并补 |
| Skills 文档的 `ContractRegistry` 与 Assembler 协议冲突 | `skills开发文档.md` §1.2 / §4.4.2 | Assembler 协议 §2 禁止新全局注册表 | 保留"ContractRegistry"作为服务内部命名，对外走 `PROVIDER_SKILLS` |
| ~~`McpToolProxy` 不实现 `ToolProvider` 而用"伪 TraitContract"~~ | ~~`mcp开发文档.md` §4.4~~ | ✅ 已修复 | 已改为实现 `shared_types` 中的 `ToolProvider` |
| **Pipeline 业务逻辑泄漏** | `protocol-Slot接入协议.md` §0 | ✅ 已修复 | "think"/"execute" hardcoded 代码已剥离到 `ThoughtSyncSlot` / `ObservationSyncSlot` |
| **Arc::new(()) 假 Provider** | `protocol-Service集成协议.md` §2.2 | ✅ 已修复 | 所有 register_provider 使用真实实现 |
| **K-R01 裸字符串违规** | `protocol-shared_types契约协议.md` §2.2 | ✅ 已修复 | llm_thinker_slot 使用 PROVIDER_SESSION_CONTEXT 常量 |

---

## 7. 与用户交互的纪律

1. **不确认不写代码**——执行计划前必须得到用户对计划的确认
2. **不省略不汇报**——每个 Phase 完成后必须汇报 `cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test` 结果
3. **不背锅不撒谎**——遇到红线冲突、文档偏差、未识别的依赖，**先停下来汇报**，不要默默绕过
4. **不超纲不扩散**——严格按计划任务清单执行，**不做"顺手优化"**——除非单独开 issue 让用户决定

---

## 8. 遇到这些情况立即停手汇报

| 情况 | 动作 |
|------|------|
| 文档说 A 但代码现状是 B，A 和 B 都不合理 | 停手，列出 A/B 和建议方案 |
| 需要修改其他不相关的文件（如 Cargo.toml 全局依赖） | 停手，问用户 |
| 发现 4 协议中某条红线被多个模块违反 | 停手，列清单问用户统一处理 |
| 任务清单遗漏了某个必需步骤 | 停手，把遗漏步骤加进计划并通知用户 |
| 跑完整 CI 发现 `cargo fmt --check` / `cargo clippy -- -D warnings` / `cargo test` 中任一失败 | **停手，修复后重跑** |

---

**本纪律无"特殊豁免"**——任何"这次赶时间"、"反正只是测试"、"内部用无所谓"的理由都不接受。
