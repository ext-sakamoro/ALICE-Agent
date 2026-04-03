use crate::permission::PermissionLevel;
use crate::tools::ToolSpec;
use serde_json::{json, Value};
use std::process::Command;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "grep_search".to_string(),
        description: "Search file contents with a regex pattern using ripgrep.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Regex pattern to search" },
                "path": { "type": "string", "description": "Directory or file to search" },
                "glob": { "type": "string", "description": "File glob filter (e.g. '*.rs')" }
            },
            "required": ["pattern"]
        }),
        permission: PermissionLevel::ReadOnly,
    }
}

pub fn execute(input: &Value, working_dir: &str) -> Result<String, String> {
    let pattern = input
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or("missing 'pattern' parameter")?;

    let search_path = input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(working_dir);

    eprintln!("  > grep: {pattern} in {search_path}");

    let mut cmd = Command::new("rg");
    cmd.arg("--no-heading")
        .arg("--line-number")
        .arg("--max-count=50")
        .arg("--max-filesize=1M");

    if let Some(file_glob) = input.get("glob").and_then(|v| v.as_str()) {
        cmd.arg("--glob").arg(file_glob);
    }

    cmd.arg(pattern).arg(search_path);

    let output = cmd
        .output()
        .map_err(|e| format!("ripgrep execution failed: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if stdout.is_empty() {
        Ok("no matches found".to_string())
    } else {
        let result = stdout.to_string();
        if result.len() > 50_000 {
            Ok(format!("{}\n... (truncated)", &result[..50_000]))
        } else {
            Ok(result)
        }
    }
}
