use crate::errors::AppError;

#[tauri::command]
pub async fn recalibrate() -> Result<(), AppError> {
    println!("[recalibrate] called (stub)");
    Ok(())
}
