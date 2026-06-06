/*! Compression —— 上下文压缩引擎（双层架构：ServicePlugin + SlotPlugin） */
pub mod components;
pub mod config;
mod service;
pub mod services;
mod slot;
pub mod types;

pub use config::CompressionConfig;
pub use service::CompressionService;
pub use slot::CompressionHookSlot;
pub use types::{CompressResult, HookEvent, ServiceState};
