use std::time::Duration;

use tauri::{async_runtime, Emitter, State};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::errors::AppError;
use crate::state::core_tools::{CoreToolsState, CoreToolsStatus};

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
#[tauri::command]
pub async fn start_core_tools(
    app: tauri::AppHandle,
    state: State<'_, CoreToolsState>,
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

    let (mut rx, child) = app
        .shell()
        .sidecar("tooth-backend")
        .map_err(|e| AppError::CoreTools(format!("failed to resolve tooth-backend: {e}")))?
        .args(SIDECAR_ARGS)
        .spawn()
        .map_err(|e| AppError::CoreTools(format!("failed to spawn tooth-backend: {e}")))?;

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
    async_runtime::spawn(async move {
        let mut ready_signalled = false;
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(bytes) => {
                    let line = String::from_utf8_lossy(&bytes);
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<serde_json::Value>(line) {
                        Ok(v) => {
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
                            log::warn!("core-tools: non-JSON stdout line: {line} ({e})");
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
