#[tauri::command]
pub async fn recalibrate() -> Result<(), String> {
    println!("[recalibrate] called (stub)");
    Ok(())
}
