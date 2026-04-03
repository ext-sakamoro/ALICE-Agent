use crate::permission::PermissionLevel;
use crate::tools::ToolSpec;
use serde_json::{json, Value};
use std::path::Path;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "edit_file".to_string(),
        description: "Replace text in a file.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "File path to edit" },
                "old_string": { "type": "string", "description": "Text to find" },
                "new_string": { "type": "string", "description": "Text to replace with" },
                "replace_all": { "type": "boolean", "description": "Replace all occurrences" }
            },
            "required": ["path", "old_string", "new_string"]
        }),
        permission: PermissionLevel::WorkspaceWrite,
    }
}

pub fn execute(input: &Value, working_dir: &str) -> Result<String, String> {
    let path_str = input
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or("missing 'path' parameter")?;
    let old_string = input
        .get("old_string")
        .and_then(|v| v.as_str())
        .ok_or("missing 'old_string' parameter")?;
    let new_string = input
        .get("new_string")
        .and_then(|v| v.as_str())
        .ok_or("missing 'new_string' parameter")?;
    let replace_all = input
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let path = if Path::new(path_str).is_absolute() {
        Path::new(path_str).to_path_buf()
    } else {
        Path::new(working_dir).join(path_str)
    };

    eprintln!("  > edit_file: {}", path.display());

    let content = std::fs::read_to_string(&path).map_err(|e| format!("read error: {e}"))?;

    let count = content.matches(old_string).count();
    if count == 0 {
        return Err(format!(
            "old_string not found in {}",
            path.display()
        ));
    }

    if !replace_all && count > 1 {
        return Err(format!(
            "old_string found {} times in {} — use replace_all or provide more context",
            count,
            path.display()
        ));
    }

    let new_content = if replace_all {
        content.replace(old_string, new_string)
    } else {
        content.replacen(old_string, new_string, 1)
    };

    std::fs::write(&path, &new_content).map_err(|e| format!("write error: {e}"))?;

    Ok(format!(
        "replaced {} occurrence(s) in {}",
        if replace_all { count } else { 1 },
        path.display()
    ))
}
