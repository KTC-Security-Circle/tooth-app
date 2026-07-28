use std::time::Duration;

use tauri::State;

use crate::errors::AppError;
use crate::state::core_tools::{CoreToolsState, CoreToolsStatus};

/// core-tools を停止する。
///
/// 1. `shutdown` コマンドを送信 (graceful)
/// 2. `Terminated` イベントを待つ (5s タイムアウト)
/// 3. タイムアウト時は子プロセスを kill (確実終了)
/// 4. cmd_tx をクリア (Writer タスク停止)
/// 5. status を Idle に遷移
#[tauri::command]
pub async fn stop_core_tools(state: State<'_, CoreToolsState>) -> Result<(), AppError> {
    // 1. graceful shutdown を試みる (チャネルが生きていれば)
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

    // 2. Terminated イベントを待つ (5s タイムアウト)
    //    notify_one() により、reader タスクが先に通知していても取りこぼさない
    match tokio::time::timeout(Duration::from_secs(5), state.terminated.notified()).await {
        Ok(()) => log::info!("core-tools: graceful shutdown confirmed"),
        Err(_) => log::warn!("core-tools: shutdown timeout after 5s, killing"),
    }

    // 3. 子プロセスを確実に終了 (graceful 終了済みでも kill は冪等)
    if let Some(child) = state
        .child
        .lock()
        .expect("core-tools child mutex poisoned")
        .take()
    {
        let _ = child.kill();
    }

    // 4. Writer タスクをチャネルクローズで停止
    *state
        .cmd_tx
        .lock()
        .expect("core-tools cmd_tx mutex poisoned") = None;

    // 5. status を Idle に遷移
    *state
        .status
        .lock()
        .expect("core-tools status mutex poisoned") = CoreToolsStatus::Idle;

    Ok(())
}
