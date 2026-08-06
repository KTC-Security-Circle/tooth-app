use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_plugin_shell::process::{CommandChild, CommandEvent};
use tauri_plugin_shell::ShellExt;

use crate::errors::AppError;
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

struct ChildGuard {
    child: Option<CommandChild>,
}

impl ChildGuard {
    fn new(child: CommandChild) -> Self {
        Self { child: Some(child) }
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            if let Err(error) = child.kill() {
                log::debug!("matching child cleanup failed: {error}");
            }
        }
    }
}

#[tauri::command]
pub async fn run_matching(app: AppHandle) -> Result<MatchingResult, AppError> {
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
            ));
        }
    };
    let voxel_size = settings.matching_voxel_size;
    let ransac_iterations = settings.matching_ransac_iterations;
    if !voxel_size.is_finite() || voxel_size <= 0.0 {
        return Err(AppError::Validation(
            "matching_voxel_size must be finite and greater than zero".to_string(),
        ));
    }
    if ransac_iterations < 1 {
        return Err(AppError::Validation(
            "matching_ransac_iterations must be at least 1".to_string(),
        ));
    }

    let args = [
        mode.as_str().to_string(),
        source_path.clone(),
        target_path.clone(),
        "--voxel-size".to_string(),
        voxel_size.to_string(),
        "--ransac-iterations".to_string(),
        ransac_iterations.to_string(),
        "--json".to_string(),
    ];
    let (mut events, child) = app
        .shell()
        .sidecar("3mcli")
        .map_err(|error| {
            log::error!("failed to resolve 3mcli for matching: {error}");
            AppError::Matching(format!("failed to resolve 3mcli: {error}"))
        })?
        .args(args)
        .spawn()
        .map_err(|error| {
            log::error!("failed to spawn 3mcli for matching: {error}");
            AppError::Matching(format!("failed to spawn 3mcli: {error}"))
        })?;
    let _child_guard = ChildGuard::new(child);

    let wait_for_output = async {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => stdout.extend(bytes),
                CommandEvent::Stderr(bytes) => stderr.extend(bytes),
                CommandEvent::Error(message) => {
                    return Err((stdout, stderr, format!("3mcli command error: {message}")));
                }
                CommandEvent::Terminated(payload) => {
                    let status_ok = payload.code == Some(0) && payload.signal.is_none();
                    return if status_ok {
                        Ok((stdout, stderr))
                    } else {
                        Err((
                            stdout,
                            stderr,
                            format!(
                                "3mcli terminated unsuccessfully (code={:?}, signal={:?})",
                                payload.code, payload.signal
                            ),
                        ))
                    };
                }
                _ => {}
            }
        }
        Err((
            stdout,
            stderr,
            "3mcli event stream closed before termination".to_string(),
        ))
    };

    let (stdout, _) = match tokio::time::timeout(MATCHING_TIMEOUT, wait_for_output).await {
        Ok(Ok(output)) => output,
        Ok(Err((_stdout, stderr, message))) => {
            log::error!(
                "matching failed: {message}; stderr: {}",
                String::from_utf8_lossy(&stderr)
            );
            return Err(AppError::Matching(message));
        }
        Err(_) => {
            log::error!("matching timed out after {MATCHING_TIMEOUT:?}");
            return Err(AppError::Matching("3mcli timed out".to_string()));
        }
    };

    let result = serde_json::from_slice::<MatchingResult>(&stdout).map_err(|error| {
        log::error!("matching JSON/schema failure: {error}");
        AppError::Matching(format!("invalid matching JSON: {error}"))
    })?;
    validate_result(&result).map_err(|error| {
        log::error!("matching result schema validation failed: {error}");
        error
    })?;
    Ok(result)
}

fn validate_ply_path(name: &str, value: &str) -> Result<String, AppError> {
    if value.trim().is_empty() {
        return Err(AppError::Validation(format!("{name} must not be empty")));
    }
    let path = std::path::Path::new(value);
    let is_ply = path
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("ply"));
    if !is_ply || !path.is_file() {
        return Err(AppError::Validation(format!(
            "{name} must be an existing regular PLY file"
        )));
    }
    Ok(value.to_string())
}

fn validate_result(result: &MatchingResult) -> Result<(), AppError> {
    if !result.fitness.is_finite() || !result.inlier_rmse.is_finite() {
        return Err(AppError::Matching(
            "matching result contains non-finite metrics".to_string(),
        ));
    }
    if result
        .transformation
        .iter()
        .flatten()
        .any(|value| !value.is_finite())
    {
        return Err(AppError::Matching(
            "matching result contains non-finite transformation values".to_string(),
        ));
    }
    Ok(())
}
