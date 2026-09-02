use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Command {
    Hello,
    #[serde(rename = "listen.start")]
    ListenStart,
    #[serde(rename = "listen.stop")]
    ListenStop,
    #[serde(rename = "listen.cancel")]
    ListenCancel,
    #[serde(rename = "keyterms.set")]
    KeytermsSet {
        terms: Vec<String>,
    },
    #[serde(rename = "context.set")]
    ContextSet {
        text: String,
    },
    // Reserved duplex protocol. V1 responds with unsupported capability errors.
    #[serde(rename = "speak.start")]
    SpeakStart,
    #[serde(rename = "speak.text")]
    SpeakText {
        text: String,
    },
    #[serde(rename = "speak.end")]
    SpeakEnd,
    #[serde(rename = "speak.cancel")]
    SpeakCancel,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
    Ready {
        version: String,
        provider: String,
    },
    Capabilities {
        stt: bool,
        tts: bool,
    },
    #[serde(rename = "listen.started")]
    ListenStarted,
    #[serde(rename = "listen.cancelled")]
    ListenCancelled,
    #[serde(rename = "transcript.partial")]
    TranscriptPartial {
        text: String,
    },
    #[serde(rename = "transcript.final")]
    TranscriptFinal {
        text: String,
    },
    Error {
        code: String,
        message: String,
    },
    Shutdown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_uses_stable_dotted_command_names() {
        let json = serde_json::to_string(&Command::ListenStart).unwrap();
        assert!(json.contains("listen.start"));
    }
}
