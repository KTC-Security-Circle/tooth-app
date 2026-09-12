use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::settings::Settings;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlotStatus {
    Processing,
    NeedsRescan,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SlotStage {
    Recorded,
    ScanValidated,
    Decoded,
    ReconstructionValidated,
    Reconstructed,
    Matched,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanSlotManifest {
    pub version: u32,
    pub session_id: String,
    pub status: SlotStatus,
    pub stage: SlotStage,
    pub attempt: u32,
    pub input_dir: String,
    pub attempt_dir: String,
    pub config: Settings,
    #[serde(default)]
    pub matching: Option<serde_json::Value>,
    #[serde(default)]
    pub reason: Option<String>,
}

impl ScanSlotManifest {
    pub fn path(data_root: &Path, session_id: &str) -> PathBuf {
        data_root
            .join("sessions")
            .join(session_id)
            .join("manifest.json")
    }

    pub fn load(path: &Path) -> Result<Option<Self>, String> {
        match std::fs::read_to_string(path) {
            Ok(contents) => serde_json::from_str(&contents)
                .map(Some)
                .map_err(|error| format!("invalid scan slot manifest {}: {error}", path.display())),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(format!("failed to read scan slot manifest: {error}")),
        }
    }

    pub fn save_atomic(&self, path: &Path) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or_else(|| "scan slot manifest has no parent directory".to_string())?;
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create manifest directory: {error}"))?;
        let temporary = path.with_extension("json.tmp");
        let contents = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("failed to serialize scan slot manifest: {error}"))?;
        std::fs::write(&temporary, contents)
            .map_err(|error| format!("failed to write scan slot manifest: {error}"))?;
        std::fs::rename(&temporary, path)
            .map_err(|error| format!("failed to atomically publish scan slot manifest: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_round_trips_atomically() {
        let root = std::env::temp_dir().join(format!("tooth-scan-slot-{}", std::process::id()));
        let path = ScanSlotManifest::path(&root, "session");
        let manifest = ScanSlotManifest {
            version: 1,
            session_id: "session".to_string(),
            status: SlotStatus::Processing,
            stage: SlotStage::Recorded,
            attempt: 1,
            input_dir: "/scan".to_string(),
            attempt_dir: "/data/attempt-0001".to_string(),
            config: Settings::default(),
            matching: None,
            reason: None,
        };
        manifest.save_atomic(&path).expect("manifest should save");
        assert_eq!(
            ScanSlotManifest::load(&path)
                .expect("manifest should load")
                .expect("manifest exists")
                .session_id,
            "session"
        );
        std::fs::remove_dir_all(root).expect("temporary manifest should be removable");
    }
}
