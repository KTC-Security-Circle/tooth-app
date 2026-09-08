use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationArtifacts {
    pub stereo_calibration_file: String,
    pub left_intrinsics_file: String,
    pub right_intrinsics_file: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CalibrationProfileManifest {
    pub profile_id: String,
    pub left_camera: String,
    pub right_camera: String,
    pub resolution_width: u32,
    pub resolution_height: u32,
    pub artifacts: CalibrationArtifacts,
    pub created_at: String,
    pub updated_at: String,
}

impl CalibrationProfileManifest {
    pub fn load(path: &Path) -> Result<Self, String> {
        let contents = std::fs::read_to_string(path).map_err(|e| {
            format!(
                "calibration profile cannot be read '{}': {e}",
                path.display()
            )
        })?;
        serde_json::from_str(&contents)
            .map_err(|e| format!("calibration profile is invalid '{}': {e}", path.display()))
    }

    pub fn validate_files(&self, profile_path: &Path) -> Result<(), String> {
        if self.profile_id.trim().is_empty()
            || self.left_camera.trim().is_empty()
            || self.right_camera.trim().is_empty()
        {
            return Err("calibration profile is missing an identity".to_string());
        }
        if self.resolution_width == 0 || self.resolution_height == 0 {
            return Err("calibration profile resolution must be non-zero".to_string());
        }
        let base = profile_path.parent().unwrap_or_else(|| Path::new("."));
        for artifact in [
            &self.artifacts.stereo_calibration_file,
            &self.artifacts.left_intrinsics_file,
            &self.artifacts.right_intrinsics_file,
        ] {
            let path = resolve(base, artifact);
            if !path.is_file() {
                return Err(format!(
                    "calibration artifact is missing: {}",
                    path.display()
                ));
            }
        }
        Ok(())
    }
}

fn resolve(base: &Path, path: &str) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_missing_profile_artifact() {
        let path = std::env::temp_dir().join(format!("tooth-profile-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&path);
        let manifest = CalibrationProfileManifest {
            profile_id: "test".to_string(),
            left_camera: "left".to_string(),
            right_camera: "right".to_string(),
            resolution_width: 640,
            resolution_height: 480,
            artifacts: CalibrationArtifacts {
                stereo_calibration_file: "stereo.json".to_string(),
                left_intrinsics_file: "left.json".to_string(),
                right_intrinsics_file: "right.json".to_string(),
            },
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        };
        assert!(manifest.validate_files(&path.join("profile.json")).is_err());
        let _ = std::fs::remove_dir_all(path);
    }
}
