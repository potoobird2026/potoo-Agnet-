use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::types::Timestamp;
use crate::shared_types::ToolCall;

/// LLM 的一次推理结果（ReAct 模式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Thought {
    /// 需要调用工具
    Action {
        action: Action,
        reasoning: String,
        generated_at: Timestamp,
    },
    /// 得到最终答案
    Final {
        answer: String,
        reasoning: String,
        generated_at: Timestamp,
    },
}

impl Thought {
    pub fn generated_at(&self) -> Timestamp {
        match self {
            Thought::Action { generated_at, .. } => *generated_at,
            Thought::Final { generated_at, .. } => *generated_at,
        }
    }
}

/// 工具调用请求（ReAct 模式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub tool_name: String,
    pub arguments: Value,
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    pub created_at: Timestamp,
}

impl Action {
    pub fn new(tool_name: impl Into<String>, arguments: Value) -> Self {
        Self {
            tool_name: tool_name.into(),
            arguments,
            tool_call_id: None,
            tool_calls: None,
            created_at: Timestamp::now(),
        }
    }

    pub fn with_tool_call_id(mut self, tool_call_id: String) -> Self {
        self.tool_call_id = Some(tool_call_id);
        self
    }

    pub fn with_created_at(mut self, created_at: Timestamp) -> Self {
        self.created_at = created_at;
        self
    }
}

/// 工具执行结果枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ActionResult {
    Success {
        output: String,
        metadata: Option<Value>,
    },
    RetryableError {
        error: String,
    },
    FatalError {
        error: String,
    },
}

/// 工具执行结果（ReAct 模式）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Observation {
    pub action: Action,
    pub result: ActionResult,
    pub completed_at: Timestamp,
}

impl Observation {
    pub fn success(action: Action, output: impl Into<String>) -> Self {
        Self {
            action,
            result: ActionResult::Success {
                output: output.into(),
                metadata: None,
            },
            completed_at: Timestamp::now(),
        }
    }

    pub fn retryable_error(action: Action, error: impl Into<String>) -> Self {
        Self {
            action,
            result: ActionResult::RetryableError {
                error: error.into(),
            },
            completed_at: Timestamp::now(),
        }
    }

    pub fn fatal_error(action: Action, error: impl Into<String>) -> Self {
        Self {
            action,
            result: ActionResult::FatalError {
                error: error.into(),
            },
            completed_at: Timestamp::now(),
        }
    }

    pub fn with_completed_at(mut self, completed_at: Timestamp) -> Self {
        self.completed_at = completed_at;
        self
    }
}
