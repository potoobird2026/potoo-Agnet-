// ============================================
// 模块：memory_saver 槽口
//
// 模块职责：
// 在 Pipeline Memorize 阶段将对话上下文持久化到记忆系统
//
// 模块边界：
// - 本模块负责：消息持久化、观察结果存储、向量索引触发、经验提取
// - 本模块不负责：记忆检索（由 llm_thinker 通过 Provider 读取）、
//                 记忆压缩（由 compression 服务处理）
//
// 依赖 Provider：
// - "memory"（由 MemoryService 注册，提供 MemoryProvider trait）
//   注意：MemoryProvider trait 定义在 shared_types 中，不在本模块定义
//   本模块 provider.rs 只做 re-export
//
// 被依赖模块：
// - compression_hook 在同一 Memorize 阶段运行，依赖本模块完成持久化
//
// 核心层实现：
// - SlotPlugin → MemorySaverSlot（无状态，无内部组件）
//
// 错误类型：见 error.rs
// 数据类型：见 types.rs
// Provider 接口：见 provider.rs（re-export from shared_types）
//
// 协议合规：
// - S-R03 合规：持久化进度（last_persisted_count）存入 StepContext，不在 Slot 字段中
// - 组件协议 §0：本槽口无子模块，不需要 Orchestrator/Component/AccessPoint
// - 组件协议 §6：mod.rs 只暴露 MemorySaverSlot + MemorySaverConfig
// ============================================

pub mod config;
pub mod error;
pub mod plugin;
pub mod types;

pub use config::MemorySaverConfig;
pub use plugin::MemorySaverSlot;
