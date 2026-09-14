use crate::commands::move_turntable::move_turntable_slot_inner;
use crate::errors::AppError;
use crate::state::settings::Settings;
use crate::state::turntable::TurntableState;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn move_turntable_slot(
    app: AppHandle,
    state: State<'_, TurntableState>,
    slot: u8,
) -> Result<(), AppError> {
    let config = Settings::load(&app)?;
    move_turntable_slot_inner(&app, state.inner(), &config, slot).await
}
