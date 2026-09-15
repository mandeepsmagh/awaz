use awaz_core::{AudioChunk, Recognizer, RecognizerError, SpeechEvent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityExpectation {
    Supported,
    Unsupported,
}

#[derive(Debug, Clone, Copy)]
pub struct ProviderExpectations {
    pub keyterms: CapabilityExpectation,
    pub context: CapabilityExpectation,
}

/// Run the provider-neutral recognizer lifecycle checks against one loaded model.
pub fn run_provider_conformance(
    recognizer: &mut dyn Recognizer,
    audio: &[AudioChunk],
    expectations: ProviderExpectations,
) -> Result<(), String> {
    let first = audio
        .first()
        .ok_or_else(|| "conformance audio must not be empty".to_owned())?;
    if first.samples.is_empty() {
        return Err("conformance audio chunks must contain samples".into());
    }

    check_capability(
        "key terms",
        recognizer.set_keyterms(&["Awaz".into()]),
        expectations.keyterms,
    )?;
    check_capability(
        "context",
        recognizer.set_context("Awaz tests local speech providers."),
        expectations.context,
    )?;

    recognizer
        .cancel()
        .map_err(|error| format!("cancel while inactive: {error}"))?;
    recognizer
        .push_audio(first)
        .map_err(|error| format!("push while inactive: {error}"))?;
    require_no_speech(
        "poll while inactive",
        recognizer
            .poll()
            .map_err(|error| format!("poll while inactive: {error}"))?,
    )?;
    require_empty_final(
        "finish while inactive",
        recognizer
            .finish()
            .map_err(|error| format!("finish while inactive: {error}"))?,
    )?;

    recognizer
        .start()
        .map_err(|error| format!("start silence: {error}"))?;
    recognizer
        .start()
        .map_err(|error| format!("duplicate start: {error}"))?;
    require_empty_final(
        "finish silence",
        recognizer
            .finish()
            .map_err(|error| format!("finish silence: {error}"))?,
    )?;

    recognizer
        .start()
        .map_err(|error| format!("start cancelled utterance: {error}"))?;
    recognizer
        .push_audio(first)
        .map_err(|error| format!("push cancelled utterance: {error}"))?;
    recognizer
        .cancel()
        .map_err(|error| format!("cancel active utterance: {error}"))?;
    require_no_speech(
        "poll after cancel",
        recognizer
            .poll()
            .map_err(|error| format!("poll after cancel: {error}"))?,
    )?;

    run_spoken_utterance(recognizer, audio, "restart after cancel")?;
    run_spoken_utterance(recognizer, audio, "repeated utterance")?;

    require_no_speech(
        "poll after finish",
        recognizer
            .poll()
            .map_err(|error| format!("poll after finish: {error}"))?,
    )?;
    require_empty_final(
        "duplicate finish",
        recognizer
            .finish()
            .map_err(|error| format!("duplicate finish: {error}"))?,
    )?;
    Ok(())
}

fn run_spoken_utterance(
    recognizer: &mut dyn Recognizer,
    audio: &[AudioChunk],
    label: &str,
) -> Result<(), String> {
    recognizer
        .start()
        .map_err(|error| format!("{label}: start: {error}"))?;
    for chunk in audio {
        recognizer
            .push_audio(chunk)
            .map_err(|error| format!("{label}: push audio: {error}"))?;
        let events = recognizer
            .poll()
            .map_err(|error| format!("{label}: poll: {error}"))?;
        if events
            .iter()
            .any(|event| matches!(event, SpeechEvent::Final(_)))
        {
            return Err(format!("{label}: provider emitted a final before finish"));
        }
    }

    let events = recognizer
        .finish()
        .map_err(|error| format!("{label}: finish: {error}"))?;
    let finals = final_texts(&events);
    if finals.len() != 1 {
        return Err(format!(
            "{label}: expected one final event, got {}",
            finals.len()
        ));
    }
    if finals[0].trim().is_empty() {
        return Err(format!("{label}: speech produced an empty final"));
    }
    Ok(())
}

fn check_capability(
    name: &str,
    result: Result<(), RecognizerError>,
    expectation: CapabilityExpectation,
) -> Result<(), String> {
    match (expectation, result) {
        (CapabilityExpectation::Supported, Ok(()))
        | (CapabilityExpectation::Unsupported, Err(RecognizerError::Unsupported(_))) => Ok(()),
        (CapabilityExpectation::Supported, Err(error)) => {
            Err(format!("expected supported {name}: {error}"))
        }
        (CapabilityExpectation::Unsupported, Ok(())) => {
            Err(format!("expected unsupported {name}, but it succeeded"))
        }
        (CapabilityExpectation::Unsupported, Err(error)) => Err(format!(
            "expected unsupported {name}, got another error: {error}"
        )),
    }
}

fn require_no_speech(label: &str, events: Vec<SpeechEvent>) -> Result<(), String> {
    if events.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{label}: expected no speech events, got {events:?}"
        ))
    }
}

fn require_empty_final(label: &str, events: Vec<SpeechEvent>) -> Result<(), String> {
    let finals = final_texts(&events);
    if finals.len() == 1 && finals[0].is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{label}: expected one empty final event, got {events:?}"
        ))
    }
}

fn final_texts(events: &[SpeechEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|event| match event {
            SpeechEvent::Final(text) => Some(text.as_str()),
            SpeechEvent::Partial(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeRecognizer {
        active: bool,
        has_audio: bool,
        omit_finals: bool,
    }

    impl Recognizer for FakeRecognizer {
        fn start(&mut self) -> Result<(), RecognizerError> {
            self.active = true;
            self.has_audio = false;
            Ok(())
        }

        fn push_audio(&mut self, chunk: &AudioChunk) -> Result<(), RecognizerError> {
            if self.active && !chunk.samples.is_empty() {
                self.has_audio = true;
            }
            Ok(())
        }

        fn poll(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
            Ok(Vec::new())
        }

        fn finish(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
            let text = if self.active && self.has_audio {
                "conforming transcript"
            } else {
                ""
            };
            self.active = false;
            self.has_audio = false;
            if self.omit_finals {
                Ok(Vec::new())
            } else {
                Ok(vec![SpeechEvent::Final(text.into())])
            }
        }

        fn cancel(&mut self) -> Result<(), RecognizerError> {
            self.active = false;
            self.has_audio = false;
            Ok(())
        }

        fn set_keyterms(&mut self, _terms: &[String]) -> Result<(), RecognizerError> {
            Ok(())
        }
    }

    #[test]
    fn shared_suite_accepts_a_conforming_recognizer() {
        let mut recognizer = FakeRecognizer::default();
        run_provider_conformance(&mut recognizer, &audio(), expectations()).unwrap();
    }

    #[test]
    fn shared_suite_rejects_a_missing_final_event() {
        let mut recognizer = FakeRecognizer {
            omit_finals: true,
            ..Default::default()
        };
        assert!(run_provider_conformance(&mut recognizer, &audio(), expectations()).is_err());
    }

    fn audio() -> [AudioChunk; 1] {
        [AudioChunk {
            samples: vec![0.25; 1_600],
            sample_rate: 16_000,
        }]
    }

    fn expectations() -> ProviderExpectations {
        ProviderExpectations {
            keyterms: CapabilityExpectation::Supported,
            context: CapabilityExpectation::Unsupported,
        }
    }
}
