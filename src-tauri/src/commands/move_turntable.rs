use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::errors::AppError;

const TURNTABLE_TIMEOUT: Duration = Duration::from_secs(10);

struct ChildGuard(Option<CommandChild>);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.take() {
            if let Err(error) = child.kill() {
                log::debug!("turntable child cleanup failed: {error}");
            }
        }
    }
}

#[tauri::command]
pub async fn move_turntable(app: AppHandle, angle: f64) -> Result<(), AppError> {
    if !angle.is_finite() {
        return Err(AppError::Validation("angle must be finite".to_string()));
    }

    let (mut events, child) = app
        .shell()
        .sidecar("turntable")
        .map_err(|error| {
            log::error!("failed to resolve turntable: {error}");
            AppError::Internal(format!("failed to resolve turntable: {error}"))
        })?
        .args([angle.to_string()])
        .spawn()
        .map_err(|error| {
            log::error!("failed to spawn turntable: {error}");
            AppError::Internal(format!("failed to spawn turntable: {error}"))
        })?;
    let _child_guard = ChildGuard(Some(child));

    let wait_for_output = async {
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Error(message) => {
                    return Err(format!("turntable command error: {message}"))
                }
                CommandEvent::Terminated(payload) => {
                    if payload.code == Some(0) && payload.signal.is_none() {
                        return Ok(());
                    }
                    return Err(format!(
                        "turntable terminated unsuccessfully (code={:?}, signal={:?})",
                        payload.code, payload.signal
                    ));
                }
                _ => {}
            }
        }
        Err("turntable event stream closed before termination".to_string())
    };

    match tokio::time::timeout(TURNTABLE_TIMEOUT, wait_for_output).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(message)) => {
            log::error!("turntable failed: {message}");
            Err(AppError::Internal(message))
        }
        Err(_) => {
            log::error!("turntable timed out after {TURNTABLE_TIMEOUT:?}");
            Err(AppError::Internal("turntable timed out".to_string()))
        }
    }
}
