use crate::state::settings::Settings;

#[tauri::command]
pub async fn validate_settings(settings: Settings) -> Result<(), String> {
    settings.validate()
}
