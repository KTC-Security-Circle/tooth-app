use std::io::{BufReader, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::mpsc;

use crate::errors::AppError;
use crate::state::matching::{MatchingResponse, MatchingState};
use crate::state::settings::Settings;
use crate::state::turntable::TurntableState;

const MATCHING_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const STEREO_SCAN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const SHUTDOWN_GRACE_PERIOD: Duration = Duration::from_secs(1);
const PLY_HEADER_LIMIT: usize = 64 * 1024;
static SCAN_REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchingResult {
    pub fitness: f64,
    pub inlier_rmse: f64,
    pub transformation: [[f64; 4]; 4],
    pub correspondence_set: Vec<[i64; 2]>,
    #[serde(default)]
    pub source_point_count: usize,
    #[serde(default)]
    pub target_point_count: usize,
    #[serde(default)]
    pub correspondence_count: usize,
    #[serde(default)]
    pub assessment: Option<MatchingAssessment>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MatchingAssessment {
    pub status: String,
    pub reasons: Vec<String>,
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
    turntable: tauri::State<'_, TurntableState>,
) -> Result<MatchingResult, AppError> {
    let live_guard = core_tools
        .live_scan_lock
        .clone()
        .try_lock_owned()
        .map_err(|_| AppError::CoreTools("a scan operation is already running".to_string()))?;
    let turntable_guard = turntable.movement.clone().lock_owned().await;
    let request_guard = state
        .request_lock
        .clone()
        .try_lock_owned()
        .map_err(|_| AppError::Matching("matching operation is already running".to_string()))?;
    let mut settings = Settings::load(&app)?;
    if settings.stereo_calibration_file.trim().is_empty() {
        settings.stereo_calibration_file = "data/calib/stereo.yml".to_string();
    }
    let source_path = home_matching_source_path(&settings)?;
    let source_path = validate_ply_path("source_path", source_path)?;
    let target_path = validate_ply_output_path(&settings.matching_target_path)?;
    let target_references = [
        source_path.as_str(),
        settings.matching_source_path.as_str(),
        settings.fixed_reference_ply.as_str(),
    ];
    validate_target_aliases(&target_path, &target_references)?;
    let monitor_index = settings
        .monitor_index
        .ok_or_else(|| AppError::Validation("monitorIndex must be selected".to_string()))?;
    if !crate::commands::list_scan_monitors::list_scan_monitors_inner(&core_tools)
        .await?
        .iter()
        .any(|monitor| monitor.index == monitor_index)
    {
        return Err(AppError::Validation(format!(
            "monitor index {monitor_index} is not available"
        )));
    }
    let left_camera_id = camera_id("camera_left", &settings.camera_left)?;
    let right_camera_id = camera_id("camera_right", &settings.camera_right)?;
    if left_camera_id == right_camera_id {
        return Err(AppError::Validation(
            "左右で異なるカメラを選択してください".to_string(),
        ));
    }
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

    let temp_path = reserve_scan_ply(std::path::Path::new(&target_path))?;
    let temp_path_string = temp_path.cloud_path.to_string_lossy().into_owned();
    let core_generation = core_tools
        .available_generation()
        .ok_or_else(|| AppError::CoreTools("core-tools is not running".to_string()))?;
    if let Err(failure) = run_stereo_scan(
        &core_tools,
        monitor_index,
        left_camera_id,
        right_camera_id,
        &temp_path_string,
        &settings,
    )
    .await
    {
        match failure {
            ScanFailure::Complete(error) => return Err(error),
            ScanFailure::Uncertain(error, terminal) => {
                let cleanup_core = core_tools.inner().clone();
                tauri::async_runtime::spawn(async move {
                    let _live_guard = live_guard;
                    let _turntable_guard = turntable_guard;
                    let _request_guard = request_guard;
                    let _temp_path = temp_path;
                    await_uncertain_scan_cleanup(cleanup_core, core_generation, terminal).await;
                });
                return Err(error);
            }
        }
    }
    publish_scan_output(&temp_path_string, &target_path, &target_references)?;

    run_matching_paths_inner(&app, &state, &source_path, &target_path, &settings).await
}

async fn await_uncertain_scan_cleanup(
    core_tools: crate::state::core_tools::CoreToolsState,
    generation: u64,
    terminal: Option<tokio::sync::oneshot::Receiver<serde_json::Value>>,
) {
    if let Some(terminal) = terminal {
        // Poll exactly once. If the sender was drained, wait only for generation death.
        if terminal.await.is_ok() {
            return;
        }
    }
    loop {
        if core_tools.has_terminated(generation)
            || core_tools.generation.load(Ordering::Acquire) != generation
        {
            break;
        }
        core_tools.terminated.notified().await;
    }
}

fn home_matching_source_path(settings: &Settings) -> Result<&str, AppError> {
    let source = settings.matching_source_path.trim();
    if source.is_empty() {
        return Err(AppError::Validation(
            "matchingSourcePath is required for home matching".to_string(),
        ));
    }
    Ok(source)
}

pub(crate) async fn run_matching_paths(
    app: &AppHandle,
    state: &MatchingState,
    source_path: &std::path::Path,
    target_path: &std::path::Path,
    settings: &Settings,
) -> Result<MatchingResult, AppError> {
    let _request_guard = acquire_matching_request(&state.request_lock)?;
    let source_path = validate_ply_path("source_path", &source_path.to_string_lossy())?;
    let target_path = validate_ply_path("target_path", &target_path.to_string_lossy())?;
    validate_source_target_separation(&source_path, &target_path)?;
    run_matching_paths_inner(app, state, &source_path, &target_path, settings).await
}

async fn run_matching_paths_inner(
    app: &AppHandle,
    state: &MatchingState,
    source_path: &str,
    target_path: &str,
    settings: &Settings,
) -> Result<MatchingResult, AppError> {
    if !settings.matching_voxel_size.is_finite()
        || settings.matching_voxel_size <= 0.0
        || settings.matching_ransac_iterations == 0
        || settings
            .minimum_fitness
            .is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value))
        || settings
            .maximum_rmse
            .is_some_and(|value| !value.is_finite() || value < 0.0)
    {
        return Err(AppError::Validation(
            "matching quality settings are invalid".to_string(),
        ));
    }
    start_matching_server_inner(app, state).await?;
    let request = serde_json::json!({
        "command": "matching",
        "mode": "matching",
        "source_path": source_path,
        "target_path": target_path,
        "voxel_size": settings.matching_voxel_size,
        "ransac_iterations": settings.matching_ransac_iterations,
        "minimum_fitness": settings.minimum_fitness,
        "maximum_rmse": settings.maximum_rmse,
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
        reset_matching_server(state);
        return Err(AppError::Matching(
            "failed to send matching request to 3mserve".to_string(),
        ));
    }

    let response = match tokio::time::timeout(MATCHING_TIMEOUT, receive_response(state)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => {
            reset_matching_server(state);
            return Err(error);
        }
        Err(_) => {
            log::error!("matching timed out after {MATCHING_TIMEOUT:?}");
            reset_matching_server(state);
            return Err(AppError::Matching("3mserve timed out".to_string()));
        }
    };
    if let Some(error) = response.get("error") {
        reset_matching_server(state);
        return Err(AppError::Matching(format!(
            "3mserve matching error: {error}"
        )));
    }
    let mut result = serde_json::from_value::<MatchingResult>(response)
        .map_err(|error| AppError::Matching(format!("invalid matching JSON: {error}")))?;
    validate_result(&result)?;
    result.assessment = Some(assess_result(
        &result,
        settings.minimum_fitness,
        settings.maximum_rmse,
    ));
    Ok(result)
}

struct TemporaryPly {
    directory: std::path::PathBuf,
    cloud_path: std::path::PathBuf,
}

impl Drop for TemporaryPly {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.directory) {
            if error.kind() != std::io::ErrorKind::NotFound {
                log::warn!(
                    "failed to clean temporary scan directory '{}': {error}",
                    self.directory.display()
                );
            }
        }
    }
}

fn reserve_scan_ply(target: &std::path::Path) -> Result<TemporaryPly, AppError> {
    let target = canonical_output_path(target).map_err(|error| {
        AppError::Validation(format!("cannot resolve matching target parent: {error}"))
    })?;
    let parent = target.parent().ok_or_else(|| {
        AppError::Validation("matching target must have a parent directory".to_string())
    })?;
    let stem = target
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("scan");
    for _ in 0..100 {
        let sequence = SCAN_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = parent.join(format!(".{stem}-{}-{sequence}.tmp", std::process::id()));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        match builder.create(&directory) {
            Ok(()) => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Err(error) =
                        std::fs::set_permissions(&directory, std::fs::Permissions::from_mode(0o700))
                    {
                        let _ = std::fs::remove_dir_all(&directory);
                        return Err(AppError::Io(format!(
                            "failed to secure private scan output directory: {error}"
                        )));
                    }
                }
                let cloud_path = directory.join("cloud.ply");
                match std::fs::symlink_metadata(&cloud_path) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Ok(_) => {
                        let _ = std::fs::remove_dir_all(&directory);
                        return Err(AppError::Io(
                            "reserved scan output unexpectedly already exists".to_string(),
                        ));
                    }
                    Err(error) => {
                        let _ = std::fs::remove_dir_all(&directory);
                        return Err(AppError::Io(format!(
                            "failed to verify reserved scan output: {error}"
                        )));
                    }
                }
                return Ok(TemporaryPly {
                    directory,
                    cloud_path,
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(AppError::Io(format!(
                    "failed to reserve private scan output directory: {error}"
                )))
            }
        }
    }
    Err(AppError::Io(
        "could not allocate a temporary scan output path".to_string(),
    ))
}

fn publish_scan_output(
    temp_path: &str,
    target_path: &str,
    source_paths: &[&str],
) -> Result<(), AppError> {
    validate_target_aliases(target_path, source_paths)?;
    validate_ply_path("stereo scan output", temp_path)?;
    let destination =
        canonical_output_path(std::path::Path::new(target_path)).map_err(|error| {
            AppError::Validation(format!("cannot resolve matching target parent: {error}"))
        })?;
    std::fs::rename(temp_path, destination)
        .map_err(|error| AppError::Io(format!("failed to publish scan output: {error}")))
}

enum ScanFailure {
    Complete(AppError),
    Uncertain(
        AppError,
        Option<tokio::sync::oneshot::Receiver<serde_json::Value>>,
    ),
}

enum ScanWaitFailure {
    Complete(AppError),
    Uncertain(AppError),
    TerminalClosed(AppError),
}

async fn await_stereo_scan(
    acknowledgement: Result<serde_json::Value, AppError>,
    terminal: &mut tokio::sync::oneshot::Receiver<serde_json::Value>,
    expected_scan_id: &str,
    expected_path: &str,
    timeout: Duration,
) -> Result<serde_json::Value, ScanWaitFailure> {
    let response = match acknowledgement {
        Ok(response) => response,
        Err(error) => return Err(ScanWaitFailure::Uncertain(error)),
    };
    if response.get("ok") == Some(&serde_json::Value::Bool(false)) {
        return Err(ScanWaitFailure::Complete(AppError::CoreTools(format!(
            "stereo_scan rejected: {response}"
        ))));
    }
    if response.get("ok") != Some(&serde_json::Value::Bool(true)) {
        return Err(ScanWaitFailure::Uncertain(AppError::CoreTools(format!(
            "stereo_scan acknowledgement was inconclusive: {response}"
        ))));
    }
    if response.get("scan_id").and_then(serde_json::Value::as_str) != Some(expected_scan_id) {
        return Err(ScanWaitFailure::Uncertain(AppError::CoreTools(format!(
            "stereo_scan acknowledgement has missing or unexpected scan_id for {expected_scan_id}"
        ))));
    }

    let event = match tokio::time::timeout(timeout, &mut *terminal).await {
        Ok(Ok(event)) => event,
        Ok(Err(_)) => {
            return Err(ScanWaitFailure::TerminalClosed(AppError::CoreTools(
                "core-tools event stream closed during scan".to_string(),
            )))
        }
        Err(_) => {
            return Err(ScanWaitFailure::Uncertain(AppError::CoreTools(
                "stereo_scan timed out".to_string(),
            )))
        }
    };
    validate_stereo_scan_completion(&response, &event, expected_path)
        .map_err(ScanWaitFailure::Complete)?;
    Ok(event.clone())
}

async fn wait_for_stereo_scan(
    core_tools: &crate::state::core_tools::CoreToolsState,
    scan_id: &str,
    acknowledgement: Result<serde_json::Value, AppError>,
    mut terminal: tokio::sync::oneshot::Receiver<serde_json::Value>,
    expected_path: &str,
    timeout: Duration,
) -> Result<serde_json::Value, ScanFailure> {
    match await_stereo_scan(
        acknowledgement,
        &mut terminal,
        scan_id,
        expected_path,
        timeout,
    )
    .await
    {
        Ok(event) => Ok(event),
        Err(ScanWaitFailure::Complete(error)) => {
            core_tools.remove_terminal_waiter(scan_id);
            Err(ScanFailure::Complete(error))
        }
        Err(ScanWaitFailure::Uncertain(error)) => {
            Err(ScanFailure::Uncertain(error, Some(terminal)))
        }
        Err(ScanWaitFailure::TerminalClosed(error)) => Err(ScanFailure::Uncertain(error, None)),
    }
}

async fn run_stereo_scan(
    core_tools: &crate::state::core_tools::CoreToolsState,
    monitor_index: u32,
    left_camera_id: i32,
    right_camera_id: i32,
    target_path: &str,
    settings: &Settings,
) -> Result<(), ScanFailure> {
    let core_generation = core_tools.available_generation().ok_or_else(|| {
        ScanFailure::Complete(AppError::CoreTools("core-tools is not running".to_string()))
    })?;
    let scan_id = format!(
        "matching-{}-{}",
        std::process::id(),
        SCAN_REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    );
    let request = serde_json::json!({
        "cmd": "stereo_scan",
        "scan_id": scan_id,
        "left_camera_id": left_camera_id,
        "right_camera_id": right_camera_id,
        "monitor_index": monitor_index,
        "calibration_file": settings.stereo_calibration_file,
        "sync_mode": "delay",
        "delay_ms": 100,
        "decode_threshold": settings.decode_threshold,
        "ply_file": target_path,
    });
    let terminal = core_tools.register_terminal_waiter(&scan_id);
    let acknowledgement = core_tools.request(request, STEREO_SCAN_TIMEOUT).await;
    let event = wait_for_stereo_scan(
        core_tools,
        &scan_id,
        acknowledgement,
        terminal,
        target_path,
        STEREO_SCAN_TIMEOUT,
    )
    .await?;
    let _ = event;
    if !std::path::Path::new(target_path).is_file() {
        return Err(ScanFailure::Complete(AppError::CoreTools(
            "stereo_scan did not create the expected PLY file".to_string(),
        )));
    }
    if core_tools.has_terminated(core_generation) {
        return Err(ScanFailure::Complete(AppError::CoreTools(
            "core-tools terminated during scan".to_string(),
        )));
    }
    Ok(())
}

fn validate_stereo_scan_completion(
    response: &serde_json::Value,
    event: &serde_json::Value,
    expected_path: &str,
) -> Result<(), AppError> {
    if response.get("ok") != Some(&serde_json::Value::Bool(true)) {
        return Err(AppError::CoreTools(format!(
            "stereo_scan rejected: {response}"
        )));
    }
    if event.get("event").and_then(serde_json::Value::as_str) != Some("stereo_scan_completed") {
        return Err(AppError::CoreTools(format!("stereo_scan failed: {event}")));
    }
    if event.get("ply_file").and_then(serde_json::Value::as_str) != Some(expected_path) {
        return Err(AppError::CoreTools(
            "stereo_scan completed with an unexpected PLY path".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn validate_ply_path(name: &str, value: &str) -> Result<String, AppError> {
    let path = std::path::Path::new(value);
    let metadata = std::fs::symlink_metadata(path).map_err(|error| {
        AppError::Validation(format!("{name} must be a regular PLY file: {error}"))
    })?;
    if !metadata.file_type().is_file() {
        return Err(AppError::Validation(format!(
            "{name} must be a nonsymlink regular PLY file"
        )));
    }
    let file = std::fs::File::open(path).map_err(|error| {
        AppError::Validation(format!("{name} must be a readable PLY file: {error}"))
    })?;
    let mut bytes = Vec::new();
    BufReader::with_capacity(PLY_HEADER_LIMIT, file)
        .take(PLY_HEADER_LIMIT as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            AppError::Validation(format!("{name} must be a readable PLY file: {error}"))
        })?;
    let header = String::from_utf8_lossy(&bytes);
    let has_vertex = header.lines().any(|line| {
        let mut fields = line.split_whitespace();
        fields.next() == Some("element")
            && fields.next() == Some("vertex")
            && fields
                .next()
                .and_then(|count| count.parse::<usize>().ok())
                .is_some_and(|count| count > 0)
    });
    if value.trim().is_empty()
        || !path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("ply"))
        || !path.is_file()
        || bytes.is_empty()
        || header
            .lines()
            .next()
            .is_none_or(|line| line.trim() != "ply")
        || !header.lines().any(|line| line.trim() == "end_header")
        || !has_vertex
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
    canonical_output_path(path)
        .map(|path| path.to_string_lossy().into_owned())
        .map_err(|error| AppError::Validation(format!("cannot resolve target parent: {error}")))
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
    let filename = path
        .file_name()
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "target path has no file name",
            )
        })?
        .to_os_string();
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "target path has no parent",
        )
    })?;
    if let Ok(mut resolved) = std::fs::canonicalize(parent) {
        if !resolved.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotADirectory,
                "target parent is not a directory",
            ));
        }
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
    let mut existing_parent = parent;
    loop {
        match std::fs::metadata(existing_parent) {
            Ok(metadata) if metadata.is_dir() => break,
            Ok(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::NotADirectory,
                    "target parent is not a directory",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing.push(
                    existing_parent
                        .file_name()
                        .ok_or_else(|| {
                            std::io::Error::new(
                                std::io::ErrorKind::NotFound,
                                "no existing target parent directory",
                            )
                        })?
                        .to_os_string(),
                );
                existing_parent = existing_parent.parent().ok_or_else(|| {
                    std::io::Error::new(
                        std::io::ErrorKind::NotFound,
                        "no existing target parent directory",
                    )
                })?;
            }
            Err(error) => return Err(error),
        }
    }
    let mut resolved = std::fs::canonicalize(existing_parent)?;
    for component in missing.iter().rev() {
        resolved.push(component);
    }
    Ok(resolved)
}

fn validate_target_aliases(target: &str, source_paths: &[&str]) -> Result<(), AppError> {
    let target_path = canonical_output_path(std::path::Path::new(target)).map_err(|error| {
        AppError::Validation(format!("cannot resolve matching target path: {error}"))
    })?;
    for source in source_paths.iter().filter(|source| !source.is_empty()) {
        if std::path::Path::new(source) == std::path::Path::new(target) {
            return Err(AppError::Validation(
                "matching source and scan target must be different files".to_string(),
            ));
        }
        let same_path = canonical_output_path(std::path::Path::new(source))
            .is_ok_and(|source_path| source_path == target_path);
        let same_file = match same_file_identity(source, target) {
            Ok(same_file) => same_file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
            Err(error) => {
                return Err(AppError::Validation(format!(
                    "cannot compare matching source and target: {error}"
                )))
            }
        };
        if same_path || same_file {
            return Err(AppError::Validation(
                "matching source and scan target must be different files".to_string(),
            ));
        }
    }
    Ok(())
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
        || !(0.0..=1.0).contains(&result.fitness)
        || result.inlier_rmse < 0.0
        || result.correspondence_count != result.correspondence_set.len()
        || result.correspondence_set.iter().any(|[source, target]| {
            *source < 0
                || *target < 0
                || (*source as usize) >= result.source_point_count
                || (*target as usize) >= result.target_point_count
        })
    {
        return Err(AppError::Matching(
            "matching result contains non-finite values".to_string(),
        ));
    }
    Ok(())
}

fn assess_result(
    result: &MatchingResult,
    minimum_fitness: Option<f64>,
    maximum_rmse: Option<f64>,
) -> MatchingAssessment {
    if result.source_point_count < 3
        || result.target_point_count < 3
        || result.correspondence_count < 3
    {
        return MatchingAssessment {
            status: "needs_rescan".to_string(),
            reasons: vec!["insufficient_quality".to_string()],
        };
    }
    if minimum_fitness.is_none() && maximum_rmse.is_none() {
        return MatchingAssessment {
            status: "needs_review".to_string(),
            reasons: vec!["thresholds_not_provided".to_string()],
        };
    }
    let passed = minimum_fitness.is_none_or(|minimum| result.fitness >= minimum)
        && maximum_rmse.is_none_or(|maximum| result.inlier_rmse <= maximum);
    if passed {
        MatchingAssessment {
            status: "matched".to_string(),
            reasons: Vec::new(),
        }
    } else {
        MatchingAssessment {
            status: "not_matched".to_string(),
            reasons: vec!["quality_threshold_not_met".to_string()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        acquire_matching_request, await_uncertain_scan_cleanup, validate_source_target_separation,
        wait_for_stereo_scan, ScanFailure,
    };
    use crate::state::settings::Settings;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    #[test]
    fn production_waiter_rejection_releases_terminal_registration_immediately() {
        let core = crate::state::core_tools::CoreToolsState::default();
        core.generation.store(1, Ordering::Release);
        *core.status.lock().expect("core status") =
            crate::state::core_tools::CoreToolsStatus::Running;
        let terminal = core.register_terminal_waiter("rejected");
        let result = tauri::async_runtime::block_on(wait_for_stereo_scan(
            &core,
            "rejected",
            Ok(serde_json::json!({"ok":false,"error":"directory_create_failed"})),
            terminal,
            "/unused.ply",
            Duration::from_secs(1),
        ));
        assert!(matches!(result, Err(ScanFailure::Complete(_))));
        assert!(core
            .terminal_waiters
            .lock()
            .expect("terminal waiters")
            .is_empty());
    }

    #[test]
    fn production_waiter_keeps_missing_or_mismatched_ack_ids_uncertain() {
        let core = crate::state::core_tools::CoreToolsState::default();
        core.generation.store(2, Ordering::Release);
        *core.status.lock().expect("core status") =
            crate::state::core_tools::CoreToolsStatus::Running;

        for (scan_id, acknowledgement) in [
            ("missing-id", serde_json::json!({"ok":true})),
            (
                "mismatched-id",
                serde_json::json!({"ok":true,"scan_id":"other-scan"}),
            ),
        ] {
            let terminal = core.register_terminal_waiter(scan_id);
            let failure = tauri::async_runtime::block_on(wait_for_stereo_scan(
                &core,
                scan_id,
                Ok(acknowledgement),
                terminal,
                "/unused.ply",
                Duration::from_secs(1),
            ))
            .expect_err("unmatched acknowledgement must remain uncertain");
            let ScanFailure::Uncertain(_, Some(terminal)) = failure else {
                panic!("invalid success ack must retain its terminal receiver");
            };
            assert!(core
                .terminal_waiters
                .lock()
                .expect("terminal waiters")
                .contains_key(scan_id));
            core.resolve_terminal_event(&serde_json::json!({
                "event":"stereo_scan_completed",
                "scan_id":scan_id,
                "ply_file":"/unused.ply"
            }));
            tauri::async_runtime::block_on(await_uncertain_scan_cleanup(
                core.clone(),
                2,
                Some(terminal),
            ));
        }
    }

    #[test]
    fn production_waiter_accepts_matching_ack_after_terminal_event() {
        let core = crate::state::core_tools::CoreToolsState::default();
        core.generation.store(3, Ordering::Release);
        *core.status.lock().expect("core status") =
            crate::state::core_tools::CoreToolsStatus::Running;
        let terminal = core.register_terminal_waiter("valid-scan");
        core.resolve_terminal_event(&serde_json::json!({
            "event":"stereo_scan_completed",
            "scan_id":"valid-scan",
            "ply_file":"/valid.ply"
        }));
        let result = tauri::async_runtime::block_on(wait_for_stereo_scan(
            &core,
            "valid-scan",
            Ok(serde_json::json!({"ok":true,"scan_id":"valid-scan"})),
            terminal,
            "/valid.ply",
            Duration::from_secs(1),
        ));
        assert!(result.is_ok());
    }

    #[test]
    fn production_waiter_timeout_retains_until_late_terminal_then_cleans_only() {
        let core = crate::state::core_tools::CoreToolsState::default();
        core.generation.store(4, Ordering::Release);
        *core.status.lock().expect("core status") =
            crate::state::core_tools::CoreToolsStatus::Running;
        let terminal = core.register_terminal_waiter("timed-out");
        let failure = tauri::async_runtime::block_on(wait_for_stereo_scan(
            &core,
            "timed-out",
            Ok(serde_json::json!({"ok":true,"scan_id":"timed-out"})),
            terminal,
            "/unused.ply",
            Duration::from_millis(1),
        ))
        .expect_err("missing terminal event is uncertain");
        let ScanFailure::Uncertain(_, Some(terminal)) = failure else {
            panic!("timeout must retain its pending terminal receiver");
        };
        let root = temp_root("late-terminal-cleanup");
        let target = root.join("target.ply");
        let reservation = super::reserve_scan_ply(&target).expect("reserve staged output");
        let staging_dir = reservation.directory.clone();
        let live_guard = core
            .live_scan_lock
            .clone()
            .try_lock_owned()
            .expect("acquire scan lock");
        tauri::async_runtime::block_on(async move {
            let cleanup_core = core.clone();
            let cleanup = tokio::spawn(async move {
                let _live_guard = live_guard;
                let _reservation = reservation;
                await_uncertain_scan_cleanup(cleanup_core, 4, Some(terminal)).await;
            });
            tokio::task::yield_now().await;
            assert!(!cleanup.is_finished());
            assert!(core.live_scan_lock.try_lock().is_err());
            assert!(staging_dir.exists());
            core.resolve_terminal_event(&serde_json::json!({
                "event":"stereo_scan_completed",
                "scan_id":"timed-out",
                "ply_file":"/unused.ply"
            }));
            cleanup.await.expect("late terminal releases cleanup");
            assert!(core.live_scan_lock.try_lock().is_ok());
            assert!(!staging_dir.exists());
        });
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn production_waiter_closed_receiver_is_consumed_once_and_death_unblocks_cleanup() {
        let core = crate::state::core_tools::CoreToolsState::default();
        core.generation.store(9, Ordering::Release);
        *core.status.lock().expect("core status") =
            crate::state::core_tools::CoreToolsStatus::Running;
        let terminal = core.register_terminal_waiter("closed");
        core.invalidate_generation(
            9,
            crate::state::core_tools::CoreToolsStatus::Disconnected,
            false,
        );
        let failure = tauri::async_runtime::block_on(wait_for_stereo_scan(
            &core,
            "closed",
            Ok(serde_json::json!({"ok":true,"scan_id":"closed"})),
            terminal,
            "/unused.ply",
            Duration::from_secs(1),
        ))
        .expect_err("closed terminal waiter is uncertain");
        let ScanFailure::Uncertain(_, None) = failure else {
            panic!("closed receiver must not be returned for a second poll");
        };
        tauri::async_runtime::block_on(await_uncertain_scan_cleanup(core, 9, None));
    }

    #[test]
    fn duplicate_matching_operation_is_rejected() {
        let lock = tokio::sync::Mutex::new(());
        let _first = acquire_matching_request(&lock).expect("first request acquires lock");
        assert!(acquire_matching_request(&lock).is_err());
    }

    #[test]
    fn home_uses_source_directly_while_main_resolves_fixed_then_source() {
        let fallback = Settings {
            matching_source_path: "/reference/source.ply".to_string(),
            matching_target_path: "/output/target.ply".to_string(),
            ..Settings::default()
        };
        assert_eq!(
            super::home_matching_source_path(&fallback).expect("home source"),
            "/reference/source.ply"
        );
        assert_eq!(
            crate::commands::preflight_scan::resolve_reference_ply(&fallback)
                .expect("main workflow fallback"),
            "/reference/source.ply"
        );
        let explicit = Settings {
            fixed_reference_ply: "/fixed/reference.ply".to_string(),
            ..fallback
        };
        assert_eq!(
            super::home_matching_source_path(&explicit).expect("home source remains authoritative"),
            "/reference/source.ply"
        );
        assert_eq!(
            crate::commands::preflight_scan::resolve_reference_ply(&explicit)
                .expect("main explicit fixed source"),
            "/fixed/reference.ply"
        );
        let only_target = Settings {
            matching_target_path: "/output/not-reference.ply".to_string(),
            ..Settings::default()
        };
        assert!(super::home_matching_source_path(&only_target).is_err());
        assert!(crate::commands::preflight_scan::resolve_reference_ply(&only_target).is_err());
    }

    #[test]
    fn uncertain_scan_cleanup_holds_until_current_generation_ends() {
        let core = crate::state::core_tools::CoreToolsState::default();
        core.generation.store(4, Ordering::Release);
        *core.status.lock().expect("core status") =
            crate::state::core_tools::CoreToolsStatus::Running;
        let (terminal_tx, terminal_rx) = tokio::sync::oneshot::channel();
        drop(terminal_tx);
        tauri::async_runtime::block_on(async move {
            let mut cleanup = tokio::spawn(super::await_uncertain_scan_cleanup(
                core.clone(),
                4,
                Some(terminal_rx),
            ));
            let premature = tokio::time::timeout(Duration::from_millis(1), &mut cleanup).await;
            assert!(premature.is_err());

            core.terminated_generation.store(4, Ordering::Release);
            core.terminated.notify_one();
            cleanup.await.expect("cleanup completes after termination");
        });
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
        assert!(super::validate_target_aliases(&source, &[&source, &source]).is_err());

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

    #[test]
    fn quality_thresholds_and_point_counts_are_assessed() {
        let result = super::MatchingResult {
            fitness: 0.9,
            inlier_rmse: 0.1,
            transformation: [[0.0; 4]; 4],
            correspondence_set: vec![[0, 0], [1, 1], [2, 2]],
            source_point_count: 3,
            target_point_count: 3,
            correspondence_count: 3,
            assessment: None,
        };
        assert_eq!(
            super::assess_result(&result, None, None).status,
            "needs_review"
        );
        assert_eq!(
            super::assess_result(&result, Some(0.8), Some(0.2)).status,
            "matched"
        );
        assert_eq!(
            super::assess_result(&result, Some(0.95), Some(0.2)).status,
            "not_matched"
        );
    }

    #[test]
    fn scan_output_is_published_only_after_valid_ply_exists() {
        let root = temp_root("atomic-publish");
        let target = root.join("target.ply");
        std::fs::write(&target, b"previous target").expect("write prior target");
        let reservation = super::reserve_scan_ply(&target).expect("reserve private output");
        let temp = reservation.cloud_path.clone();
        assert!(!temp.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&reservation.directory)
                    .expect("private directory metadata")
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
        }
        std::fs::write(&temp, b"invalid scan").expect("write invalid temp");
        assert!(super::publish_scan_output(
            &temp.to_string_lossy(),
            &target.to_string_lossy(),
            &[]
        )
        .is_err());
        assert_eq!(
            std::fs::read(&target).expect("read prior target"),
            b"previous target"
        );

        std::fs::write(
            &temp,
            b"ply\nformat ascii 1.0\nelement vertex 1\nend_header\n0 0 0\n",
        )
        .expect("write valid scan");
        super::publish_scan_output(&temp.to_string_lossy(), &target.to_string_lossy(), &[])
            .expect("publish valid scan");
        assert!(std::fs::read(&target)
            .expect("read published target")
            .starts_with(b"ply\n"));
        assert!(!temp.exists());
        drop(reservation);
        assert!(!temp.parent().expect("temp parent").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn ply_validation_rejects_symlinks_and_zero_vertices_without_reading_payload() {
        let root = temp_root("ply-validation");
        let regular = root.join("cloud.ply");
        let mut data =
            b"ply\nformat binary_little_endian 1.0\nelement vertex 1\nend_header\n".to_vec();
        data.extend(std::iter::repeat_n(0, 1024 * 1024));
        std::fs::write(&regular, data).expect("write large PLY");
        assert!(super::validate_ply_path("PLY", &regular.to_string_lossy()).is_ok());

        let symlink = root.join("linked.ply");
        std::os::unix::fs::symlink(&regular, &symlink).expect("create symlink");
        assert!(super::validate_ply_path("PLY", &symlink.to_string_lossy()).is_err());

        let empty = root.join("empty.ply");
        std::fs::write(
            &empty,
            b"ply\nformat ascii 1.0\nelement vertex 0\nend_header\n",
        )
        .expect("write zero-vertex PLY");
        assert!(super::validate_ply_path("PLY", &empty.to_string_lossy()).is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_scan_ack_failure_and_stale_completion_paths() {
        let event = serde_json::json!({
            "event": "stereo_scan_completed",
            "ply_file": "/old/target.ply"
        });
        assert!(super::validate_stereo_scan_completion(
            &serde_json::json!({"ok": false}),
            &event,
            "/new/temp.ply"
        )
        .is_err());
        assert!(super::validate_stereo_scan_completion(
            &serde_json::json!({"ok": true}),
            &event,
            "/new/temp.ply"
        )
        .is_err());
    }
}
