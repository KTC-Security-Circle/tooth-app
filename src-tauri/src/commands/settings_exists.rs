use crate::utils::config::config_file_path;

#[tauri::command]
pub async fn settings_exists(app: tauri::AppHandle) -> Result<bool, String> {
    let path = config_file_path(&app)?;
    Ok(path.exists())
}
