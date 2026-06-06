/*! MCP —— Model Context Protocol 连接器 */
pub mod bundle;
pub mod config;
pub mod connector;
pub mod protocol;
pub mod proxy;
pub mod service;
pub use bundle::McpBundleImpl;
pub use service::McpService;
