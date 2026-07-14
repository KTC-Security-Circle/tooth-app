use crate::errors::AppError;

#[tauri::command]
pub async fn recalibrate() -> Result<(), AppError> {
    log::info!("[recalibrate] called (stub)");
    Ok(())
}
