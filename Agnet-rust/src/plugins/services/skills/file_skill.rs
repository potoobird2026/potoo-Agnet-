/*! FileSkill —— .skill.md 加载/解析/分级 + SkillContract 实现
 *
 * 本文件是 Skills→Assembler 集成 Phase A 的核心改动：
 * - SkillFrontmatter 扩展 5 个新字段（title / injection_policy / quota_preference / dependencies / summary）
 * - FileSkill 持有 SkillMatcher（每个技能自己的 cache，跨调用共享）
 * - get_content 改为返回 owned String（解决旧版 &str 引用问题 + 修复 KeyPoints 永远返回 "" 的 bug）
 * - impl SkillContract for FileSkill（满足 T-R02：服务方实现 trait）
 * - impl From<FileSkill::SkillLevel> for shared_types::SkillLevel（适配器）
 *
 * 协议参考：docs/development/integration_plans/Skills→Assembler.md A-3/A-4/A-5
 */
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::matcher::SkillMatcher;
use crate::shared_types::skills as st;

// ============================================
// 本地枚举（保留向后兼容，与 shared_types::SkillLevel 通过 From 互转）
// ============================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillLevel {
    TitleOnly,
    Summary,
    KeyPoints,
    Full,
}

impl From<SkillLevel> for st::SkillLevel {
    fn from(local: SkillLevel) -> Self {
        match local {
            SkillLevel::TitleOnly => st::SkillLevel::TitleOnly,
            SkillLevel::Summary => st::SkillLevel::Summary,
            SkillLevel::KeyPoints => st::SkillLevel::KeyPoints,
            SkillLevel::Full => st::SkillLevel::Full,
        }
    }
}

impl From<st::SkillLevel> for SkillLevel {
    fn from(s: st::SkillLevel) -> Self {
        match s {
            st::SkillLevel::TitleOnly => SkillLevel::TitleOnly,
            st::SkillLevel::Summary => SkillLevel::Summary,
            st::SkillLevel::KeyPoints => SkillLevel::KeyPoints,
            st::SkillLevel::Full => SkillLevel::Full,
        }
    }
}

// ============================================
// SkillFrontmatter（扩展 5 个新字段）
// ============================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFrontmatter {
    pub name: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub injection_policy: st::InjectionPolicy,
    #[serde(default)]
    pub quota_preference: st::QuotaPreference,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub summary: Option<String>,
}

// ============================================
// FileSkill
// ============================================

#[derive(Debug, Clone)]
pub struct FileSkill {
    pub path: PathBuf,
    pub frontmatter: SkillFrontmatter,
    pub tldr: String,
    pub key_points: Vec<String>,
    pub full_content: String,
    /// 技能自己的匹配器——文档 §4.3.4/§8.4 要求 matcher cache 跨调用共享
    /// 放在 FileSkill 里天然实现：每个技能一个 cache，add_document 在 parse 时一次性建立
    matcher: SkillMatcher,
}

impl FileSkill {
    pub fn load_sync(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path).map_err(|e| format!("读取失败: {}", e))?;
        Self::parse(path, &content)
    }

    pub async fn load(path: &Path) -> Result<Self, String> {
        let content = tokio::fs::read_to_string(path)
            .await
            .map_err(|e| format!("读取失败: {}", e))?;
        Self::parse(path, &content)
    }

    fn parse(path: &Path, content: &str) -> Result<Self, String> {
        let frontmatter = Self::parse_frontmatter(content)?;
        let body = Self::extract_body(content);
        let (tldr, key_points, full) = Self::split_sections(&body);
        let mut matcher = SkillMatcher::new();
        // 一次性把技能自己的描述+TL;DR 灌入 matcher cache
        matcher.add_document(
            &frontmatter.name,
            &format!("{} {}", frontmatter.description, tldr),
        );
        Ok(Self {
            path: path.to_path_buf(),
            frontmatter,
            tldr,
            key_points,
            full_content: full,
            matcher,
        })
    }

    fn parse_frontmatter(content: &str) -> Result<SkillFrontmatter, String> {
        let mut lines = content.lines();
        let mut in_fm = false;
        let mut fm_lines = Vec::new();
        for line in &mut lines {
            let trimmed = line.trim();
            if trimmed == "---" {
                if in_fm {
                    break;
                } else {
                    in_fm = true;
                    continue;
                }
            }
            if in_fm {
                fm_lines.push(line.to_string());
            }
        }
        if !in_fm {
            return Err("无 frontmatter".into());
        }
        let mut name = String::new();
        let mut title = String::new();
        let mut description = String::new();
        let mut tags = Vec::new();
        let mut version = String::from("1.0.0");
        let mut group = String::new();
        let mut injection_policy = st::InjectionPolicy::default();
        let mut quota_preference = st::QuotaPreference::default();
        let mut dependencies = Vec::new();
        let mut summary: Option<String> = None;
        for line in &fm_lines {
            let line = line.trim();
            if let Some((key, value)) = line.split_once(':') {
                let val = value.trim().trim_matches('"').trim();
                match key.trim() {
                    "name" => name = val.to_string(),
                    "title" => title = val.to_string(),
                    "description" => description = val.to_string(),
                    "tags" => {
                        tags = Self::parse_list(val);
                    }
                    "version" => version = val.to_string(),
                    "group" => group = val.to_string(),
                    "injection_policy" => injection_policy = Self::parse_injection_policy(val),
                    "quota_preference" => quota_preference = Self::parse_quota_preference(val),
                    "dependencies" => {
                        dependencies = Self::parse_list(val);
                    }
                    "summary" => summary = Some(val.to_string()),
                    _ => {}
                }
            }
        }
        if name.is_empty() {
            return Err("frontmatter 缺少必填字段 name".into());
        }
        if title.is_empty() {
            return Err("frontmatter 缺少必填字段 title".into());
        }
        Ok(SkillFrontmatter {
            name,
            title,
            description,
            tags,
            version,
            group,
            injection_policy,
            quota_preference,
            dependencies,
            summary,
        })
    }

    fn parse_list(val: &str) -> Vec<String> {
        val.trim_matches(|c| c == '[' || c == ']')
            .split(',')
            .map(|s| s.trim().trim_matches('"').to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    fn parse_injection_policy(val: &str) -> st::InjectionPolicy {
        match val {
            "Always" => st::InjectionPolicy::Always,
            "Never" => st::InjectionPolicy::Never,
            _ => st::InjectionPolicy::Auto,
        }
    }

    fn parse_quota_preference(val: &str) -> st::QuotaPreference {
        match val {
            "Summary" => st::QuotaPreference::Summary,
            "TitleOnly" => st::QuotaPreference::TitleOnly,
            _ => st::QuotaPreference::Full,
        }
    }

    fn extract_body(content: &str) -> String {
        let mut found_first = false;
        let mut found_second = false;
        content
            .lines()
            .filter(|line| {
                if line.trim() == "---" {
                    if !found_first {
                        found_first = true;
                        return false;
                    }
                    if !found_second {
                        found_second = true;
                        return false;
                    }
                }
                found_second
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn split_sections(body: &str) -> (String, Vec<String>, String) {
        let mut tldr = String::new();
        let mut key_points = Vec::new();
        let mut current_section = "";
        for line in body.lines() {
            if line.starts_with("## TL;DR") || line.starts_with("## Summary") {
                current_section = "tldr";
                continue;
            }
            if line.starts_with("## Key Points") {
                current_section = "key_points";
                continue;
            }
            if line.starts_with("## ") {
                current_section = "full";
            }
            match current_section {
                "tldr" => {
                    if !tldr.is_empty() {
                        tldr.push('\n');
                    }
                    tldr.push_str(line);
                }
                "key_points" => {
                    let trimmed = line.trim().trim_start_matches('-').trim();
                    if !trimmed.is_empty() {
                        key_points.push(trimmed.to_string());
                    }
                }
                _ => {}
            }
        }
        (tldr, key_points, body.to_string())
    }

    /// 获取指定细节级别的内容（owned String）
    pub fn get_content(&self, level: SkillLevel) -> String {
        match level {
            SkillLevel::TitleOnly => self.frontmatter.title.clone(),
            SkillLevel::Summary => self.tldr.clone(),
            SkillLevel::KeyPoints => {
                if self.key_points.is_empty() {
                    self.tldr.clone()
                } else {
                    self.key_points
                        .iter()
                        .map(|kp| format!("- {kp}"))
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            SkillLevel::Full => self.full_content.clone(),
        }
    }

    pub fn get_key_points(&self) -> &[String] {
        &self.key_points
    }
    pub fn name(&self) -> &str {
        &self.frontmatter.name
    }
    pub fn description(&self) -> &str {
        &self.frontmatter.description
    }
    pub fn tags(&self) -> &[String] {
        &self.frontmatter.tags
    }

    /// 匹配打分（被 SkillContract::match_score 调用，也对外暴露）
    pub fn match_score(&self, context: &str) -> f64 {
        self.matcher.compute_score(
            &self.frontmatter.name,
            context,
            &self.frontmatter.tags,
            &self.frontmatter.description,
            &self.tldr,
        )
    }
}

// ============================================
// impl SkillContract for FileSkill
// ============================================

impl st::SkillContract for FileSkill {
    fn name(&self) -> &str {
        &self.frontmatter.name
    }
    fn version(&self) -> &str {
        &self.frontmatter.version
    }
    fn description(&self) -> &str {
        &self.frontmatter.description
    }
    fn group(&self) -> &str {
        &self.frontmatter.group
    }
    fn tags(&self) -> &[String] {
        &self.frontmatter.tags
    }
    fn dependencies(&self) -> &[String] {
        &self.frontmatter.dependencies
    }
    fn injection_policy(&self) -> st::InjectionPolicy {
        self.frontmatter.injection_policy
    }
    fn quota_preference(&self) -> st::QuotaPreference {
        self.frontmatter.quota_preference
    }
    fn get_content(&self, level: st::SkillLevel) -> String {
        self.get_content(SkillLevel::from(level))
    }
    fn match_score(&self, context: &str) -> f64 {
        FileSkill::match_score(self, context)
    }
}

// ============================================
// 单元测试
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    const FULL_FRONTMATTER: &str = "\
---
name: test_skill
title: Test Skill
description: A skill for E2E testing
tags: [test, e2e]
version: 1.0.0
group: testing
injection_policy: Auto
quota_preference: Full
dependencies: [other_skill]
summary: One line summary
---

## TL;DR
A test skill that verifies the skills → assembler integration.

## Key Points
- Tests provider registration
- Tests query flow
- Tests content formatting

## Full Content
This is the full content used for SkillLevel::Full.
";

    #[test]
    fn parse_frontmatter_full() {
        let skill = FileSkill::parse(Path::new("test.skill.md"), FULL_FRONTMATTER).unwrap();
        assert_eq!(skill.frontmatter.name, "test_skill");
        assert_eq!(skill.frontmatter.title, "Test Skill");
        assert_eq!(skill.frontmatter.description, "A skill for E2E testing");
        assert_eq!(skill.frontmatter.tags, vec!["test", "e2e"]);
        assert_eq!(skill.frontmatter.version, "1.0.0");
        assert_eq!(skill.frontmatter.group, "testing");
        assert_eq!(
            skill.frontmatter.injection_policy,
            st::InjectionPolicy::Auto
        );
        assert_eq!(
            skill.frontmatter.quota_preference,
            st::QuotaPreference::Full
        );
        assert_eq!(skill.frontmatter.dependencies, vec!["other_skill"]);
        assert_eq!(
            skill.frontmatter.summary,
            Some("One line summary".to_string())
        );
        assert_eq!(skill.key_points.len(), 3);
        assert!(skill.full_content.contains("full content"));
    }

    #[test]
    fn parse_frontmatter_missing_title() {
        let content = "\
---
name: x
description: y
---
body
";
        let err = FileSkill::parse(Path::new("x.skill.md"), content).unwrap_err();
        assert!(err.contains("title"), "应提示 title 缺失: {}", err);
    }

    #[test]
    fn parse_frontmatter_missing_name() {
        let content = "\
---
title: X
description: y
---
body
";
        let err = FileSkill::parse(Path::new("x.skill.md"), content).unwrap_err();
        assert!(err.contains("name"), "应提示 name 缺失: {}", err);
    }

    #[test]
    fn parse_frontmatter_minimal() {
        let content = "\
---
name: x
title: X
description: Y
---
body
";
        let skill = FileSkill::parse(Path::new("x.skill.md"), content).unwrap();
        assert_eq!(skill.frontmatter.name, "x");
        assert_eq!(skill.frontmatter.title, "X");
        assert_eq!(skill.frontmatter.description, "Y");
        // 默认值
        assert!(skill.frontmatter.tags.is_empty());
        assert_eq!(skill.frontmatter.version, "1.0.0");
        assert_eq!(skill.frontmatter.group, "");
        assert_eq!(
            skill.frontmatter.injection_policy,
            st::InjectionPolicy::Auto
        );
        assert_eq!(
            skill.frontmatter.quota_preference,
            st::QuotaPreference::Full
        );
        assert!(skill.frontmatter.dependencies.is_empty());
        assert_eq!(skill.frontmatter.summary, None);
    }

    #[test]
    fn parse_frontmatter_with_tags_array() {
        let content = "\
---
name: x
title: X
description: Y
tags: [a, b, c]
---
body
";
        let skill = FileSkill::parse(Path::new("x.skill.md"), content).unwrap();
        assert_eq!(skill.frontmatter.tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_frontmatter_no_frontmatter() {
        let content = "no frontmatter at all";
        let err = FileSkill::parse(Path::new("x.skill.md"), content).unwrap_err();
        assert!(err.contains("frontmatter"), "应提示无 frontmatter: {}", err);
    }

    #[test]
    fn parse_frontmatter_injection_policy_always() {
        let content = "\
---
name: x
title: X
description: y
injection_policy: Always
---
body
";
        let skill = FileSkill::parse(Path::new("x.skill.md"), content).unwrap();
        assert_eq!(
            skill.frontmatter.injection_policy,
            st::InjectionPolicy::Always
        );
    }

    #[test]
    fn parse_frontmatter_unknown_injection_policy_falls_back_to_auto() {
        let content = "\
---
name: x
title: X
description: y
injection_policy: BogusValue
---
body
";
        let skill = FileSkill::parse(Path::new("x.skill.md"), content).unwrap();
        assert_eq!(
            skill.frontmatter.injection_policy,
            st::InjectionPolicy::Auto
        );
    }

    #[test]
    fn get_content_all_levels() {
        let skill = FileSkill::parse(Path::new("test.skill.md"), FULL_FRONTMATTER).unwrap();
        assert_eq!(skill.get_content(SkillLevel::TitleOnly), "Test Skill");
        assert!(skill
            .get_content(SkillLevel::Summary)
            .contains("test skill that verifies"));
        let kp = skill.get_content(SkillLevel::KeyPoints);
        assert!(kp.contains("Tests provider registration"));
        assert!(kp.contains("Tests query flow"));
        assert!(skill.get_content(SkillLevel::Full).contains("full content"));
    }

    #[test]
    fn get_content_keypoints_empty_falls_back_to_tldr() {
        let content = "\
---
name: x
title: X
description: y
---
## TL;DR
A skill with no key points.
";
        let skill = FileSkill::parse(Path::new("x.skill.md"), content).unwrap();
        let kp = skill.get_content(SkillLevel::KeyPoints);
        assert!(
            kp.contains("A skill with no key points"),
            "KeyPoints 缺时应回退到 TL;DR: {}",
            kp
        );
    }

    #[test]
    fn match_score_perfect_match() {
        let content = "\
---
name: rust_programming
title: Rust Programming
description: Learn Rust language
tags: [rust, programming]
---
## TL;DR
Rust is a systems programming language.
";
        let skill = FileSkill::parse(Path::new("rust.skill.md"), content).unwrap();
        let score = skill.match_score("I want to learn rust programming");
        assert!(score > 0.3, "完全匹配应得高分: {}", score);
    }

    #[test]
    fn match_score_no_match() {
        let content = "\
---
name: cooking
title: Cooking
description: How to cook food
tags: [cooking, food]
---
## TL;DR
This skill is about cooking.
";
        let skill = FileSkill::parse(Path::new("cooking.skill.md"), content).unwrap();
        let score = skill.match_score("I want to learn quantum physics");
        assert!(score < 0.1, "完全无关应得低分: {}", score);
    }

    #[test]
    fn skill_contract_trait_returns_correct_values() {
        let skill = FileSkill::parse(Path::new("test.skill.md"), FULL_FRONTMATTER).unwrap();
        // 通过 trait object 调用
        let boxed: std::sync::Arc<dyn st::SkillContract> = std::sync::Arc::new(skill);
        assert_eq!(boxed.name(), "test_skill");
        assert_eq!(boxed.version(), "1.0.0");
        assert_eq!(boxed.group(), "testing");
        assert_eq!(boxed.dependencies(), &["other_skill".to_string()]);
        assert_eq!(boxed.injection_policy(), st::InjectionPolicy::Auto);
        assert_eq!(boxed.quota_preference(), st::QuotaPreference::Full);
        let full = boxed.get_content(st::SkillLevel::Full);
        assert!(full.contains("full content"));
    }
}
