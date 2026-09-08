use std::sync::{Arc, Mutex};

pub const SLOT_COUNT: u8 = 12;
pub const STEPS_PER_REVOLUTION: i64 = 3_200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    Unknown,
    Confirmed { slot: u8, steps: i64 },
}

#[derive(Clone)]
pub struct TurntableState {
    pub position: Arc<Mutex<Position>>,
    pub movement: Arc<tokio::sync::Mutex<()>>,
}

impl Default for TurntableState {
    fn default() -> Self {
        Self {
            position: Arc::new(Mutex::new(Position::Unknown)),
            movement: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}

pub fn target_steps(slot: u8) -> Option<i64> {
    (slot < SLOT_COUNT).then(|| i64::from(slot) * STEPS_PER_REVOLUTION / i64::from(SLOT_COUNT))
}

impl TurntableState {
    pub fn position(&self) -> Position {
        *self
            .position
            .lock()
            .expect("turntable position mutex poisoned")
    }
    pub fn set(&self, position: Position) {
        *self
            .position
            .lock()
            .expect("turntable position mutex poisoned") = position;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slot_targets_use_one_integer_revolution() {
        assert_eq!(target_steps(0), Some(0));
        assert_eq!(target_steps(1), Some(266));
        assert_eq!(target_steps(6), Some(1_600));
        assert_eq!(target_steps(11), Some(2_933));
        assert_eq!(target_steps(12), None);
    }
    #[test]
    fn state_can_be_confirmed_or_invalidated() {
        let state = TurntableState::default();
        assert_eq!(state.position(), Position::Unknown);
        state.set(Position::Confirmed {
            slot: 3,
            steps: 800,
        });
        assert_eq!(
            state.position(),
            Position::Confirmed {
                slot: 3,
                steps: 800
            }
        );
        state.set(Position::Unknown);
        assert_eq!(state.position(), Position::Unknown);
    }
}
