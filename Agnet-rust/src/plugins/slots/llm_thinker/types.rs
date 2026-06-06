//! LlmThinkerSlot 内部类型
//!
//! 设计文档 §1.2：仅保留 Slot 内部使用的类型。
//! 跨插件类型已迁移到 shared_types/llm.rs。

use serde::{Deserialize, Serialize};

use crate::shared_types::{Observation, Thought};

// ── LLM Thinker 专属类型（不跨插件共享，留在本模块） ──

/// 一轮完整的 Think + Observe（ReAct 模式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Turn {
    pub thought: Thought,
    pub observation: Observation,
}

/// 模块级配置（槽位内部使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleConfig {
    pub raw: serde_json::Value,
}
