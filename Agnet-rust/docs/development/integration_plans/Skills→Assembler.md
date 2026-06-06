# Skills → Assembler 集成开发计划

> 上位约束：`docs/development/AI开发红线与纪律.md`
> 现有计划：`docs/services/skills/Skills 严格 AI 开发计划.md`（**不替代**——本计划是它的"集成补全"）
> 集成方向：Skills Service → Assembler Slot（via `PROVIDER_SKILLS`）
> 计划日期：2026-06-01
> 适用代码版本：aagnet v0.1.0

---

## 0. 目标

把 Skills Service 从"内部工具（不暴露）"升级为"通过 `PROVIDER_SKILLS` 对外暴露"，并让 Assembler 的 ContextProvider 列表加入 `skills` 一项，使 LLM 在拼装 System Prompt 时能拿到技能列表。

**完成定义**：
1. `cargo check` 0 errors, 0 new warnings
2. `cargo test` 通过
3. 跑 4 协议 grep 守卫（见总纲 §4.3）全部 0 匹配（裸字符串/未注册 trait/自定义 Dyn 包装）
4. 在 `resources/skills/test.skill.md` 创建一个测试技能，跑 E2E 验证 Assembler 输出包含该技能
5. Assembler 的 `disabled_providers` 加入 `"skills"` 后，E2E 输出不包含该技能

---

## 1. 协议与红线引用

| 红线 | 来源 | 本计划如何遵守 |
|------|------|---------------|
| **K-R01** | shared_types §2 | 注册/查询都用 `PROVIDER_SKILLS` 常量，不用裸字符串 `"skills"` |
| **K-R02** | shared_types §2 | `PROVIDER_SKILLS` 在 `shared_types/skills.rs` 中先定义，再被 Skills 与 Assembler 引用 |
| **T-R01** | shared_types §3 | `SkillContract` 和 `SkillsContractBundle` 都在 `shared_types/skills.rs`，不放在 `services/skills/` 内部 |
| **T-R02** | shared_types §3 | 本计划第一阶段就定义 trait（不让消费方去猜） |
| **D-R01** | shared_types §4 | 用现有的 `DynProvider<T>`，不造 `DynSkillProvider` |
| **P-R01** | Service §6 | 不留 `Arc::new(())` 占位（除非带详细 TODO 注释） |
| **P-R02** | Service §6 | 至少有一个消费者（Assembler） |
| **V-R01** | Service §8 | `HealthCheck` 在 5s 内 `tokio::time::timeout(5s, ...)` |
| **V-R02** | Service §8 | `ConfigReload` 重扫放 `tokio::spawn` |
| **V-R03** | Service §8 | 插件 metadata YAML 的 `provides` 与 `start()` 注册一致 |
| **C-R04** | 内部组件 | 若启用 Orchestrator 模式，主循环必须触发 `process_all()` |
| 跨平台 | `docs/跨平台与硬编码规范.md` | `skills_dir` 默认 `dirs::data_dir() + join("potoobird/skills")`；相对路径用 `data_dir` 锚定，不用 `current_dir` |

---

## 2. 架构决策（已与用户拍板）

### 2.1 关于 `ContractRegistry`

**Skills 文档**（`docs/services/skills/skills开发文档.md` §1.2/§4.4.2）描述了一个**全局 `ContractRegistry`** 来注册和查询 `SkillContract`。

**Assembler 协议**（`docs/protocol-shared_types契约协议.md` §6 + `ConversationAssembler-开发设计文档.md` §2）**明确禁止**引入新的全局注册表，要求跨插件查询走 `provider_raw()`。

**决策**：保留 `ContractRegistry` 这个**名字**作为 Skills 服务内部的数据结构（Vec/RwLock<HashMap>），但**对外只通过 `ProviderRegistry` + `PROVIDER_SKILLS` 暴露**。

理由：
- 不破坏 Skills 文档的术语一致性
- 满足 Assembler 协议的红线
- 数据仍然在 Skills 服务内部，Assembler 拿到的 `Arc<DynProvider<dyn SkillsContractBundle>>` 持有同一份引用

### 2.2 关于 `SkillInjectionProvider` 现有 API

**现状**：`SkillInjectionProvider`（`src/plugins/services/skills/provider.rs:7-52`）有 `select_skills()` 和 `format_injection()` 方法，**不是** Assembler 的 `ContextProvider` trait 实现。

**决策**：**重写** `SkillInjectionProvider` 让它实现 `shared_types::assembler::ContextProvider` trait，并把 `select_skills/format_injection` 作为私有方法（如果是 `provide()` 调用的辅助方法则保留，否则删除）。

### 2.3 关于 Provider 形状

**现状**：Assembler 消费者需要从 `ap.provider_raw(PROVIDER_SKILLS)` 拿到一个能列举技能的句柄。

**决策**：
- 在 `shared_types/skills.rs` 定义 **`SkillsContractBundle`** trait：`fn all(&self) -> Vec<Arc<dyn SkillContract>>`
- `SkillsService::start()` 注册 `Arc<DynProvider<dyn SkillsContractBundle>>`
- Assembler 侧的 `SkillsProvider` `provide()` 时 `downcast` 到 `DynProvider<dyn SkillsContractBundle>`，调 `.all()` 拿到所有技能

---

## 3. 任务清单

### Phase A：定义契约（必须先做，3 个任务）

#### A-1. 在 `shared_types` 新建 `skills.rs`

**文件**：新建 `src/shared_types/skills.rs`，在 `src/shared_types/mod.rs:23-46` 加 `pub mod skills;` + re-export。

**内容**（必须完整包含）：
- `pub const PROVIDER_SKILLS: &str = "skills";`（K-R01）
- `pub enum InjectionPolicy { Auto, Always, Never }` + `Default`（Auto）
- `pub enum QuotaPreference { Full, Summary, TitleOnly }` + `Default`（Full）
- `pub trait SkillContract: Send + Sync`：10 个方法
  - `fn name(&self) -> &str`
  - `fn version(&self) -> &str`
  - `fn description(&self) -> &str`
  - `fn group(&self) -> &str`（默认 `""`）
  - `fn tags(&self) -> &[String]`
  - `fn dependencies(&self) -> &[String]`（默认 `&[]`）
  - `fn injection_policy(&self) -> InjectionPolicy`（默认 Auto）
  - `fn quota_preference(&self) -> QuotaPreference`（默认 Full）
  - `fn get_content(&self, level: SkillLevel) -> String`（**返回 owned String**，解决 S-4 引用问题）
  - `fn match_score(&self, context: &str) -> f64`
- `pub enum SkillLevel { TitleOnly, Summary, KeyPoints, Full }`（保留，与 FileSkill 共用——`FileSkill::SkillLevel` 移到 shared_types？或保持重复？**决策：保留两套 enum，加 `From` 转换**）
- `pub trait SkillsContractBundle: Send + Sync`：`fn all(&self) -> Vec<Arc<dyn SkillContract>>`
- `pub struct SkillContent`（可选用作 `get_content` 返回值）

**禁止**：
- ❌ 不要在 `services/skills/` 下定义这些类型（T-R01）
- ❌ 不要造 `DynSkillContract` / `DynSkillsBundle`（D-R01）
- ❌ 不要在 enum 上加 `Serialize`（Provider trait 不需要）

**验证**：编译通过；`rg "pub trait Skills\|pub const PROVIDER_SKILLS" src/shared_types/skills.rs` 命中 1 个 trait + 1 个常量。

#### A-2. 修改 `shared_types/mod.rs`

**文件**：`src/shared_types/mod.rs:23-46`

**操作**：
- 在 `pub mod tool;` 行附近加 `pub mod skills;`（放在 `pub mod memory;` 后面）
- 在末尾 re-export 加 `pub use skills::{PROVIDER_SKILLS, SkillContract, SkillsContractBundle, SkillLevel, InjectionPolicy, QuotaPreference};`

**验证**：`cargo check` 0 errors。

#### A-3. 修改 `services/skills/file_skill.rs`：让 `FileSkill` 完整实现 `SkillContract`

**文件**：`src/plugins/services/skills/file_skill.rs:1-119`

**操作**：
- `SkillFrontmatter` 扩展字段：
  - `title: String`（必填，frontmatter 缺则 `parse_frontmatter` 报错）
  - `injection_policy: InjectionPolicy`
  - `quota_preference: QuotaPreference`
  - `dependencies: Vec<String>`
  - `summary: Option<String>`（独立于 body 中的 TL;DR）
- `parse_frontmatter` (line 44-75) 增加上述字段的解析 + 默认值
- `FileSkill::SkillLevel` **保留不变**（避免大改），但 `provider.rs` 用 `From<FileSkill::SkillLevel> for shared_types::SkillLevel`
- `impl SkillContract for FileSkill`（新 impl 块）：
  - 9 个方法直接读 frontmatter 字段
  - `get_content` 返回 `String`（不再是 `&str`）：
    - `TitleOnly` → `self.frontmatter.title.clone()`
    - `Summary` → `self.tldr.clone()`
    - `KeyPoints` → `self.key_points.iter().map(|kp| format!("- {kp}")).collect::<Vec<_>>().join("\n")`
    - `Full` → `self.full_content.clone()`
  - `match_score` 调用 `self.matcher.compute_score(...)`（**注意**：当前 `matcher` 是 `SkillInjectionProvider` 的字段，**需要把 `SkillMatcher` 移到 `FileSkill` 里**——见 A-4）

**禁止**：
- ❌ 不要改 `SkillLevel` 的命名（破坏性变更）
- ❌ 不要在 `get_content` 里继续返回 `&str`（解决 S-4 引用问题）

**验证**：
- `cargo check` 0 errors
- 跑 `rg "impl SkillContract for FileSkill" src/` 命中 1 行

#### A-4. 把 `SkillMatcher` 移到 `FileSkill` 内部

**文件**：`src/plugins/services/skills/file_skill.rs`（追加），`src/plugins/services/skills/provider.rs`（删除/重写）

**操作**：
- `FileSkill` 字段加 `matcher: SkillMatcher`（line 18-24）
- `FileSkill::new()` 初始化 matcher 并 `add_document(self.name(), "{description} {tldr}")`
- `FileSkill::match_score` 内部调 `self.matcher.compute_score(self.name(), context, &self.tags, &self.description, &self.tldr)`
- `provider.rs` 的 `SkillInjectionProvider` **不再** 拥有 `matcher` 字段，删掉
- `SkillMatcher::add_document/remove_document` 不需要再对外暴露

**理由**：
- 文档 §4.3.4/§8.4 要求 matcher cache 跨调用共享——放 `FileSkill` 里天然共享（每个技能有自己的 cache）
- 简化调用链：`file_skill.match_score(context)` 一行搞定

**验证**：
- 现有 `SkillMatcher` 测试不破坏
- 跑 `rg "SkillMatcher" src/` 只在 `file_skill.rs` 和 `matcher.rs` 出现

#### A-5. 扩展 `parse_frontmatter` 支持 5 个新字段

**文件**：`src/plugins/services/skills/file_skill.rs:44-75`

**操作**：
- 解析 `title:` → `frontmatter.title`（必填，空字符串返回 `Err("frontmatter 缺少必填字段 title")`）
- 解析 `injection_policy:` → 通过 `FromStr` 转 `InjectionPolicy`（无法识别返回 `Auto` + warn）
- 解析 `quota_preference:` → 同上
- 解析 `dependencies:` → 逗号分隔的 `Vec<String>`
- 解析 `summary:` → `Option<String>`

**验证**：
- 加 1 个新测试 `parse_frontmatter_full`：写一段完整 frontmatter 文本，确认 5 个新字段正确解析
- 加 1 个新测试 `parse_frontmatter_missing_title`：确认返回 Err

---

### Phase B：Skills 服务侧自完成（8 个任务）

#### B-1. `SkillConfig::resolve_paths` 改用 `data_dir` 锚定

**文件**：`src/plugins/services/skills/config.rs:27-33`

**操作**：
- `resolve_paths(&mut self, data_dir: &Path)` 增加参数
- 相对路径用 `data_dir.join(&self.skills_dir)` 而不是 `std::env::current_dir()`
- 删除 `std::env::current_dir()` 调用（C-1 跨平台规范 + 修 S-13）
- 调用方（`SkillsService::init`）改为 `config.resolve_paths(&ctx.data_dir)`

**验证**：
- 跑 `rg "current_dir" src/plugins/services/skills/` 命中 0
- `cargo check` 0 errors

#### B-2. `SkillsService::init` 异步化 I/O

**文件**：`src/plugins/services/skills/service.rs:31-39, 89-105`

**操作**：
- `scan_skills` 改用 `tokio::fs::read_dir`
- `init` 中的 `unwrap_or_default()` 改成 `?` + 映射 `PluginError::Config`（修复 S-11 错误吞掉）
- 启动时记日志：成功/失败/技能数量

**验证**：
- 跑 `rg "unwrap_or_default" src/plugins/services/skills/service.rs` 命中 0（除注释）
- `cargo check` 0 errors

#### B-3. `SkillsService::init` 文件名过滤改 `.skill.md`

**文件**：`src/plugins/services/skills/service.rs:96`

**操作**：
- `path.extension().map(|e| e == "md")` 改为 `path.file_name().to_string_lossy().ends_with(".skill.md")`
- 启动时记日志：扫到 N 个 .md，其中 M 个 .skill.md

**验证**：
- 创建 `resources/skills/test.skill.md`（B-8）+ 创建一个 `not_a_skill.md`（应被过滤）
- 跑测试：扫描后只 1 个技能

#### B-4. `SkillsService::start` 注册真 `Arc<DynProvider<dyn SkillsContractBundle>>`

**文件**：`src/plugins/services/skills/service.rs:41-57`

**操作**：
- 删除 `Arc::new(())`（违反 P-R01）
- 新建 `inner` 时构造 `SkillsContractBundleImpl { skills: inner.skills.clone() }`（用 `Arc<Vec<Arc<FileSkill>>>` 持有）
- 构造 `let bundle: Arc<dyn SkillsContractBundle> = Arc::new(bundle);`
- 调用 `ap.register_provider(PROVIDER_SKILLS, Arc::new(DynProvider(bundle)));`
- 删除 47-54 行的 TODO 注释块

**禁止**：
- ❌ 不要用裸字符串 `"skills"`（K-R01）—— import `PROVIDER_SKILLS`
- ❌ 不要 `Arc::new(())`（P-R01）
- ❌ 不要忘记 `use crate::shared_types::{DynProvider, PROVIDER_SKILLS, SkillsContractBundle};`

**验证**：
- `rg "register_provider" src/plugins/services/skills/service.rs` 应只 1 处调用，使用 `PROVIDER_SKILLS`
- `rg '"skills"' src/plugins/services/skills/service.rs` 命中 0

#### B-5. `SkillsService::handle_signal(HealthCheck)` 真检查

**文件**：`src/plugins/services/skills/service.rs:74`

**操作**：
- 改 `ServiceSignal::HealthCheck => return Ok(())` 为：
  ```rust
  ServiceSignal::HealthCheck => {
      let dir = inner.config.skills_dir.clone();
      match tokio::time::timeout(std::time::Duration::from_secs(5), tokio::fs::read_dir(&dir)).await {
          Ok(Ok(_)) => return Ok(()),
          Ok(Err(e)) => tracing::warn!("Skills HealthCheck: 读目录失败: {}", e),
          Err(_) => tracing::warn!("Skills HealthCheck: 5s 超时"),
      }
      return Ok(());
  }
  ```
- 满足 V-R01（5s 内返回）和 V-R02（不阻塞）

**验证**：
- 新测试：mock 一个不存在的目录，确认 5s 内返回 Ok
- 新测试：mock 一个超时场景，确认仍 5s 内返回

#### B-6. `SkillsService::handle_signal(ConfigReload)` 异步 + 增量

**文件**：`src/plugins/services/skills/service.rs:66-71`

**操作**：
- 不要在 `handle_signal` 同步重扫（违反 V-R02）
- 改为 `tokio::spawn` 异步执行：
  ```rust
  ServiceSignal::ConfigReload => {
      let inner_clone = self.inner.clone();
      tokio::spawn(async move {
          let mut guard = inner_clone.write().await;
          if let Some(inner) = guard.as_mut() {
              let new_skills = Self::scan_skills(&inner.config.skills_dir).await;
              // ... 增量更新 inner.skills 和 inner.provider
          }
      });
  }
  ```
- 增量更新：计算新/旧 name 集合的 diff，`remove_document` + `add_document`

**验证**：
- `cargo test` 通过
- `rg "ServiceSignal::ConfigReload" src/plugins/services/skills/service.rs` 命中 1 处且使用 spawn

#### B-7. `SkillsService::shutdown` 反注册 Provider

**文件**：`src/plugins/services/skills/service.rs:83-86`

**操作**：
- `shutdown` 需要 `ap: ServiceAccessPoint` 参数——**等等**：`ServicePlugin::shutdown(&mut self)` 当前签名只取 `&mut self`！
- **停下来汇报**——这个发现违反现有协议假设。需检查 `ServicePlugin::shutdown` 真实签名。

**⚠️ 阻塞**：在确认 `ServicePlugin::shutdown` 真实签名（是否能传 ap）之前，**不要写** shutdown 反注册代码。可能的方案：
- (A) 修改 `ServicePlugin::shutdown` 签名加 `ap: &ServiceAccessPoint`（破坏性变更）
- (B) 把"反注册"放到 `stop()` 里（stop 当前签名如何？）
- (C) 让 Runtime 在 `shutdown` 之前自己 unregister all

**汇报给用户前不写代码**——这违反"反模式 A-05 不背锅不撒谎"。

#### B-8. 创建 `resources/skills/test.skill.md`

**文件**：新建 `resources/skills/test.skill.md`

**内容**（最小可工作测试用例）：
```yaml
---
name: test_skill
title: Test Skill
description: A skill for E2E testing
tags: [test, e2e]
version: 1.0.0
injection_policy: Auto
quota_preference: Full
dependencies: []
---

## TL;DR
A test skill that verifies the skills → assembler integration.

## Key Points
- Tests provider registration
- Tests query flow
- Tests content formatting

## Full Content
This is the full content used for SkillLevel::Full.
```

**验证**：
- `file_skill.rs` 解析测试能加载这个文件
- E2E 测试能拿到这个技能的 title

---

### Phase C：Assembler 消费侧（7 个任务）

#### C-1. 新建 `assembler/providers/skills.rs`

**文件**：新建 `src/plugins/slots/assembler/providers/skills.rs`（约 80 行）

**内容**：
```rust
/*! SkillsProvider（设计文档 §5.7, pri=30）
从 provider_raw(PROVIDER_SKILLS) 获取 SkillsContractBundle。
提供技能注入到 System Prompt。
*/

use async_trait::async_trait;
use std::sync::Arc;
use crate::core::access::SlotAccessPoint;
use crate::shared_types::assembler::*;
use crate::shared_types::{DynProvider, MessageRole, PROVIDER_SKILLS, SkillsContractBundle};
use crate::plugins::services::skills::SkillInjectionProvider;  // ❌ 不允许！见下

pub struct SkillsProvider;

#[async_trait]
impl ContextProvider for SkillsProvider {
    fn name(&self) -> &str { "skills" }
    fn priority(&self) -> u8 { 30 }
    fn allow_truncation(&self) -> bool { true }
    fn silent_on_empty(&self) -> bool { true }
    fn estimate_max_tokens(&self, config: &ProviderSlotConfig) -> usize {
        config.max_tokens
    }

    async fn provide(
        &self,
        ap: &dyn SlotAccessPoint,
        quota: &ContextQuota,
        _config: &ProviderSlotConfig,
    ) -> Result<ProvidedContext, ProviderError> {
        let bundle = match ap.provider_raw(PROVIDER_SKILLS) {
            Some(raw) => match raw.downcast::<DynProvider<dyn SkillsContractBundle>>() {
                Ok(wrapper) => wrapper.0.clone(),
                Err(_) => return Ok(ProvidedContext { blocks: vec![], tokens_used: 0 }),
            },
            None => return Ok(ProvidedContext { blocks: vec![], tokens_used: 0 }),
        };

        let skills = bundle.all();
        if skills.is_empty() {
            return Ok(ProvidedContext { blocks: vec![], tokens_used: 0 });
        }

        let messages = ap.messages();
        let context: String = messages.iter().rev()
            .filter(|m| m.role == MessageRole::User)
            .take(5)
            .map(|m| m.text_content())
            .collect::<Vec<_>>()
            .join(" ");

        if context.is_empty() {
            return Ok(ProvidedContext { blocks: vec![], tokens_used: 0 });
        }

        // 评分
        let mut scored: Vec<_> = skills.iter().map(|s| {
            let score = s.match_score(&context);
            (s, score)
        }).collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.retain(|(_, s)| *s > 0.0);

        if scored.is_empty() {
            return Ok(ProvidedContext { blocks: vec![], tokens_used: 0 });
        }

        let max_skills = quota.max_items.min(scored.len());
        let mut blocks = Vec::new();
        let mut total_tokens = 0usize;

        for (skill, _score) in scored.iter().take(max_skills) {
            let level = if quota.max_tokens > 0 { /* select_level */ } else { /* ... */ };
            let content = skill.get_content(/* level */);
            let tokens = (content.len() as f64 / 4.0).ceil() as usize;
            if total_tokens + tokens > quota.max_tokens { break; }
            total_tokens += tokens;
            blocks.push(ContextBlock {
                section_title: format!("## Skill: {}", skill.name()),
                content,
                source: format!("skill/{}", skill.name()),
                token_count: tokens,
            });
        }

        Ok(ProvidedContext { blocks, tokens_used: total_tokens })
    }
}
```

**⚠️ 阻塞 1**：注释行 `use crate::plugins::services::skills::SkillInjectionProvider;` 是错的——`SkillInjectionProvider` **不应该**在 Assembler 侧使用（违反 K-R01/T-R01 的精神："不让 Slot 知道 Service 的内部类型"）。

**修正方案**：
- 把 `select_level` 算法做成 `SkillsContractBundle` trait 的默认方法或新增一个 `SkillsSelector` 结构体
- 或者：`SkillInjectionProvider` 的算法逻辑移到 `shared_types/skills.rs` 的默认方法里
- **或者最简单**：在 Assembler 侧不调 `SkillInjectionProvider`，直接用 `SkillContract::get_content` + 简单的 rank-based level 选择（`max_tokens` 多用 Full，少用 Summary，最少用 TitleOnly）

**汇报给用户前不写代码**。

**验证**（写完后）：
- `cargo check` 0 errors
- 跑 `rg "use.*plugins::services::skills" src/plugins/slots/assembler/providers/skills.rs` 命中 0（不允许直接引用 Service 内部类型）

#### C-2. 注册 `SkillsProvider` 到 `assembler/providers/mod.rs`

**文件**：`src/plugins/slots/assembler/providers/mod.rs:3-18, 30-40`

**操作**：
- 加 `mod skills;`
- 加 `pub use skills::SkillsProvider;`
- 在 `provider_map` 加 `("skills", Arc::new(SkillsProvider) as Arc<dyn ContextProvider>)`

**验证**：
- `cargo check` 0 errors
- `rg "SkillsProvider" src/plugins/slots/assembler/providers/mod.rs` 命中 1 处

#### C-3. `injection_order` 默认加 `"skills"`

**文件**：`src/shared_types/assembler/config.rs:89-95`

**操作**：
- `injection_order` vec 加 `"skills".into()`（位置：在 `"vector_memory"` 之后，pri=30 同级）

**验证**：
- `cargo test` 通过 `quota.rs` 现有 7 个测试

#### C-4. `providers` 默认配置加 `"skills"` 一项

**文件**：`src/shared_types/assembler/config.rs:73-77`

**操作**：
- 在 `providers.insert("vector_memory".into(), ...)` 之后加 `providers.insert("skills".into(), ProviderSlotConfig { max_tokens: 3000, max_items: 5, max_chars_per_item: 1500, min_guaranteed_tokens: 0, allow_compaction: true, allow_truncation: true, ..Default::default() })`

**验证**：
- `cargo test` 通过

#### C-5. `quota.rs` 5 策略加 `"skills"` 配额

**文件**：`src/plugins/slots/assembler/assembly/quota.rs:12-28`

**操作**：
- 5 个策略的 ratios 都加 `("skills", X)`：
  - `balanced` → 0.05（建议值）
  - `memory_focused` → 0.05
  - `token_efficient` → 0.10
  - `identity_only` → 0（不加）
  - `minimal` → HashMap 已空，不动
- 注意：现有 4 个策略的 sum 已经是 0.95 / 0.95 / 0.75 / 0.90，加 skills 后**必须保证总和不超 1.0**：
  - `balanced`: 0.10 + 0.15 + 0.40 + 0.30 + 0.05 = **1.00** ✅
  - `memory_focused`: 0.05 + 0.10 + 0.55 + 0.25 + 0.05 = **1.00** ✅
  - `token_efficient`: 0.15 + 0.15 + 0.30 + 0.15 + 0.10 = **0.85**（保留 0.15 buffer）
  - `identity_only`: 0.90 + 0 = 0.90
  - `minimal`: 空

**调整**：
- 若 sum > 1.0，需要把其他比例下调（如 `vector_memory` 0.30 → 0.25）

**验证**：
- 现有 7 个 quota 测试不破坏
- 加新测试 `test_allocate_balanced_with_skills`：确认 5 个 key 都存在且 `skills.max_tokens > 0`
- 加新测试 `test_allocate_balanced_sum_not_exceed_budget`：跑 10000 token budget，确认每个 quota 不超 max_tokens

---

### Phase D：测试 + 收尾（5 个任务）

#### D-1. `FileSkill` 单元测试

**文件**：`src/plugins/services/skills/file_skill.rs`（追加 `#[cfg(test)] mod tests`）

**测试用例**：
- `parse_frontmatter_minimal`：`{name: "x", title: "X", description: "Y"}` → OK
- `parse_frontmatter_missing_title` → Err
- `parse_frontmatter_missing_name` → Err
- `parse_frontmatter_with_tags_array`：tags: [a, b, c] → 3 个
- `parse_frontmatter_with_injection_policy_always` → InjectionPolicy::Always
- `get_content_all_levels`：4 个 level 都返回非空
- `match_score_perfect_match`：query 完全匹配技能名 → score > 0.5
- `match_score_no_match`：query 完全不相关 → score < 0.1

#### D-2. `SkillsService` 生命周期测试

**文件**：`src/plugins/services/skills/service.rs`（追加 `#[cfg(test)] mod tests`）

**测试用例**：
- `init_scans_skills_dir`：临时目录放 1 个 `.skill.md`，init 后 `inner.skills.len() == 1`
- `init_skips_non_skill_md`：临时目录放 1 个 `.md` + 1 个 `.skill.md`，init 后只 1 个
- `init_propagates_config_error`：plugin_config 是无效 JSON → Err
- `start_registers_provider`：init + start 后，`ap` 应收到 `register_provider(PROVIDER_SKILLS, ...)` 调用（用 mock `ServiceAccessPoint`）
- `handle_signal_healthcheck_returns_within_5s`：mock 一个 10000 个文件的目录，确认 5s 内返回
- `handle_signal_configreload_spawns`：信号触发后 `tokio::spawn` 启动（用 mock 跟踪）

#### D-3. `SkillsProvider` 单元测试

**文件**：`src/plugins/slots/assembler/providers/skills.rs`（追加 tests）

**测试用例**：
- `provide_returns_empty_when_no_provider`：`ap.provider_raw(PROVIDER_SKILLS) == None` → 0 blocks
- `provide_returns_empty_when_downcast_fails`：mock 一个不匹配类型 → 0 blocks
- `provide_returns_blocks_with_real_bundle`：构造 mock bundle 含 2 个技能，context 匹配其中一个 → 1 block
- `provide_respects_quota_max_items`：quota.max_items=1，bundle 含 5 个 → 只 1 block
- `provide_respects_quota_max_tokens`：超出 max_tokens 时截断

#### D-4. 端到端集成测试

**文件**：新建 `tests/integration_skills_assembler.rs`（如不存在）

**测试用例**：
- 启动完整 aagnet runtime，启用 skills 服务，配置 `assembler.disabled_providers` 不含 skills
- 跑一个用户消息 "I want to test the skill system"
- 检查 `assembler_messages` 中包含 `## Skill: test_skill` 或 `## Available Skills`
- 再跑一次，`assembler.disabled_providers` 加 `"skills"`，确认不包含

#### D-5. 4 协议 grep 守卫

**操作**：跑以下命令，全部 0 匹配：

```bash
# K-R01: 裸字符串
rg '"[a-z_]+"' src/plugins/services/skills/ src/plugins/slots/assembler/providers/skills.rs \
  | rg "register_provider|provider_raw" || echo "✅ K-R01 OK"

# T-R01: 内部 Provider trait
rg "pub trait.*Provider" src/plugins/services/skills/ src/plugins/slots/assembler/providers/ || echo "✅ T-R01 OK"

# D-R01: 自定义包装
rg "DynSkill" src/ || echo "✅ D-R01 OK"

# P-R01: Arc::new(()) 占位
rg "Arc::new\(\(\)\)" src/plugins/services/skills/ || echo "✅ P-R01 OK"

# V-R03: YAML provides 与 register 一致
# （如无 YAML，标记为 N/A）
```

---

## 4. 任务依赖图

```
A-1 ──┬── A-2 (改 mod.rs)
      └── A-3 (impl SkillContract for FileSkill)
              └── A-4 (SkillMatcher 移到 FileSkill)
                      └── A-5 (parse_frontmatter 扩展)
                              └── B-1..B-3 (配置 + init)
                                      └── B-4 (start 注册)
                                              └── B-5 (HealthCheck)
                                              └── B-6 (ConfigReload)
                                              └── B-7 (shutdown) ← ⚠️ 阻塞
                                              └── B-8 (测试技能)
                                                      └── C-1 (SkillsProvider)
                                                              ├── C-2..C-4 (注册 + 配置)
                                                              │       └── C-5 (quota)
                                                              │               └── D-1..D-3 (测试)
                                                              │                       └── D-4 (E2E)
                                                              │                               └── D-5 (grep 守卫)
```

## 5. 汇报节奏

| Phase 完成 | 汇报内容 |
|-----------|---------|
| Phase A 完 | `cargo check` 输出、新增的 shared_types 列表 |
| Phase B 完 | `cargo check` 输出、`cargo test` 输出、新警告数 |
| Phase C 完 | `cargo check` 输出、quota.rs 测试通过情况 |
| Phase D 完 | `cargo test` 全过、D-5 grep 守卫结果、E2E 输出 |

## 6. 阻塞项汇报清单

下列问题**遇到时立即停手**：

1. **B-7**: `ServicePlugin::shutdown` 签名不接收 `ap`，无法反注册 Provider
2. **C-1**: `SkillInjectionProvider` 在 Assembler 侧引用，违反 "Slot 不依赖 Service 内部" 原则
3. **跨任务冲突**：若其他集成计划（MCP / Vector）也修改 `shared_types/mod.rs`、`shared_types/assembler/config.rs` 或 `assembler/providers/mod.rs`，需协调
4. **测试发现新 bug**：如发现 Skills 文档本身有内部矛盾，停下汇报，不要默默修文档
