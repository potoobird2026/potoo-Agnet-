/*!
 * shared_types/skills —— Skills 服务跨插件契约
 *
 * 定义内容（按 protocol-shared_types契约协议.md §1）：
 * 1. Provider key 常量 PROVIDER_SKILLS
 * 2. Provider trait SkillContract（单个技能的契约）
 * 3. Provider trait SkillsContractBundle（技能集合的契约）
 * 4. 跨插件数据结构 SkillLevel / InjectionPolicy / QuotaPreference
 *
 * 归属：shared_types（中立层，不归属 SkillsService 也不归属 Assembler）
 * 服务方：SkillsService::start() 注册 Arc<DynProvider<dyn SkillsContractBundle>>
 * 消费方：Assembler Slot 的 SkillsProvider::provide() 中查找并 downcast
 *
 * 红线遵守：
 * - K-R01: PROVIDER_SKILLS 常量在此定义，调用方禁止用裸字符串
 * - K-R02: 跨插件 key 必须先在此定义再被引用
 * - T-R01: trait 在此定义，禁止在 services/skills/ 或 slots/ 内部定义
 * - T-R02: 谁先开发谁定义 trait——本计划先定义
 * - T-R03: trait 不写归属注释
 * - D-R01: 用现有的 DynProvider<T>，不造 DynSkillProvider
 */

use std::sync::Arc;

use serde::{Deserialize, Serialize};

// ============================================
// Provider key 常量
// ============================================

/// Provider key 常量——Skills 服务通过此 key 注册技能集合，
/// Assembler Slot 通过此 key 查找技能集合。
///
/// 协议参考：protocol-shared_types契约协议.md §2
pub const PROVIDER_SKILLS: &str = "skills";

// ============================================
// 跨插件数据结构
// ============================================

/// 技能注入策略——决定技能是否在 System Prompt 中出现
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum InjectionPolicy {
    /// 自动（默认）：按 match_score 阈值决定
    #[default]
    Auto,
    /// 总是注入（无视 match_score）
    Always,
    /// 从不注入
    Never,
}

/// 技能配额偏好——当 LLM context window 受限时，技能应如何压缩
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum QuotaPreference {
    /// 完整内容（默认）
    #[default]
    Full,
    /// 摘要（TL;DR）
    Summary,
    /// 仅标题
    TitleOnly,
}

/// 技能细节级别——get_content() 的入参，决定返回的粒度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkillLevel {
    /// 仅技能标题
    TitleOnly,
    /// TL;DR 摘要
    Summary,
    /// Key Points 列表
    KeyPoints,
    /// 完整内容
    Full,
}

// ============================================
// Provider trait
// ============================================

/// 单个技能契约——每个技能是一个 SkillContract 实例
///
/// 由 SkillsService::start() 通过 SkillsContractBundle 暴露给 Assembler 等消费者。
pub trait SkillContract: Send + Sync {
    /// 技能唯一标识（与 .skill.md 文件名前缀一致）
    fn name(&self) -> &str;
    /// 技能版本（语义版本字符串，如 "1.0.0"）
    fn version(&self) -> &str;
    /// 技能描述（来自 frontmatter）
    fn description(&self) -> &str;
    /// 技能分组（默认空字符串）
    fn group(&self) -> &str;
    /// 技能标签（用于匹配打分）
    fn tags(&self) -> &[String];
    /// 技能依赖列表（其他技能的 name）
    fn dependencies(&self) -> &[String];
    /// 注入策略
    fn injection_policy(&self) -> InjectionPolicy;
    /// 配额偏好
    fn quota_preference(&self) -> QuotaPreference;
    /// 获取指定细节级别的内容
    ///
    /// 返回 owned String（不是引用），避免生命周期问题——调用方可以自由持有。
    fn get_content(&self, level: SkillLevel) -> String;
    /// 对给定上下文做匹配打分（0.0 - 1.0）
    ///
    /// 0.0 表示不相关，1.0 表示完全匹配。
    fn match_score(&self, context: &str) -> f64;
}

/// 技能集合契约——Assembler 通过此 trait 一次性获取所有技能
///
/// 注册时以 `Arc<dyn SkillsContractBundle>` 形式注册到 PROVIDER_SKILLS。
pub trait SkillsContractBundle: Send + Sync {
    /// 获取当前所有技能列表
    fn all(&self) -> Vec<Arc<dyn SkillContract>>;
}

// 不再需要独立的 DynSkillProvider——统一使用 shared_types::DynProvider<T>。
// 参见 protocol-shared_types契约协议.md §4。
