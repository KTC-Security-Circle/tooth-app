use std::time::Duration;

use tauri::{async_runtime, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::errors::AppError;
use crate::state::core_tools::{resolve_pending_response, CoreToolsState, CoreToolsStatus};

/// tooth-backend (core-tools) の起動引数。
const SIDECAR_ARGS: &[&str] = &[
    "serve",
    "--control",
    "stdio",
    "--mjpeg-host",
    "127.0.0.1",
    "--mjpeg-port",
    "39010",
];

/// ready event 受信を待つ最大時間。
const READY_TIMEOUT: Duration = Duration::from_secs(10);

/// core-tools (tooth-backend) を起動し ready event を待つ。
///
/// - Writer タスク: `cmd_tx` 受信 → stdin write (書き込みを直列化)
/// - Reader タスク: stdout 行を JSON parse し `ready` event で `Ready` 遷移 + emit、
///   それ以外は `core-tools:response` / `core-tools:event` として emit
/// - `tokio::time::timeout(10s, ready)` で無限待ちを防止
/// - タイムアウト/異常終了時は `Failed` 遷移 + `AppError::CoreTools` を返す
///
/// この関数は `#[tauri::command]` と `.setup()` フックの両方から呼ばれる。
/// `.setup()` からは非ブロッキング (fire-and-forget) で起動する。
pub async fn start_core_tools_inner(
    app: &tauri::AppHandle,
    state: &CoreToolsState,
) -> Result<(), AppError> {
    // 二重起動防止
    {
        let status = *lock_status(&state.status);
        if matches!(
            status,
            CoreToolsStatus::Starting | CoreToolsStatus::Ready | CoreToolsStatus::Running
        ) {
            return Err(AppError::CoreTools(
                "core-tools is already running".to_string(),
            ));
        }
    }

    *lock_status(&state.status) = CoreToolsStatus::Starting;

    let sidecar_result = app
        .shell()
        .sidecar("tooth-backend")
        .map_err(|e| AppError::CoreTools(format!("failed to resolve tooth-backend: {e}")))
        .and_then(|cmd| {
            cmd.args(SIDECAR_ARGS)
                .spawn()
                .map_err(|e| AppError::CoreTools(format!("failed to spawn tooth-backend: {e}")))
        });

    let (mut rx, child) = match sidecar_result {
        Ok(pair) => pair,
        Err(e) => {
            *lock_status(&state.status) = CoreToolsStatus::Failed;
            *lock_child(&state.child) = None;
            *lock_cmd_tx(&state.cmd_tx) = None;
            return Err(e);
        }
    };

    *lock_child(&state.child) = Some(child);

    let (cmd_tx, mut cmd_rx) = async_runtime::channel::<String>(64);
    *lock_cmd_tx(&state.cmd_tx) = Some(cmd_tx);

    // ready / 失敗を start 側に伝える 1-shot チャネル
    let (outcome_tx, mut outcome_rx) = async_runtime::channel::<Result<(), String>>(1);

    // Writer タスク: stdin への書き込みを直列化する。
    let child_for_writer = state.child.clone();
    async_runtime::spawn(async move {
        while let Some(json) = cmd_rx.recv().await {
            let mut guard = lock_child(&child_for_writer);
            match guard.as_mut() {
                Some(child) => {
                    let mut payload = json.into_bytes();
                    payload.push(b'\n');
                    if child.write(&payload).is_err() {
                        // stdin が閉じた場合は writer を停止
                        break;
                    }
                }
                None => break,
            }
        }
    });

    // Reader タスク: stdout/stderr を処理しイベント/レスポンスを emit する。
    let app_for_reader = app.clone();
    let status_for_reader = state.status.clone();
    let terminated_for_reader = state.terminated.clone();
    let pending_requests_for_reader = state.pending_requests.clone();
    async_runtime::spawn(async move {
        let mut ready_signalled = false;
        let mut stdout = JsonLines::default();
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    for line in stdout.push(&bytes) {
                        match serde_json::from_slice::<serde_json::Value>(&line) {
                            Ok(v) => {
                                resolve_pending_response(&pending_requests_for_reader, &v);
                                if v.get("event").is_some() {
                                    if !ready_signalled
                                        && v.get("event").and_then(serde_json::Value::as_str)
                                            == Some("ready")
                                    {
                                        ready_signalled = true;
                                        *lock_status(&status_for_reader) = CoreToolsStatus::Ready;
                                        let _ = outcome_tx.send(Ok(())).await;
                                    }
                                    let _ = app_for_reader.emit("core-tools:event", &v);
                                } else {
                                    let _ = app_for_reader.emit("core-tools:response", &v);
                                }
                            }
                            Err(e) => {
                                log::warn!("core-tools: non-JSON stdout line ({e})");
                            }
                        }
                    }
                }
                CommandEvent::Stderr(bytes) => {
                    log::warn!(
                        "core-tools stderr: {}",
                        String::from_utf8_lossy(&bytes).trim()
                    );
                }
                CommandEvent::Error(msg) => {
                    log::error!("core-tools command error: {msg}");
                    if !ready_signalled {
                        ready_signalled = true;
                        *lock_status(&status_for_reader) = CoreToolsStatus::Failed;
                        let _ = outcome_tx.send(Err(msg)).await;
                    } else {
                        *lock_status(&status_for_reader) = CoreToolsStatus::Failed;
                    }
                }
                CommandEvent::Terminated(payload) => {
                    log::warn!(
                        "core-tools terminated: code={:?} signal={:?}",
                        payload.code,
                        payload.signal
                    );
                    terminated_for_reader.notify_one();
                    // ガードを await の前に確実に落とす (Send 要求)
                    let was_idle = *lock_status(&status_for_reader) == CoreToolsStatus::Idle;
                    if was_idle {
                        // stop 側で既に Idle 化済み
                        break;
                    }
                    if !ready_signalled {
                        ready_signalled = true;
                        *lock_status(&status_for_reader) = CoreToolsStatus::Failed;
                        let _ = outcome_tx
                            .send(Err(format!(
                                "tooth-backend terminated (code={:?})",
                                payload.code
                            )))
                            .await;
                    } else {
                        *lock_status(&status_for_reader) = CoreToolsStatus::Disconnected;
                    }
                    break;
                }
                _ => {
                    log::debug!("core-tools: ignored unknown command event variant");
                }
            }
        }
        // rx クローズ = プロセス終了
        let was_idle = *lock_status(&status_for_reader) == CoreToolsStatus::Idle;
        if was_idle {
            return;
        }
        if !ready_signalled {
            *lock_status(&status_for_reader) = CoreToolsStatus::Failed;
            let _ = outcome_tx
                .send(Err("tooth-backend exited without ready".to_string()))
                .await;
        } else {
            *lock_status(&status_for_reader) = CoreToolsStatus::Disconnected;
        }
    });

    // ready 待ち (10s タイムアウト)
    match tokio::time::timeout(READY_TIMEOUT, outcome_rx.recv()).await {
        Ok(Some(Ok(()))) => {
            *lock_status(&state.status) = CoreToolsStatus::Running;
            Ok(())
        }
        Ok(Some(Err(msg))) => {
            *lock_status(&state.status) = CoreToolsStatus::Failed;
            Err(AppError::CoreTools(msg))
        }
        Ok(None) => {
            // outcome が送られずチャネルが閉じた
            *lock_status(&state.status) = CoreToolsStatus::Failed;
            Err(AppError::CoreTools(
                "tooth-backend closed before ready".to_string(),
            ))
        }
        Err(_) => {
            // タイムアウト: プロセスを kill してクリーンアップ
            log::error!("core-tools: ready timeout after {READY_TIMEOUT:?}");
            *lock_status(&state.status) = CoreToolsStatus::Failed;
            if let Some(child) = lock_child(&state.child).take() {
                let _ = child.kill();
            }
            *lock_cmd_tx(&state.cmd_tx) = None;
            Err(AppError::CoreTools(
                "tooth-backend did not become ready within 10s".to_string(),
            ))
        }
    }
}

#[derive(Default)]
struct JsonLines {
    buffer: Vec<u8>,
}

impl JsonLines {
    fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        self.buffer.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(position) = self.buffer.iter().position(|byte| *byte == b'\n') {
            let mut line = self.buffer.drain(..=position).collect::<Vec<_>>();
            line.pop();
            if line.last() == Some(&b'\r') {
                line.pop();
            }
            if !line.iter().all(u8::is_ascii_whitespace) {
                lines.push(line);
            }
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::JsonLines;

    #[test]
    fn handles_fragmented_and_coalesced_json_lines() {
        let mut lines = JsonLines::default();
        assert!(lines.push(br#"{"id":"a"}"#).is_empty());
        assert_eq!(
            lines.push(b"\n{\"id\":\"b\"}\n"),
            vec![br#"{"id":"a"}"#.to_vec(), br#"{"id":"b"}"#.to_vec()]
        );
    }

    #[test]
    fn handles_crlf_and_discards_blank_lines() {
        let mut lines = JsonLines::default();
        assert_eq!(
            lines.push(b"\r\n {\"ok\":true} \r\n"),
            vec![b" {\"ok\":true} ".to_vec()]
        );
    }
}

/// core-tools (tooth-backend) を起動する Tauri コマンド。
/// 実装は `start_core_tools_inner` に委譲する。
#[tauri::command]
pub async fn start_core_tools(
    app: tauri::AppHandle,
    state: tauri::State<'_, CoreToolsState>,
) -> Result<(), AppError> {
    start_core_tools_inner(&app, &state).await
}

fn lock_status(
    m: &std::sync::Arc<std::sync::Mutex<CoreToolsStatus>>,
) -> std::sync::MutexGuard<'_, CoreToolsStatus> {
    m.lock().expect("core-tools status mutex poisoned")
}

fn lock_cmd_tx(
    m: &std::sync::Arc<std::sync::Mutex<Option<async_runtime::Sender<String>>>>,
) -> std::sync::MutexGuard<'_, Option<async_runtime::Sender<String>>> {
    m.lock().expect("core-tools cmd_tx mutex poisoned")
}

fn lock_child(
    m: &std::sync::Arc<std::sync::Mutex<Option<tauri_plugin_shell::process::CommandChild>>>,
) -> std::sync::MutexGuard<'_, Option<tauri_plugin_shell::process::CommandChild>> {
    m.lock().expect("core-tools child mutex poisoned")
}
