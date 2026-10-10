use crate::errors::AppError;
use crate::state::settings::Settings;
use crate::state::turntable::{target_steps, Position, TurntableState};
use crate::state::turntable_sidecar::{args, run};
use std::time::Duration;
use tauri::{AppHandle, State};

pub(crate) async fn move_turntable_slot_inner(
    app: &AppHandle,
    state: &TurntableState,
    config: &Settings,
    slot: u8,
) -> Result<(), AppError> {
    let target = target_steps(slot)
        .ok_or_else(|| AppError::Validation("slot must be between 0 and 11".to_string()))?;
    let _lock = state.movement.lock().await;
    let current = match state.position()? {
        Position::Confirmed { steps, .. } => steps,
        Position::Unknown => {
            return Err(AppError::Validation(
                "turntable position is unknown; re-zero before moving".to_string(),
            ))
        }
    };
    let result = run(
        app,
        state,
        args(config, "", Some(target - current))?,
        Duration::from_millis(config.turntable_move_timeout_ms),
        true,
    )
    .await;
    if result.is_err() {
        state.set(Position::Unknown)?;
        return result;
    }
    tokio::time::sleep(Duration::from_millis(config.turntable_settle_time_ms)).await;
    state.set(Position::Confirmed {
        slot,
        steps: target,
    })
}

/// Compatibility command for the pre-slot frontend.
#[tauri::command]
pub async fn move_turntable(
    app: AppHandle,
    state: State<'_, TurntableState>,
    angle: f64,
) -> Result<(), AppError> {
    if !angle.is_finite() {
        return Err(AppError::Validation("angle must be finite".to_string()));
    }
    let _lock = state.movement.lock().await;
    let config = Settings::load(&app)?;
    let mut command = args(&config, "", None)?;
    command.push(angle.to_string());
    let result = run(
        &app,
        state.inner(),
        command,
        Duration::from_millis(config.turntable_move_timeout_ms),
        true,
    )
    .await;
    // A relative legacy angle does not establish an absolute slot position.
    state.set(Position::Unknown)?;
    result
}
