/*! read_file builtin */
pub const NAME: &str = "read_file";
pub const DESCRIPTION: &str = "Read contents of a file";
pub fn parameters() -> serde_json::Value {
    serde_json::json!({"path": {"type":"string","description":"File path"}})
}
pub async fn execute(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let path = args["path"].as_str().ok_or("缺少 path 参数")?;
    let content = tokio::fs::read_to_string(path)
        .await
        .map_err(|e| format!("读取失败: {}", e))?;
    Ok(serde_json::json!({"content": content}))
}
