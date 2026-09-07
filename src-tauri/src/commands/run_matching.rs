use std::sync::atomic::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc;

use crate::errors::AppError;
use crate::state::matching::{MatchingResponse, MatchingState};
use crate::state::settings::Settings;

const MATCHING_TIMEOUT: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchingMode {
    Ransac,
    Icp,
    Matching,
}

impl MatchingMode {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Ransac => "ransac",
            Self::Icp => "icp",
            Self::Matching => "matching",
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchingResult {
    pub fitness: f64,
    pub inlier_rmse: f64,
    pub transformation: [[f64; 4]; 4],
    pub correspondence_set: Vec<[i64; 2]>,
}

pub async fn start_matching_server(app: &AppHandle, state: &MatchingState) -> Result<(), AppError> {
    let _request_guard = state.request_lock.lock().await;
    start_matching_server_inner(app, state).await
}

async fn start_matching_server_inner(
    app: &AppHandle,
    state: &MatchingState,
) -> Result<(), AppError> {
    if state
        .cmd_tx
        .lock()
        .map_err(|_| AppError::Matching("matching state mutex poisoned".to_string()))?
        .is_some()
    {
        return Ok(());
    }

    let (mut events, child) = app
        .shell()
        .sidecar("3mserve")
        .map_err(|error| AppError::Matching(format!("failed to resolve 3mserve: {error}")))?
        .spawn()
        .map_err(|error| AppError::Matching(format!("failed to spawn 3mserve: {error}")))?;
    let (cmd_tx, mut cmd_rx) = mpsc::channel::<String>(8);
    let (response_tx, response_rx) = mpsc::channel::<MatchingResponse>(8);
    let generation = state.generation.fetch_add(1, Ordering::AcqRel) + 1;

    *state
        .child
        .lock()
        .map_err(|_| AppError::Matching("matching state mutex poisoned".to_string()))? =
        Some(child);
    *state
        .cmd_tx
        .lock()
        .map_err(|_| AppError::Matching("matching state mutex poisoned".to_string()))? =
        Some(cmd_tx);
    *state.responses.lock().await = Some(response_rx);

    let child_for_writer = state.child.clone();
    tauri::async_runtime::spawn(async move {
        while let Some(line) = cmd_rx.recv().await {
            let write_result = child_for_writer
                .lock()
                .ok()
                .and_then(|mut guard| guard.as_mut().map(|child| child.write(line.as_bytes())));
            if write_result.is_none_or(|result| result.is_err()) {
                break;
            }
        }
    });

    let cmd_tx_for_reader = state.cmd_tx.clone();
    let child_for_reader = state.child.clone();
    let generation_for_reader = state.generation.clone();
    tauri::async_runtime::spawn(async move {
        let mut buffer = Vec::new();
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    buffer.extend(bytes);
                    while let Some(position) = buffer.iter().position(|byte| *byte == b'\n') {
                        let line: Vec<_> = buffer.drain(..=position).collect();
                        let line = String::from_utf8_lossy(&line);
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        match serde_json::from_str(line) {
                            Ok(value) => {
                                if response_tx.send(Ok(value)).await.is_err() {
                                    return;
                                }
                            }
                            Err(error) => {
                                log::warn!("3mserve returned non-JSON stdout: {line} ({error})")
                            }
                        }
                    }
                }
                CommandEvent::Stderr(bytes) => {
                    log::warn!("3mserve stderr: {}", String::from_utf8_lossy(&bytes).trim())
                }
                CommandEvent::Error(message) => {
                    let _ = response_tx
                        .send(Err(format!("3mserve command error: {message}")))
                        .await;
                    break;
                }
                CommandEvent::Terminated(payload) => {
                    let _ = response_tx
                        .send(Err(format!(
                            "3mserve terminated (code={:?}, signal={:?})",
                            payload.code, payload.signal
                        )))
                        .await;
                    break;
                }
                _ => {}
            }
        }
        if generation_for_reader.load(Ordering::Acquire) == generation {
            if let Ok(mut guard) = cmd_tx_for_reader.lock() {
                *guard = None;
            }
            if let Ok(mut guard) = child_for_reader.lock() {
                *guard = None;
            }
        }
    });

    let ready = match tokio::time::timeout(Duration::from_secs(10), receive_response(state)).await {
        Ok(result) => result?,
        Err(_) => {
            reset_matching_server(state);
            return Err(AppError::Matching(
                "3mserve did not become ready within 10s".to_string(),
            ));
        }
    };
    if ready.get("ready") != Some(&serde_json::Value::Bool(true)) {
        reset_matching_server(state);
        return Err(AppError::Matching(
            "3mserve did not become ready".to_string(),
        ));
    }
    Ok(())
}

pub fn stop_matching_server(state: &MatchingState) {
    state.generation.fetch_add(1, Ordering::AcqRel);
    if let Ok(mut guard) = state.child.lock() {
        if let Some(child) = guard.as_mut() {
            // Exit handling is synchronous. Write directly to stdin so the
            // shutdown request is not stranded in the async writer queue.
            let _ = child.write(b"{\"command\":\"shutdown\"}\n");
        }
    }
    if let Ok(mut guard) = state.cmd_tx.lock() {
        *guard = None;
    }
}

fn reset_matching_server(state: &MatchingState) {
    state.generation.fetch_add(1, Ordering::AcqRel);
    if let Ok(mut guard) = state.cmd_tx.lock() {
        *guard = None;
    }
    if let Ok(mut guard) = state.child.lock() {
        if let Some(child) = guard.take() {
            let _ = child.kill();
        }
    }
}

async fn receive_response(state: &MatchingState) -> Result<serde_json::Value, AppError> {
    let mut responses = state.responses.lock().await;
    responses
        .as_mut()
        .ok_or_else(|| AppError::Matching("3mserve response channel is unavailable".to_string()))?
        .recv()
        .await
        .ok_or_else(|| AppError::Matching("3mserve response stream closed".to_string()))?
        .map_err(AppError::Matching)
}

#[tauri::command]
pub async fn run_matching(
    app: AppHandle,
    state: tauri::State<'_, MatchingState>,
) -> Result<MatchingResult, AppError> {
    let _request_guard = state.request_lock.lock().await;
    let settings = Settings::load(&app)?;
    if !settings.developer_mode {
        return Err(AppError::Validation(
            "developer mode must be enabled to run matching".to_string(),
        ));
    }
    let source_path = validate_ply_path("source_path", &settings.matching_source_path)?;
    let target_path = validate_ply_path("target_path", &settings.matching_target_path)?;
    let mode = match settings.matching_mode.as_str() {
        "ransac" => MatchingMode::Ransac,
        "icp" => MatchingMode::Icp,
        "matching" => MatchingMode::Matching,
        _ => {
            return Err(AppError::Validation(
                "matching_mode must be one of ransac, icp, or matching".to_string(),
            ))
        }
    };
    if !settings.matching_voxel_size.is_finite() || settings.matching_voxel_size <= 0.0 {
        return Err(AppError::Validation(
            "matching_voxel_size must be finite and greater than zero".to_string(),
        ));
    }
    if settings.matching_ransac_iterations < 1 {
        return Err(AppError::Validation(
            "matching_ransac_iterations must be at least 1".to_string(),
        ));
    }

    start_matching_server_inner(&app, &state).await?;
    let request = serde_json::json!({
        "command": "matching",
        "mode": mode.as_str(),
        "source_path": source_path,
        "target_path": target_path,
        "voxel_size": settings.matching_voxel_size,
        "ransac_iterations": settings.matching_ransac_iterations,
    });
    let sender = state
        .cmd_tx
        .lock()
        .map_err(|_| AppError::Matching("matching state mutex poisoned".to_string()))?
        .clone()
        .ok_or_else(|| AppError::Matching("3mserve is not running".to_string()))?;
    if sender
        .send(format!(
            "{}\n",
            serde_json::to_string(&request).map_err(|e| AppError::Matching(e.to_string()))?
        ))
        .await
        .is_err()
    {
        reset_matching_server(&state);
        return Err(AppError::Matching(
            "failed to send matching request to 3mserve".to_string(),
        ));
    }

    let response = match tokio::time::timeout(MATCHING_TIMEOUT, receive_response(&state)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            reset_matching_server(&state);
            return Err(error);
        }
        Err(_) => {
            log::error!("matching timed out after {MATCHING_TIMEOUT:?}");
            reset_matching_server(&state);
            return Err(AppError::Matching("3mserve timed out".to_string()));
        }
    };
    if let Some(error) = response.get("error") {
        reset_matching_server(&state);
        return Err(AppError::Matching(format!(
            "3mserve matching error: {error}"
        )));
    }
    let result = serde_json::from_value::<MatchingResult>(response)
        .map_err(|error| AppError::Matching(format!("invalid matching JSON: {error}")))?;
    validate_result(&result)?;
    Ok(result)
}

fn validate_ply_path(name: &str, value: &str) -> Result<String, AppError> {
    let path = std::path::Path::new(value);
    if value.trim().is_empty()
        || !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("ply"))
        || !path.is_file()
    {
        return Err(AppError::Validation(format!(
            "{name} must be an existing regular PLY file"
        )));
    }
    Ok(value.to_string())
}

fn validate_result(result: &MatchingResult) -> Result<(), AppError> {
    if !result.fitness.is_finite()
        || !result.inlier_rmse.is_finite()
        || result
            .transformation
            .iter()
            .flatten()
            .any(|value| !value.is_finite())
    {
        return Err(AppError::Matching(
            "matching result contains non-finite values".to_string(),
        ));
    }
    Ok(())
}
