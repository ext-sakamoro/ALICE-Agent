use crate::permission::PermissionLevel;
use crate::tools::ToolSpec;
use serde_json::{json, Value};
use std::path::Path;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "write_file".to_string(),
        description: "Write content to a file.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path to write" },
                "content": { "type": "string", "description": "Content to write" }
            },
            "required": ["path", "content"]
        }),
        permission: PermissionLevel::WorkspaceWrite,
    }
}

pub fn execute(input: &Value, working_dir: &str) -> Result<String, String> {
    let path_str = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("missing 'path' parameter")?;
    let content = input
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or("missing 'content' parameter")?;

    let path = if Path::new(path_str).is_absolute() {
        Path::new(path_str).to_path_buf()
    } else {
        Path::new(working_dir).join(path_str)
    };

    eprintln!("  > write_file: {}", path.display());

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("mkdir error: {e}"))?;
    }

    std::fs::write(&path, content).map_err(|e| format!("write error: {e}"))?;

    Ok(format!("wrote {} bytes to {}", content.len(), path.display()))
}
