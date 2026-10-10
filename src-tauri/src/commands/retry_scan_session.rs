use crate::{
    commands::scan_session_orchestration::{
        delete_slot_manifest, load, resolve_settings, run, RetryScanSessionRequest,
    },
    errors::AppError,
    state::{
        core_tools::CoreToolsState,
        matching::MatchingState,
        scan_session::{ScanSessionState, SESSION_SLOT_COUNT},
        turntable::TurntableState,
    },
};
use std::path::Path;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn retry_scan_session(
    app: AppHandle,
    sessions: State<'_, ScanSessionState>,
    turntable: State<'_, TurntableState>,
    core_tools: State<'_, CoreToolsState>,
    matching: State<'_, MatchingState>,
    request: RetryScanSessionRequest,
) -> Result<super::scan_session_types::ScanSessionResult, AppError> {
    let _guard = sessions.running.lock().await;
    let settings = resolve_settings(&app)?;
    let (mut session, path) = load(Path::new(&settings.data_root), &request.session_id)?;
    if request.slot as usize >= SESSION_SLOT_COUNT {
        return Err(AppError::Validation("slot must be between 0 and 11".into()));
    }
    delete_slot_manifest(
        Path::new(&settings.data_root),
        &request.session_id,
        request.slot,
    )?;
    session.slots[request.slot as usize].status = None;
    session.slots[request.slot as usize].reason = None;
    session.save_atomic(&path).map_err(AppError::Internal)?;
    run(
        &app,
        &turntable,
        &core_tools,
        &matching,
        session,
        &path,
        &settings,
    )
    .await
}
