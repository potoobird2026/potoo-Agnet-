/*! SkillInjectionProvider —— 旧版技能注入逻辑
 *
 * Phase A 改动（A-4）：
 * - 删除 matcher 字段（matcher 移入 FileSkill）
 * - new() 不再接收 skills 参数（matcher 已在 FileSkill::parse 中初始化）
 * - select_skills 调 skill.match_score() 而非 self.matcher.compute_score()
 *
 * 注意：本文件在 Phase C 会被进一步重写：
 * - 或重写为 ContextProvider trait 实现
 * - 或在 Assembler 侧重写为 SkillsProvider（不再依赖 SkillInjectionProvider）
 *   详见 docs/development/integration_plans/Skills→Assembler.md C-1
 */
use super::config::SkillConfig;
use super::file_skill::{FileSkill, SkillLevel};
use crate::shared_types::Message;

pub struct SkillInjectionProvider {
    config: SkillConfig,
}

impl SkillInjectionProvider {
    pub fn new(config: SkillConfig) -> Self {
        Self { config }
    }

    pub fn select_skills(
        &self,
        messages: &[Message],
        available_skills: &[FileSkill],
    ) -> Vec<(String, f64, SkillLevel)> {
        let context: String = messages
            .iter()
            .rev()
            .take(5)
            .map(|m| m.text_content())
            .collect::<Vec<_>>()
            .join(" ");
        let mut scored: Vec<_> = available_skills
            .iter()
            .map(|skill| {
                let score = skill.match_score(&context);
                (skill.name().to_string(), score, skill.clone())
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.retain(|(_, s, _)| *s as f32 >= self.config.min_match_score);
        scored.truncate(self.config.max_candidates);
        scored
            .into_iter()
            .take(self.config.max_skills)
            .map(|(n, s, _skill)| {
                let level = if s > 0.7 {
                    SkillLevel::KeyPoints
                } else if s > 0.4 {
                    SkillLevel::Summary
                } else {
                    SkillLevel::TitleOnly
                };
                (n, s, level)
            })
            .collect()
    }

    pub fn format_injection(
        &self,
        selected: &[(String, f64, SkillLevel)],
        skills: &[FileSkill],
    ) -> String {
        let mut out = String::from("## Available Skills\n\n");
        for (name, score, level) in selected {
            if let Some(skill) = skills.iter().find(|s| s.name() == name) {
                out.push_str(&format!("- **{}** (match: {:.2})\n", name, score));
                match level {
                    SkillLevel::Summary => {
                        out.push_str(&format!("  {}\n", skill.tldr));
                    }
                    SkillLevel::KeyPoints => {
                        for kp in skill.get_key_points() {
                            out.push_str(&format!("  - {}\n", kp));
                        }
                    }
                    _ => {}
                }
            }
        }
        out
    }
}
