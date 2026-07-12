use crate::state::settings::Settings;

#[tauri::command]
pub async fn load_settings(app: tauri::AppHandle) -> Result<Settings, String> {
    Settings::load(&app)
}
