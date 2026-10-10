use crate::{
    commands::scan_session_orchestration::{
        new_session, resolve_settings, run, StartScanSessionRequest,
    },
    errors::AppError,
    state::{
        core_tools::CoreToolsState,
        matching::MatchingState,
        scan_session::{ScanSessionManifest, ScanSessionState},
        turntable::TurntableState,
    },
};
use std::path::Path;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn start_scan_session(
    app: AppHandle,
    sessions: State<'_, ScanSessionState>,
    turntable: State<'_, TurntableState>,
    core_tools: State<'_, CoreToolsState>,
    matching: State<'_, MatchingState>,
    request: StartScanSessionRequest,
) -> Result<super::scan_session_types::ScanSessionResult, AppError> {
    let _guard = sessions.running.lock().await;
    let settings = resolve_settings(&app)?;
    let session = new_session(request)?;
    let path = ScanSessionManifest::path(Path::new(&settings.data_root), &session.session_id);
    if ScanSessionManifest::load(&path)
        .map_err(AppError::Internal)?
        .is_some()
    {
        return Err(AppError::Validation("scan session already exists".into()));
    }
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
