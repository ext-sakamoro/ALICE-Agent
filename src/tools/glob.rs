use crate::permission::PermissionLevel;
use crate::tools::ToolSpec;
use serde_json::{json, Value};
use std::path::Path;

pub fn spec() -> ToolSpec {
    ToolSpec {
        name: "glob_search".to_string(),
        description: "Find files matching a glob pattern.".to_string(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "pattern": { "type": "string", "description": "Glob pattern (e.g. '**/*.rs')" },
                "path": { "type": "string", "description": "Base directory (default: working dir)" }
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

    let base = input
        .get("path")
        .and_then(|v| v.as_str())
        .unwrap_or(working_dir);

    let full_pattern = if Path::new(pattern).is_absolute() {
        pattern.to_string()
    } else {
        format!("{base}/{pattern}")
    };

    eprintln!("  > glob: {full_pattern}");

    let mut matches: Vec<String> = Vec::new();
    for entry in glob::glob(&full_pattern).map_err(|e| format!("invalid pattern: {e}"))? {
        match entry {
            Ok(path) => matches.push(path.display().to_string()),
            Err(e) => eprintln!("  glob error: {e}"),
        }
    }

    matches.sort();

    if matches.is_empty() {
        Ok("no matches found".to_string())
    } else {
        // 多すぎる場合は切り詰め
        let total = matches.len();
        if total > 200 {
            matches.truncate(200);
            matches.push(format!("... ({} more)", total - 200));
        }
        Ok(matches.join("\n"))
    }
}
