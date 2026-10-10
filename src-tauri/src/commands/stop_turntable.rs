use crate::errors::AppError;
use crate::state::settings::Settings;
use crate::state::turntable::{Position, TurntableState};
use crate::state::turntable_sidecar::{args, run};
use std::time::Duration;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn stop_turntable(
    app: AppHandle,
    state: State<'_, TurntableState>,
) -> Result<(), AppError> {
    if let Some(child) = state.take_active_child()? {
        let _ = child.kill();
    }
    let config = Settings::load(&app)?;
    let result = run(
        &app,
        &state,
        args(&config, "--stop", None)?,
        Duration::from_millis(config.turntable_move_timeout_ms),
        false,
    )
    .await;
    state.set(Position::Unknown)?;
    result
}
