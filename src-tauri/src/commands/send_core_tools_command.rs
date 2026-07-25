use tauri::State;

use crate::errors::AppError;
use crate::state::core_tools::CoreToolsState;

/// core-tools への JSON Lines コマンドを stdin 経由で送信する。
///
/// 書き込み自体は start_core_tools が起動した Writer タスクが直列化するため、
/// このコマンドはチャネルへ送るのみで即座に戻る。
#[tauri::command]
pub async fn send_core_tools_command(
    state: State<'_, CoreToolsState>,
    json: String,
) -> Result<(), AppError> {
    let tx = state
        .cmd_tx
        .lock()
        .expect("core-tools cmd_tx mutex poisoned")
        .clone();
    match tx {
        Some(tx) => tx
            .send(json)
            .await
            .map_err(|e| AppError::CoreTools(format!("core-tools command channel closed: {e}"))),
        None => Err(AppError::CoreTools("core-tools is not running".to_string())),
    }
}
