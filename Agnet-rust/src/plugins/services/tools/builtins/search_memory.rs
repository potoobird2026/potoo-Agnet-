/*! search_memory builtin */
pub const NAME: &str = "search_memory";
pub const DESCRIPTION: &str = "Search agent memory";
pub fn parameters() -> serde_json::Value {
    serde_json::json!({"query":{"type":"string"},"top_k":{"type":"integer","default":5}})
}
pub async fn execute(args: serde_json::Value) -> Result<serde_json::Value, String> {
    let query = args["query"].as_str().ok_or("缺少 query 参数")?;
    let _top_k = args["top_k"].as_u64().unwrap_or(5) as usize;
    // 搜索由 MemoryService 的 Provider 提供，这里占位
    Ok(serde_json::json!({"query": query, "results": []}))
}
