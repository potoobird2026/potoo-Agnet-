/*! Skills 配置 */
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillConfig {
    #[serde(default = "default_skills_dir")]
    pub skills_dir: PathBuf,
    #[serde(default = "default_budget")]
    pub skill_budget_ratio: f64,
    #[serde(default = "default_max_skills")]
    pub max_skills: usize,
    #[serde(default = "default_false")]
    pub allow_external_skills: bool,
    #[serde(default = "default_20")]
    pub max_candidates: usize,
    #[serde(default = "default_min_score")]
    pub min_match_score: f32,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
fn default_skills_dir() -> PathBuf {
    PathBuf::from(".")
}
fn default_budget() -> f64 {
    0.05
}
fn default_max_skills() -> usize {
    3
}
fn default_false() -> bool {
    false
}
fn default_true() -> bool {
    true
}
fn default_20() -> usize {
    20
}
fn default_min_score() -> f32 {
    0.15
}

impl Default for SkillConfig {
    fn default() -> Self {
        Self {
            skills_dir: default_skills_dir(),
            skill_budget_ratio: 0.05,
            max_skills: 3,
            allow_external_skills: false,
            max_candidates: 20,
            min_match_score: 0.15,
            enabled: true,
        }
    }
}
impl SkillConfig {
    /// 用 data_dir 锚定相对路径（不再依赖 current_dir）
    ///
    /// 调用方（SkillsService::init）传入 `ctx.data_dir`。
    /// 保留 ~/ 展开作为兜底（用户可显式写 ~/... 覆盖默认 data_dir）。
    pub fn resolve_paths(&mut self, data_dir: &Path) {
        let home = dirs::home_dir().unwrap_or_default();
        if let Some(s) = self.skills_dir.to_str() {
            if let Some(stripped) = s.strip_prefix("~/") {
                self.skills_dir = home.join(stripped);
            }
        }
        if self.skills_dir.is_relative() {
            self.skills_dir = data_dir.join(&self.skills_dir);
        }
    }
}
