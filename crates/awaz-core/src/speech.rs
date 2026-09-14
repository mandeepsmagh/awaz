use crate::AudioChunk;
use std::sync::atomic::{AtomicBool, Ordering};
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
    #[error("recognizer does not support {0}")]
    Unsupported(&'static str),
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

    fn finish_cancellable(
        &mut self,
        cancelled: &AtomicBool,
    ) -> Result<Vec<SpeechEvent>, RecognizerError> {
        if cancelled.load(Ordering::Acquire) {
            self.cancel()?;
            Ok(Vec::new())
        } else {
            self.finish()
        }
    }

    fn set_keyterms(&mut self, _terms: &[String]) -> Result<(), RecognizerError> {
        Err(RecognizerError::Unsupported("key terms"))
    }

    fn set_context(&mut self, _context: &str) -> Result<(), RecognizerError> {
        Err(RecognizerError::Unsupported("context"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BasicRecognizer;

    impl Recognizer for BasicRecognizer {
        fn start(&mut self) -> Result<(), RecognizerError> {
            Ok(())
        }

        fn push_audio(&mut self, _chunk: &AudioChunk) -> Result<(), RecognizerError> {
            Ok(())
        }

        fn poll(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
            Ok(Vec::new())
        }

        fn finish(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
            Ok(Vec::new())
        }

        fn cancel(&mut self) -> Result<(), RecognizerError> {
            Ok(())
        }
    }

    #[test]
    fn optional_customization_is_unsupported_by_default() {
        let mut recognizer = BasicRecognizer;
        assert!(matches!(
            recognizer.set_keyterms(&[]),
            Err(RecognizerError::Unsupported("key terms"))
        ));
        assert!(matches!(
            recognizer.set_context(""),
            Err(RecognizerError::Unsupported("context"))
        ));
    }
}
