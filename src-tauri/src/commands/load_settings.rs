use crate::errors::AppError;
use crate::state::settings::Settings;

#[tauri::command]
pub async fn load_settings(app: tauri::AppHandle) -> Result<Settings, AppError> {
    let mut settings = Settings::load(&app)?;
    if !settings.developer_mode {
        settings.calibration_image_path = String::new();
        settings.matching_source_path = String::new();
        settings.matching_target_path = String::new();
        settings.matching_mode = "matching".to_string();
        settings.matching_voxel_size = 0.25;
        settings.matching_ransac_iterations = 30;
    }
    Ok(settings)
}
