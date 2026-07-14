#[cfg(not(target_os = "linux"))]
compile_error!(
    "This application is Linux-only. Build on Linux or use a Linux cross-compilation toolchain."
);

use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

mod commands;
mod errors;
mod state;
mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .level(if cfg!(debug_assertions) {
                    log::LevelFilter::Debug
                } else {
                    log::LevelFilter::Info
                })
                .targets({
                    let mut targets = vec![Target::new(TargetKind::Stdout)];
                    if cfg!(debug_assertions) {
                        // 開発時: Stdout + Webview (ファイルには書かない)
                        targets.push(Target::new(TargetKind::Webview));
                    } else {
                        // 本番: Stdout + LogDir (ファイルに書く)
                        targets.push(Target::new(TargetKind::LogDir { file_name: None }));
                    }
                    targets
                })
                .max_file_size(10_000_000)
                .rotation_strategy(RotationStrategy::KeepOne)
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            commands::greet::greet,
            commands::camera::list_cameras,
            commands::load_settings::load_settings,
            commands::recalibrate::recalibrate,
            commands::default_calibration_path::default_calibration_path,
            commands::settings_exists::settings_exists,
            commands::update_settings::update_settings,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
