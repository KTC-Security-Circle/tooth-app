use crate::errors::AppError;
use crate::state::settings::{Settings, SettingsPatch};

#[tauri::command]
pub async fn update_settings(app: tauri::AppHandle, patch: SettingsPatch) -> Result<(), AppError> {
    let mut settings = Settings::load(&app)?;
    patch.apply_to(&mut settings);
    settings.validate()?;
    settings.save(&app)
}
