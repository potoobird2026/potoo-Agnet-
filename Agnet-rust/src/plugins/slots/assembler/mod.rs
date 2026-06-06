/*! AssemblerSlot —— 上下文组装器

Pipeline CONTEXT 阶段 SlotPlugin，在 ToolRegistrySlot 之后、LlmThinkerSlot 之前运行。
设计文档：docs/slots/assembler/ConversationAssembler-开发设计文档.md
*/

mod assembly;
mod compaction;
pub mod config;
mod output_adapters;
pub mod providers;
mod rule_pool;
pub mod slot;

pub use crate::shared_types::assembler::AssemblerConfig;
pub use slot::AssemblerSlot;
