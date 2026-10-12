use crate::errors::AppError;
use crate::state::settings::Settings;

#[tauri::command]
pub async fn load_settings(app: tauri::AppHandle) -> Result<Settings, AppError> {
    Settings::load(&app)
}
