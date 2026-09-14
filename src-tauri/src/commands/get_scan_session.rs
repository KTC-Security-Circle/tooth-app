use crate::{
    commands::scan_session_orchestration::{load, resolve_settings, SessionRequest},
    errors::AppError,
};
use std::path::Path;
use tauri::AppHandle;

#[tauri::command]
pub async fn get_scan_session(
    app: AppHandle,
    request: SessionRequest,
) -> Result<super::scan_session_types::ScanSessionResult, AppError> {
    // save_atomic publishes with rename, so reading the final path is safe while
    // orchestration holds the session mutex and writes its temporary file.
    let settings = resolve_settings(&app)?;
    let (session, _) = load(Path::new(&settings.data_root), &request.session_id)?;
    Ok(session.into())
}
