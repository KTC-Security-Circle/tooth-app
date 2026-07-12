use crate::utils::config::calibration_dir_path;

#[tauri::command]
pub async fn default_calibration_path(app: tauri::AppHandle) -> Result<String, String> {
    let path = calibration_dir_path(&app)?;
    Ok(path.to_string_lossy().to_string())
}
