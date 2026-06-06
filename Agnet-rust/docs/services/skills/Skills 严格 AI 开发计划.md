# Skills（技能注入服务）严格 AI 开发计划

> 集成补全版本：2026-06-01（v0.2.0 集成）—— Skills→Assembler 集成计划已完成

本计划用于指导 AI 严格按照 `docs/services/skills/skills开发文档.md` 生成 skills 模块的全部代码。

---

## 项目背景

- **模块名称**：skills（技能注入服务）
- **模块定位**：管理 `.skill.md` 技能文件，根据对话上下文语义匹配最相关技能，分级注入 Assembler 的 System Prompt。
- **外部接口**：
  - `SkillsService` — ServicePlugin 入口（待创建）
  - `SkillConfig` — 配置
  - `FileSkill` — 技能文件加载/解析
  - `SkillMatcher` — 语义匹配（Jaccard + TF-IDF）
  - `SkillInjectionProvider` — ContextProvider（Assembler 注入）
- **当前状态**：file_skill.rs ✅、matcher.rs ✅、provider.rs ✅、config.rs（默认路径待修复 ⚠️）、service.rs ❌
- **依赖项**：`tokio::fs`、`serde_json`、`dirs`

---

## 硬编码分类定义（skills 特有）

| 类别 | 错误示例 | 正确做法 |
|------|---------|---------|
| 技能目录 | `"resources/skills"`（相对路径） | `dirs::data_dir().join("potoobird").join("skills")`，AAGNET_HOME 覆盖 |
| 预算比例 | `0.05` | 从 `SkillConfig.skill_budget_ratio` 读取 |
| 最大技能 | `3` | 从 `SkillConfig.max_skills` 读取 |
| 最低分数 | `0.15` | 从 `SkillConfig.min_match_score` 读取 |
| 最大候选 | `20` | 从 `SkillConfig.max_candidates` 读取 |
| 匹配权重 | `0.5` / `0.5` | Jaccard / TF-IDF 固定权重（设计决策，非配置） |
| IDF 平滑 | `ln((N+1)/(df+1)) + 1` | 算法固定公式，定义为 `const` |

---

## 项目目录结构

```
src/plugins/services/skills/
├── mod.rs        # 模块入口：SkillsService / SkillConfig / FileSkill / SkillMatcher / SkillInjectionProvider
├── config.rs     # SkillConfig（修复默认 skills_dir 为平台无关路径）
├── service.rs    # SkillsService（ServicePlugin 实现，新建）
├── file_skill.rs # FileSkill — .skill.md 加载/解析/内容分级（已有）
├── matcher.rs    # SkillMatcher — Jaccard + TF-IDF 匹配器（已有）
└── provider.rs   # SkillInjectionProvider — ContextProvider 实现（已有）
```

---

## AI 宪法

```
[宪法已生效]

1. **文档唯一真理**：所有类型、签名、默认值、流程步骤与 skills开发文档.md 一致。

2. **零幻觉**：
   a. Skills 只有 4 个内部模块（file_skill/matcher/provider/config），加上 service 共 5 个源文件。
   b. 匹配算法只有 Jaccard + TF-IDF，无 embedding API 或向量数据库。
   c. SkillInjectionProvider 是 ContextProvider（在 Assembler 中使用），非标准 Provider。

3. **零硬编码**：
   a. skills_dir 默认用 dirs::data_dir() + join()，不依赖 CWD。
   b. 所有数字阈值（min_match_score/max_skills/max_candidates）从 SkillConfig 读取。
   c. 路径 ~ 通过 resolve_paths() 展开。

4. **完整实现**：SkillsService 的 init/start/handle_signal/stop/shutdown 必须有完整实现。

5. **错误处理**：
   - 单个 .skill.md 解析失败（warn+跳过）不影响其他技能加载。
   - 技能目录不存在（warn+空列表）不 panic。
   - ConfigReload 时新增技能注册、已删除技能反注册。

6. **测试同步生成**：
   - SkillsService：init 扫描/start 注册/handle_signal 重载/shutdown 反注册。
   - 已有测试（matcher/provider）不动，补充 service 测试。
```

---

## 详细开发步骤

### 步骤 0：确认骨架

**操作**：确认现有文件（file_skill.rs / matcher.rs / provider.rs / config.rs）已就位，创建 service.rs。声明 mod chain。

**验收**：`cargo check` 通过

---

### 步骤 1：Config 层（config.rs）

现有 `SkillConfig`，需修复：

```rust
pub struct SkillConfig {
    pub skill_budget_ratio: f64,    // 0.05
    pub max_skills: usize,           // 3
    pub skills_dir: PathBuf,         // 修复前: PathBuf::from("resources/skills")
    pub allow_external_skills: bool, // false
    pub max_candidates: usize,       // 20
    pub min_match_score: f32,        // 0.15
}

impl Default for SkillConfig {
    fn default() -> Self {
        Self {
            skills_dir: dirs::data_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join("potoobird").join("skills"),
            // ... 其他字段默认值
        }
    }
}

impl SkillConfig {
    /// 展开 ~ 和环境变量，确保 skills_dir 为绝对路径
    pub fn resolve_paths(&mut self) {
        // 展开 self.skills_dir 中的 ~ → dirs::home_dir()
        // 如非绝对路径，以 dirs::data_dir() 为 base
    }
}
```

### 步骤 2：FileSkill（file_skill.rs，已有，确认逻辑）

确认实现：

| 方法 | 行为 |
|------|------|
| `load(path)` / `load_sync(path)` | 读取 .skill.md 文件内容 |
| `parse(path, content)` | 解析 frontmatter + 提取 TL;DR / Key Points / Full Content 三段 |
| `parse_frontmatter(content)` | YAML frontmatter 逐行解析，必填字段校验 |
| `get_content(level)` | 按 SkillLevel 返回分级内容（TitleOnly/Summary/KeyPoints/Full） |

**验收**：已有测试通过，无需修改

### 步骤 3：SkillMatcher（matcher.rs，已有）

确认 `compute_score(skill_name, context_text, skill_tags, skill_description, skill_summary)`：
- Jaccard 标签系数 (0.5)
- TF-IDF 余弦相似度 (0.5)
- 快速过滤：标签零交集且 tags 非空 → 0.0
- 缓存管理：doc_vectors 缓存

**建议优化**：将 SkillMatcher 提升为 SkillInjectionProvider 的成员（跨技能共享缓存），但当前保持现有实现不变。

**验收**：已有测试通过

### 步骤 4：SkillInjectionProvider（provider.rs，已有）

确认 `provide(ctx_data, quota, slot_config)` 流程：
1. quota.max_tokens == 0 → 空
2. ContractRegistry 获取 SkillContract → 过滤（injection_policy==Auto / 未禁用 / visible）
3. 最近 5 条消息 → context_text
4. 逐技能 match_score → 过滤 min_match_score
5. 降序排序 → 取前 N
6. 贪心 token 分配（select_level 策略）
7. 返回 ContextBlock 列表

**验收**：已有测试通过

### 步骤 5：SkillsService（service.rs，新建）

```rust
pub struct SkillsService {
    skills: Vec<FileSkill>,
    config: SkillConfig,
    contract_registry: Option<Arc<ContractRegistry>>,
}

impl ServicePlugin for SkillsService {
    fn name(&self) -> &str { "skills" }

    async fn init(&mut self, ctx: &PluginInitContext) -> Result<()> {
        1. 解析 SkillConfig（ctx.get_config()）
        2. config.resolve_paths() 展开 ~ 和相对路径
        3. 扫描 skills_dir → 遍历 *.skill.md
        4. FileSkill::load() 逐个加载
        5. 解析失败的 warn 日志跳过，成功加入 skills 列表
    }

    async fn start(&mut self, ap: ServiceAccessPoint) -> Result<()> {
        1. 通过 ap 获取 ContractRegistry 引用
        2. 将所有 FileSkill 注册到 ContractRegistry（register_skill）
        3. 保存 contract_registry 以便后续操作
    }

    async fn handle_signal(&self, signal: ServiceSignal) -> Result<()>
        HealthCheck → skills_dir 可读性检查（5s 内返回）
        ConfigReload → 重扫目录 → 增量更新注册表（新注册/反注册）
        GracefulShutdown → 清空 SkillMatcher 缓存
        Suspend → 标记暂停
        Resume → 标记恢复
        _ → Ok(())
    }

    async fn stop(&self) -> Result<()> { /* 暂停新技能注册 */ }

    async fn shutdown(&self) -> Result<()> {
        1. 从 ContractRegistry 反注册所有 FileSkill
        2. 清空 skills 列表
    }
}
```

### 步骤 6：mod.rs

```
pub use service::SkillsService;
pub use config::SkillConfig;
pub use file_skill::FileSkill;
pub use matcher::SkillMatcher;
pub use provider::SkillInjectionProvider;
```

### 步骤 7：终态自检

1. `cargo test --all` 全量通过，`cargo build` 无 error
2. 对照 skills开发文档.md §6.3 的 10 项自查清单（重点修复 #6：skills_dir 默认值）
3. SkillsService 完整生命周期测试
