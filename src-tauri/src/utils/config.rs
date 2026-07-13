use std::path::PathBuf;
use tauri::Manager;

pub fn config_file_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get app config dir: {}", e))?;

    std::fs::create_dir_all(&config_dir).map_err(|e| {
        format!(
            "Failed to create config dir '{}': {}",
            config_dir.display(),
            e
        )
    })?;

    Ok(config_dir.join("settings.json"))
}

pub fn calibration_dir_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let config_dir = app
        .path()
        .app_config_dir()
        .map_err(|e| format!("Failed to get app config dir: {}", e))?;

    let calib_dir = config_dir.join("calibration_images");

    std::fs::create_dir_all(&calib_dir).map_err(|e| {
        format!(
            "Failed to create calibration dir '{}': {}",
            calib_dir.display(),
            e
        )
    })?;

    Ok(calib_dir)
}
