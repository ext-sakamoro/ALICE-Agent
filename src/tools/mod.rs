pub mod bash;
pub mod edit_file;
pub mod glob;
pub mod grep;
pub mod read_file;
pub mod write_file;

use crate::permission::PermissionLevel;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub permission: PermissionLevel,
}

/// ツール実行 trait。
pub trait ToolExecutor: Send {
    fn execute(&self, name: &str, input: &Value) -> Result<String, String>;
    fn specs(&self) -> Vec<ToolSpec>;
}

/// 標準ツールレジストリ。
pub struct StandardTools {
    working_dir: String,
}

impl StandardTools {
    pub fn new(working_dir: &str) -> Self {
        Self {
            working_dir: working_dir.to_string(),
        }
    }
}

impl ToolExecutor for StandardTools {
    fn execute(&self, name: &str, input: &Value) -> Result<String, String> {
        match name {
            "bash" => bash::execute(input),
            "read_file" => read_file::execute(input, &self.working_dir),
            "write_file" => write_file::execute(input, &self.working_dir),
            "edit_file" => edit_file::execute(input, &self.working_dir),
            "glob_search" => glob::execute(input, &self.working_dir),
            "grep_search" => grep::execute(input, &self.working_dir),
            _ => Err(format!("unknown tool: {name}")),
        }
    }

    fn specs(&self) -> Vec<ToolSpec> {
        vec![
            bash::spec(),
            read_file::spec(),
            write_file::spec(),
            edit_file::spec(),
            glob::spec(),
            grep::spec(),
        ]
    }
}
