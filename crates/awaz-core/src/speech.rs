use crate::AudioChunk;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpeechEvent {
    Partial(String),
    Final(String),
}

#[derive(Debug, Error)]
pub enum RecognizerError {
    #[error("recognizer unavailable: {0}")]
    Unavailable(String),
    #[error("recognizer operation failed: {0}")]
    Operation(String),
}

/// Provider-neutral speech recognizer contract.
///
/// Implementations own model state but never own microphone/device capture.
pub trait Recognizer: Send + 'static {
    fn start(&mut self) -> Result<(), RecognizerError>;
    fn push_audio(&mut self, chunk: &AudioChunk) -> Result<(), RecognizerError>;
    fn poll(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError>;
    fn finish(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError>;
    fn cancel(&mut self) -> Result<(), RecognizerError>;

    fn set_keyterms(&mut self, _terms: &[String]) -> Result<(), RecognizerError> {
        Ok(())
    }

    fn set_context(&mut self, _context: &str) -> Result<(), RecognizerError> {
        Ok(())
    }
}
