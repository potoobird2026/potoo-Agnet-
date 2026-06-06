/*! Skills —— 技能注入服务 */
pub mod config;
pub mod file_skill;
pub mod matcher;
pub mod provider;
mod service;

pub use config::SkillConfig;
pub use file_skill::{FileSkill, SkillLevel};
pub use matcher::SkillMatcher;
pub use provider::SkillInjectionProvider;
pub use service::SkillsService;
