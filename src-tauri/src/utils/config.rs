use anyhow::Context;
use std::path::PathBuf;
use tauri::Manager;

pub fn config_file_path(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    let config_dir = app
        .path()
        .app_config_dir()
        .context("Failed to get app config dir")?;

    std::fs::create_dir_all(&config_dir)
        .with_context(|| format!("Failed to create config dir '{}'", config_dir.display()))?;

    Ok(config_dir.join("settings.json"))
}

pub fn calibration_dir_path(app: &tauri::AppHandle) -> anyhow::Result<PathBuf> {
    let config_dir = app
        .path()
        .app_config_dir()
        .context("Failed to get app config dir")?;

    let calib_dir = config_dir.join("calibration_images");

    std::fs::create_dir_all(&calib_dir)
        .with_context(|| format!("Failed to create calibration dir '{}'", calib_dir.display()))?;

    Ok(calib_dir)
}
