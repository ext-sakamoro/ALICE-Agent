use crate::permission::PermissionLevel;
use crate::tools::ToolSpec;
use serde_json::{json, Value};
use std::path::Path;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "read_file".to_string(),
        description: "Read a file from the filesystem.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path to read" },
                "offset": { "type": "integer", "description": "Line number to start from (0-based)" },
                "limit": { "type": "integer", "description": "Number of lines to read" }
            },
            "required": ["path"]
        }),
        permission: PermissionLevel::ReadOnly,
    }
}

pub fn execute(input: &Value, working_dir: &str) -> Result<String, String> {
    let path_str = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("missing 'path' parameter")?;

    let path = resolve_path(path_str, working_dir);

    eprintln!("  > read_file: {}", path.display());

    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("read error: {e}"))?;

    let offset = input.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit = input
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(2000) as usize;

    let lines: Vec<&str> = content.lines().collect();
    let end = (offset + limit).min(lines.len());

    if offset >= lines.len() {
        return Ok(format!("(file has {} lines, offset {} is past end)", lines.len(), offset));
    }

    let mut result = String::new();
    for (i, line) in lines[offset..end].iter().enumerate() {
        result.push_str(&format!("{}\t{}\n", offset + i + 1, line));
    }

    Ok(result)
}

fn resolve_path(path_str: &str, working_dir: &str) -> std::path::PathBuf {
    let path = Path::new(path_str);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        Path::new(working_dir).join(path)
    }
}
