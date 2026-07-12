mod commands;
mod state;
mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::greet::greet,
            commands::camera::list_cameras,
            commands::load_settings::load_settings,
            commands::save_settings::save_settings,
            commands::recalibrate::recalibrate,
            commands::default_calibration_path::default_calibration_path,
            commands::settings_exists::settings_exists,
            commands::validate_settings::validate_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
