use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};

use serde::Serialize;
use tauri_plugin_shell::process::CommandChild;
use tokio::sync::{mpsc, Mutex as AsyncMutex};

pub type MatchingResponse = Result<serde_json::Value, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchingServerPhase {
    Idle,
    Starting,
    Ready,
    Disconnected,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MatchingServerStatus {
    pub status: MatchingServerPhase,
    pub version: u64,
}

#[derive(Clone)]
pub struct MatchingState {
    pub child: Arc<Mutex<Option<CommandChild>>>,
    pub cmd_tx: Arc<Mutex<Option<mpsc::Sender<String>>>>,
    pub responses: Arc<AsyncMutex<Option<mpsc::Receiver<MatchingResponse>>>>,
    pub request_lock: Arc<AsyncMutex<()>>,
    pub generation: Arc<AtomicU64>,
    pub lifecycle: Arc<Mutex<()>>,
    pub status: Arc<Mutex<MatchingServerStatus>>,
}

impl MatchingState {
    pub fn status_snapshot(&self) -> MatchingServerStatus {
        self.status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    pub fn begin_start(&self) -> (u64, MatchingServerStatus) {
        let _lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let generation = self.generation.fetch_add(1, Ordering::AcqRel) + 1;
        let status = self.transition_locked(MatchingServerPhase::Starting);
        (generation, status)
    }

    pub fn transition_if(
        &self,
        generation: u64,
        expected: MatchingServerPhase,
        next: MatchingServerPhase,
    ) -> Option<MatchingServerStatus> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.generation.load(Ordering::Acquire) != generation
            || self.status_snapshot().status != expected
        {
            return None;
        }
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if status.status == next {
            return None;
        }
        status.status = next;
        status.version += 1;
        Some(status.clone())
    }

    pub fn publish_if_starting<T>(
        &self,
        generation: u64,
        publish: impl FnOnce() -> T,
    ) -> Option<T> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.generation.load(Ordering::Acquire) != generation
            || self.status_snapshot().status != MatchingServerPhase::Starting
        {
            return None;
        }
        Some(publish())
    }

    pub fn with_generation_child<T>(
        &self,
        generation: u64,
        operation: impl FnOnce(&mut CommandChild) -> T,
    ) -> Option<T> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.generation.load(Ordering::Acquire) != generation {
            return None;
        }
        self.child
            .lock()
            .ok()
            .and_then(|mut child| child.as_mut().map(operation))
    }

    pub fn invalidate(
        &self,
        generation: Option<u64>,
        next: MatchingServerPhase,
        cleanup: impl FnOnce(),
    ) -> Option<MatchingServerStatus> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if generation
            .is_some_and(|generation| self.generation.load(Ordering::Acquire) != generation)
        {
            return None;
        }
        self.generation.fetch_add(1, Ordering::AcqRel);
        let unchanged = self.status_snapshot().status == next;
        let status = self.transition_locked(next);
        cleanup();
        (!unchanged).then_some(status)
    }

    pub fn reader_terminated(
        &self,
        generation: u64,
        cleanup: impl FnOnce(),
    ) -> Option<MatchingServerStatus> {
        let _lifecycle = self
            .lifecycle
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.generation.load(Ordering::Acquire) != generation {
            return None;
        }
        let phase = self.status_snapshot().status;
        let status = match phase {
            MatchingServerPhase::Starting => {
                Some(self.transition_locked(MatchingServerPhase::Failed))
            }
            MatchingServerPhase::Ready => {
                Some(self.transition_locked(MatchingServerPhase::Disconnected))
            }
            _ => None,
        };
        self.generation.fetch_add(1, Ordering::AcqRel);
        cleanup();
        status
    }

    fn transition_locked(&self, next: MatchingServerPhase) -> MatchingServerStatus {
        let mut status = self
            .status
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if status.status != next {
            status.status = next;
            status.version += 1;
        }
        status.clone()
    }
}

impl Default for MatchingState {
    fn default() -> Self {
        Self {
            child: Arc::new(Mutex::new(None)),
            cmd_tx: Arc::new(Mutex::new(None)),
            responses: Arc::new(AsyncMutex::new(None)),
            request_lock: Arc::new(AsyncMutex::new(())),
            generation: Arc::new(AtomicU64::new(0)),
            lifecycle: Arc::new(Mutex::new(())),
            status: Arc::new(Mutex::new(MatchingServerStatus {
                status: MatchingServerPhase::Idle,
                version: 0,
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::{MatchingServerPhase, MatchingState};

    #[test]
    fn status_contract_versions_transitions_and_rejects_stale_generations() {
        let state = MatchingState::default();
        assert_eq!(state.status_snapshot().status, MatchingServerPhase::Idle);
        assert_eq!(state.status_snapshot().version, 0);
        assert_eq!(
            serde_json::to_value(state.status_snapshot()).expect("status should serialize"),
            serde_json::json!({"status":"idle","version":0})
        );

        let (first_generation, starting) = state.begin_start();
        assert_eq!(starting.status, MatchingServerPhase::Starting);
        assert_eq!(starting.version, 1);
        assert!(state
            .transition_if(
                first_generation,
                MatchingServerPhase::Starting,
                MatchingServerPhase::Ready
            )
            .is_some());

        let disconnected = state
            .reader_terminated(first_generation, || {})
            .expect("current reader should publish disconnect");
        assert_eq!(disconnected.status, MatchingServerPhase::Disconnected);
        assert_eq!(disconnected.version, 3);

        let (second_generation, starting) = state.begin_start();
        assert_eq!(starting.status, MatchingServerPhase::Starting);
        assert_eq!(starting.version, 4);
        assert!(state
            .transition_if(
                first_generation,
                MatchingServerPhase::Disconnected,
                MatchingServerPhase::Disconnected
            )
            .is_none());
        assert_eq!(
            state.status_snapshot().status,
            MatchingServerPhase::Starting
        );
        assert_eq!(state.generation.load(Ordering::Acquire), second_generation);
        let failed = state
            .transition_if(
                second_generation,
                MatchingServerPhase::Starting,
                MatchingServerPhase::Failed,
            )
            .expect("startup failure should be published");
        assert_eq!(failed.version, 5);
        let idle = state
            .invalidate(Some(second_generation), MatchingServerPhase::Idle, || {})
            .expect("intentional reset should invalidate current generation");
        assert_eq!(idle.status, MatchingServerPhase::Idle);
        assert_eq!(idle.version, 6);
        assert!(state.reader_terminated(second_generation, || {}).is_none());
        assert_eq!(state.status_snapshot(), idle);
    }
}
