/*! assembler —— ConversationAssembler 共享契约层

所有跨插件类型定义在此处，遵循 shared_types契约协议。
设计文档：docs/slots/assembler/ConversationAssembler-开发设计文档.md §3
*/

pub mod adapter;
pub mod compaction;
pub mod config;
pub mod context;
pub mod report;
pub mod rule_pool;

pub use adapter::*;
pub use compaction::*;
pub use config::*;
pub use context::*;
pub use report::*;
pub use rule_pool::*;
