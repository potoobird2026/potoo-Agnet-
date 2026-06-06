/*!
 * Logger ?EventLogger 核心实现
 *
 * 功能描述：EventLogger 实现 EventRecorder 接口? * 通过无界 mpsc 通道将事件传递给后台写入任务? * 提供全局注册表，任何模块无需构造函数注入即可调?record()? */

use std::sync::{Arc, OnceLock};

use tokio::sync::mpsc;

use super::config::LoggerConfig;
use super::event::{LogEntry, SystemEvent};
use super::recorder::EventRecorder;
use super::writer::AsyncWriter;

/// Global logger instance.
static GLOBAL_LOGGER: OnceLock<Arc<dyn EventRecorder>> = OnceLock::new();

/// Initialize the global file logger.
pub fn init(config: LoggerConfig) {
    let logger = EventLogger::spawn(config);
    GLOBAL_LOGGER
        .set(logger)
        .expect("logger already initialized");
}

/// Record an event through the global logger.
/// No-op if not initialized.
pub fn record_event(event: SystemEvent) {
    if let Some(logger) = GLOBAL_LOGGER.get() {
        logger.record(event);
    }
}

/// Record an event with session/trace context
pub fn record_event_with_ctx(
    event: SystemEvent,
    session_id: Option<String>,
    trace_id: Option<String>,
) {
    if let Some(logger) = GLOBAL_LOGGER.get() {
        logger.record_with_ctx(event, session_id, trace_id);
    }
}

/// Initialize the global logger with a custom recorder (for testing).
pub fn init_with(recorder: Arc<dyn EventRecorder>) {
    let _ = GLOBAL_LOGGER.set(recorder);
}

/// The file-based event logger.
#[derive(Debug)]
pub struct EventLogger {
    tx: mpsc::UnboundedSender<LogEntry>,
    config: LoggerConfig,
}

impl EventLogger {
    pub fn spawn(config: LoggerConfig) -> Arc<dyn EventRecorder> {
        let (tx, rx) = mpsc::unbounded_channel::<LogEntry>();
        let writer_config = config.clone();
        tokio::spawn(async move {
            let mut writer = AsyncWriter::new(writer_config);
            writer.run(rx).await;
        });
        let logger = Self { tx, config };
        Arc::new(logger)
    }
}

impl EventRecorder for EventLogger {
    fn record(&self, event: SystemEvent) {
        if !self.config.enabled {
            return;
        }
        let level = event.level();
        if level < self.config.min_level {
            return;
        }
        let meta = event.into_meta();
        let entry = LogEntry::from_meta(meta, None);
        let _ = self.tx.send(entry);
    }

    fn record_with_ctx(
        &self,
        event: SystemEvent,
        session_id: Option<String>,
        _trace_id: Option<String>,
    ) {
        if !self.config.enabled {
            return;
        }
        let level = event.level();
        if level < self.config.min_level {
            return;
        }
        let meta = event.into_meta();
        let entry = LogEntry::from_meta(meta, session_id);
        let _ = self.tx.send(entry);
    }
}
