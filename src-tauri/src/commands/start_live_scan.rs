use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::commands::list_scan_monitors::list_scan_monitors_inner;
use crate::commands::run_matching::run_matching_paths;
use crate::errors::AppError;
use crate::state::core_tools::CoreToolsState;
use crate::state::matching::MatchingState;
use crate::state::settings::Settings;
use crate::state::turntable::{Position, TurntableState};
use crate::state::turntable_sidecar::{args, run};

const POSITIONS: u32 = 12;
const CORE_TIMEOUT: Duration = Duration::from_secs(10 * 60);
const SCAN_TIMEOUT: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct LiveStatus {
    position_index: u32,
    total_positions: u32,
    phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    scan_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ply_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

static NEXT_SCAN_ID: AtomicU64 = AtomicU64::new(1);

fn camera_id(path: &str) -> Result<u32, AppError> {
    path.strip_prefix("/dev/video")
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse().ok())
        .ok_or_else(|| AppError::Validation(format!("camera must be a /dev/videoN device: {path}")))
}

fn scan_id(position: u32) -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let sequence = NEXT_SCAN_ID.fetch_add(1, Ordering::Relaxed);
    format!(
        "live-{timestamp}-{}-{position}-{sequence}",
        std::process::id()
    )
}

fn scan_ack(response: &serde_json::Value, scan_id: &str) -> Result<(), AppError> {
    if response.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(AppError::CoreTools(response.to_string()));
    }
    if response.get("scan_id").and_then(serde_json::Value::as_str) != Some(scan_id) {
        return Err(AppError::CoreTools(format!(
            "stereo scan acknowledgement has unexpected scan_id for {scan_id}"
        )));
    }
    Ok(())
}

fn emit(app: &AppHandle, status: LiveStatus) {
    let _ = app.emit("live-scan:status", status);
}

fn terminal_reason(event: &serde_json::Value) -> String {
    event
        .get("error_message")
        .or_else(|| event.get("reason"))
        .or_else(|| event.get("error").and_then(|error| error.get("message")))
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| event.to_string())
}

#[tauri::command]
pub async fn start_live_scan(
    app: AppHandle,
    core: State<'_, CoreToolsState>,
    matching: State<'_, MatchingState>,
    turntable: State<'_, TurntableState>,
) -> Result<(), AppError> {
    let lock = core.live_scan_lock.clone();
    let guard = lock
        .try_lock_owned()
        .map_err(|_| AppError::Validation("a live scan is already running".into()))?;
    // Keep manual turntable commands from moving the table between scans.
    let turntable_guard = turntable.movement.clone().lock_owned().await;
    let mut settings = Settings::load(&app)?;
    if settings.matching_target_path.trim().is_empty() {
        return Err(AppError::Validation(
            "matchingTargetPath is required".into(),
        ));
    }
    let monitor_index = settings
        .monitor_index
        .ok_or_else(|| AppError::Validation("monitorIndex must be selected".into()))?;
    let left_camera = camera_id(&settings.camera_left)?;
    let right_camera = camera_id(&settings.camera_right)?;
    if !list_scan_monitors_inner(&core)
        .await?
        .iter()
        .any(|monitor| monitor.index == monitor_index)
    {
        return Err(AppError::Validation(format!(
            "monitor index {monitor_index} is not available"
        )));
    }
    let calibration = if settings.stereo_calibration_file.trim().is_empty() {
        PathBuf::from("data/calib/stereo.yml")
    } else {
        PathBuf::from(&settings.stereo_calibration_file)
    };
    settings.stereo_calibration_file = calibration.to_string_lossy().into_owned();
    let root = if settings.data_root.trim().is_empty() {
        None
    } else {
        let configured = Path::new(&settings.data_root);
        let absolute = if configured.is_absolute() {
            configured.to_path_buf()
        } else {
            std::env::current_dir()
                .map_err(|e| AppError::Io(e.to_string()))?
                .join(configured)
        };
        let root = absolute.join("live-scans");
        std::fs::create_dir_all(&root).map_err(|e| AppError::Io(e.to_string()))?;
        Some(root)
    };

    for position in 0..POSITIONS {
        let id = scan_id(position);
        let output = root.as_ref().map(|root| root.join(&id));
        emit(
            &app,
            LiveStatus {
                position_index: position,
                total_positions: POSITIONS,
                phase: "preparing".into(),
                scan_id: Some(id.clone()),
                ply_file: None,
                message: None,
            },
        );
        let mut terminal = Box::pin(core.register_terminal_waiter(&id));
        let mut progress = core.register_progress_waiter(&id);
        let progress_finished = Arc::new(AtomicBool::new(false));
        let progress_finished_for_task = progress_finished.clone();
        let (progress_done_tx, progress_done_rx) = tokio::sync::oneshot::channel();
        let progress_app = app.clone();
        tauri::async_runtime::spawn(async move {
            while let Some(event) = progress.recv().await {
                if progress_finished_for_task.load(Ordering::Acquire) {
                    break;
                }
                let phase = match event.get("event").and_then(serde_json::Value::as_str) {
                    Some("stereo_scan_scanning") => "scanning",
                    Some("stereo_scan_decoding") => "decoding",
                    Some("stereo_scan_reconstructing") => "reconstructing",
                    _ => continue,
                };
                emit(
                    &progress_app,
                    LiveStatus {
                        position_index: position,
                        total_positions: POSITIONS,
                        phase: phase.into(),
                        scan_id: event
                            .get("scan_id")
                            .and_then(serde_json::Value::as_str)
                            .map(ToOwned::to_owned),
                        ply_file: None,
                        message: None,
                    },
                );
            }
            let _ = progress_done_tx.send(());
        });
        let mut command = serde_json::json!({
            "cmd":"stereo_scan", "scan_id":id, "monitor_index":monitor_index,
            "left_camera_id":left_camera, "right_camera_id":right_camera,
            "calibration_file":settings.stereo_calibration_file,
            "sync_mode":"delay", "delay_ms":100
        });
        if let Some(output) = &output {
            command["output_dir"] =
                serde_json::Value::String(output.to_string_lossy().into_owned());
        }
        let response = core.request(command, CORE_TIMEOUT).await;
        let response = match response {
            Ok(response) => response,
            Err(error) => {
                let lock_guard = guard;
                let movement_guard = turntable_guard;
                let cleanup_core = core.inner().clone();
                let cleanup_id = id.clone();
                let cleanup_finished = progress_finished.clone();
                let cleanup_done = progress_done_rx;
                tauri::async_runtime::spawn(async move {
                    let _guard = lock_guard;
                    let _movement_guard = movement_guard;
                    let _ = terminal.await;
                    cleanup_finished.store(true, Ordering::Release);
                    cleanup_core.remove_progress_waiter(&cleanup_id);
                    let _ = cleanup_done.await;
                });
                return Err(error);
            }
        };
        if response.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
            progress_finished.store(true, Ordering::Release);
            core.remove_progress_waiter(&id);
            let _ = progress_done_rx.await;
            let error = AppError::CoreTools(response.to_string());
            emit(
                &app,
                LiveStatus {
                    position_index: position,
                    total_positions: POSITIONS,
                    phase: "failed".into(),
                    scan_id: Some(id.clone()),
                    ply_file: None,
                    message: Some(error.to_string()),
                },
            );
            return Err(error);
        }
        if response.get("scan_id").and_then(serde_json::Value::as_str) != Some(id.as_str()) {
            let error = AppError::CoreTools(format!(
                "stereo scan acknowledgement has unexpected scan_id for {id}"
            ));
            progress_finished.store(true, Ordering::Release);
            let lock_guard = guard;
            let movement_guard = turntable_guard;
            let cleanup_core = core.inner().clone();
            let cleanup_id = id.clone();
            let cleanup_finished = progress_finished.clone();
            let cleanup_done = progress_done_rx;
            tauri::async_runtime::spawn(async move {
                let _guard = lock_guard;
                let _movement_guard = movement_guard;
                let _ = terminal.await;
                cleanup_finished.store(true, Ordering::Release);
                cleanup_core.remove_progress_waiter(&cleanup_id);
                let _ = cleanup_done.await;
            });
            emit(
                &app,
                LiveStatus {
                    position_index: position,
                    total_positions: POSITIONS,
                    phase: "failed".into(),
                    scan_id: Some(id.clone()),
                    ply_file: None,
                    message: Some(error.to_string()),
                },
            );
            return Err(error);
        }
        scan_ack(&response, &id)?;
        let event = match tokio::time::timeout(SCAN_TIMEOUT, &mut terminal).await {
            Ok(Ok(event)) => event,
            Ok(Err(_)) | Err(_) => {
                // Keep the exclusive guard until the reader observes termination; a timed-out
                // worker must never overlap a subsequent scan.
                let lock_guard = guard;
                let movement_guard = turntable_guard;
                let cleanup_core = core.inner().clone();
                let cleanup_id = id.clone();
                let cleanup_finished = progress_finished.clone();
                let cleanup_done = progress_done_rx;
                tauri::async_runtime::spawn(async move {
                    let _guard = lock_guard;
                    let _movement_guard = movement_guard;
                    let _ = terminal.await;
                    cleanup_finished.store(true, Ordering::Release);
                    cleanup_core.remove_progress_waiter(&cleanup_id);
                    let _ = cleanup_done.await;
                });
                let message = format!("stereo scan {id} did not finish within {SCAN_TIMEOUT:?}");
                emit(
                    &app,
                    LiveStatus {
                        position_index: position,
                        total_positions: POSITIONS,
                        phase: "failed".into(),
                        scan_id: Some(id.clone()),
                        ply_file: None,
                        message: Some(message.clone()),
                    },
                );
                return Err(AppError::CoreTools(message));
            }
        };
        progress_finished.store(true, Ordering::Release);
        core.remove_progress_waiter(&id);
        let _ = progress_done_rx.await;
        if event.get("event").and_then(serde_json::Value::as_str) != Some("stereo_scan_completed") {
            emit(
                &app,
                LiveStatus {
                    position_index: position,
                    total_positions: POSITIONS,
                    phase: "failed".into(),
                    scan_id: Some(id.clone()),
                    ply_file: None,
                    message: Some(terminal_reason(&event)),
                },
            );
            return Err(AppError::CoreTools(event.to_string()));
        }
        let Some(ply) = event
            .get("ply_file")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
        else {
            let message = "stereo_scan_completed omitted authoritative ply_file".to_string();
            emit(
                &app,
                LiveStatus {
                    position_index: position,
                    total_positions: POSITIONS,
                    phase: "failed".into(),
                    scan_id: Some(id.clone()),
                    ply_file: None,
                    message: Some(message.clone()),
                },
            );
            return Err(AppError::CoreTools(message));
        };
        emit(
            &app,
            LiveStatus {
                position_index: position,
                total_positions: POSITIONS,
                phase: "matching".into(),
                scan_id: Some(id.clone()),
                ply_file: Some(ply.to_string_lossy().into_owned()),
                message: None,
            },
        );
        let matching_result = run_matching_paths(
            &app,
            &matching,
            &ply,
            Path::new(&settings.matching_target_path),
            &settings,
        )
        .await;
        let matching_result = match matching_result {
            Ok(result) => result,
            Err(error) => {
                emit(
                    &app,
                    LiveStatus {
                        position_index: position,
                        total_positions: POSITIONS,
                        phase: "failed".into(),
                        scan_id: Some(id.clone()),
                        ply_file: Some(ply.to_string_lossy().into_owned()),
                        message: Some(error.to_string()),
                    },
                );
                return Err(error);
            }
        };
        if matching_result
            .assessment
            .as_ref()
            .is_some_and(|assessment| assessment.status == "needs_rescan")
        {
            let error = AppError::Matching("matching assessment requires rescan".into());
            emit(
                &app,
                LiveStatus {
                    position_index: position,
                    total_positions: POSITIONS,
                    phase: "failed".into(),
                    scan_id: Some(id.clone()),
                    ply_file: Some(ply.to_string_lossy().into_owned()),
                    message: Some(error.to_string()),
                },
            );
            return Err(error);
        }
        emit(
            &app,
            LiveStatus {
                position_index: position,
                total_positions: POSITIONS,
                phase: "completed".into(),
                scan_id: Some(id.clone()),
                ply_file: Some(ply.to_string_lossy().into_owned()),
                message: Some("position completed".into()),
            },
        );
        if position + 1 < POSITIONS {
            emit(
                &app,
                LiveStatus {
                    position_index: position,
                    total_positions: POSITIONS,
                    phase: "rotating".into(),
                    scan_id: Some(id.clone()),
                    ply_file: Some(ply.to_string_lossy().into_owned()),
                    message: None,
                },
            );
            let steps = (crate::state::turntable::STEPS_PER_REVOLUTION as f64 * 30.0 / 360.0)
                .round() as i64;
            // A relative move cannot preserve a previously confirmed absolute slot.
            turntable.set(Position::Unknown)?;
            let result = run(
                &app,
                &turntable,
                args(&settings, "", Some(steps))?,
                Duration::from_millis(settings.turntable_move_timeout_ms),
                true,
            )
            .await;
            if let Err(error) = result {
                emit(
                    &app,
                    LiveStatus {
                        position_index: position,
                        total_positions: POSITIONS,
                        phase: "failed".into(),
                        scan_id: Some(id.clone()),
                        ply_file: Some(ply.to_string_lossy().into_owned()),
                        message: Some(error.to_string()),
                    },
                );
                return Err(error);
            }
            tokio::time::sleep(Duration::from_millis(settings.turntable_settle_time_ms)).await;
        }
    }
    emit(
        &app,
        LiveStatus {
            position_index: POSITIONS - 1,
            total_positions: POSITIONS,
            phase: "completed".into(),
            scan_id: None,
            ply_file: None,
            message: Some("all positions completed".into()),
        },
    );
    drop(guard);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_video_device_ids() {
        assert_eq!(camera_id("/dev/video12").ok(), Some(12));
        assert!(camera_id("camera0").is_err());
    }
    #[test]
    fn sequence_has_twelve_positions_and_eleven_moves() {
        assert_eq!(POSITIONS, 12);
        assert_eq!(POSITIONS - 1, 11);
        assert_eq!((3200.0_f64 * 30.0 / 360.0).round() as i64, 267);
    }

    #[test]
    fn scan_ids_are_unique_across_runs() {
        assert_ne!(scan_id(0), scan_id(0));
    }

    #[test]
    fn acknowledgement_requires_success_and_matching_id() {
        assert!(scan_ack(&serde_json::json!({"ok":true,"scan_id":"x"}), "x").is_ok());
        assert!(scan_ack(&serde_json::json!({"ok":false,"scan_id":"x"}), "x").is_err());
        assert!(scan_ack(&serde_json::json!({"ok":true,"scan_id":"y"}), "x").is_err());
    }
}
