/*! execute_command builtin */
use super::super::platform::NativePlatform;
pub const NAME: &str = "execute_command";
pub const DESCRIPTION: &str = "Execute a shell command";
pub fn parameters() -> serde_json::Value {
    serde_json::json!({"command":{"type":"string"},"args":{"type":"array","items":{"type":"string"}}})
}
pub async fn execute(
    args: serde_json::Value,
    platform: &NativePlatform,
) -> Result<serde_json::Value, String> {
    let command = args["command"].as_str().ok_or("缺少 command 参数")?;
    let shell_args: Vec<String> = args["args"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    let (shell, shell_flag) = platform.default_shell();
    let mut cmd = std::process::Command::new(shell);
    cmd.args(shell_flag);
    cmd.arg(command);
    for a in &shell_args {
        cmd.arg(a);
    }
    let output = cmd.output().map_err(|e| format!("命令执行失败: {}", e))?;
    Ok(
        serde_json::json!({"stdout": String::from_utf8_lossy(&output.stdout), "stderr": String::from_utf8_lossy(&output.stderr), "exit_code": output.status.code().unwrap_or(-1)}),
    )
}
