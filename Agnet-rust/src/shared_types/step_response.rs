use crate::core::types::Timestamp;

/// Pipeline 执行结果 —— 携带最终回复内容
#[derive(Debug, Clone)]
pub enum StepResponse {
    /// Pipeline 正常执行完成
    Completed {
        response: String,
        completed_at: Timestamp,
    },
    /// Slot 请求中断当前 Step
    Interrupted {
        reason: String,
        response: String,
        completed_at: Timestamp,
    },
    /// 请求重新执行当前 Step
    RestartRequested {
        session_id: String,
        completed_at: Timestamp,
    },
    /// 达到系统限制（如 max_turns）
    LimitReached {
        detail: String,
        response: String,
        completed_at: Timestamp,
    },
}

impl StepResponse {
    pub fn completed_at(&self) -> Timestamp {
        match self {
            StepResponse::Completed { completed_at, .. } => *completed_at,
            StepResponse::Interrupted { completed_at, .. } => *completed_at,
            StepResponse::RestartRequested { completed_at, .. } => *completed_at,
            StepResponse::LimitReached { completed_at, .. } => *completed_at,
        }
    }

    /// 提取回复内容
    pub fn response(&self) -> &str {
        match self {
            StepResponse::Completed { response, .. } => response,
            StepResponse::Interrupted { response, .. } => response,
            StepResponse::RestartRequested { .. } => "",
            StepResponse::LimitReached { response, .. } => response,
        }
    }

    /// 设置回复内容（由 runtime.step() 在 pipeline 完成后填入）
    pub fn with_response(mut self, response: String) -> Self {
        match &mut self {
            StepResponse::Completed { response: r, .. }
            | StepResponse::Interrupted { response: r, .. }
            | StepResponse::LimitReached { response: r, .. } => *r = response,
            StepResponse::RestartRequested { .. } => {}
        }
        self
    }
}
