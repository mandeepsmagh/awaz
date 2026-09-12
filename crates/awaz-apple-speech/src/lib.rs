use awaz_core::{AudioChunk, Recognizer, RecognizerError, SpeechEvent};

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use crossbeam_channel::{Receiver, bounded};
    use serde_json::Value;
    use std::{
        io::{BufRead, BufReader, Write},
        path::PathBuf,
        process::{Child, ChildStdin, Command, Stdio},
        thread,
        time::Duration,
    };

    const START: u8 = 1;
    const AUDIO: u8 = 2;
    const FINISH: u8 = 3;
    const CANCEL: u8 = 4;
    const SHUTDOWN: u8 = 5;

    enum HelperEvent {
        Ready,
        Started,
        Partial(String),
        Final(String),
        Finished,
        Cancelled,
        Error(String),
    }

    pub struct AppleSpeechRecognizer {
        child: Child,
        input: ChildStdin,
        events: Receiver<HelperEvent>,
        active: bool,
    }

    impl AppleSpeechRecognizer {
        pub fn load(language: &str) -> Result<Self, RecognizerError> {
            let helper = helper_path();
            let mut child = Command::new(&helper)
                .arg(language)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
                .map_err(|error| {
                    RecognizerError::Unavailable(format!(
                        "cannot start Apple Speech helper {}: {error}",
                        helper.display()
                    ))
                })?;
            let input = child.stdin.take().ok_or_else(|| {
                RecognizerError::Unavailable("Apple Speech stdin unavailable".into())
            })?;
            let output = child.stdout.take().ok_or_else(|| {
                RecognizerError::Unavailable("Apple Speech stdout unavailable".into())
            })?;
            let (sender, events) = bounded(64);
            thread::spawn(move || {
                for line in BufReader::new(output).lines() {
                    let event = match line {
                        Ok(line) => parse_event(&line),
                        Err(error) => HelperEvent::Error(error.to_string()),
                    };
                    if sender.send(event).is_err() {
                        break;
                    }
                }
            });

            let recognizer = Self {
                child,
                input,
                events,
                active: false,
            };
            match recognizer
                .events
                .recv_timeout(Duration::from_secs(600))
                .map_err(|error| RecognizerError::Unavailable(error.to_string()))?
            {
                HelperEvent::Ready => Ok(recognizer),
                HelperEvent::Error(message) => Err(RecognizerError::Unavailable(message)),
                _ => Err(RecognizerError::Unavailable(
                    "unexpected response from Apple Speech helper".into(),
                )),
            }
        }

        fn send(&mut self, command: u8, payload: &[u8]) -> Result<(), RecognizerError> {
            let payload_len = u32::try_from(payload.len()).map_err(|_| {
                RecognizerError::Operation("Apple Speech audio chunk is too large".into())
            })?;
            self.input
                .write_all(&[command])
                .and_then(|()| self.input.write_all(&payload_len.to_le_bytes()))
                .and_then(|()| self.input.write_all(payload))
                .and_then(|()| self.input.flush())
                .map_err(|error| RecognizerError::Operation(error.to_string()))
        }

        fn wait_for(&self, expected: fn(&HelperEvent) -> bool) -> Result<(), RecognizerError> {
            loop {
                match self.events.recv() {
                    Ok(event) if expected(&event) => return Ok(()),
                    Ok(HelperEvent::Error(message)) => {
                        return Err(RecognizerError::Operation(message));
                    }
                    Ok(_) => {}
                    Err(error) => return Err(RecognizerError::Operation(error.to_string())),
                }
            }
        }
    }

    impl Recognizer for AppleSpeechRecognizer {
        fn start(&mut self) -> Result<(), RecognizerError> {
            if self.active {
                return Ok(());
            }
            self.send(START, &[])?;
            self.wait_for(|event| matches!(event, HelperEvent::Started))?;
            self.active = true;
            Ok(())
        }

        fn push_audio(&mut self, chunk: &AudioChunk) -> Result<(), RecognizerError> {
            if !self.active {
                return Ok(());
            }
            let mut payload = Vec::with_capacity(4 + chunk.samples.len() * 4);
            payload.extend_from_slice(&chunk.sample_rate.to_le_bytes());
            for sample in &chunk.samples {
                payload.extend_from_slice(&sample.to_le_bytes());
            }
            self.send(AUDIO, &payload)
        }

        fn poll(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
            let mut speech = Vec::new();
            while let Ok(event) = self.events.try_recv() {
                match event {
                    HelperEvent::Partial(text) => speech.push(SpeechEvent::Partial(text)),
                    HelperEvent::Final(text) => speech.push(SpeechEvent::Final(text)),
                    HelperEvent::Error(message) => {
                        return Err(RecognizerError::Operation(message));
                    }
                    _ => {}
                }
            }
            Ok(speech)
        }

        fn finish(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
            if !self.active {
                return Ok(vec![SpeechEvent::Final(String::new())]);
            }
            self.send(FINISH, &[])?;
            let mut speech = Vec::new();
            loop {
                match self.events.recv() {
                    Ok(HelperEvent::Partial(text)) => speech.push(SpeechEvent::Partial(text)),
                    Ok(HelperEvent::Final(text)) => speech.push(SpeechEvent::Final(text)),
                    Ok(HelperEvent::Finished) => break,
                    Ok(HelperEvent::Error(message)) => {
                        return Err(RecognizerError::Operation(message));
                    }
                    Ok(_) => {}
                    Err(error) => return Err(RecognizerError::Operation(error.to_string())),
                }
            }
            self.active = false;
            Ok(speech)
        }

        fn cancel(&mut self) -> Result<(), RecognizerError> {
            if self.active {
                self.send(CANCEL, &[])?;
                self.wait_for(|event| matches!(event, HelperEvent::Cancelled))?;
            }
            self.active = false;
            Ok(())
        }
    }

    impl Drop for AppleSpeechRecognizer {
        fn drop(&mut self) {
            let _ = self.send(SHUTDOWN, &[]);
            let _ = self.child.wait();
        }
    }

    fn helper_path() -> PathBuf {
        if let Ok(executable) = std::env::current_exe()
            && let Some(directory) = executable.parent()
        {
            let sibling = directory.join("awaz-apple-speech");
            if sibling.exists() {
                return sibling;
            }
        }
        PathBuf::from(env!("AWAZ_APPLE_SPEECH_BUILD_HELPER"))
    }

    fn parse_event(line: &str) -> HelperEvent {
        let parsed = serde_json::from_str::<Value>(line);
        let Ok(value) = parsed else {
            return HelperEvent::Error("invalid output from Apple Speech helper".into());
        };
        let text = || {
            value
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        match value.get("type").and_then(Value::as_str) {
            Some("ready") => HelperEvent::Ready,
            Some("started") => HelperEvent::Started,
            Some("partial") => HelperEvent::Partial(text()),
            Some("final") => HelperEvent::Final(text()),
            Some("finished") => HelperEvent::Finished,
            Some("cancelled") => HelperEvent::Cancelled,
            Some("error") => HelperEvent::Error(
                value
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Apple Speech failed")
                    .to_owned(),
            ),
            _ => HelperEvent::Error("unknown output from Apple Speech helper".into()),
        }
    }
}

#[cfg(target_os = "macos")]
pub use platform::AppleSpeechRecognizer;

#[cfg(not(target_os = "macos"))]
pub struct AppleSpeechRecognizer;

#[cfg(not(target_os = "macos"))]
impl AppleSpeechRecognizer {
    pub fn load(_language: &str) -> Result<Self, RecognizerError> {
        Err(unavailable())
    }
}

#[cfg(not(target_os = "macos"))]
impl Recognizer for AppleSpeechRecognizer {
    fn start(&mut self) -> Result<(), RecognizerError> {
        Err(unavailable())
    }

    fn push_audio(&mut self, _chunk: &AudioChunk) -> Result<(), RecognizerError> {
        Err(unavailable())
    }

    fn poll(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
        Err(unavailable())
    }

    fn finish(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
        Err(unavailable())
    }

    fn cancel(&mut self) -> Result<(), RecognizerError> {
        Err(unavailable())
    }
}

#[cfg(not(target_os = "macos"))]
fn unavailable() -> RecognizerError {
    RecognizerError::Unavailable("Apple Speech requires macOS 26 or newer".into())
}
