use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum PermissionLevel {
    ReadOnly = 0,
    WorkspaceWrite = 1,
    FullAccess = 2,
}

#[derive(Clone, Debug)]
pub struct PermissionPolicy {
    pub level: PermissionLevel,
}

impl PermissionPolicy {
    pub fn new(level: PermissionLevel) -> Self {
        Self { level }
    }

    /// 指定レベルのツール実行を許可するか。
    pub fn allows(&self, required: PermissionLevel) -> bool {
        self.level >= required
    }
}

impl Default for PermissionPolicy {
    fn default() -> Self {
        Self {
            level: PermissionLevel::WorkspaceWrite,
        }
    }
}
