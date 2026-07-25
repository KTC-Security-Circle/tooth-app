#[cfg(not(target_os = "linux"))]
compile_error!(
    "This application is Linux-only. Build on Linux or use a Linux cross-compilation toolchain."
);

use tauri::Manager;
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

mod commands;
mod errors;
mod state;
mod utils;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
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
        .plugin(tauri_plugin_shell::init())
        .manage(state::core_tools::CoreToolsState::default())
        .invoke_handler(tauri::generate_handler![
            commands::greet::greet,
            commands::camera::list_cameras,
            commands::load_settings::load_settings,
            commands::recalibrate::recalibrate,
            commands::default_calibration_path::default_calibration_path,
            commands::settings_exists::settings_exists,
            commands::update_settings::update_settings,
            commands::start_core_tools::start_core_tools,
            commands::send_core_tools_command::send_core_tools_command,
            commands::stop_core_tools::stop_core_tools,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    // アプリ終了時に core-tools (tooth-backend) を確実に kill する (ゾンビプロセス対策)
    app.run(|app_handle, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(core_tools) = app_handle.try_state::<state::core_tools::CoreToolsState>() {
                if let Some(child) = core_tools
                    .child
                    .lock()
                    .expect("core-tools child mutex poisoned")
                    .take()
                {
                    let _ = child.kill();
                    log::info!("core-tools: killed tooth-backend on app exit");
                }
            }
        }
    });
}
