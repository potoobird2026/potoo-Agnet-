/*! write_file builtin */
pub const NAME: &str = "write_file";
pub const DESCRIPTION: &str = "Write content to a file";
pub fn parameters() -> serde_json::Value {
    serde_json::json!({"path":{"type":"string"},"content":{"type":"string"}})
}
pub async fn execute(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let path = args["path"].as_str().ok_or("缺少 path 参数")?;
    let content = args["content"].as_str().ok_or("缺少 content 参数")?;
    tokio::fs::write(path, content)
        .await
        .map_err(|e| format!("写入失败: {}", e))?;
    Ok(serde_json::json!({"success": true}))
}
