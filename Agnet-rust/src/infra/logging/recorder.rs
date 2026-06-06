/*!
 * Logger — EventRecorder trait
 *
 * 功能描述：定义事件记录器的契约接口。
 * 实现者可以是 FileLogger、NoopLogger 或远程日志后端。
 */

use std::fmt::Debug;

use super::event::SystemEvent;

/// Trait for recording system events.
///
/// Implementations include:
///   - `FileLogger` — writes events to JSONL files
///   - `NoopLogger` — silently discards all events (testing / disabled)
pub trait EventRecorder: Send + Sync + Debug {
    /// Record a system event. Must be non-blocking.
    fn record(&self, event: SystemEvent);

    /// Record a system event with session/trace context.
    /// 默认实现忽略额外上下文，直接调用 record()
    fn record_with_ctx(
        &self,
        event: SystemEvent,
        _session_id: Option<String>,
        _trace_id: Option<String>,
    ) {
        self.record(event);
    }
}
