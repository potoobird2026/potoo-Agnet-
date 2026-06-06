use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::core::types::Timestamp;

/// 权限字符串（由各 Provider 自行定义和校验）
pub type Permission = String;

/// 统一组件描述符——所有组件自描述的单一数据结构
///
/// 设计意图：
/// - 包含 LLM/UI/监控 所需的全部元数据
/// - 该 trait 的元数据方法（documentation/examples/permissions 等）通过 Describe trait
///   转换为 ComponentDescriptor，注册到 MetadataBus
/// - `extensions` 提供可扩展的类型专属数据（平台信息、注入策略等）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentDescriptor {
    /// 全局唯一 ID：`aagnet.<kind>.<name>`，如 `aagnet.tool.read_file`
    pub id: String,
    /// 组件类型
    pub kind: DescriptorKind,
    /// 语义化版本（如 "1.0.0"）
    pub version: String,
    /// 简短摘要（工具卡片、列表展示）
    pub summary: String,
    /// 完整 Markdown 文档（送给 LLM 的详细描述）
    pub documentation: String,
    /// 能力标签：`["file", "read", "text"]`
    pub capabilities: Vec<String>,
    /// JSON Schema（工具必有；技术 MCP/平台 = None）
    pub parameters: Option<Value>,
    /// 使用示例
    pub examples: Vec<ComponentExample>,
    /// 所需权限
    pub required_permissions: Vec<Permission>,
    /// 可见性控制
    pub visibility: ComponentVisibility,
    /// 运行时状态（由系统维护，非组件自填）
    pub status: ComponentStatus,
    /// 扩展属性：特定组件类型可填充平台信息、注入策略、配额偏好等
    pub extensions: HashMap<String, Value>,
}

/// 描述符组件类型——元数据分类，涵盖所有可注册到总线的组件种类
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum DescriptorKind {
    Tool,
    Skill,
    McpConnector,
    Platform,
    MemoryStore,
    BackgroundService,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentExample {
    /// 场景说明
    pub description: String,
    /// 输入参数
    pub input: Value,
    /// 期望输出
    pub expected_output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ComponentVisibility {
    Hidden,
    DebugOnly,
    AlwaysVisible,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComponentStatus {
    pub healthy: bool,
    pub circuit_breaker_open: bool,
    pub last_heartbeat: Option<Timestamp>,
}

impl Default for ComponentStatus {
    fn default() -> Self {
        Self {
            healthy: true,
            circuit_breaker_open: false,
            last_heartbeat: None,
        }
    }
}
