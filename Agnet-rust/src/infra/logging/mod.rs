/*!
 * Logger ?aagnet 统一业务事件日志系统
 *
 * 功能描述：为框架提供统一、规范、可配置的结构化业务事件日志系统? * 解决各模?内部有数据、外部无可见?的问题? *
 * 设计原则? *   - 零侵入：通过全局注册表，不改变现有模块的内部数据结构
 *   - 统一输出：所有业务事件汇?JSONL 文件? *   - 可配置：输出路径、滚动策略、事件级别过滤等
 *   - 商用级：异步写入、文件滚动、崩溃恢? *
 *
 * 模块结构:
 *   - config.rs   : LoggerConfig
 *   - event.rs    : SystemEvent 枚举 + 所有事?payload 定义
 *   - recorder.rs : EventRecorder trait
 *   - file_logger.rs : EventLogger 实现 + 全局注册? *   - writer.rs   : 异步写入?+ 文件滚动
 *   - aggregator.rs : 周期性汇总器
 *   - retention.rs  : 保留策略
 *
 * Usage:
 *   // Startup
 *   EventLogger::init(LoggerConfig::default());
 *   record_event(SystemEvent::SystemStartup(..));
 *
 *   // Anywhere in the codebase
 *   record_event(SystemEvent::CompressionCompleted { .. });
 */

pub mod aggregator;
pub mod config;
pub mod event;
pub mod file_logger;
pub mod recorder;
pub mod retention;
pub mod writer;

pub use config::{EventLevel, LoggerConfig, RotationPolicy};
pub use event::{LogEntry, SystemEvent};
pub use file_logger::{init, init_with, record_event, EventLogger};
pub use recorder::EventRecorder;
