use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VoiceState {
    Idle,
    Listening,
    Finalizing,
    /// Reserved for future TTS. Intentionally unreachable in v1.
    Speaking,
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("invalid voice transition: {from:?} -> {to:?}")]
pub struct TransitionError {
    pub from: VoiceState,
    pub to: VoiceState,
}

impl VoiceState {
    pub fn can_transition_to(self, to: VoiceState) -> bool {
        matches!(
            (self, to),
            (VoiceState::Idle, VoiceState::Listening)
                | (VoiceState::Listening, VoiceState::Finalizing)
                | (VoiceState::Listening, VoiceState::Idle)
                | (VoiceState::Finalizing, VoiceState::Idle)
                // Future duplex path, defined now so the controller contract is stable.
                | (VoiceState::Idle, VoiceState::Speaking)
                | (VoiceState::Speaking, VoiceState::Idle)
                | (VoiceState::Speaking, VoiceState::Listening)
        )
    }

    pub fn transition(&mut self, to: VoiceState) -> Result<(), TransitionError> {
        if self.can_transition_to(to) {
            *self = to;
            Ok(())
        } else {
            Err(TransitionError { from: *self, to })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_push_to_talk_cycle_is_valid() {
        let mut state = VoiceState::Idle;
        state.transition(VoiceState::Listening).unwrap();
        state.transition(VoiceState::Finalizing).unwrap();
        state.transition(VoiceState::Idle).unwrap();
        assert_eq!(state, VoiceState::Idle);
    }

    #[test]
    fn invalid_transition_is_rejected() {
        let mut state = VoiceState::Idle;
        assert!(state.transition(VoiceState::Finalizing).is_err());
    }
}
