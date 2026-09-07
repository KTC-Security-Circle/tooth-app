use std::sync::{atomic::AtomicU64, Arc, Mutex};

use tauri_plugin_shell::process::CommandChild;
use tokio::sync::{mpsc, Mutex as AsyncMutex};

pub type MatchingResponse = Result<serde_json::Value, String>;

pub struct MatchingState {
    pub child: Arc<Mutex<Option<CommandChild>>>,
    pub cmd_tx: Arc<Mutex<Option<mpsc::Sender<String>>>>,
    pub responses: Arc<AsyncMutex<Option<mpsc::Receiver<MatchingResponse>>>>,
    pub request_lock: Arc<AsyncMutex<()>>,
    pub generation: Arc<AtomicU64>,
}

impl Default for MatchingState {
    fn default() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            cmd_tx: Arc::new(Mutex::new(None)),
            responses: Arc::new(AsyncMutex::new(None)),
            request_lock: Arc::new(AsyncMutex::new(())),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }
}
