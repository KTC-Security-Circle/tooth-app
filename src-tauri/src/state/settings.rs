use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::errors::AppError;
use crate::utils::config::config_file_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub camera_left: String,
    pub camera_right: String,
    pub fps: u32,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub calibration_image_path: String,
    pub developer_mode: bool,
    #[serde(default)]
    pub matching_source_path: String,
    #[serde(default)]
    pub matching_target_path: String,
    #[serde(default = "default_matching_mode")]
    pub matching_mode: String,
    #[serde(default = "default_matching_voxel_size")]
    pub matching_voxel_size: f64,
    #[serde(default = "default_matching_ransac_iterations")]
    pub matching_ransac_iterations: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    #[serde(default)]
    pub camera_left: Option<String>,
    #[serde(default)]
    pub camera_right: Option<String>,
    #[serde(default)]
    pub fps: Option<u32>,
    #[serde(default)]
    pub calibration_image_path: Option<String>,
    #[serde(default)]
    pub developer_mode: Option<bool>,
    #[serde(default)]
    pub matching_source_path: Option<String>,
    #[serde(default)]
    pub matching_target_path: Option<String>,
    #[serde(default)]
    pub matching_mode: Option<String>,
    #[serde(default)]
    pub matching_voxel_size: Option<f64>,
    #[serde(default)]
    pub matching_ransac_iterations: Option<u64>,
}

impl SettingsPatch {
    pub fn apply_to(&self, settings: &mut Settings) {
        if let Some(v) = &self.camera_left {
            settings.camera_left = v.clone();
        }
        if let Some(v) = &self.camera_right {
            settings.camera_right = v.clone();
        }
        if let Some(v) = self.fps {
            settings.fps = v;
        }
        if let Some(v) = &self.calibration_image_path {
            settings.calibration_image_path = v.clone();
        }
        if let Some(v) = self.developer_mode {
            settings.developer_mode = v;
        }
        if let Some(v) = &self.matching_source_path {
            settings.matching_source_path = v.clone();
        }
        if let Some(v) = &self.matching_target_path {
            settings.matching_target_path = v.clone();
        }
        if let Some(v) = &self.matching_mode {
            settings.matching_mode = v.clone();
        }
        if let Some(v) = self.matching_voxel_size {
            settings.matching_voxel_size = v;
        }
        if let Some(v) = self.matching_ransac_iterations {
            settings.matching_ransac_iterations = v;
        }
        settings.derive_matching_paths();
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            camera_left: String::new(),
            camera_right: String::new(),
            fps: 30,
            calibration_image_path: String::new(),
            developer_mode: false,
            matching_source_path: String::new(),
            matching_target_path: String::new(),
            matching_mode: default_matching_mode(),
            matching_voxel_size: default_matching_voxel_size(),
            matching_ransac_iterations: default_matching_ransac_iterations(),
        }
    }
}

fn default_matching_mode() -> String {
    "matching".to_string()
}

const fn default_matching_voxel_size() -> f64 {
    0.25
}

const fn default_matching_ransac_iterations() -> u64 {
    30
}

impl Settings {
    /// 設定ファイルから読み込む。ファイル不在・パース失敗時はデフォルト値を返す。
    pub fn load(app: &tauri::AppHandle) -> Result<Self, AppError> {
        let path = config_file_path(app)?;

        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<Settings>(&contents) {
                Ok(mut settings) => {
                    settings.derive_matching_paths();
                    Ok(settings)
                }
                Err(e) => {
                    log::warn!(
                        "failed to parse settings file '{}': {}. Using defaults.",
                        path.display(),
                        e
                    );
                    Ok(Settings::default())
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File doesn't exist yet — return defaults
                Ok(Settings::default())
            }
            Err(e) => {
                let msg = format!("Failed to read settings file '{}': {}", path.display(), e);
                log::error!("{}", msg);
                Err(AppError::Io(msg))
            }
        }
    }

    /// Fill in matching paths that were not explicitly configured.
    fn derive_matching_paths(&mut self) {
        if self.calibration_image_path.is_empty() {
            return;
        }

        let calibration_dir = Path::new(&self.calibration_image_path);
        let Some(parent) = calibration_dir.parent() else {
            return;
        };
        if parent.as_os_str().is_empty() {
            return;
        }

        if self.matching_source_path.is_empty() {
            self.matching_source_path = parent
                .join("3d_data")
                .join("source.ply")
                .to_string_lossy()
                .into_owned();
        }
        if self.matching_target_path.is_empty() {
            self.matching_target_path = parent
                .join("3d_data")
                .join("target.ply")
                .to_string_lossy()
                .into_owned();
        }
    }

    /// 設定値を検証する。calibrationImagePath が空文字の場合は実在性・ディレクトリ判定をスキップする（空文字は許可）。
    pub fn validate(&self) -> Result<(), AppError> {
        if self.camera_left.is_empty() {
            return Err(AppError::Validation(
                "左カメラを選択してください".to_string(),
            ));
        }
        if self.camera_right.is_empty() {
            return Err(AppError::Validation(
                "右カメラを選択してください".to_string(),
            ));
        }
        if !self.calibration_image_path.is_empty() {
            let path = std::path::Path::new(&self.calibration_image_path);
            if !path.exists() {
                return Err(AppError::Validation(format!(
                    "キャリブレーション用画像保存パスが存在しません: {}",
                    self.calibration_image_path
                )));
            }
            if !path.is_dir() {
                return Err(AppError::Validation(format!(
                    "キャリブレーション用画像保存パスにはディレクトリを指定してください: {}",
                    self.calibration_image_path
                )));
            }
        }
        Ok(())
    }

    /// 設定を JSON ファイルへ保存する。
    pub fn save(&self, app: &tauri::AppHandle) -> Result<(), AppError> {
        let path = config_file_path(app)?;
        let mut settings = self.clone();
        settings.derive_matching_paths();

        let contents = serde_json::to_string_pretty(&settings).map_err(|e| {
            let msg = format!("Failed to serialize settings: {}", e);
            log::error!("{}", msg);
            AppError::Config(msg)
        })?;

        std::fs::write(&path, &contents).map_err(|e| {
            let msg = format!("Failed to write settings to '{}': {}", path.display(), e);
            log::error!("{}", msg);
            AppError::Io(msg)
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::Settings;

    #[test]
    fn derives_empty_matching_paths_from_calibration_directory_parent() {
        let mut settings = Settings {
            calibration_image_path: "/data/calibration/images".to_string(),
            ..Settings::default()
        };

        settings.derive_matching_paths();

        assert_eq!(
            settings.matching_source_path,
            "/data/calibration/3d_data/source.ply"
        );
        assert_eq!(
            settings.matching_target_path,
            "/data/calibration/3d_data/target.ply"
        );
    }

    #[test]
    fn preserves_nonempty_matching_paths() {
        let mut settings = Settings {
            calibration_image_path: "/data/new-calibration/images".to_string(),
            matching_source_path: "/custom/source.ply".to_string(),
            matching_target_path: "/custom/target.ply".to_string(),
            ..Settings::default()
        };

        settings.derive_matching_paths();

        assert_eq!(settings.matching_source_path, "/custom/source.ply");
        assert_eq!(settings.matching_target_path, "/custom/target.ply");
    }

    #[test]
    fn leaves_matching_paths_empty_when_calibration_has_no_parent() {
        let mut settings = Settings {
            calibration_image_path: "calibration".to_string(),
            ..Settings::default()
        };

        settings.derive_matching_paths();

        assert!(settings.matching_source_path.is_empty());
        assert!(settings.matching_target_path.is_empty());
    }
}
