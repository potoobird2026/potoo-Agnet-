/*! MCP JSON-RPC 2.0 协议类型 */
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// MCP 协议版本——B-1 改名（原 PROTOCOL_VERSION），MCP §6.3 #4 要求
pub const MCP_PROTOCOL_VERSION: &str = "2024-11-05";

/// 客户端标识——B-1 新增
pub const CLIENT_NAME: &str = "aagnet";
pub const CLIENT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const METHOD_INITIALIZE: &str = "initialize";
pub const METHOD_TOOLS_LIST: &str = "tools/list";
pub const METHOD_TOOLS_CALL: &str = "tools/call";

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ToolManifest {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub input_schema: Value,
}

/// B-1: 客户端标识（initialize 请求用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    pub version: String,
}

/// B-1: initialize 请求参数（强类型化）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: Value,
    pub client_info: ClientInfo,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: &str, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params,
        }
    }
}

impl JsonRpcResponse {
    pub fn new(id: u64, result: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result,
            error: None,
        }
    }

    /// B-1: 从 JSON 字符串反序列化
    pub fn from_json(s: &str) -> Result<Self, String> {
        serde_json::from_str(s).map_err(|e| format!("JsonRpcResponse 反序列化失败: {}", e))
    }

    /// B-1: 是否包含 error 字段
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// B-1: 提取错误消息
    pub fn error_message(&self) -> Option<String> {
        self.error
            .as_ref()
            .map(|e| format!("{}: {}", e.code, e.message))
    }
}
