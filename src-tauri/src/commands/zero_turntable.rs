use crate::errors::AppError;
use crate::state::settings::Settings;
use crate::state::turntable::{Position, TurntableState};
use crate::state::turntable_sidecar::{args, run};
use std::time::Duration;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn zero_turntable(
    app: AppHandle,
    state: State<'_, TurntableState>,
) -> Result<(), AppError> {
    let _lock = state.movement.lock().await;
    let config = Settings::load(&app)?;
    let result = run(
        &app,
        &state,
        args(&config, "--zero", None)?,
        Duration::from_millis(config.turntable_move_timeout_ms),
        true,
    )
    .await;
    if result.is_ok() {
        state.set(Position::Confirmed { slot: 0, steps: 0 })?;
    } else {
        state.set(Position::Unknown)?;
    }
    result
}
