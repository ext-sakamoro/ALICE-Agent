use crate::permission::PermissionLevel;
use crate::tools::ToolSpec;
use serde_json::{json, Value};
use std::process::Command;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "bash".to_string(),
        description: "Execute a bash command in the current workspace.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "command": { "type": "string", "description": "The command to execute" },
                "timeout": { "type": "integer", "description": "Timeout in seconds" }
            },
            "required": ["command"]
        }),
        permission: PermissionLevel::FullAccess,
    }
}

pub fn execute(input: &Value) -> Result<String, String> {
    let command = input
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or("missing 'command' parameter")?;

    let _timeout_secs = input
        .get("timeout")
        .and_then(|v| v.as_u64())
        .unwrap_or(120);

    eprintln!("  > bash: {command}");

    let output = Command::new("bash")
        .arg("-c")
        .arg(command)
        .output()
        .map_err(|e| format!("failed to execute command: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    let mut result = String::new();
    if !stdout.is_empty() {
        result.push_str(&stdout);
    }
    if !stderr.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str("[stderr] ");
        result.push_str(&stderr);
    }

    if output.status.success() {
        // 出力が大きすぎる場合は切り詰め
        if result.len() > 50_000 {
            result.truncate(50_000);
            result.push_str("\n... (truncated)");
        }
        Ok(result)
    } else {
        let code = output.status.code().unwrap_or(-1);
        Err(format!("exit code {code}\n{result}"))
    }
}
