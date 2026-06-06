# Skills（技能注入服务）开发文档

## 0. 协议依据

本文档严格遵循以下三份协议标准，逐条对标：

| 协议 | 应用层级 | 关键条款 |
|------|---------|---------|
| **protocol-Service集成协议** | 模块对框架的接入方式 | §1 ServicePlugin 单入口、§2 ServiceAccessPoint 受控访问句柄、§3 运行时信号、§4 插件元数据、§5 生命周期、§8 协议特有红线 |
| **protocol-模块内部组件协议** | 模块内部子模块组织方式 | §1 Component 单入口、§3 AccessPoint 内部数据共享通道、§4 Processing 处理结果、§6 模块边界规范 |
| **跨平台与硬编码规范** | 全局代码约束 | §1 硬编码值分类定义、§2 跨平台路径规则、§3 测试代码规范、§4 自查清单 |

---

## 1. 模块定位

### 1.1 一句话描述

**管理 Markdown 格式的技能文件（.skill.md），根据对话上下文语义匹配最相关的技能，将技能内容分级注入 Assembler 的 System Prompt 上下文，增强 LLM 领域能力。**

### 1.2 架构定位

Skills 模块定位为 **ServicePlugin**（服务插件），当前实现聚焦于**文件解析 + 语义匹配 + 上下文注入**三个核心组件：

```
用户对话: "帮我写一个 Python 爬虫"
  │
  ▼
┌──────────────────────────────────────────────────────────────┐
│  SkillsService (impl ServicePlugin) ← 待补齐                    │
│  - init(): 扫描 skills_dir → 加载所有 .skill.md               │
│  - start(): 注册 FileSkill 到 ContractRegistry               │
└──────────────────────────────────────────────────────────────┘
          │ 技能已注册到 ContractRegistry
          ▼
┌──────────────────────────────────────────────────────────────┐
│  SkillInjectionProvider (impl ContextProvider)                │
│  - 由 Assembler 调度，在构建 System Prompt 时调用               │
│  - 从 ContractRegistry 获取所有 SkillContract                 │
│  - 根据最近 5 条消息匹配 → 贪心分配 token 预算                   │
│  - 产出 ContextBlock 列表                                     │
└──────────────────────────────────────────────────────────────┘
          │ 每个候选技能调用 match_score()
          ▼
┌──────────────────────────────────────────────────────────────┐
│  SkillMatcher                                                │
│  - 算法：Jaccard 标签系数 (0.5) + TF-IDF 余弦相似度 (0.5)       │
│  - 快速过滤：至少一个标签匹配才继续计算                            │
│  - 得分归一化到 [0, 1]                                         │
└──────────────────────────────────────────────────────────────┘
          │ 匹配到的技能调用 get_content(level)
          ▼
┌──────────────────────────────────────────────────────────────┐
│  FileSkill (impl SkillContract)                               │
│  - 加载本地 .skill.md 文件                                      │
│  - 解析 YAML frontmatter + TL;DR + Key Points + Full Content  │
│  - 按 SkillLevel 返回不同粒度的内容                              │
└──────────────────────────────────────────────────────────────┘
```

**技能文件格式**（`.skill.md`）：

```markdown
---
name: web_scraping
title: Web Scraping Guide
version: 1.0.0
description: How to scrape websites using Python
tags: [python, scraping, http]
injection_policy: auto
quota_preference: full
summary: Use requests + BS4 to scrape
---

# TL;DR
Quick intro to web scraping with Python.

# Key Points
- Install requests and beautifulsoup4
- Send GET request
- Parse HTML with BeautifulSoup

# Full Content
## Step 1: Installation
pip install requests beautifulsoup4
## Step 2: Basic Request
...
```

---

## 2. 文件结构

```
src/plugins/services/skills/
├── mod.rs        # 模块入口：子模块声明 + 公开类型 re-export
├── config.rs     # SkillConfig 配置结构体 + Default 实现
├── file_skill.rs # FileSkill — 加载和解析 .skill.md 文件
├── matcher.rs    # SkillMatcher — Jaccard + TF-IDF 语义匹配
└── provider.rs   # SkillInjectionProvider — 上下文注入，实现 ContextProvider
```

> **模块边界规范（§6.1）**：`mod.rs` 仅暴露 `SkillConfig`、`FileSkill`、`SkillMatcher`、`SkillInjectionProvider` 四个公共类型。内部 `SkillFrontmatter`、`TfIdfVector` 等为 `pub(crate)` 或私有。

---

## 3. 功能清单

| 功能 | 描述 | 实现状态 | 对应源码 |
|------|------|:---:|---------|
| 技能文件加载 | 异步/同步读取 `.skill.md` 文件，解析 YAML frontmatter + 分节内容 | ✅ | `file_skill.rs:load()` / `load_sync()` |
| Frontmatter 解析 | 解析 name、title、version、description、tags、injection_policy、quota_preference、dependencies、summary | ✅ | `file_skill.rs:parse_frontmatter()` |
| 内容分级提取 | 按 TL;DR / Key Points / Full Content 三个层级提取 | ✅ | `file_skill.rs:parse()` |
| 语义匹配 | Jaccard 标签系数 (0.5) + TF-IDF 余弦相似度 (0.5) | ✅ | `matcher.rs:compute_score()` |
| 快速过滤 | 标签零交集直接返回 0.0，跳过计算 | ✅ | `matcher.rs:compute_score()` |
| Token 预算控制 | 贪心分配：第1名 Full → 第2名 KeyPoints → 第3+ Summary → TitleOnly | ✅ | `provider.rs:select_level()` |
| 配置化阈值 | `min_match_score` / `max_skills` / `max_candidates` 可配 | ✅ | `config.rs` |
| 技能开关 | 通过 `ComponentSwitch` 按名禁用/启用技能 | ✅ | `provider.rs:provide()` 过滤 |
| 配额偏好 | 技能 frontmatter 声明 `quota_preference` 限制最大注入级别 | ✅ | `provider.rs:get_quota_preference()` |
| ServicePlugin | 完整的生命周期管理 | ❌ 待补齐 | — |
| Provider 注册 | 将 SkillContract 注册到 ContractRegistry | ❌ 待补齐 | — |

---

## 4. 核心设计

### 4.1 SkillConfig（配置）

**文件**：`config.rs`

```rust
pub struct SkillConfig {
    pub skill_budget_ratio: f64,    // 技能占比 (0.0~1.0)，默认 0.05
    pub max_skills: usize,           // 最多注入技能数，默认 3
    pub skills_dir: PathBuf,         // 技能文件目录
    pub allow_external_skills: bool, // 允许外部技能，默认 false
    pub max_candidates: usize,       // 匹配候选上限，默认 20
    pub min_match_score: f32,        // 最低匹配分数，默认 0.15
}
```

**跨平台与硬编码规范（§1）对标**：

| # | 类别 | 合规 | 说明 |
|---|------|:---:|------|
| 7 | 数字阈值 | ✅ | `skill_budget_ratio` / `max_skills` / `min_match_score` 从配置读取 |
| 6 | 文件路径 | ⚠️ | `skills_dir` 在 `Default` 中为 `PathBuf::from("resources/skills")`（相对路径依赖 CWD），**违反跨平台规范 §2.3**，应改为通过 `AAGNET_HOME` 环境变量 + `dirs::data_dir()` 解析 |

> **待修复**：`SkillConfig::default()` 中 `skills_dir` 的默认值应使用 `dirs::data_dir().unwrap_or_default().join("potoobird").join("skills")`，并通过 `AAGNET_HOME` 环境变量支持覆盖。

### 4.2 FileSkill（技能文件）

**文件**：`file_skill.rs`

**实现 trait**：`SkillContract` + `Describe`

单个 `.skill.md` 文件的加载、解析和内容分发。

#### 4.2.1 加载方式

| 方法 | 说明 |
|------|------|
| `load(path)` | 异步读取文件（`tokio::fs::read_to_string`） |
| `load_sync(path)` | 同步读取文件（`std::fs::read_to_string`） |

#### 4.2.2 解析流程

```
FileSkill::parse(path, content)
  │
  ├─ 1. parse_frontmatter(content)
  │      - 定位 "---" ... "---" 区块
  │      - 逐行解析 key: value
  │      - 必填字段校验：name, title, version 非空 → 否则 Err
  │      - 列表字段（tags, dependencies）通过 parse_yaml_list_value() 解析
  │
  ├─ 2. 提取 body（frontmatter 之后的内容）
  │
  ├─ 3. 分段提取
  │      - extract_section(body, "# TL;DR", "# Key Points") → tldr
  │      - extract_section(body, "# Key Points", "# Full Content") → key_points
  │        └─ 按 "- " 前缀的行切分为 Vec<String>
  │
  ├─ 4. Full Content = body 中 "# Full Content" 之后的内容
  │
  └─ 5. 构造 FileSkill { path, frontmatter, tldr, key_points, full_content }
```

#### 4.2.3 Frontmatter 字段

| 字段 | 类型 | 必需 | 默认值 | 说明 |
|------|------|:---:|--------|------|
| `name` | String | ✅ | — | 技能唯一标识 |
| `title` | String | ✅ | — | 显示标题 |
| `version` | String | ✅ | — | 语义版本 |
| `description` | String | — | `""` | 简短描述，用于匹配 |
| `tags` | Vec\<String\> | — | `[]` | 标签列表，用于 Jaccard 匹配 |
| `injection_policy` | String | — | `"auto"` | 注入策略：`auto` / `always` / `never` |
| `quota_preference` | String | — | `"full"` | 配额偏好：`full` / `summary` / `title_only` |
| `dependencies` | Vec\<String\> | — | `[]` | 依赖的其他技能名 |
| `summary` | String | — | — | 自定义摘要（可选，未设置则回退到 TL;DR） |

#### 4.2.4 SkillContract 实现

| 方法 | 行为 |
|------|------|
| `name()` | 返回 `frontmatter.name` |
| `version()` | 解析 `frontmatter.version` → `Version`，失败回退 `1.0.0` |
| `description()` | 返回 `frontmatter.description` |
| `group()` | 固定返回 `"skill"` |
| `tags()` | 返回 `frontmatter.tags` clone |
| `dependencies()` | 返回 `frontmatter.dependencies` clone |
| `injection_policy()` | 解析 `frontmatter.injection_policy` → `InjectionPolicy`，失败 `Auto` |
| `quota_preference()` | 解析 `frontmatter.quota_preference` → `QuotaPreference`，失败 `Full` |
| `get_content(level)` | 按 `SkillLevel` 返回分级内容（见 4.2.5） |
| `match_score(context)` | 新建 `SkillMatcher`，计算匹配分 |

#### 4.2.5 内容分级（SkillLevel）

| SkillLevel | 返回内容 | token 估算 | 使用场景 |
|------------|---------|-----------|---------|
| `TitleOnly` | `frontmatter.title` | ~10 | 最低预算、第 N 名（N≥3） |
| `Summary` | `frontmatter.summary` 或 `tldr` | ~50 | 第 3+ 名、summary 配额偏好 |
| `KeyPoints` | `key_points` 列表（以 `\n` 拼接） | ~100 | 第 2 名 |
| `Full` | `full_content` 全文 | ~500+ | 第 1 名、充足预算 |

> **降级逻辑**：`KeyPoints` 无内容时自动退到 `Summary` 级别。

### 4.3 SkillMatcher（语义匹配器）

**文件**：`matcher.rs`

纯函数型匹配器，无外部依赖，自实现 TF-IDF。

#### 4.3.1 算法

```
compute_score(skill_name, context_text, skill_tags, skill_description, skill_summary)
  │
  ├─ 1. 标签 Jaccard 系数（权重 0.5）
  │      jaccard = |context_words ∩ skill_tags| / |context_words ∪ skill_tags|
  │
  ├─ 2. 快速过滤：intersection == 0 && tags 非空 → return 0.0
  │
  ├─ 3. TF-IDF 余弦相似度（权重 0.5）
  │      - 对 context_text 和 (description + summary) 分别建 TF 向量
  │      - 单字特征（权重 1.0）+ Bigram 特征（权重 0.5）
  │      - IDF 基于已缓存的全部文档向量计算
  │      - cos = dot(q, d) / (||q|| * ||d||)
  │
  └─ 4. score = 0.5 * jaccard + 0.5 * tfidf
```

#### 4.3.2 关键字段

```rust
pub struct SkillMatcher {
    doc_vectors: HashMap<String, TfIdfVector>,  // skill_name → TF-IDF 向量缓存
}

struct TfIdfVector {
    terms: HashMap<String, f32>,  // term → TF 值
}
```

#### 4.3.3 分词策略

- 全小写 + 去标点 + 最小长度 ≥ 2
- 单字（unigram）权重 1.0
- Bigram 权重 0.5（`"word1_word2"` 格式）
- IDF 平滑公式：`ln((total_docs + 1) / (df + 1)) + 1`

#### 4.3.4 缓存管理

- 每次 `compute_score()` 成功计算后，将文档向量缓存到 `doc_vectors`
- `clear_cache()` 清空全部缓存

> **注意**：当前 `FileSkill::match_score()` 每次调用都新建 `SkillMatcher`，缓存无法跨技能复用。后续优化方向：将 `SkillMatcher` 提升为 `SkillInjectionProvider` 的成员，跨技能共享缓存。

### 4.4 SkillInjectionProvider（上下文注入器）

**文件**：`provider.rs`

**实现 trait**：`ContextProvider`（Assembler 框架的上下文提供者接口）

这是 Skills 模块的**核心业务入口**——它不直接暴露给用户，而是作为 Assembler 的一个 Provider Slot 被调用。

#### 4.4.1 ContextProvider 实现

| 方法 | 行为 |
|------|------|
| `name()` | 返回 `"skills"` |
| `priority()` | 返回 `30`（在 Assembler 的 Provider 链中的优先级） |
| `silent_on_empty()` | 返回 `true`（无匹配技能时不输出空块） |
| `estimate_max_tokens()` | 返回 `min(config.max_tokens, 5000)` |

#### 4.4.2 provide() 流程

```
provide(ctx_data, quota, slot_config)
  │
  ├─ 1. quota.max_tokens == 0 → 直接返回空
  │
  ├─ 2. 从 ContractRegistry 获取全部 SkillContract
  │     过滤条件：
  │       - injection_policy == Auto
  │       - ComponentSwitch 未禁用此技能
  │       - visibility == AlwaysVisible
  │
  ├─ 3. 构建上下文文本：最近 5 条消息的 text_content() 拼接
  │
  ├─ 4. 逐技能计算 match_score(context_text)
  │     过滤 score < min_match_score 的候选
  │
  ├─ 5. 按 score 降序排序，取前 min(max_skills, max_candidates) 个
  │
  ├─ 6. 贪心 token 分配：
  │     for each (rank, score, skill):
  │       level = select_level(rank, remaining_tokens, quota_preference)
  │       content = skill.get_content(level)
  │       估算 token = content.text.chars().count() / 2
  │       如果超过剩余 → 截断
  │       push ContextBlock { section_title, content, source, token_count }
  │       remaining_tokens -= actual_tokens
  │       if remaining_tokens < 50 → break
  │
  └─ 7. 返回 ProvidedContext { blocks, tokens_used }
```

#### 4.4.3 select_level() 分级策略

| 排名 | 剩余 token > 阈值 | 选择级别 | 回退 |
|------|------------------|---------|------|
| 第1名 | > 2000 | `Full` | → KeyPoints → Summary → TitleOnly |
| 第1名 | > 500 | `KeyPoints` | → Summary → TitleOnly |
| 第1名 | > 100 | `Summary` | → TitleOnly |
| 第1名 | ≤ 100 | `TitleOnly` | — |
| 第2名 | > 1000 | `KeyPoints` | → Summary → TitleOnly |
| 第2名 | > 200 | `Summary` | → TitleOnly |
| 第2名 | ≤ 200 | `TitleOnly` | — |
| 第3+名 | > 500 | `Summary` | → TitleOnly |
| 第3+名 | ≤ 500 | `TitleOnly` | — |

> **配额偏好覆盖**：技能 frontmatter 声明 `quota_preference = summary` 时，即使排名第1且剩余 token > 2000，也最多注入 `Summary` 级别。`title_only` 同理。

#### 4.4.4 quota_preference 来源

`get_quota_preference()` 通过 `MetadataBus` 查询 `aagnet.skill.{name}` 的描述符的 `extensions["quota_preference"]` 字段。如果 MetadataBus 中无记录，回退到 `SkillContract::quota_preference()`。

#### 4.4.5 输出格式

```markdown
## {skill.title}
{skill.title}

(match: {score}, level: {level})

{truncated_content}
```

---

## 5. 数据流全景

```
┌──────────┐    ┌───────────┐    ┌────────────────────┐    ┌──────────────┐
│ .skill.md │ → │ FileSkill │ → │ ContractRegistry   │ ← │ SkillsService│
│ 文件系统   │    │ .load()   │    │ .register(skill)   │    │ (待补齐)      │
└──────────┘    └───────────┘    └─────────┬──────────┘    └──────────────┘
                                           │
                          Assembler 调用    │  all_skills()
                                           ▼
                              ┌─────────────────────────┐
                              │ SkillInjectionProvider  │
                              │ .provide(ctx, quota)    │
                              └───────────┬─────────────┘
                                          │
                          for each skill  │  match_score()
                                          ▼
                              ┌─────────────────────────┐
                              │ SkillMatcher            │
                              │ .compute_score()        │
                              │ Jaccard(0.5)+TFIDF(0.5) │
                              └───────────┬─────────────┘
                                          │
                              按 score 排序 + 过滤      │
                                          ▼
                              ┌─────────────────────────┐
                              │ select_level(rank, tok) │
                              │ → get_content(level)    │
                              └───────────┬─────────────┘
                                          │
                                          ▼
                              ┌─────────────────────────┐
                              │ Vec<ContextBlock>       │
                              │ → Assembler SystemPrompt│
                              └─────────────────────────┘
```

---

## 6. 协议合规性分析

### 6.1 Service 集成协议（protocol-Service集成协议）对标

#### 6.1.1 ServicePlugin 方法职责（协议 §1）

| 方法 | 调用次数 | 用途 | 当前状态 |
|------|---------|------|:---:|
| `name()` | 多次 | 返回全局唯一服务标识 `"skills"` | ❌ 无 SkillsService |
| `init(ctx)` | 1 | 扫描 `skills_dir`，加载全部 `.skill.md` → `FileSkill` | ❌ |
| `start(ap)` | 1 | 将 FileSkill 注册到 ContractRegistry | ❌ |
| `handle_signal(signal)` | 多次 | 响应运行时信号（见 6.1.2） | ❌ |
| `stop()` | 多次 | 暂停服务，Provider 仍可用但不更新 | ❌ |
| `shutdown()` | 1 | 从 ContractRegistry 反注册 + 清理缓存 | ❌ |

#### 6.1.2 运行时信号处理（协议 §3）

| 信号 | 说明 | 当前处理 | 合规 |
|------|------|:---:|:---:|
| `GracefulShutdown` | 正常关闭，完成后台任务再退出 | ❌ 无 | — |
| `ImmediateShutdown` | 强制关闭，立即停止 | ❌ 无 | — |
| `ConfigReload` | 重载配置，重扫 skills_dir 目录 | ❌ 无 | — |
| `HealthCheck` | 健康检查，需在 5s 内返回 `Ok(())`（红线 V-R01） | ❌ 无 | V-R01 ❌ |
| `Suspend` | 暂停服务，释放临时资源 | ❌ 无 | — |
| `Resume` | 从暂停中恢复 | ❌ 无 | — |

#### 6.1.3 生命周期（协议 §5）

```
PluginLoader 读元数据 → 校验 provides/requires
→ init(ctx) → start(ap) ↔ [handle_signal() ...] → stop() → shutdown()
```

当前状态：**全部未实现**。Skills 模块通过 `ContextProvider` 紧耦合到 Assembler，绕过了 Service 框架的标准接入路径。

#### 6.1.3.1 计划声明（ServicePlugin 各方法职责与实现要点）

```rust
#[async_trait]
impl ServicePlugin for SkillsService {
    fn name(&self) -> &str { "skills" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<(), PluginError> {
        // 1. resolve_paths() → 展开 skills_dir 中的 ~ 和相对路径
        // 2. 扫描 skills_dir → 遍历所有 .skill.md 文件
        // 3. FileSkill::load() 逐个加载 → 校验 frontmatter 必填字段
        // 4. 将加载成功的 FileSkill 暂存到 Vec，失败的 warn 日志跳过
        Ok(())
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<(), PluginError> {
        // 注册 Provider（协议 §2.2）：
        //   将 init() 中加载的 FileSkill 注册到 ContractRegistry
        //   SkillInjectionProvider 通过 all_skills() 查询即可消费
        // 注意：SkillsService 不注册 Provider 到 ProviderRegistry，
        //   而是将 SkillContract 注册到 ContractRegistry（组件注册表）
        Ok(())
    }

    async fn handle_signal(&mut self, signal: ServiceSignal) -> Result<(), PluginError> {
        match signal {
            ServiceSignal::HealthCheck => {
                // 红线 V-R01：5s 内检查 skills_dir 是否可读
                Ok(())
            }
            ServiceSignal::ConfigReload => {
                // 重新 resolve_paths() → 重扫 skills_dir → 增量更新注册表
                // 新增的 FileSkill → register；已删除的 → unregister
                Ok(())
            }
            ServiceSignal::GracefulShutdown => {
                // SkillMatcher 缓存清空
                Ok(())
            }
            _ => Ok(()),
        }
    }

    async fn stop(&mut self) -> Result<(), PluginError> {
        // 暂停新技能注册，已注册的 SkillContract 仍可查询
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), PluginError> {
        // 从 ContractRegistry 反注册所有 FileSkill
        Ok(())
    }
}
```

> 以上声明将 §7 待办的 8 个步骤具体化为可执行的代码框架。与 Memory 不同，Skills 的 `start()` 通过 ContractRegistry 注册组件而非通过 ProviderRegistry 注册 Provider——这是因为 SkillInjectionProvider 作为 Assembler 的 ContextProvider 消费技能的方式是通过 `ContractRegistry::all_skills()` 查询。

#### 6.1.4 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 ServicePlugin 单入口 | 模块需实现 `ServicePlugin` trait | ❌ | 未实现 `SkillsService`（详见 6.1.1） |
| §2.1 ServiceAccessPoint | 通过 `get_config()` / `log()` 与 core 交互 | ❌ | 无 ServiceAccessPoint 注入 |
| §2.2 register_provider() | 在 `start()` 中注册 Provider | ❌ | SkillInjectionProvider 是 Assembler 的 ContextProvider，非标准 Provider |
| §3 运行时信号 | 响应全部 6 个信号 | ❌ | 无 handle_signal() 实现（详见 6.1.2） |
| §4 插件元数据 | YAML 声明 provides / requires / run_mode | ❌ | 元数据已设计（见 §9），未接入 PluginLoader |
| §5 生命周期 | init → start → stop → shutdown | ❌ | 无完整生命周期（详见 6.1.3） |
| §6 补充说明 | ServiceAccessPoint Clone、handle_signal<5s、不假设 start/stop 配对 | ❌ | 待实现 |
| §7 标准流程 | 8 步骤从零到运行 | ⚠️ | 步骤 1-4 已完成（config/file_skill/matcher/provider），步骤 5-8 待完成（见 §7） |
| §8 V-R01 HealthCheck | 5s 内返回 `Ok(())` | ❌ | 无实现 |
| §8 V-R02 handle_signal 不阻塞 | 超 5s 须 spawn | ❌ | 无实现 |
| §8 V-R03 provides 一致 | 声明 = 实际注册 | ❌ | 无注册 |

### 6.2 模块内部组件协议（protocol-模块内部组件协议）对标

#### 6.2.1 依赖方向（协议 §6.2）

```
┌──────────────────────┐
│  模块 mod.rs          │  （对外暴露的公共 API）
│  SkillConfig          │
│  FileSkill            │
│  SkillMatcher         │
│  SkillInjectionProvider│
└──────────┬───────────┘
           │
           ▼
┌──────────────────────────────────────────────┐
│  组件（无 Orchestrator — 组件通过 ContractRegistry 间接通信）│
│                                              │
│  FileSkill ──→ SkillContract                  │
│       │                                       │
│       │ 被查询                                 │
│       ▼                                       │
│  ContractRegistry                             │
│       │                                       │
│       │ all_skills()                           │
│       ▼                                       │
│  SkillInjectionProvider ──→ ContextProvider    │
│       │                                       │
│       │ 调用                                  │
│       ▼                                       │
│  SkillMatcher (纯函数，Provider 每次新建)       │
│                                              │
│  ✅ 组件间无直接 struct 引用                   │
│  ✅ FileSkill ↔ SkillInjectionProvider        │
│     通过 ContractRegistry 解耦                │
└──────────────────────────────────────────────┘
```

#### 6.2.2 条款逐条对标

| 条款 | 要求 | 当前状态 | 差距 |
|------|------|:---:|------|
| §1 Component 单入口 | 实现 `Component` trait | ❌ | FileSkill 实现 SkillContract，SkillInjectionProvider 实现 ContextProvider，均非 Component |
| §3 AccessPoint | 组件通过 AccessPoint 通信 | N/A | 组件间通过 ContractRegistry 间接通信，近似 AccessPoint 模式 |
| §5 Orchestrator | 编排器调度 | N/A | 无多组件编排需求 |
| §6 模块边界 | mod.rs 只暴露入口+配置 | ✅ | 公共导出 4 个类型 |

### 6.3 跨平台与硬编码规范对标（协议 §4 完整 10 项自查清单）

| # | 检查项 | 合规 | 说明 |
|---|--------|:---:|------|
| 1 | 所有 URL 端点来自配置或常量，非字面量写死 | ✅ | 不涉及 HTTP 端点 |
| 2 | 所有模型名称来自配置字段，非硬编码 | ✅ | 不涉及 LLM 模型 |
| 3 | 所有超时值来自配置或 `DEFAULT_*` 常量 | ✅ | 无网络超时场景 |
| 4 | API 版本号定义为模块级 `const`，不散落 | ✅ | 不涉及 API 版本号 |
| 5 | User-Agent 定义为 `const USER_AGENT` | ✅ | 不涉及 HTTP 请求 |
| 6 | 文件路径通过 `dirs` + `PathBuf::join()` 构建，无 `/tmp/`、`~`、相对路径 | ⚠️ | `skills_dir` 默认值 `PathBuf::from("resources/skills")` 为相对路径，**违反 §2.3** |
| 7 | 数字阈值（max_tokens 等）默认 `None` 或从配置读取 | ✅ | `min_match_score` / `max_skills` / `max_candidates` 从 `SkillConfig` 读取 |
| 8 | 平台特定指令通过 `OsKind` 枚举分支，不假设 `sh` 或 `cmd` | ✅ | 不涉及 shell 指令 |
| 9 | 测试中无 Unix-only 路径，均用 `std::env::temp_dir()` | ✅ | `matcher.rs` / `provider.rs` 测试无文件路径依赖 |
| 10 | `cargo build` + `cargo test` + `cargo clippy` 全部通过 | 待验证 | — |

> **违规项 #6**：`SkillConfig::default()` 中 `skills_dir: PathBuf::from("resources/skills")` 依赖 CWD（违反协议 §2.3）。应改为 `dirs::data_dir().unwrap_or_default().join("potoobird").join("skills")`，并通过 `AAGNET_HOME` 环境变量覆盖。

---

## 7. Service 接入待办

按 **Service 集成协议 §7 新增 Service 标准流程**：

| 步骤 | 做什么 | 涉及文件 |
|------|--------|---------|
| 1 | 创建 `SkillsService` 结构体 | 新建 `service.rs` |
| 2 | 实现 `ServicePlugin` trait | `service.rs` |
| 3 | `init()` 扫描 `skills_dir`，加载全部 `.skill.md` → `FileSkill` | `service.rs` |
| 4 | `start()` 将 `FileSkill` 注册到 `ContractRegistry`（通过 `ServiceAccessPoint` 获取 registry 引用） | `service.rs` |
| 5 | `handle_signal(ConfigReload)` 重扫目录 + 更新注册表 | `service.rs` |
| 6 | `shutdown()` 从 `ContractRegistry` 反注册 | `service.rs` |
| 7 | 更新 `mod.rs` 导出 `SkillsService` | `mod.rs` |
| 8 | 更新 `SkillConfig::default()` 的 `skills_dir` 使用平台无关路径 | `config.rs` |

---

## 8. 设计决策

### 8.1 为什么技能是 Markdown 文件而不是代码

**决策**：技能存储为 `.skill.md` 文件，注入为 Prompt 文本。

**理由**：
1. **安全性**：不执行任意代码，只是文本注入到 System Prompt
2. **LLM 原生格式**：Markdown 是 LLM 训练数据的主要格式，理解最佳
3. **易于编写**：用户只需写 Markdown，无需编程技能
4. **分级注入**：通过 TL;DR / Key Points / Full Content 三级结构，支持按 token 预算精细控制

### 8.2 为什么用 Jaccard + TF-IDF 混合匹配

**决策**：标签 Jaccard 系数（0.5）+ TF-IDF 余弦相似度（0.5），综合评分。

**理由**：
1. **召回率**：单一策略容易漏掉相关技能
2. **精度**：标签 Jaccard 快速过滤完全不相关的技能（至少一个标签匹配）
3. **零外部依赖**：自实现 TF-IDF，不依赖 embedding API 或向量数据库
4. **可解释**：得分可直接追溯到标签匹配和关键词重叠

### 8.3 为什么作为 ContextProvider 而非独立 Provider

**当前选择**：`SkillInjectionProvider` 实现 `ContextProvider`，直接集成到 Assembler。

**权衡**：
- 优点：与 Assembler 的 token 预算系统无缝集成，贪心分配算法利用 Assembler 的 `ContextQuota`
- 缺点：紧耦合到 Assembler，无法被其他 Slot 独立使用

**演进方向**：补 `SkillsService` 后，`SkillInjectionProvider` 仍保留为 Assembler 的 Provider，但技能注册和生命周期管理由 `SkillsService` 负责，实现数据面（Provider）/ 控制面（Service）分离。

### 8.4 SkillMatcher 缓存策略

**当前选择**：每次 `FileSkill::match_score()` 调用新建 `SkillMatcher`，不跨技能共享缓存。

**性能边界**：
- 技能数量 < 20：每次匹配 O(N) 向量化，性能可接受（< 10ms）
- 技能数量 20~50：可接受的退化，但应监控
- 技能数量 > 50：必须优化——将 `SkillMatcher` 提升为 `SkillInjectionProvider` 的成员字段，跨技能共享 `doc_vectors` 缓存

**触发条件**：当 `ContractRegistry::all_skills()` 返回的技能数首次超过 50 时，执行此优化。

---

## 9. 插件元数据

```yaml
name: skills
category: service
version: 0.2.0
run_mode: background
provides:
  - skills
requires:
  - storage
conflicts: []
config_schema:
  type: object
  properties:
    skill_budget_ratio:
      type: number
      default: 0.05
      description: 技能注入占总 token 预算的比例
    max_skills:
      type: integer
      default: 3
      description: 最多注入的技能数
    skills_dir:
      type: string
      description: 技能文件搜索目录（必须为绝对路径或通过 AAGNET_HOME 解析）
    allow_external_skills:
      type: boolean
      default: false
    max_candidates:
      type: integer
      default: 20
    min_match_score:
      type: number
      default: 0.15
      description: 最低匹配分数阈值 [0, 1]
```

---

## 10. 红线与质量

| 编号 | 来源 | 红线 | 合规 |
|------|------|------|:---:|
| V-R01 | Service集成协议 | 必须响应 `HealthCheck` | ❌ 待补齐 |
| V-R02 | Service集成协议 | `handle_signal` 不阻塞超 5s | ❌ 待补齐 |
| V-R03 | Service集成协议 | `provides` = `register_provider` 一致 | ❌ 待补齐 |
| — | aagnet-lessons | 外部输入必须校验 | ✅ frontmatter 必填字段校验（name/title/version 非空） |
| — | aagnet-lessons | 不可在库代码中 unwrap/expect | ⚠️ `file_skill.rs` 中 `injection_policy().parse().unwrap_or()` / `quota_preference().parse().unwrap_or()` 使用 `unwrap_or` 降级，合规；`Version::parse().unwrap_or_else()` 合规 |
| — | 跨平台规范 §2.3 | 禁止相对路径依赖 CWD | ⚠️ `skills_dir` 默认值为相对路径，待修复 |

---

## 11. 测试

**文件**：`matcher.rs`（末尾 `#[cfg(test)]`）

| 测试 | 说明 | 合规（跨平台规范 §3） |
|------|------|:---:|
| `test_jaccard_full_match` | 标签匹配验证 | ✅ 无外部依赖 |
| `test_no_tag_match_returns_zero` | 快速过滤验证 | ✅ |
| `test_empty_context` | 空上下文边界测试 | ✅ |
| `test_score_bounds` | 得分范围 [0,1] 验证 | ✅ |

**文件**：`provider.rs`（末尾 `#[cfg(test)]`）

| 测试 | 说明 | 合规 |
|------|------|:---:|
| `test_select_level_first_rank` | 第1名分级策略 | ✅ |
| `test_select_level_second_rank` | 第2名分级策略 | ✅ |
| `test_select_level_third_rank` | 第3+名分级策略 | ✅ |

---

## 12. 依赖关系

```
FileSkill                ──→  SkillContract (core::contract::skill)
FileSkill                ──→  SkillMatcher (内部)
SkillInjectionProvider   ──→  ContextProvider (assembler::providers)
SkillInjectionProvider   ──→  ContractRegistry (core::contract)
SkillInjectionProvider   ──→  SkillContract (core::contract::skill)
SkillInjectionProvider   ──→  MetadataBus (core::metadata_bus)
```

- 对外依赖：`tokio::fs`（异步文件读取）、`serde_json`（序列化）
- 框架层依赖：`core::contract::skill`（技能契约）、`core::contract::ContractRegistry`（组件注册表）、`core::metadata_bus`（元数据总线）
- Assembler 层依赖：`assembler::providers::{ContextProvider, ContextBlock, ContextQuota}`
