/*!
 * Chronos —— 自适应定时调度服务
 *
 * 遵循《Service 集成协议》：
 *   - 插件入口：ChronosServicePlugin（impl ServicePlugin）
 *   - 配置：ChronosConfig
 *   - 错误：ChronosError
 *
 * 内部组件通过 Orchestrator 编排，对外不可见。
 */

pub mod components;
pub mod config;
pub mod error;
mod orchestrator;
mod service;
pub mod types;

// ── 协议暴露（仅 3 样）──
pub use config::ChronosConfig;
pub use error::{ChronosError, ChronosErrorKind};
pub use service::ChronosServicePlugin;
