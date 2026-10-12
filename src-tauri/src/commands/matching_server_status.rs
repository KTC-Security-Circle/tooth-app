use tauri::State;

use crate::state::matching::{MatchingServerStatus, MatchingState};

#[tauri::command]
pub fn matching_server_status(state: State<'_, MatchingState>) -> MatchingServerStatus {
    state.status_snapshot()
}
