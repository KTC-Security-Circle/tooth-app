use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use tauri::async_runtime::Sender;
use tauri_plugin_shell::process::CommandChild;
use tokio::sync::Notify;

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
    pub generation: Arc<AtomicU64>,
    pub terminated_generation: Arc<AtomicU64>,
}

impl Default for CoreToolsState {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(CoreToolsStatus::Idle)),
            cmd_tx: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(None)),
            terminated: Arc::new(Notify::new()),
            generation: Arc::new(AtomicU64::new(0)),
            terminated_generation: Arc::new(AtomicU64::new(u64::MAX)),
        }
    }
}

impl CoreToolsState {
    pub fn is_generation(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Acquire) == generation
    }

    pub fn has_terminated(&self, generation: u64) -> bool {
        self.terminated_generation.load(Ordering::Acquire) == generation
    }

    pub fn mark_ready(&self, generation: u64) -> bool {
        let mut status = match self.status.lock() {
            Ok(status) => status,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !self.is_generation(generation) {
            return false;
        }
        *status = CoreToolsStatus::Ready;
        true
    }

    pub fn mark_running(&self, generation: u64) -> bool {
        let mut status = match self.status.lock() {
            Ok(status) => status,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !self.is_generation(generation) || *status != CoreToolsStatus::Ready {
            return false;
        }
        *status = CoreToolsStatus::Running;
        true
    }

    pub fn available_generation(&self) -> Option<u64> {
        self.status
            .lock()
            .ok()
            .filter(|status| matches!(**status, CoreToolsStatus::Ready | CoreToolsStatus::Running))
            .map(|_| self.generation.load(Ordering::Acquire))
    }

    /// Invalidate only the sidecar generation that reported the failure.
    pub fn mark_unavailable(&self, generation: u64, failure_status: CoreToolsStatus) -> bool {
        let mut status = match self.status.lock() {
            Ok(status) => status,
            Err(poisoned) => poisoned.into_inner(),
        };
        if !self.is_generation(generation) {
            return false;
        }
        if *status != CoreToolsStatus::Idle {
            *status = failure_status;
        }
        let mut sender = self
            .cmd_tx
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *sender = None;
        let mut child_guard = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let child = child_guard.take();
        drop(child_guard);
        drop(sender);
        drop(status);
        if let Some(child) = child {
            let child = child;
            let _ = child.kill();
        }
        self.terminated_generation
            .store(generation, Ordering::Release);
        self.terminated.notify_one();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{CoreToolsState, CoreToolsStatus};
    use std::time::Duration;

    #[test]
    fn disconnected_status_rejects_a_stale_sender() {
        let state = CoreToolsState::default();
        let (sender, _receiver) = tauri::async_runtime::channel(1);
        *state.cmd_tx.lock().expect("cmd sender lock") = Some(sender);
        *state.status.lock().expect("status lock") = CoreToolsStatus::Disconnected;

        assert_eq!(state.available_generation(), None);
    }

    #[test]
    fn current_generation_failure_clears_sender_and_notifies_waiters() {
        let state = CoreToolsState::default();
        let (sender, _receiver) = tauri::async_runtime::channel(1);
        *state.cmd_tx.lock().expect("cmd sender lock") = Some(sender);
        *state.status.lock().expect("status lock") = CoreToolsStatus::Running;

        assert!(state.mark_unavailable(0, CoreToolsStatus::Disconnected));
        assert_eq!(
            *state.status.lock().expect("status lock"),
            CoreToolsStatus::Disconnected
        );
        assert!(state.cmd_tx.lock().expect("cmd sender lock").is_none());
        let notified = tauri::async_runtime::block_on(async {
            tokio::time::timeout(Duration::from_millis(10), state.terminated.notified()).await
        });
        assert!(notified.is_ok());
    }

    #[test]
    fn stale_generation_does_not_invalidate_new_sidecar() {
        let state = CoreToolsState::default();
        state
            .generation
            .store(2, std::sync::atomic::Ordering::Release);
        *state.status.lock().expect("status lock") = CoreToolsStatus::Running;

        assert!(!state.mark_unavailable(1, CoreToolsStatus::Disconnected));
        assert_eq!(
            *state.status.lock().expect("status lock"),
            CoreToolsStatus::Running
        );
    }

    #[test]
    fn old_termination_notification_does_not_match_new_generation() {
        let state = CoreToolsState::default();
        state
            .generation
            .store(2, std::sync::atomic::Ordering::Release);
        state
            .terminated_generation
            .store(1, std::sync::atomic::Ordering::Release);

        assert!(!state.has_terminated(2));
    }
}
