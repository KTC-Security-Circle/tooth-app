use serde::{Deserialize, Serialize};

use crate::utils::config::config_file_path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub camera_left: String,
    pub camera_right: String,
    pub fps: u32,
    pub calibration_image_path: String,
    pub developer_mode: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            camera_left: String::new(),
            camera_right: String::new(),
            fps: 30,
            calibration_image_path: String::new(),
            developer_mode: false,
        }
    }
}

impl Settings {
    /// 設定ファイルから読み込む。ファイル不在・パース失敗時はデフォルト値を返す。
    pub fn load(app: &tauri::AppHandle) -> Result<Self, String> {
        let path = config_file_path(app)?;

        match std::fs::read_to_string(&path) {
            Ok(contents) => match serde_json::from_str::<Settings>(&contents) {
                Ok(settings) => Ok(settings),
                Err(e) => {
                    eprintln!(
                        "Warning: failed to parse settings file '{}': {}. Using defaults.",
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
            Err(e) => Err(format!(
                "Failed to read settings file '{}': {}",
                path.display(),
                e
            )),
        }
    }

    /// 設定値を検証する。calibrationImagePath が空文字の場合は実在性・ディレクトリ判定をスキップする（空文字は許可）。
    pub fn validate(&self) -> Result<(), String> {
        if self.camera_left.is_empty() {
            return Err("左カメラを選択してください".to_string());
        }
        if self.camera_right.is_empty() {
            return Err("右カメラを選択してください".to_string());
        }
        if !self.calibration_image_path.is_empty() {
            let path = std::path::Path::new(&self.calibration_image_path);
            if !path.exists() {
                return Err(format!(
                    "キャリブレーション用画像保存パスが存在しません: {}",
                    self.calibration_image_path
                ));
            }
            if !path.is_dir() {
                return Err(format!(
                    "キャリブレーション用画像保存パスにはディレクトリを指定してください: {}",
                    self.calibration_image_path
                ));
            }
        }
        Ok(())
    }

    /// 設定を JSON ファイルへ保存する。
    pub fn save(&self, app: &tauri::AppHandle) -> Result<(), String> {
        let path = config_file_path(app)?;

        let contents = serde_json::to_string_pretty(&self)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;

        std::fs::write(&path, &contents)
            .map_err(|e| format!("Failed to write settings to '{}': {}", path.display(), e))?;

        Ok(())
    }
}
