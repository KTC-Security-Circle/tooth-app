use tauri::State;

use crate::errors::AppError;
use crate::state::core_tools::{CoreToolsState, CoreToolsStatus};

/// core-tools を停止する。
///
/// 1. `shutdown` コマンドを送信 (graceful)
/// 2. 子プロセスを kill (確実終了)
/// 3. cmd_tx をクリア (Writer タスク停止)
/// 4. status を Idle に遷移
#[tauri::command]
pub async fn stop_core_tools(state: State<'_, CoreToolsState>) -> Result<(), AppError> {
    // graceful shutdown を試みる (チャネルが生きていれば)
    let tx = state
        .cmd_tx
        .lock()
        .expect("core-tools cmd_tx mutex poisoned")
        .clone();
    if let Some(tx) = tx {
        let _ = tx
            .send("{\"id\":\"shutdown\",\"cmd\":\"shutdown\"}".to_string())
            .await;
    }

    // 子プロセスを確実に終了
    if let Some(child) = state
        .child
        .lock()
        .expect("core-tools child mutex poisoned")
        .take()
    {
        let _ = child.kill();
    }

    // Writer タスクをチャネルクローズで停止
    *state
        .cmd_tx
        .lock()
        .expect("core-tools cmd_tx mutex poisoned") = None;

    *state
        .status
        .lock()
        .expect("core-tools status mutex poisoned") = CoreToolsStatus::Idle;

    Ok(())
}
