use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

use tauri::async_runtime::Sender;
use tauri_plugin_shell::process::CommandChild;
use tokio::sync::{oneshot, Notify};

use crate::errors::AppError;

/// core-tools (tooth-backend) の起動状態。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreToolsStatus {
    /// 未起動。
    #[default]
    Idle,
    /// 起動中 (ready 待ち)。
    Starting,
    /// ready event 受信済み (start コマンド戻り直前)。
    Ready,
    /// start コマンド完了、通常稼働中。
    Running,
    /// プロセスが終了した (明示的停止以外)。
    Disconnected,
    /// 起動失敗・タイムアウト・異常終了。
    Failed,
}

/// core-tools プロセスの状態・通信チャネル・子プロセスを保持する共有状態。
///
/// `status` / `child` は非同期タスク (reader/writer) からも参照するため
/// `Arc<Mutex<_>>` で保持し、クローンしてタスクに渡す。
#[derive(Clone)]
pub struct CoreToolsState {
    pub status: Arc<Mutex<CoreToolsStatus>>,
    pub cmd_tx: Arc<Mutex<Option<Sender<String>>>>,
    pub child: Arc<Mutex<Option<CommandChild>>>,
    pub terminated: Arc<Notify>,
    pub pending_requests: Arc<Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
    next_request_id: Arc<AtomicU64>,
}

impl CoreToolsState {
    pub fn register_request(&self) -> (String, oneshot::Receiver<serde_json::Value>) {
        let id = format!(
            "rust-{}",
            self.next_request_id.fetch_add(1, Ordering::Relaxed)
        );
        let (sender, receiver) = oneshot::channel();
        self.pending_requests
            .lock()
            .expect("core-tools pending requests mutex poisoned")
            .insert(id.clone(), sender);
        (id, receiver)
    }

    pub fn resolve_response(&self, response: &serde_json::Value) -> bool {
        resolve_pending_response(&self.pending_requests, response)
    }

    /// Send a JSON command with a Rust-owned id and await its matching response.
    pub async fn request(
        &self,
        command: serde_json::Value,
        timeout: Duration,
    ) -> Result<serde_json::Value, AppError> {
        let mut command = command.as_object().cloned().ok_or_else(|| {
            AppError::CoreTools("core-tools command must be a JSON object".to_string())
        })?;
        if command.contains_key("id") {
            return Err(AppError::CoreTools(
                "core-tools command already contains an id".to_string(),
            ));
        }

        let (id, receiver) = self.register_request();
        command.insert("id".to_string(), serde_json::Value::String(id.clone()));
        let payload = match serde_json::to_string(&command) {
            Ok(payload) => payload,
            Err(error) => {
                self.remove_pending_request(&id);
                return Err(AppError::CoreTools(format!(
                    "failed to serialize core-tools command: {error}"
                )));
            }
        };
        let sender = self
            .cmd_tx
            .lock()
            .expect("core-tools cmd_tx mutex poisoned")
            .clone();
        let Some(sender) = sender else {
            self.remove_pending_request(&id);
            return Err(AppError::CoreTools("core-tools is not running".to_string()));
        };
        if let Err(error) = sender.send(payload).await {
            self.remove_pending_request(&id);
            return Err(AppError::CoreTools(format!(
                "core-tools command channel closed: {error}"
            )));
        }

        match tokio::time::timeout(timeout, receiver).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                self.remove_pending_request(&id);
                Err(AppError::CoreTools(
                    "core-tools response channel closed".to_string(),
                ))
            }
            Err(_) => {
                self.remove_pending_request(&id);
                Err(AppError::CoreTools(format!(
                    "core-tools request timed out after {timeout:?}"
                )))
            }
        }
    }

    fn remove_pending_request(&self, id: &str) {
        self.pending_requests
            .lock()
            .expect("core-tools pending requests mutex poisoned")
            .remove(id);
    }
}

pub(crate) fn resolve_pending_response(
    pending_requests: &Arc<Mutex<HashMap<String, oneshot::Sender<serde_json::Value>>>>,
    response: &serde_json::Value,
) -> bool {
    let Some(id) = response.get("id").and_then(response_id) else {
        return false;
    };
    pending_requests
        .lock()
        .expect("core-tools pending requests mutex poisoned")
        .remove(&id)
        .is_some_and(|sender| sender.send(response.clone()).is_ok())
}

impl Default for CoreToolsState {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(CoreToolsStatus::Idle)),
            cmd_tx: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(None)),
            terminated: Arc::new(Notify::new()),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            next_request_id: Arc::new(AtomicU64::new(1)),
        }
    }
}

fn response_id(value: &serde_json::Value) -> Option<String> {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .or_else(|| value.as_u64().map(|id| id.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn correlates_registered_response_by_id() {
        let state = CoreToolsState::default();
        let (id, receiver) = state.register_request();
        let response = serde_json::json!({"id": id, "ok": true});
        assert!(state.resolve_response(&response));
        assert_eq!(receiver.blocking_recv().expect("response"), response);
        assert!(!state.resolve_response(&response));
    }

    #[test]
    fn ignores_unregistered_or_idless_responses() {
        let state = CoreToolsState::default();
        assert!(!state.resolve_response(&serde_json::json!({"id": "other"})));
        assert!(!state.resolve_response(&serde_json::json!({"ok": true})));
    }

    #[test]
    fn rejects_commands_with_an_existing_id() {
        let state = CoreToolsState::default();
        let result = tauri::async_runtime::block_on(state.request(
            serde_json::json!({"id": "caller-owned", "cmd": "camera"}),
            Duration::from_millis(1),
        ));
        assert!(result.is_err());
        assert!(state
            .pending_requests
            .lock()
            .expect("pending requests")
            .is_empty());
    }

    #[test]
    fn sends_generated_id_and_awaits_matching_response() {
        tauri::async_runtime::block_on(async {
            let state = CoreToolsState::default();
            let (sender, mut receiver) = tauri::async_runtime::channel(1);
            *state.cmd_tx.lock().expect("cmd_tx") = Some(sender);
            let responder_state = state.clone();
            let responder = tauri::async_runtime::spawn(async move {
                let payload = receiver.recv().await.expect("command");
                let command: serde_json::Value = serde_json::from_str(&payload).expect("json");
                let id = command.get("id").expect("generated id").clone();
                let response = serde_json::json!({"id": id, "ok": true});
                assert!(responder_state.resolve_response(&response));
            });

            let response = state
                .request(serde_json::json!({"cmd": "camera"}), Duration::from_secs(1))
                .await
                .expect("response");
            responder.await.expect("responder");
            assert_eq!(response["ok"], true);
            assert!(state
                .pending_requests
                .lock()
                .expect("pending requests")
                .is_empty());
        });
    }

    #[test]
    fn removes_pending_request_on_timeout() {
        tauri::async_runtime::block_on(async {
            let state = CoreToolsState::default();
            let (sender, _receiver) = tauri::async_runtime::channel(1);
            *state.cmd_tx.lock().expect("cmd_tx") = Some(sender);
            let result = state
                .request(
                    serde_json::json!({"cmd": "camera"}),
                    Duration::from_millis(1),
                )
                .await;
            assert!(result.is_err());
            assert!(state
                .pending_requests
                .lock()
                .expect("pending requests")
                .is_empty());
        });
    }
}
