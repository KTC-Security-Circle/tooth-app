use tauri::State;

use crate::errors::AppError;
use crate::state::core_tools::{CoreToolsState, CoreToolsStatus};

/// core-tools (tooth-backend) の現在の起動状態を返す。
#[tauri::command]
pub async fn core_tools_status(
    state: State<'_, CoreToolsState>,
) -> Result<CoreToolsStatus, AppError> {
    let status = *state
        .status
        .lock()
        .expect("core-tools status mutex poisoned");
    Ok(status)
}
