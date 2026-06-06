use serde::Deserialize;

/// 设计文档 §5.1——数字阈值从配置读取
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ReactLoopConfig {
    pub max_turns: Option<usize>,
}

/// 设计文档 §1.5——SlotPlugin 错误类型
#[derive(Debug)]
pub enum ReactLoopError {
    Config(String),
    Internal(String),
}

/// 设计文档 §3.2——循环决策结果
#[derive(Debug, Clone, PartialEq)]
pub enum LoopAction {
    Continue,
    JumpToThink,
    ForceBreak,
}

/// 设计文档 §5.1——数字阈值使用常量集中管理
pub const DEFAULT_MAX_TURNS: usize = 10;

/// 设计文档 AI 宪法 §3.d——日志前缀统一管理
pub const LOG_PREFIX: &str = "[react_loop]";

/// 设计文档 §3.1——配置键名集中管理，禁止散落字面量
#[allow(dead_code)]
pub const CFG_KEY_MAX_TURNS: &str = "max_turns";
