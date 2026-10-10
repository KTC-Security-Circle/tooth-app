use crate::{
    commands::scan_session_orchestration::{load, resolve_settings, run, SessionRequest},
    errors::AppError,
    state::{
        core_tools::CoreToolsState, matching::MatchingState, scan_session::ScanSessionState,
        turntable::TurntableState,
    },
};
use std::path::Path;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn resume_scan_session(
    app: AppHandle,
    sessions: State<'_, ScanSessionState>,
    turntable: State<'_, TurntableState>,
    core_tools: State<'_, CoreToolsState>,
    matching: State<'_, MatchingState>,
    request: SessionRequest,
) -> Result<super::scan_session_types::ScanSessionResult, AppError> {
    let _guard = sessions.running.lock().await;
    let settings = resolve_settings(&app)?;
    let (session, path) = load(Path::new(&settings.data_root), &request.session_id)?;
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
