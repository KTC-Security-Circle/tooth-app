use crate::errors::AppError;
use crate::state::settings::Settings;
use crate::state::turntable::TurntableState;
use std::time::Duration;
use tauri::{AppHandle, State};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

pub fn args(
    settings: &Settings,
    operation: &str,
    steps: Option<i64>,
) -> Result<Vec<String>, AppError> {
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
        result.push(operation.to_string());
    }
    result.extend([
        "--port".to_string(),
        settings.turntable_port.clone(),
        "--speed".to_string(),
        settings.turntable_speed.to_string(),
        "--accel".to_string(),
        settings.turntable_acceleration.to_string(),
    ]);
    if let Some(steps) = steps {
        result.extend(["--steps".to_string(), steps.to_string()]);
    }
    Ok(result)
}

pub async fn run(
    app: &AppHandle,
    state: &State<'_, TurntableState>,
    args: Vec<String>,
    timeout: Duration,
    track: bool,
) -> Result<(), AppError> {
    let (mut events, child) = app
        .shell()
        .sidecar("turntable")
        .map_err(|e| AppError::Internal(format!("failed to resolve turntable: {e}")))?
        .args(args)
        .spawn()
        .map_err(|e| AppError::Internal(format!("failed to spawn turntable: {e}")))?;
    let mut child = Some(child);
    if track {
        state.set_active_child(child.take().ok_or_else(|| {
            AppError::Internal("turntable child ownership was lost".to_string())
        })?)?;
    }
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
    let result = tokio::time::timeout(timeout, wait)
        .await
        .map_err(|_| AppError::Internal(format!("turntable timed out after {timeout:?}")))?;
    if track {
        if let Some(child) = state.take_active_child()? {
            let _ = child.kill();
        }
    }
    drop(child);
    result
}
