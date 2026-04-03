use crate::conversation::message::AgentMessage;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub created_at: String,
    pub working_dir: String,
    pub model_name: String,
    pub messages: Vec<AgentMessage>,
}

impl Session {
    pub fn new(working_dir: &str, model_name: &str) -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        let created_at = chrono_now();
        Self {
            id,
            created_at,
            working_dir: working_dir.to_string(),
            model_name: model_name.to_string(),
            messages: Vec::new(),
        }
    }

    /// セッションをJSONファイルに保存。
    pub fn save(&self, dir: &Path) -> io::Result<()> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{}.json", self.id));
        let json = serde_json::to_string_pretty(self)
            .map_err(io::Error::other)?;
        std::fs::write(path, json)
    }

    /// 最新セッションを復元。
    pub fn load_latest(dir: &Path) -> io::Result<Option<Self>> {
        if !dir.exists() {
            return Ok(None);
        }

        let mut latest: Option<(PathBuf, String)> = None;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
                if latest.as_ref().is_none_or(|(_, n)| name > *n) {
                    latest = Some((path, name));
                }
            }
        }

        match latest {
            Some((path, _)) => {
                let data = std::fs::read_to_string(path)?;
                let session: Session = serde_json::from_str(&data)
                    .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
                Ok(Some(session))
            }
            None => Ok(None),
        }
    }
}

fn chrono_now() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}
