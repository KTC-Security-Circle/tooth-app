use std::path::Path;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};

use crate::commands::{
    move_turntable::move_turntable_slot_inner,
    preflight_scan::validate_scan_configuration,
    process_scan_slot::{process_scan_slot_inner, ProcessScanSlotRequest},
};
use crate::errors::AppError;
use crate::state::{
    core_tools::CoreToolsState,
    matching::MatchingState,
    scan_session::{
        ScanSessionManifest, ScanSessionSlot, ScanSessionState, SessionStatus, SESSION_SLOT_COUNT,
    },
    scan_slot::SlotStatus,
    settings::Settings,
    turntable::TurntableState,
};

#[derive(Debug, Deserialize)]
pub struct StartScanSessionRequest {
    pub session_id: String,
    pub input_dirs: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct SessionRequest {
    pub session_id: String,
}

#[derive(Debug, Deserialize)]
pub struct RetryScanSessionRequest {
    pub session_id: String,
    pub slot: u8,
}

#[derive(Debug, Serialize)]
pub struct ScanSessionResult {
    pub session: ScanSessionManifest,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id != "." && id != ".." && !id.contains('/') && !id.contains('\\')
}

fn load(root: &Path, id: &str) -> Result<(ScanSessionManifest, std::path::PathBuf), AppError> {
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
            .any(|(index, slot)| slot.slot != index as u8)
    {
        return Err(AppError::Internal(
            "scan session has an invalid slot manifest".into(),
        ));
    }
    Ok((session, path))
}

#[allow(clippy::too_many_arguments)]
async fn run(
    app: &AppHandle,
    lock: &ScanSessionState,
    turntable: &TurntableState,
    core_tools: &CoreToolsState,
    matching: &MatchingState,
    mut session: ScanSessionManifest,
    path: &Path,
    settings: &Settings,
) -> Result<ScanSessionResult, AppError> {
    let _guard = lock.running.lock().await;
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
                return Ok(ScanSessionResult { session });
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
    Ok(ScanSessionResult { session })
}

#[tauri::command]
pub async fn start_scan_session(
    app: AppHandle,
    sessions: State<'_, ScanSessionState>,
    turntable: State<'_, TurntableState>,
    core_tools: State<'_, CoreToolsState>,
    matching: State<'_, MatchingState>,
    request: StartScanSessionRequest,
) -> Result<ScanSessionResult, AppError> {
    let settings = Settings::load(&app)?;
    let preflight = validate_scan_configuration(&settings)?;
    if !preflight.ready {
        return Err(AppError::Validation("scan preflight failed".into()));
    }
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
    let path = ScanSessionManifest::path(Path::new(&settings.data_root), &request.session_id);
    if ScanSessionManifest::load(&path)
        .map_err(AppError::Internal)?
        .is_some()
    {
        return Err(AppError::Validation("scan session already exists".into()));
    }
    let session = ScanSessionManifest {
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
    };
    session.save_atomic(&path).map_err(AppError::Internal)?;
    run(
        &app,
        sessions.inner(),
        turntable.inner(),
        core_tools.inner(),
        matching.inner(),
        session,
        &path,
        &settings,
    )
    .await
}

#[tauri::command]
pub async fn resume_scan_session(
    app: AppHandle,
    sessions: State<'_, ScanSessionState>,
    turntable: State<'_, TurntableState>,
    core_tools: State<'_, CoreToolsState>,
    matching: State<'_, MatchingState>,
    request: SessionRequest,
) -> Result<ScanSessionResult, AppError> {
    let settings = Settings::load(&app)?;
    let (session, path) = load(Path::new(&settings.data_root), &request.session_id)?;
    run(
        &app,
        sessions.inner(),
        turntable.inner(),
        core_tools.inner(),
        matching.inner(),
        session,
        &path,
        &settings,
    )
    .await
}

#[tauri::command]
pub async fn retry_scan_session(
    app: AppHandle,
    sessions: State<'_, ScanSessionState>,
    turntable: State<'_, TurntableState>,
    core_tools: State<'_, CoreToolsState>,
    matching: State<'_, MatchingState>,
    request: RetryScanSessionRequest,
) -> Result<ScanSessionResult, AppError> {
    let settings = Settings::load(&app)?;
    let (mut session, path) = load(Path::new(&settings.data_root), &request.session_id)?;
    if request.slot as usize >= SESSION_SLOT_COUNT {
        return Err(AppError::Validation("slot must be between 0 and 11".into()));
    }
    let slot_path = crate::state::scan_slot::ScanSlotManifest::path(
        Path::new(&settings.data_root),
        &format!("{}-slot-{}", request.session_id, request.slot),
    );
    let _ = std::fs::remove_file(slot_path);
    session.slots[request.slot as usize].status = None;
    session.slots[request.slot as usize].reason = None;
    session.save_atomic(&path).map_err(AppError::Internal)?;
    run(
        &app,
        sessions.inner(),
        turntable.inner(),
        core_tools.inner(),
        matching.inner(),
        session,
        &path,
        &settings,
    )
    .await
}

#[tauri::command]
pub async fn get_scan_session(
    app: AppHandle,
    request: SessionRequest,
) -> Result<ScanSessionResult, AppError> {
    let settings = Settings::load(&app)?;
    let (session, _) = load(Path::new(&settings.data_root), &request.session_id)?;
    Ok(ScanSessionResult { session })
}
