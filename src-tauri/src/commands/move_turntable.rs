use crate::errors::AppError;
use crate::state::settings::Settings;
use crate::state::turntable::{target_steps, Position, TurntableState};
use std::time::Duration;
use tauri::{AppHandle, State};
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

struct ChildGuard(Option<CommandChild>);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.take() {
            let _ = child.kill();
        }
    }
}

async fn run_sidecar(
    app: &AppHandle,
    args: Vec<String>,
    timeout: Duration,
) -> Result<(), AppError> {
    let (mut events, child) = app
        .shell()
        .sidecar("turntable")
        .map_err(|e| AppError::Internal(format!("failed to resolve turntable: {e}")))?
        .args(args)
        .spawn()
        .map_err(|e| AppError::Internal(format!("failed to spawn turntable: {e}")))?;
    let _guard = ChildGuard(Some(child));
    let wait = async {
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Error(message) => {
                    return Err(AppError::Internal(format!(
                        "turntable transport error: {message}"
                    )))
                }
                CommandEvent::Terminated(p) if p.code == Some(0) && p.signal.is_none() => {
                    return Ok(())
                }
                CommandEvent::Terminated(p) => {
                    return Err(AppError::Internal(format!(
                        "turntable terminated unsuccessfully (code={:?}, signal={:?})",
                        p.code, p.signal
                    )))
                }
                _ => {}
            }
        }
        Err(AppError::Internal(
            "turntable event stream closed before termination".to_string(),
        ))
    };
    tokio::time::timeout(timeout, wait)
        .await
        .map_err(|_| AppError::Internal(format!("turntable timed out after {timeout:?}")))?
}

fn args(settings: &Settings, operation: &str, steps: Option<i64>) -> Result<Vec<String>, AppError> {
    if settings.turntable_port.trim().is_empty() {
        return Err(AppError::Validation(
            "turntable port is not configured".to_string(),
        ));
    }
    if !settings.turntable_speed.is_finite() || settings.turntable_speed <= 0.0 {
        return Err(AppError::Validation(
            "turntable speed must be positive".to_string(),
        ));
    }
    if !settings.turntable_acceleration.is_finite() || settings.turntable_acceleration <= 0.0 {
        return Err(AppError::Validation(
            "turntable acceleration must be positive".to_string(),
        ));
    }
    if settings.turntable_move_timeout_ms == 0 {
        return Err(AppError::Validation(
            "turntable move timeout must be positive".to_string(),
        ));
    }
    let mut result = Vec::new();
    if !operation.is_empty() {
        result.push(operation.into());
    }
    result.extend([
        "--port".into(),
        settings.turntable_port.clone(),
        "--speed".into(),
        settings.turntable_speed.to_string(),
        "--accel".into(),
        settings.turntable_acceleration.to_string(),
    ]);
    if let Some(steps) = steps {
        result.extend(["--steps".into(), steps.to_string()]);
    }
    Ok(result)
}

async fn settings(app: &AppHandle) -> Result<Settings, AppError> {
    Settings::load(app)
}

/// Compatibility command for the pre-slot frontend. Slot workflows use move_turntable_slot.
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
    let config = settings(&app).await?;
    let mut command = args(&config, "", None)?;
    command.push(angle.to_string());
    let result = run_sidecar(
        &app,
        command,
        Duration::from_millis(config.turntable_move_timeout_ms),
    )
    .await;
    // A relative legacy angle does not establish an absolute slot position.
    state.set(Position::Unknown);
    result
}

#[tauri::command]
pub async fn move_turntable_slot(
    app: AppHandle,
    state: State<'_, TurntableState>,
    slot: u8,
) -> Result<(), AppError> {
    let target = target_steps(slot)
        .ok_or_else(|| AppError::Validation("slot must be between 0 and 11".to_string()))?;
    let _lock = state.movement.lock().await;
    let current = match state.position() {
        Position::Confirmed { steps, .. } => steps,
        Position::Unknown => {
            return Err(AppError::Validation(
                "turntable position is unknown; re-zero before moving".to_string(),
            ))
        }
    };
    let config = settings(&app).await?;
    let result = run_sidecar(
        &app,
        args(&config, "", Some(target - current))?,
        Duration::from_millis(config.turntable_move_timeout_ms),
    )
    .await;
    if result.is_err() {
        state.set(Position::Unknown);
        return result;
    }
    tokio::time::sleep(Duration::from_millis(config.turntable_settle_time_ms)).await;
    state.set(Position::Confirmed {
        slot,
        steps: target,
    });
    Ok(())
}

#[tauri::command]
pub async fn zero_turntable(
    app: AppHandle,
    state: State<'_, TurntableState>,
) -> Result<(), AppError> {
    let _lock = state.movement.lock().await;
    let config = settings(&app).await?;
    let result = run_sidecar(
        &app,
        args(&config, "--zero", None)?,
        Duration::from_millis(config.turntable_move_timeout_ms),
    )
    .await;
    if result.is_ok() {
        state.set(Position::Confirmed { slot: 0, steps: 0 });
    } else {
        state.set(Position::Unknown);
    }
    result
}

#[tauri::command]
pub async fn stop_turntable(
    app: AppHandle,
    state: State<'_, TurntableState>,
) -> Result<(), AppError> {
    let _lock = state.movement.lock().await;
    let config = settings(&app).await?;
    let result = run_sidecar(
        &app,
        args(&config, "--stop", None)?,
        Duration::from_millis(config.turntable_move_timeout_ms),
    )
    .await;
    state.set(Position::Unknown);
    result
}
