use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{async_runtime, Emitter};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::errors::AppError;
use crate::state::core_tools::{
    resolve_pending_response, resolve_progress_event, CoreToolsState, CoreToolsStatus,
};

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
    if state.live_scan_lock.try_lock().is_err() {
        return Err(AppError::CoreTools(
            "cannot restart core-tools while a live scan is running".to_string(),
        ));
    }
    // Serialize start publication with generation-scoped invalidation.
    let generation = {
        let _lifecycle = state
            .lifecycle
            .lock()
            .expect("core-tools lifecycle mutex poisoned");
        let status = *lock_status(&state.status);
        if matches!(
            status,
            CoreToolsStatus::Starting | CoreToolsStatus::Ready | CoreToolsStatus::Running
        ) {
            return Err(AppError::CoreTools(
                "core-tools is already running".to_string(),
            ));
        }
        *lock_status(&state.status) = CoreToolsStatus::Starting;
        state.generation.fetch_add(1, Ordering::AcqRel) + 1
    };
    use std::path::PathBuf;
    use tauri::Manager;

    // start_core_tools_inner() 内
    let log_path = if cfg!(debug_assertions) {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("project root")
            .join("tools/core/logs/tooth-backend.log")
    } else {
        app.path()
            .app_log_dir()
            .map_err(|e| AppError::CoreTools(e.to_string()))?
            .join("tooth-backend.log")
    };

    // 親ディレクトリを作成
    std::fs::create_dir_all(log_path.parent().unwrap())
        .map_err(|e| AppError::CoreTools(e.to_string()))?;

    let sidecar_result = app
        .shell()
        .sidecar("tooth-backend")
        .map_err(|e| AppError::CoreTools(format!("failed to resolve tooth-backend: {e}")))
        .and_then(|cmd| {
            cmd.args(SIDECAR_ARGS)
                .env("TOOTH_BACKEND_LOG_PATH", &log_path)
                .spawn()
                .map_err(|e| AppError::CoreTools(format!("failed to spawn tooth-backend: {e}")))
        });

    let (mut rx, child) = match sidecar_result {
        Ok(pair) => pair,
        Err(e) => {
            state.invalidate_generation(generation, CoreToolsStatus::Failed, false);
            return Err(e);
        }
    };

    let (cmd_tx, mut cmd_rx) = async_runtime::channel::<String>(64);
    if !state.publish_transport(
        generation,
        cmd_tx,
        child,
        |cmd_tx, child| {
            *lock_child(&state.child) = Some(child);
            *lock_cmd_tx(&state.cmd_tx) = Some(cmd_tx);
        },
        |child| {
            let _ = child.kill();
        },
    ) {
        return Err(AppError::CoreTools(
            "core-tools start was superseded before transport installation".to_string(),
        ));
    }

    // ready / 失敗を start 側に伝える 1-shot チャネル
    let (outcome_tx, mut outcome_rx) = async_runtime::channel::<Result<(), String>>(1);

    // Writer タスク: stdin への書き込みを直列化する。
    let state_for_writer = state.clone();
    let outcome_for_writer = outcome_tx.clone();
    async_runtime::spawn(async move {
        while let Some(json) = cmd_rx.recv().await {
            let mut payload = json.into_bytes();
            payload.push(b'\n');
            let write_result =
                state_for_writer.with_generation_child(generation, |child| child.write(&payload));
            let Some(write_result) = write_result else {
                break;
            };
            if let Err(error) = write_result {
                log::error!("core-tools stdin write failed: {error}");
                let already_terminated = state_for_writer.has_terminated(generation);
                let starting = *lock_status(&state_for_writer.status) == CoreToolsStatus::Starting;
                if already_terminated {
                    return;
                }
                let status = if starting {
                    CoreToolsStatus::Failed
                } else {
                    CoreToolsStatus::Disconnected
                };
                if state_for_writer.invalidate_generation(generation, status, true) {
                    let _ = outcome_for_writer
                        .send(Err(format!("core-tools stdin write failed: {error}")))
                        .await;
                }
                break;
            }
        }
    });

    // Reader タスク: stdout/stderr を処理しイベント/レスポンスを emit する。
    let app_for_reader = app.clone();
    let status_for_reader = state.status.clone();
    let state_for_reader = state.clone();
    let pending_requests_for_reader = state.pending_requests.clone();
    let terminal_waiters_for_reader = state.terminal_waiters.clone();
    let progress_waiters_for_reader = state.progress_waiters.clone();
    let generation_for_reader = state.generation.clone();
    async_runtime::spawn(async move {
        let mut ready_signalled = false;
        let mut stdout = JsonLines::default();
        while let Some(event) = rx.recv().await {
            if generation_for_reader.load(Ordering::Acquire) != generation
                || state_for_reader.has_terminated(generation)
            {
                return;
            }
            match event {
                CommandEvent::Stdout(bytes) => {
                    for line in stdout.push(&bytes) {
                        match serde_json::from_slice::<serde_json::Value>(&line) {
                            Ok(v) => {
                                resolve_pending_response(&pending_requests_for_reader, &v);
                                crate::state::core_tools::resolve_terminal_event(
                                    &terminal_waiters_for_reader,
                                    &v,
                                );
                                resolve_progress_event(&progress_waiters_for_reader, &v);
                                if v.get("event").is_some() {
                                    if !ready_signalled
                                        && v.get("event").and_then(serde_json::Value::as_str)
                                            == Some("ready")
                                    {
                                        if state_for_reader.mark_ready(generation) {
                                            ready_signalled = true;
                                            let _ = outcome_tx.send(Ok(())).await;
                                        } else {
                                            let _ = outcome_tx
                                                .send(Err("core-tools transport is unavailable"
                                                    .to_string()))
                                                .await;
                                        }
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
                    if generation_for_reader.load(Ordering::Acquire) != generation {
                        break;
                    }
                    let status = if ready_signalled {
                        CoreToolsStatus::Disconnected
                    } else {
                        CoreToolsStatus::Failed
                    };
                    if state_for_reader.invalidate_generation(generation, status, true)
                        && !ready_signalled
                    {
                        ready_signalled = true;
                        let _ = outcome_tx.send(Err(msg)).await;
                    }
                }
                CommandEvent::Terminated(payload) => {
                    log::warn!(
                        "core-tools terminated: code={:?} signal={:?}",
                        payload.code,
                        payload.signal
                    );
                    if generation_for_reader.load(Ordering::Acquire) != generation {
                        break;
                    }
                    // ガードを await の前に確実に落とす (Send 要求)
                    let was_idle = *lock_status(&status_for_reader) == CoreToolsStatus::Idle;
                    if was_idle {
                        // stop 側で既に Idle 化済み
                        break;
                    }
                    let status = if ready_signalled {
                        CoreToolsStatus::Disconnected
                    } else {
                        CoreToolsStatus::Failed
                    };
                    if !state_for_reader.invalidate_generation(generation, status, false) {
                        break;
                    }
                    if !ready_signalled {
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
        if generation_for_reader.load(Ordering::Acquire) != generation
            || state_for_reader.has_terminated(generation)
        {
            return;
        }
        // rx クローズ = プロセス終了
        let was_idle = *lock_status(&status_for_reader) == CoreToolsStatus::Idle;
        if was_idle {
            return;
        }
        let status = if ready_signalled {
            CoreToolsStatus::Disconnected
        } else {
            CoreToolsStatus::Failed
        };
        if !state_for_reader.invalidate_generation(generation, status, true) {
            return;
        }
        if !ready_signalled {
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
                    "core-tools transport became unavailable before startup completed".to_string(),
                ))
            }
        }
        Ok(Some(Err(msg))) => Err(AppError::CoreTools(msg)),
        Ok(None) => {
            // outcome が送られずチャネルが閉じた
            Err(AppError::CoreTools(
                "tooth-backend closed before ready".to_string(),
            ))
        }
        Err(_) => {
            // タイムアウト: プロセスを kill してクリーンアップ
            log::error!("core-tools: ready timeout after {READY_TIMEOUT:?}");
            state.invalidate_generation(generation, CoreToolsStatus::Failed, true);
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
