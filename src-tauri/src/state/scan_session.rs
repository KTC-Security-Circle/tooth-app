use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::scan_slot::SlotStatus;

pub const SESSION_SLOT_COUNT: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Failed,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSessionSlot {
    pub slot: u8,
    pub input_dir: String,
    pub status: Option<SlotStatus>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSessionManifest {
    pub version: u32,
    pub session_id: String,
    pub status: SessionStatus,
    pub current_slot: Option<u8>,
    pub slots: Vec<ScanSessionSlot>,
    pub reason: Option<String>,
}

impl ScanSessionManifest {
    pub fn path(data_root: &Path, session_id: &str) -> PathBuf {
        data_root
            .join("sessions")
            .join(session_id)
            .join("session.json")
    }

    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map(Some)
                .map_err(|e| format!("invalid scan session manifest: {e}")),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("failed to read scan session manifest: {e}")),
        }
    }

    pub fn save_atomic(&self, path: &Path) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or_else(|| "session manifest has no parent".to_string())?;
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create session directory: {e}"))?;
        let temporary = path.with_extension("json.tmp");
        let contents = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&temporary, contents).map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, path).map_err(|e| e.to_string())
    }
}

#[derive(Clone, Default)]
pub struct ScanSessionState {
    pub running: Arc<tokio::sync::Mutex<()>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_manifest_round_trips() {
        let root = std::env::temp_dir().join(format!("tooth-session-{}", std::process::id()));
        let path = ScanSessionManifest::path(&root, "test");
        let manifest = ScanSessionManifest {
            version: 1,
            session_id: "test".into(),
            status: SessionStatus::Running,
            current_slot: Some(0),
            slots: (0..SESSION_SLOT_COUNT)
                .map(|slot| ScanSessionSlot {
                    slot: slot as u8,
                    input_dir: format!("/scan/{slot}"),
                    status: None,
                    reason: None,
                })
                .collect(),
            reason: None,
        };
        manifest.save_atomic(&path).expect("save");
        assert_eq!(
            ScanSessionManifest::load(&path)
                .expect("load")
                .expect("exists")
                .slots
                .len(),
            12
        );
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
