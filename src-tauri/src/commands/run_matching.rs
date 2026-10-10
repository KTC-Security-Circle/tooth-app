use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Listener};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc;

use crate::errors::AppError;
use crate::state::matching::{MatchingResponse, MatchingState};
use crate::state::settings::Settings;

const MATCHING_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const STEREO_SCAN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(1);
static SCAN_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Default)]
struct StereoScanProgress {
    acknowledged: bool,
    completed_path: Option<String>,
}

enum StereoScanMessage {
    Response(serde_json::Value),
    Event(serde_json::Value),
}

async fn await_stereo_scan(
    receiver: &mut tokio::sync::mpsc::UnboundedReceiver<StereoScanMessage>,
    scan_id: &str,
    core_generation: u64,
    timeout: Duration,
) -> Result<StereoScanProgress, AppError> {
    tokio::time::timeout(timeout, async {
        let mut progress = StereoScanProgress::default();
        while !progress.is_complete() {
            let message = receiver
                .recv()
                .await
                .ok_or_else(|| AppError::CoreTools("core-tools event stream closed".to_string()))?;
            match message {
                StereoScanMessage::Response(value) => progress.observe_response(&value, scan_id)?,
                StereoScanMessage::Event(value) => {
                    progress.observe_event(&value, scan_id, core_generation)?
                }
            }
        }
        Ok::<StereoScanProgress, AppError>(progress)
    })
    .await
    .map_err(|_| AppError::CoreTools("stereo_scan timed out".to_string()))?
}

impl StereoScanProgress {
    fn observe_response(
        &mut self,
        value: &serde_json::Value,
        scan_id: &str,
    ) -> Result<(), AppError> {
        if value.get("id").and_then(serde_json::Value::as_str) != Some(scan_id) {
            return Ok(());
        }
        if value.get("ok") == Some(&serde_json::Value::Bool(false)) {
            return Err(AppError::CoreTools(format!(
                "stereo_scan rejected: {value}"
            )));
        }
        if value.get("ok") == Some(&serde_json::Value::Bool(true)) {
            self.acknowledged = true;
        }
        Ok(())
    }

    fn observe_event(
        &mut self,
        value: &serde_json::Value,
        scan_id: &str,
        core_generation: u64,
    ) -> Result<(), AppError> {
        if value.get("event").and_then(serde_json::Value::as_str) == Some("core_tools_terminated") {
            if value
                .get("generation")
                .and_then(serde_json::Value::as_u64)
                .is_some_and(|generation| generation != core_generation)
            {
                return Ok(());
            }
            return Err(AppError::CoreTools(
                "core-tools terminated during scan".to_string(),
            ));
        }
        if value.get("scan_id").and_then(serde_json::Value::as_str) == Some(scan_id) {
            match value.get("event").and_then(serde_json::Value::as_str) {
                Some("stereo_scan_failed") => {
                    return Err(AppError::CoreTools(format!("stereo_scan failed: {value}")));
                }
                Some("stereo_scan_completed") => {
                    self.completed_path = value
                        .get("ply_file")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn is_complete(&self) -> bool {
        self.acknowledged && self.completed_path.is_some()
    }
}

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
    let child = match state.child.lock() {
        Ok(mut guard) => guard.take(),
        Err(poisoned) => poisoned.into_inner().take(),
    };
    if let Some(mut child) = child {
        // Exit handling is synchronous. Write directly to stdin so the
        // shutdown request is not stranded in the async writer queue.
        if child.write(b"{\"command\":\"shutdown\"}\n").is_err() {
            log::warn!("3mserve shutdown request failed; killing sidecar");
            let _ = child.kill();
        } else {
            // Give a healthy server a short opportunity to shut down cleanly,
            // then kill it so a server that ignores shutdown cannot be orphaned.
            std::thread::sleep(SHUTDOWN_GRACE_PERIOD);
            if child.kill().is_ok() {
                log::warn!(
                    "3mserve did not shut down within {SHUTDOWN_GRACE_PERIOD:?}; killed sidecar"
                );
            }
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

fn acquire_matching_request(
    lock: &tokio::sync::Mutex<()>,
) -> Result<tokio::sync::MutexGuard<'_, ()>, AppError> {
    lock.try_lock()
        .map_err(|_| AppError::Matching("matching operation is already running".to_string()))
}

#[tauri::command]
pub async fn run_matching(
    app: AppHandle,
    state: tauri::State<'_, MatchingState>,
    core_tools: tauri::State<'_, crate::state::core_tools::CoreToolsState>,
) -> Result<MatchingResult, AppError> {
    let _request_guard = acquire_matching_request(&state.request_lock)?;
    let settings = Settings::load(&app)?;
    if !settings.developer_mode {
        return Err(AppError::Validation(
            "developer mode must be enabled to run matching".to_string(),
        ));
    }
    let source_path = validate_ply_path("source_path", &settings.matching_source_path)?;
    let target_path = validate_ply_output_path(&settings.matching_target_path)?;
    validate_source_target_separation(&source_path, &target_path)?;
    let monitor_index = settings.projector_monitor_index.ok_or_else(|| {
        AppError::Validation("プロジェクターのモニターを選択してください".to_string())
    })?;
    let left_camera_id = camera_id("camera_left", &settings.camera_left)?;
    let right_camera_id = camera_id("camera_right", &settings.camera_right)?;
    if left_camera_id == right_camera_id {
        return Err(AppError::Validation(
            "左右で異なるカメラを選択してください".to_string(),
        ));
    }
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

    run_stereo_scan(
        &app,
        &core_tools,
        monitor_index,
        left_camera_id,
        right_camera_id,
        &target_path,
    )
    .await?;

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

async fn run_stereo_scan(
    app: &AppHandle,
    core_tools: &crate::state::core_tools::CoreToolsState,
    monitor_index: u32,
    left_camera_id: i32,
    right_camera_id: i32,
    target_path: &str,
) -> Result<(), AppError> {
    use tokio::sync::mpsc;

    let scan_id = format!(
        "matching-{}-{}",
        std::process::id(),
        SCAN_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let request = serde_json::json!({
        "id": scan_id,
        "cmd": "stereo_scan",
        "scan_id": scan_id,
        "left_camera_id": left_camera_id,
        "right_camera_id": right_camera_id,
        "monitor_index": monitor_index,
        "ply_file": target_path,
        "overwrite": true,
    });
    let (sender, mut receiver) = mpsc::unbounded_channel::<StereoScanMessage>();
    let response_sender = sender.clone();
    let response_listener = app.listen("core-tools:response", move |event| {
        if let Ok(value) = serde_json::from_str(event.payload()) {
            let _ = response_sender.send(StereoScanMessage::Response(value));
        }
    });
    let event_listener = app.listen("core-tools:event", move |event| {
        if let Ok(value) = serde_json::from_str(event.payload()) {
            let _ = sender.send(StereoScanMessage::Event(value));
        }
    });

    let result = async {
        let core_generation = core_tools
            .available_generation()
            .ok_or_else(|| AppError::CoreTools("core-tools is not running".to_string()))?;
        let command_sender = core_tools
            .cmd_tx
            .lock()
            .map_err(|_| AppError::CoreTools("core-tools state mutex poisoned".to_string()))?
            .clone()
            .ok_or_else(|| AppError::CoreTools("core-tools is not running".to_string()))?;
        command_sender
            .send(serde_json::to_string(&request).map_err(|e| AppError::CoreTools(e.to_string()))?)
            .await
            .map_err(|e| AppError::CoreTools(format!("failed to send stereo_scan: {e}")))?;

        let progress = await_stereo_scan(
            &mut receiver,
            &scan_id,
            core_generation,
            STEREO_SCAN_TIMEOUT,
        )
        .await?;

        if progress.completed_path.as_deref() != Some(target_path) {
            return Err(AppError::CoreTools(
                "stereo_scan completed with an unexpected PLY path".to_string(),
            ));
        }
        if !std::path::Path::new(target_path).is_file() {
            return Err(AppError::CoreTools(
                "stereo_scan did not create the expected PLY file".to_string(),
            ));
        }
        Ok(())
    }
    .await;

    app.unlisten(response_listener);
    app.unlisten(event_listener);
    result
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

fn validate_ply_output_path(value: &str) -> Result<String, AppError> {
    let path = std::path::Path::new(value);
    if value.trim().is_empty()
        || !path.is_absolute()
        || !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ply"))
    {
        return Err(AppError::Validation(
            "target_path must be an absolute .ply output path".to_string(),
        ));
    }
    Ok(value.to_string())
}

fn validate_source_target_separation(source: &str, target: &str) -> Result<(), AppError> {
    let source_path = std::fs::canonicalize(source).map_err(|error| {
        AppError::Validation(format!("cannot resolve matching source path: {error}"))
    })?;
    let target_path = canonical_output_path(std::path::Path::new(target)).map_err(|error| {
        AppError::Validation(format!("cannot resolve matching target path: {error}"))
    })?;
    let same_file = same_file_identity(source, target).map_err(|error| {
        AppError::Validation(format!(
            "cannot compare matching source and target: {error}"
        ))
    })?;
    if source_path == target_path || same_file {
        return Err(AppError::Validation(
            "matching source and scan target must be different files".to_string(),
        ));
    }
    Ok(())
}

fn canonical_output_path(path: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => return std::fs::canonicalize(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let filename = path
        .file_name()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "target path has no file name",
            )
        })?
        .to_os_string();
    let mut parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "target path has no parent",
        )
    })?;
    if std::fs::canonicalize(parent).is_ok() {
        let mut resolved = std::fs::canonicalize(parent)?;
        resolved.push(filename);
        return Ok(resolved);
    }
    if parent
        .components()
        .any(|component| component == std::path::Component::ParentDir)
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "cannot resolve parent traversal through a missing directory",
        ));
    }
    let mut missing = vec![filename];
    loop {
        match std::fs::metadata(parent) {
            Ok(metadata) if metadata.is_dir() => break,
            Ok(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotADirectory,
                    "target parent is not a directory",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    parent
                        .file_name()
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "no existing target parent directory",
                            )
                        })?
                        .to_os_string(),
                );
                parent = parent.parent().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "no existing target parent directory",
                    )
                })?;
            }
            Err(error) => return Err(error),
        }
    }
    let mut resolved = std::fs::canonicalize(parent)?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

#[cfg(unix)]
fn same_file_identity(source: &str, target: &str) -> std::io::Result<bool> {
    use std::os::unix::fs::MetadataExt;

    let source = std::fs::metadata(source)?;
    let target = match std::fs::metadata(target) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error),
    };
    Ok(source.dev() == target.dev() && source.ino() == target.ino())
}

#[cfg(not(unix))]
fn same_file_identity(_source: &str, _target: &str) -> std::io::Result<bool> {
    Ok(false)
}

fn camera_id(name: &str, value: &str) -> Result<i32, AppError> {
    let numeric = value.strip_prefix("/dev/video").unwrap_or(value);
    numeric
        .parse::<i32>()
        .ok()
        .filter(|id| *id >= 0)
        .ok_or_else(|| {
            AppError::Validation(format!("{name} must identify a numeric camera device"))
        })
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

#[cfg(test)]
mod tests {
    use super::{
        acquire_matching_request, await_stereo_scan, validate_source_target_separation,
        StereoScanMessage, StereoScanProgress,
    };
    use std::time::Duration;

    #[test]
    fn stereo_scan_waits_for_ack_and_matching_completion_in_any_order() {
        let mut progress = StereoScanProgress::default();
        progress
            .observe_response(&serde_json::json!({"id":"other","ok":true}), "scan")
            .expect("unrelated response is ignored");
        progress
            .observe_event(
                &serde_json::json!({"event":"stereo_scan_completed","scan_id":"other","ply_file":"/target.ply"}),
                "scan",
                7,
            )
            .expect("unrelated event is ignored");
        assert!(!progress.is_complete());

        progress
            .observe_event(
                &serde_json::json!({"event":"stereo_scan_completed","scan_id":"scan","ply_file":"/target.ply"}),
                "scan",
                7,
            )
            .expect("completion before ack is valid");
        assert!(!progress.is_complete());
        progress
            .observe_response(&serde_json::json!({"id":"scan","ok":true}), "scan")
            .expect("matching ack is valid");
        assert!(progress.is_complete());
    }

    #[test]
    fn stereo_scan_ack_alone_is_not_completion() {
        let mut progress = StereoScanProgress::default();
        progress
            .observe_response(&serde_json::json!({"id":"scan","ok":true}), "scan")
            .expect("ack is valid");
        assert!(!progress.is_complete());
    }

    #[test]
    fn stereo_scan_failure_never_completes() {
        let mut progress = StereoScanProgress::default();
        let target = std::env::temp_dir().join(format!("scan-failure-{}.ply", std::process::id()));
        std::fs::write(&target, b"previous result").expect("write existing target");
        let result = progress.observe_event(
            &serde_json::json!({"event":"stereo_scan_failed","scan_id":"scan"}),
            "scan",
            7,
        );
        assert!(result.is_err());
        assert!(!progress.is_complete());
        assert_eq!(
            std::fs::read(&target).expect("read existing target"),
            b"previous result"
        );
        let _ = std::fs::remove_file(target);
    }

    #[test]
    fn stereo_scan_termination_fails_waiter() {
        let mut progress = StereoScanProgress::default();
        let result = progress.observe_event(
            &serde_json::json!({"event":"core_tools_terminated","generation":7}),
            "scan",
            7,
        );
        assert!(result.is_err());
    }

    #[test]
    fn stale_core_termination_does_not_fail_current_scan() {
        let mut progress = StereoScanProgress::default();
        let result = progress.observe_event(
            &serde_json::json!({"event":"core_tools_terminated","generation":6}),
            "scan",
            7,
        );
        assert!(result.is_ok());
    }

    #[test]
    fn stereo_scan_ack_without_completion_times_out() {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        sender
            .send(StereoScanMessage::Response(
                serde_json::json!({"id":"scan","ok":true}),
            ))
            .expect("send ack");
        let result = tauri::async_runtime::block_on(await_stereo_scan(
            &mut receiver,
            "scan",
            7,
            Duration::from_millis(1),
        ));
        assert!(result.is_err());
    }

    #[test]
    fn duplicate_matching_operation_is_rejected() {
        let lock = tokio::sync::Mutex::new(());
        let _first = acquire_matching_request(&lock).expect("first request acquires lock");
        assert!(acquire_matching_request(&lock).is_err());
    }

    fn temp_root(name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "matching_path_safety_{}_{}",
            std::process::id(),
            name
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("create test directory");
        root
    }

    #[test]
    fn matching_rejects_source_path_and_symlink_aliases() {
        let root = temp_root("symlink");
        let source = root.join("source.ply");
        std::fs::write(&source, b"source").expect("write source");
        let source = source.to_string_lossy().into_owned();
        assert!(validate_source_target_separation(&source, &source).is_err());

        let alias = root.join("target.ply");
        std::os::unix::fs::symlink(&source, &alias).expect("create source symlink");
        assert!(validate_source_target_separation(&source, &alias.to_string_lossy()).is_err());

        let directory_alias = root.join("directory-alias");
        std::os::unix::fs::symlink(root.as_path(), &directory_alias)
            .expect("create directory symlink");
        let parent_alias = directory_alias.join("source.ply");
        assert!(
            validate_source_target_separation(&source, &parent_alias.to_string_lossy()).is_err()
        );
        let missing_parent_traversal = directory_alias.join("missing/../source.ply");
        assert!(validate_source_target_separation(
            &source,
            &missing_parent_traversal.to_string_lossy()
        )
        .is_err());
    }

    #[test]
    fn matching_rejects_hardlink_and_accepts_new_output() {
        let root = temp_root("hardlink");
        let source = root.join("source.ply");
        std::fs::write(&source, b"source").expect("write source");
        let hardlink = root.join("hardlink.ply");
        std::fs::hard_link(&source, &hardlink).expect("create hardlink");
        assert!(validate_source_target_separation(
            &source.to_string_lossy(),
            &hardlink.to_string_lossy()
        )
        .is_err());

        let new_output = root.join("new/nested/target.ply");
        assert!(validate_source_target_separation(
            &source.to_string_lossy(),
            &new_output.to_string_lossy()
        )
        .is_ok());
    }
}
