use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::{Emitter, State};

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
pub async fn stop_core_tools(
    app: tauri::AppHandle,
    state: State<'_, CoreToolsState>,
) -> Result<(), AppError> {
    let generation = state.generation.load(Ordering::Acquire);
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

    // 2. この世代の終了だけを待つ。古いNotify permitはstatusを再確認して捨てる。
    match tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let should_wait = {
                let status = state
                    .status
                    .lock()
                    .expect("core-tools status mutex poisoned");
                state.is_generation(generation)
                    && !state.has_terminated(generation)
                    && matches!(
                        *status,
                        CoreToolsStatus::Starting
                            | CoreToolsStatus::Ready
                            | CoreToolsStatus::Running
                    )
            };
            if !should_wait {
                break;
            }
            state.terminated.notified().await;
        }
    })
    .await
    {
        Ok(()) => log::info!("core-tools: graceful shutdown confirmed"),
        Err(_) => log::warn!("core-tools: shutdown timeout after 5s, killing"),
    }

    // State lock中にtransportを無効化し、reader schedulingに頼らずfailureを通知する。
    invalidate_stopped_generation(&state, generation, |generation| {
        let _ = app.emit(
            "core-tools:event",
            serde_json::json!({
                "event": "core_tools_terminated",
                "generation": generation,
                "message": "core-tools stopped"
            }),
        );
    });

    Ok(())
}

fn invalidate_stopped_generation(
    state: &CoreToolsState,
    generation: u64,
    notify: impl FnOnce(u64),
) -> bool {
    let mut status = state
        .status
        .lock()
        .expect("core-tools status mutex poisoned");
    if !state.is_generation(generation) {
        return false;
    }

    *state
        .cmd_tx
        .lock()
        .expect("core-tools cmd_tx mutex poisoned") = None;
    let child = state
        .child
        .lock()
        .expect("core-tools child mutex poisoned")
        .take();
    *status = CoreToolsStatus::Idle;
    notify(generation);
    state
        .terminated_generation
        .store(generation, Ordering::Release);
    state.terminated.notify_one();
    drop(status);
    if let Some(child) = child {
        let _ = child.kill();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::invalidate_stopped_generation;
    use crate::state::core_tools::{CoreToolsState, CoreToolsStatus};

    #[test]
    fn explicit_stop_notifies_old_generation_before_restart_and_preserves_new_generation() {
        let state = CoreToolsState::default();
        state
            .generation
            .store(1, std::sync::atomic::Ordering::Release);
        *state.status.lock().expect("status lock") = CoreToolsStatus::Running;
        let (sender, receiver) = std::sync::mpsc::channel();

        assert!(invalidate_stopped_generation(&state, 1, |generation| {
            sender.send(generation).expect("send stop notification");
        }));
        assert_eq!(receiver.try_recv(), Ok(1));

        *state.status.lock().expect("status lock") = CoreToolsStatus::Starting;
        state
            .generation
            .store(2, std::sync::atomic::Ordering::Release);
        assert_eq!(
            *state.status.lock().expect("status lock"),
            CoreToolsStatus::Starting
        );
        assert!(!state.has_terminated(2));
        assert!(state.mark_ready(2));
        assert!(state.mark_running(2));
        assert!(state.is_generation(2));
    }

    #[test]
    fn stale_stop_does_not_notify_or_change_current_generation() {
        let state = CoreToolsState::default();
        state
            .generation
            .store(2, std::sync::atomic::Ordering::Release);
        *state.status.lock().expect("status lock") = CoreToolsStatus::Running;
        let mut notified = false;

        assert!(!invalidate_stopped_generation(&state, 1, |_| notified = true));
        assert!(!notified);
        assert_eq!(
            *state.status.lock().expect("status lock"),
            CoreToolsStatus::Running
        );
        assert!(!state.has_terminated(2));
    }
}
