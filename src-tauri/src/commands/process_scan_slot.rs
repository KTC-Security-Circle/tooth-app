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
    manifest.status = SlotStatus::Processing;
    manifest
        .save_atomic(&manifest_path)
        .map_err(AppError::Internal)?;
    let attempt = PathBuf::from(&manifest.attempt_dir);

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
        let response = core_tools.request(serde_json::json!({"cmd":"decode_patterns", "input_dir":manifest.input_dir, "output_dir":decode, "threshold":settings.decode_threshold, "allow_partial":false}), CORE_TIMEOUT).await?;
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
        let response = core_tools.request(serde_json::json!({"cmd":"reconstruct_validate", "decode_dir":decode, "calibration_file":settings.stereo_calibration_file}), CORE_TIMEOUT).await?;
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
        let response = core_tools.request(serde_json::json!({"cmd":"reconstruct_point_cloud", "decode_dir":decode, "calibration_file":settings.stereo_calibration_file, "output_file":cloud}), CORE_TIMEOUT).await?;
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
            Path::new(&settings.fixed_reference_ply),
            settings,
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
    path.is_file()
        && std::fs::read(path).ok().is_some_and(|b| {
            let h = String::from_utf8_lossy(&b[..b.len().min(65536)]);
            h.lines().next() == Some("ply")
                && h.lines().any(|l| l.trim() == "end_header")
                && h.lines().any(|l| l.starts_with("element vertex "))
        })
}
