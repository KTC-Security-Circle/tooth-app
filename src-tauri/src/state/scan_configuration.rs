use anyhow::Context;
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
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let contents = std::fs::read_to_string(path)
            .with_context(|| format!("reading calibration profile '{}'", path.display()))?;
        serde_json::from_str(&contents)
            .with_context(|| format!("parsing calibration profile '{}'", path.display()))
    }

    pub fn validate_files(&self, profile_path: &Path) -> anyhow::Result<()> {
        if self.profile_id.trim().is_empty()
            || self.left_camera.trim().is_empty()
            || self.right_camera.trim().is_empty()
        {
            anyhow::bail!("calibration profile is missing an identity");
        }
        if self.resolution_width == 0 || self.resolution_height == 0 {
            anyhow::bail!("calibration profile resolution must be non-zero");
        }
        let base = profile_path.parent().unwrap_or_else(|| Path::new("."));
        for artifact in [
            &self.artifacts.stereo_calibration_file,
            &self.artifacts.left_intrinsics_file,
            &self.artifacts.right_intrinsics_file,
        ] {
            let path = resolve(base, artifact);
            if !path.is_file() {
                anyhow::bail!("calibration artifact is missing: {}", path.display());
            }
            std::fs::File::open(&path)
                .with_context(|| format!("reading calibration artifact '{}'", path.display()))?;
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

    #[test]
    fn rejects_profile_artifact_that_is_not_a_file() {
        let path = std::env::temp_dir().join(format!("tooth-profile-dir-{}", std::process::id()));
        let _ = std::fs::create_dir_all(path.join("stereo.json"));
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
