use tauri::State;

use crate::errors::AppError;
use crate::state::core_tools::CoreToolsState;

/// core-tools を停止する。
#[tauri::command]
pub async fn stop_core_tools(state: State<'_, CoreToolsState>) -> Result<(), AppError> {
    // Invalidate synchronously: cleanup waiters are woken, all waiter maps
    // are drained, and the child is explicitly killed before restart is open.
    state.stop_generation();
    Ok(())
}
