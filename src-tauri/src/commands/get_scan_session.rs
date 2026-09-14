use crate::{
    commands::scan_session_orchestration::{load, resolve_settings, SessionRequest},
    errors::AppError,
    state::scan_session::ScanSessionState,
};
use std::path::Path;
use tauri::{AppHandle, State};

#[tauri::command]
pub async fn get_scan_session(
    app: AppHandle,
    sessions: State<'_, ScanSessionState>,
    request: SessionRequest,
) -> Result<super::scan_session_types::ScanSessionResult, AppError> {
    let _guard = sessions.running.lock().await;
    let settings = resolve_settings(&app)?;
    let (session, _) = load(Path::new(&settings.data_root), &request.session_id)?;
    Ok(session.into())
}
