use crate::state::settings::Settings;

#[tauri::command]
pub async fn load_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    let mut settings = Settings::load(&app)?;
    if !settings.developer_mode {
        settings.calibration_image_path = String::new();
    }
    Ok(settings)
}
