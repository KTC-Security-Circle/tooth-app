use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::commands::run_matching::{run_matching_paths, MatchingResult};
use crate::errors::AppError;
use crate::state::core_tools::CoreToolsState;
use crate::state::scan_slot::{ScanSlotManifest, SlotStage, SlotStatus};
use crate::state::settings::Settings;

const CORE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessScanSlotRequest {
    pub input_dir: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProcessScanSlotResult {
    pub manifest: ScanSlotManifest,
    pub matching: Option<MatchingResult>,
}

#[tauri::command]
pub async fn process_scan_slot(
    app: AppHandle,
    core_tools: tauri::State<'_, CoreToolsState>,
    matching: tauri::State<'_, crate::state::matching::MatchingState>,
    request: ProcessScanSlotRequest,
) -> Result<ProcessScanSlotResult, AppError> {
    let settings = Settings::load(&app)?;
    process_scan_slot_inner(
        &app,
        core_tools.inner(),
        matching.inner(),
        &settings,
        request,
    )
    .await
}

async fn process_scan_slot_inner(
    app: &AppHandle,
    core_tools: &CoreToolsState,
    matching: &crate::state::matching::MatchingState,
    settings: &Settings,
    request: ProcessScanSlotRequest,
) -> Result<ProcessScanSlotResult, AppError> {
    let input = Path::new(&request.input_dir);
    if !input.is_dir() {
        return Err(AppError::Validation(
            "input_dir must be a directory".to_string(),
        ));
    }
    if settings.data_root.trim().is_empty() {
        return Err(AppError::Validation("data_root is required".to_string()));
    }
    let session_id = request.session_id.unwrap_or_else(|| {
        input
            .file_name()
            .and_then(|v| v.to_str())
            .unwrap_or("scan")
            .to_string()
    });
    if session_id.is_empty()
        || session_id.contains('/')
        || session_id.contains('\\')
        || session_id == "."
        || session_id == ".."
    {
        return Err(AppError::Validation("session_id is invalid".to_string()));
    }
    let root = Path::new(&settings.data_root);
    let manifest_path = ScanSlotManifest::path(root, &session_id);
    let mut manifest = match ScanSlotManifest::load(&manifest_path).map_err(AppError::Internal)? {
        Some(existing) => existing,
        None => new_manifest(root, &session_id, &request.input_dir, settings)
            .map_err(AppError::Internal)?,
    };
    if manifest.input_dir != request.input_dir {
        return Err(AppError::Validation(
            "input_dir does not match the immutable scan session manifest".to_string(),
        ));
    }
    if manifest.status == SlotStatus::Complete {
        return Ok(ProcessScanSlotResult {
            matching: manifest
                .matching
                .clone()
                .and_then(|v| serde_json::from_value(v).ok()),
            manifest,
        });
    }
    if manifest.status == SlotStatus::NeedsRescan {
        start_retry(&mut manifest).map_err(AppError::Internal)?;
    }
    manifest.status = SlotStatus::Processing;
    manifest
        .save_atomic(&manifest_path)
        .map_err(AppError::Internal)?;
    let attempt = PathBuf::from(&manifest.attempt_dir);
    let config = &manifest.config;

    if manifest.stage == SlotStage::Recorded {
        let response = core_tools.request(serde_json::json!({"cmd":"scan_validate", "input_dir":manifest.input_dir, "allow_partial":false}), CORE_TIMEOUT).await?;
        if !response
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            || response.get("valid").and_then(serde_json::Value::as_str) != Some("true")
        {
            return needs_rescan(
                manifest,
                &manifest_path,
                "scan dataset is incomplete or invalid",
            );
        }
        manifest.stage = SlotStage::ScanValidated;
        manifest
            .save_atomic(&manifest_path)
            .map_err(AppError::Internal)?;
    }
    let decode = attempt.join("decode");
    if manifest.stage == SlotStage::ScanValidated {
        let response = core_tools.request(serde_json::json!({"cmd":"decode_patterns", "input_dir":manifest.input_dir, "output_dir":decode, "threshold":config.decode_threshold, "allow_partial":false}), CORE_TIMEOUT).await?;
        if !response
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            || !decode_artifact_valid(&decode)
        {
            return needs_rescan(manifest, &manifest_path, "decoded artifact is incomplete");
        }
        manifest.stage = SlotStage::Decoded;
        manifest
            .save_atomic(&manifest_path)
            .map_err(AppError::Internal)?;
    }
    if manifest.stage == SlotStage::Decoded {
        let response = core_tools.request(serde_json::json!({"cmd":"reconstruct_validate", "decode_dir":decode, "calibration_file":config.stereo_calibration_file}), CORE_TIMEOUT).await?;
        if !response
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            return needs_rescan(
                manifest,
                &manifest_path,
                "reconstruction inputs are invalid",
            );
        }
        manifest.stage = SlotStage::ReconstructionValidated;
        manifest
            .save_atomic(&manifest_path)
            .map_err(AppError::Internal)?;
    }
    let cloud = attempt.join("reconstruction.ply");
    if manifest.stage == SlotStage::ReconstructionValidated {
        let response = core_tools.request(serde_json::json!({"cmd":"reconstruct_point_cloud", "decode_dir":decode, "calibration_file":config.stereo_calibration_file, "output_file":cloud}), CORE_TIMEOUT).await?;
        if !response
            .get("ok")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            || !valid_ply(&cloud)
        {
            return needs_rescan(
                manifest,
                &manifest_path,
                "point cloud artifact is incomplete",
            );
        }
        manifest.stage = SlotStage::Reconstructed;
        manifest
            .save_atomic(&manifest_path)
            .map_err(AppError::Internal)?;
    }
    if manifest.stage == SlotStage::Reconstructed {
        let result = run_matching_paths(
            app,
            matching,
            &cloud,
            Path::new(&config.fixed_reference_ply),
            config,
        )
        .await?;
        manifest.matching =
            Some(serde_json::to_value(&result).map_err(|e| AppError::Internal(e.to_string()))?);
        manifest.stage = SlotStage::Matched;
        manifest.status = if result
            .assessment
            .as_ref()
            .is_some_and(|a| a.status == "needs_rescan")
        {
            SlotStatus::NeedsRescan
        } else {
            SlotStatus::Complete
        };
        manifest
            .save_atomic(&manifest_path)
            .map_err(AppError::Internal)?;
        return Ok(ProcessScanSlotResult {
            manifest,
            matching: Some(result),
        });
    }
    Ok(ProcessScanSlotResult {
        matching: manifest
            .matching
            .clone()
            .and_then(|v| serde_json::from_value(v).ok()),
        manifest,
    })
}

fn new_manifest(
    root: &Path,
    session_id: &str,
    input: &str,
    settings: &Settings,
) -> Result<ScanSlotManifest, String> {
    let dir = root.join("sessions").join(session_id);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut attempt = 1;
    while dir.join(format!("attempt-{attempt:04}")).exists() {
        attempt += 1;
    }
    let attempt_dir = dir.join(format!("attempt-{attempt:04}"));
    std::fs::create_dir_all(&attempt_dir).map_err(|e| e.to_string())?;
    Ok(ScanSlotManifest {
        version: 1,
        session_id: session_id.to_string(),
        status: SlotStatus::Processing,
        stage: SlotStage::Recorded,
        attempt,
        input_dir: input.to_string(),
        attempt_dir: attempt_dir.to_string_lossy().into_owned(),
        config: settings.clone(),
        matching: None,
        reason: None,
    })
}

fn needs_rescan(
    mut manifest: ScanSlotManifest,
    path: &Path,
    reason: &str,
) -> Result<ProcessScanSlotResult, AppError> {
    manifest.status = SlotStatus::NeedsRescan;
    manifest.reason = Some(reason.to_string());
    manifest.save_atomic(path).map_err(AppError::Internal)?;
    Ok(ProcessScanSlotResult {
        matching: None,
        manifest,
    })
}

fn start_retry(manifest: &mut ScanSlotManifest) -> Result<(), String> {
    let attempt_dir = Path::new(&manifest.attempt_dir);
    let session_dir = attempt_dir
        .parent()
        .ok_or_else(|| "scan slot attempt has no session directory".to_string())?;
    let mut attempt = manifest.attempt + 1;
    while session_dir.join(format!("attempt-{attempt:04}")).exists() {
        attempt += 1;
    }
    let attempt_dir = session_dir.join(format!("attempt-{attempt:04}"));
    std::fs::create_dir_all(&attempt_dir)
        .map_err(|error| format!("failed to create retry attempt directory: {error}"))?;
    manifest.attempt = attempt;
    manifest.attempt_dir = attempt_dir.to_string_lossy().into_owned();
    manifest.stage = SlotStage::Recorded;
    manifest.status = SlotStatus::Processing;
    manifest.matching = None;
    manifest.reason = None;
    Ok(())
}

fn decode_artifact_valid(path: &Path) -> bool {
    path.join("metadata.json").is_file()
        && [
            "left/projector_x.yml",
            "left/projector_y.yml",
            "left/valid_mask.png",
            "right/projector_x.yml",
            "right/projector_y.yml",
            "right/valid_mask.png",
        ]
        .iter()
        .all(|p| path.join(p).is_file())
}

fn valid_ply(path: &Path) -> bool {
    const MAX_HEADER_BYTES: u64 = 64 * 1024;
    let Ok(file) = std::fs::File::open(path) else {
        return false;
    };
    let mut reader = std::io::BufReader::new(file).take(MAX_HEADER_BYTES);
    let mut first_line = true;
    let mut has_vertex = false;
    loop {
        let mut line = Vec::new();
        let Ok(read) = reader.read_until(b'\n', &mut line) else {
            return false;
        };
        if read == 0 {
            return false;
        }
        let line = line.strip_suffix(b"\r").unwrap_or(&line);
        let line = line.strip_suffix(b"\n").unwrap_or(line);
        if first_line {
            if line != b"ply" {
                return false;
            }
            first_line = false;
            continue;
        }
        if line == b"end_header" {
            return has_vertex;
        }
        if line.starts_with(b"element vertex ") {
            has_vertex = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_starts_new_attempt_without_changing_input_or_config() {
        let root = std::env::temp_dir().join(format!("tooth-scan-retry-{}", std::process::id()));
        let session = root.join("session");
        let attempt = session.join("attempt-0001");
        std::fs::create_dir_all(&attempt).expect("attempt should be created");
        let config = Settings::default();
        let mut manifest = ScanSlotManifest {
            version: 1,
            session_id: "session".to_string(),
            status: SlotStatus::NeedsRescan,
            stage: SlotStage::Matched,
            attempt: 1,
            input_dir: "/scan/input".to_string(),
            attempt_dir: attempt.to_string_lossy().into_owned(),
            config: config.clone(),
            matching: Some(serde_json::json!({"status": "needs_rescan"})),
            reason: Some("retry".to_string()),
        };

        start_retry(&mut manifest).expect("retry should start");

        assert_eq!(manifest.attempt, 2);
        assert_eq!(manifest.stage, SlotStage::Recorded);
        assert_eq!(manifest.status, SlotStatus::Processing);
        assert_eq!(manifest.input_dir, "/scan/input");
        assert_eq!(
            serde_json::to_value(&manifest.config).expect("config should serialize"),
            serde_json::to_value(&config).expect("config should serialize")
        );
        assert!(manifest.matching.is_none());
        assert!(manifest.reason.is_none());
        assert!(Path::new(&manifest.attempt_dir).is_dir());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn valid_ply_does_not_read_payload() {
        let path =
            std::env::temp_dir().join(format!("tooth-ply-header-{}.ply", std::process::id()));
        let mut contents =
            b"ply\nformat binary_little_endian 1.0\nelement vertex 1\nend_header\n".to_vec();
        contents.extend(std::iter::repeat(0).take(1024 * 1024));
        std::fs::write(&path, contents).expect("PLY should be written");
        assert!(valid_ply(&path));
        let _ = std::fs::remove_file(path);
    }
}
