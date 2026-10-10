use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{async_runtime, Emitter};
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
///
/// この関数は `#[tauri::command]` と `.setup()` フックの両方から呼ばれる。
/// `.setup()` からは非ブロッキング (fire-and-forget) で起動する。
pub async fn start_core_tools_inner(
    app: &tauri::AppHandle,
    state: &CoreToolsState,
) -> Result<(), AppError> {
    // 二重起動防止と世代更新をstatus lock内で行い、古いreader/writerの失敗通知を隔離する。
    let generation = {
        let mut status = lock_status(&state.status);
        if matches!(
            *status,
            CoreToolsStatus::Starting | CoreToolsStatus::Ready | CoreToolsStatus::Running
        ) {
            return Err(AppError::CoreTools(
                "core-tools is already running".to_string(),
            ));
        }
        *status = CoreToolsStatus::Starting;
        state.generation.fetch_add(1, Ordering::AcqRel) + 1
    };

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
            state.mark_unavailable(generation, CoreToolsStatus::Failed);
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
    let app_for_writer = app.clone();
    let state_for_writer = state.clone();
    async_runtime::spawn(async move {
        while let Some(json) = cmd_rx.recv().await {
            if !state_for_writer.is_generation(generation) {
                break;
            }
            let write_failed = {
                let mut guard = lock_child(&child_for_writer);
                match guard.as_mut() {
                    Some(child) => {
                        let mut payload = json.into_bytes();
                        payload.push(b'\n');
                        child.write(&payload).is_err()
                    }
                    None => true,
                }
            };
            if write_failed {
                if state_for_writer.mark_unavailable(generation, CoreToolsStatus::Disconnected) {
                    let _ = app_for_writer.emit(
                        "core-tools:event",
                        serde_json::json!({
                            "event": "core_tools_terminated",
                            "generation": generation,
                            "message": "stdin write failed"
                        }),
                    );
                }
                break;
            }
        }
    });

    // Reader タスク: stdout/stderr を処理しイベント/レスポンスを emit する。
    let app_for_reader = app.clone();
    let state_for_reader = state.clone();
    async_runtime::spawn(async move {
        let mut ready_signalled = false;
        let mut unavailable = false;
        while let Some(event) = rx.recv().await {
            if !state_for_reader.is_generation(generation) {
                break;
            }
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
                                    if state_for_reader.mark_ready(generation) {
                                        ready_signalled = true;
                                        let _ = outcome_tx.send(Ok(())).await;
                                    } else {
                                        unavailable = true;
                                        break;
                                    }
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
                    let failure_status = if ready_signalled {
                        CoreToolsStatus::Disconnected
                    } else {
                        CoreToolsStatus::Failed
                    };
                    unavailable = state_for_reader.mark_unavailable(generation, failure_status);
                    if unavailable {
                        let _ = app_for_reader.emit(
                            "core-tools:event",
                            serde_json::json!({
                                "event": "core_tools_terminated",
                                "generation": generation,
                                "message": msg
                            }),
                        );
                    }
                    if unavailable && !ready_signalled {
                        ready_signalled = true;
                        let _ = outcome_tx.send(Err(msg)).await;
                    }
                    break;
                }
                CommandEvent::Terminated(payload) => {
                    log::warn!(
                        "core-tools terminated: code={:?} signal={:?}",
                        payload.code,
                        payload.signal
                    );
                    let failure_status = if ready_signalled {
                        CoreToolsStatus::Disconnected
                    } else {
                        CoreToolsStatus::Failed
                    };
                    unavailable = state_for_reader.mark_unavailable(generation, failure_status);
                    if unavailable {
                        let _ = app_for_reader.emit(
                            "core-tools:event",
                            serde_json::json!({
                                "event": "core_tools_terminated",
                                "generation": generation,
                                "code": payload.code,
                                "signal": payload.signal,
                            }),
                        );
                    }
                    if !ready_signalled && unavailable {
                        ready_signalled = true;
                        let _ = outcome_tx
                            .send(Err(format!(
                                "tooth-backend terminated (code={:?})",
                                payload.code
                            )))
                            .await;
                    }
                    break;
                }
                _ => {
                    log::debug!("core-tools: ignored unknown command event variant");
                }
            }
        }
        if unavailable || !state_for_reader.is_generation(generation) {
            return;
        }
        // rx クローズ = プロセス終了
        let failure_status = if ready_signalled {
            CoreToolsStatus::Disconnected
        } else {
            CoreToolsStatus::Failed
        };
        let invalidated = state_for_reader.mark_unavailable(generation, failure_status);
        if invalidated {
            let _ = app_for_reader.emit(
                "core-tools:event",
                serde_json::json!({
                    "event": "core_tools_terminated",
                    "generation": generation,
                    "message": "event stream closed"
                }),
            );
        }
        if invalidated && !ready_signalled {
            let _ = outcome_tx
                .send(Err("tooth-backend exited without ready".to_string()))
                .await;
        }
    });

    // ready 待ち (10s タイムアウト)
    match tokio::time::timeout(READY_TIMEOUT, outcome_rx.recv()).await {
        Ok(Some(Ok(()))) => {
            if state.mark_running(generation) {
                Ok(())
            } else {
                Err(AppError::CoreTools(
                    "core-tools exited before becoming available".to_string(),
                ))
            }
        }
        Ok(Some(Err(msg))) => Err(AppError::CoreTools(msg)),
        Ok(None) => {
            // outcome が送られずチャネルが閉じた
            state.mark_unavailable(generation, CoreToolsStatus::Failed);
            Err(AppError::CoreTools(
                "tooth-backend closed before ready".to_string(),
            ))
        }
        Err(_) => {
            // タイムアウト: プロセスを kill してクリーンアップ
            log::error!("core-tools: ready timeout after {READY_TIMEOUT:?}");
            if state.mark_unavailable(generation, CoreToolsStatus::Failed) {
                let _ = app.emit(
                    "core-tools:event",
                    serde_json::json!({
                        "event": "core_tools_terminated",
                        "generation": generation,
                        "message": "ready timeout"
                    }),
                );
            }
            Err(AppError::CoreTools(
                "tooth-backend did not become ready within 10s".to_string(),
            ))
        }
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
