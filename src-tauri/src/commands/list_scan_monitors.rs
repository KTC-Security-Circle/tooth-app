use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::State;

use crate::errors::AppError;
use crate::state::core_tools::CoreToolsState;

const MONITOR_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanMonitor {
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoreMonitor {
    #[serde(rename = "monitor_index")]
    monitor_index: u32,
    name: String,
    width: u32,
    height: u32,
    primary: bool,
}

fn parse_monitors(response: &serde_json::Value) -> Result<Vec<ScanMonitor>, AppError> {
    if response.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        return Err(AppError::CoreTools(response.to_string()));
    }
    let raw = response
        .get("monitors_json")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AppError::CoreTools("list_monitors omitted monitors_json".into()))?;
    let monitors: Vec<CoreMonitor> = serde_json::from_str(raw)
        .map_err(|error| AppError::CoreTools(format!("invalid monitors_json: {error}")))?;
    Ok(monitors
        .into_iter()
        .map(|monitor| ScanMonitor {
            index: monitor.monitor_index,
            name: monitor.name,
            width: monitor.width,
            height: monitor.height,
            primary: monitor.primary,
        })
        .collect())
}

pub(crate) async fn list_scan_monitors_inner(
    core: &CoreToolsState,
) -> Result<Vec<ScanMonitor>, AppError> {
    let response = core
        .request(
            serde_json::json!({ "cmd": "list_monitors" }),
            MONITOR_TIMEOUT,
        )
        .await?;
    parse_monitors(&response)
}

#[tauri::command]
pub async fn list_scan_monitors(
    core: State<'_, CoreToolsState>,
) -> Result<Vec<ScanMonitor>, AppError> {
    list_scan_monitors_inner(&core).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_documented_monitor_payload() {
        let response = serde_json::json!({
            "ok": true,
            "monitors_json": "[{\"monitor_index\":2,\"name\":\"desk\",\"width\":1920,\"height\":1080,\"primary\":true}]"
        });
        let monitors = parse_monitors(&response).expect("valid monitor JSON");
        assert_eq!(monitors[0].index, 2);
        assert_eq!(monitors[0].width, 1920);
    }

    #[test]
    fn rejects_unsuccessful_monitor_response() {
        let response = serde_json::json!({ "ok": false });
        assert_ne!(
            response.get("ok").and_then(serde_json::Value::as_bool),
            Some(true)
        );
    }
}
