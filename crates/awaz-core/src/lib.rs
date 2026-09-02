pub mod audio;
pub mod protocol;
pub mod speech;
pub mod state;

pub use audio::AudioChunk;
pub use protocol::{Command, Event};
pub use speech::{Recognizer, RecognizerError, SpeechEvent};
pub use state::{TransitionError, VoiceState};
