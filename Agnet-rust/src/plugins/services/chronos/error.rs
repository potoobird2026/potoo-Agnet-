/*!
 * ChronosError —— Chronos 服务错误类型
 */

use std::fmt;

/// Chronos 错误
#[derive(Debug, Clone)]
pub struct ChronosError {
    /// 错误类型
    pub kind: ChronosErrorKind,
    /// 错误描述
    pub description: String,
    /// 修复建议
    pub recommendation: Option<String>,
}

impl ChronosError {
    pub fn new(kind: ChronosErrorKind, description: impl Into<String>) -> Self {
        Self {
            kind,
            description: description.into(),
            recommendation: None,
        }
    }

    pub fn with_recommendation(mut self, rec: impl Into<String>) -> Self {
        self.recommendation = Some(rec.into());
        self
    }
}

impl fmt::Display for ChronosError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{:?}] {}", self.kind, self.description)
    }
}

impl std::error::Error for ChronosError {}

/// 错误类型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChronosErrorKind {
    /// 配置无效
    ConfigInvalid,
    /// 初始化失败
    InitFailed,
    /// 任务队列操作失败
    TaskQueueError,
    /// 持久化失败
    PersistenceError,
    /// 决策错误
    DecisionError,
    /// 执行失败
    ExecutionError,
    /// 内部错误
    Internal,
}
