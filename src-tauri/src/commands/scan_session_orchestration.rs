use std::path::{Path, PathBuf};

use tauri::AppHandle;

use crate::commands::{
    move_turntable::move_turntable_slot_inner,
    preflight_scan::validate_scan_configuration,
    process_scan_slot::{process_scan_slot_inner, ProcessScanSlotRequest},
};
use crate::errors::AppError;
use crate::state::{
    core_tools::CoreToolsState,
    matching::MatchingState,
    scan_session::{ScanSessionManifest, ScanSessionSlot, SessionStatus, SESSION_SLOT_COUNT},
    scan_slot::SlotStatus,
    settings::Settings,
    turntable::TurntableState,
};

pub use super::scan_session_types::{
    RetryScanSessionRequest, ScanSessionResult, SessionRequest, StartScanSessionRequest,
};

pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id != "." && id != ".." && !id.contains('/') && !id.contains('\\')
}

pub fn load(root: &Path, id: &str) -> Result<(ScanSessionManifest, PathBuf), AppError> {
    if !valid_id(id) {
        return Err(AppError::Validation("session_id is invalid".into()));
    }
    let path = ScanSessionManifest::path(root, id);
    let session = ScanSessionManifest::load(&path)
        .map_err(AppError::Internal)?
        .ok_or_else(|| AppError::Validation("scan session does not exist".into()))?;
    if session.slots.len() != SESSION_SLOT_COUNT
        || session
            .slots
            .iter()
            .enumerate()
            .any(|(i, slot)| slot.slot != i as u8)
    {
        return Err(AppError::Internal(
            "scan session has an invalid slot manifest".into(),
        ));
    }
    Ok((session, path))
}

pub fn resolve_settings(app: &AppHandle) -> Result<Settings, AppError> {
    let mut settings = Settings::load(app)?;
    settings.resolve_calibration_profile(app)?;
    let preflight = validate_scan_configuration(&settings)?;
    if !preflight.ready {
        return Err(AppError::Validation("scan preflight failed".into()));
    }
    Ok(settings)
}

pub async fn run(
    app: &AppHandle,
    turntable: &TurntableState,
    core_tools: &CoreToolsState,
    matching: &MatchingState,
    mut session: ScanSessionManifest,
    path: &Path,
    settings: &Settings,
) -> Result<ScanSessionResult, AppError> {
    session.status = SessionStatus::Running;
    session.reason = None;
    session.save_atomic(path).map_err(AppError::Internal)?;
    for index in 0..SESSION_SLOT_COUNT {
        if session.slots[index].status == Some(SlotStatus::Complete) {
            continue;
        }
        let slot = index as u8;
        session.current_slot = Some(slot);
        session.save_atomic(path).map_err(AppError::Internal)?;
        let result = async {
            move_turntable_slot_inner(app, turntable, settings, slot).await?;
            process_scan_slot_inner(
                app,
                core_tools,
                matching,
                settings,
                ProcessScanSlotRequest {
                    input_dir: session.slots[index].input_dir.clone(),
                    session_id: Some(format!("{}-slot-{slot}", session.session_id)),
                },
            )
            .await
        }
        .await;
        match result {
            Ok(result) if result.manifest.status == SlotStatus::Complete => {
                session.slots[index].status = Some(SlotStatus::Complete);
                session.slots[index].reason = None;
            }
            Ok(result) => {
                let reason = result
                    .manifest
                    .reason
                    .unwrap_or_else(|| "slot requires rescan".into());
                session.slots[index].status = Some(SlotStatus::NeedsRescan);
                session.slots[index].reason = Some(reason.clone());
                session.status = SessionStatus::Failed;
                session.reason = Some(reason);
                session.save_atomic(path).map_err(AppError::Internal)?;
                return Ok(ScanSessionResult::from(session));
            }
            Err(error) => {
                let reason = error.to_string();
                session.slots[index].status = Some(SlotStatus::Processing);
                session.slots[index].reason = Some(reason.clone());
                session.status = SessionStatus::Failed;
                session.reason = Some(reason);
                session.save_atomic(path).map_err(AppError::Internal)?;
                return Err(error);
            }
        }
        session.save_atomic(path).map_err(AppError::Internal)?;
    }
    session.status = SessionStatus::Complete;
    session.current_slot = None;
    session.save_atomic(path).map_err(AppError::Internal)?;
    Ok(ScanSessionResult::from(session))
}

pub fn new_session(request: StartScanSessionRequest) -> Result<ScanSessionManifest, AppError> {
    if request.input_dirs.len() != SESSION_SLOT_COUNT {
        return Err(AppError::Validation(
            "exactly 12 input directories are required".into(),
        ));
    }
    if request
        .input_dirs
        .iter()
        .any(|dir| !Path::new(dir).is_dir())
    {
        return Err(AppError::Validation(
            "every input directory must exist".into(),
        ));
    }
    if !valid_id(&request.session_id) {
        return Err(AppError::Validation("session_id is invalid".into()));
    }
    Ok(ScanSessionManifest {
        version: 1,
        session_id: request.session_id,
        status: SessionStatus::Running,
        current_slot: None,
        slots: request
            .input_dirs
            .into_iter()
            .enumerate()
            .map(|(slot, input_dir)| ScanSessionSlot {
                slot: slot as u8,
                input_dir,
                status: None,
                reason: None,
            })
            .collect(),
        reason: None,
    })
}

pub fn delete_slot_manifest(root: &Path, session_id: &str, slot: u8) -> Result<(), AppError> {
    let path =
        crate::state::scan_slot::ScanSlotManifest::path(root, &format!("{session_id}-slot-{slot}"));
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::Internal(format!(
            "failed to delete slot manifest '{}': {error}",
            path.display()
        ))),
    }
}
