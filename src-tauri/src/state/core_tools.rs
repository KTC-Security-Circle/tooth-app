use std::sync::{Arc, Mutex};

use tauri::async_runtime::Sender;
use tauri_plugin_shell::process::CommandChild;

/// core-tools (tooth-backend) の起動状態。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
pub struct CoreToolsState {
    pub status: Arc<Mutex<CoreToolsStatus>>,
    pub cmd_tx: Arc<Mutex<Option<Sender<String>>>>,
    pub child: Arc<Mutex<Option<CommandChild>>>,
}

impl Default for CoreToolsState {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(CoreToolsStatus::Idle)),
            cmd_tx: Arc::new(Mutex::new(None)),
            child: Arc::new(Mutex::new(None)),
        }
    }
}
