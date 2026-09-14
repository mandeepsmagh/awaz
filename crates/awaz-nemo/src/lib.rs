mod ffi;

use awaz_core::{AudioChunk, Recognizer, RecognizerError, SpeechEvent};
use ffi::*;
use std::{
    cell::Cell,
    ffi::{CStr, CString, c_char},
    marker::PhantomData,
    path::{Path, PathBuf},
    ptr,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum NemoModel {
    #[default]
    Nemotron35,
    ParakeetTdtV3,
}

impl NemoModel {
    pub fn slug(self) -> &'static str {
        match self {
            Self::Nemotron35 => "nemotron-3.5",
            Self::ParakeetTdtV3 => "parakeet-tdt-v3",
        }
    }

    pub fn filename(self) -> &'static str {
        match self {
            Self::Nemotron35 => "nemotron-3.5-asr-streaming-0.6b.q8_0.gguf",
            Self::ParakeetTdtV3 => "parakeet-tdt-0.6b-v3.q8_0.gguf",
        }
    }

    pub fn repository(self) -> &'static str {
        match self {
            Self::Nemotron35 => "nvidia/nemotron-3.5-asr-streaming-0.6b",
            Self::ParakeetTdtV3 => "nvidia/parakeet-tdt-0.6b-v3",
        }
    }

    pub fn revision(self) -> &'static str {
        match self {
            Self::Nemotron35 => "1c8deaecc64b91f034d73e08dd8b64625eb3395d",
            Self::ParakeetTdtV3 => "541d1f99c6b0c3cd0b11a95167540bb8edefd82b",
        }
    }

    pub fn size(self) -> u64 {
        match self {
            Self::Nemotron35 => 741_548_352,
            Self::ParakeetTdtV3 => 713_975_456,
        }
    }

    pub fn sha256(self) -> &'static str {
        match self {
            Self::Nemotron35 => "a5c435f294eea8f88ce68dd27b8c3bfea7f777cb2fbba04fcd30eaa555f429ae",
            Self::ParakeetTdtV3 => {
                "e3880d0aaaaf2c308ea2c35016b2b895c423eb3fda924c1b463d1c19b7f4d32e"
            }
        }
    }

    pub fn download_url(self) -> String {
        format!(
            "https://huggingface.co/{}/resolve/{}/{}?download=true",
            self.repository(),
            self.revision(),
            self.filename()
        )
    }

    pub fn supports_streaming(self) -> bool {
        matches!(self, Self::Nemotron35)
    }
}

pub fn default_model_path(model: NemoModel) -> Option<PathBuf> {
    cache_root().map(|root| {
        root.join("awaz/models/nemo")
            .join(model.slug())
            .join(model.filename())
    })
}

fn cache_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.cache_dir().to_path_buf()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| directories::BaseDirs::new().map(|dirs| dirs.home_dir().join(".cache")))
    }
}

pub struct NemoRecognizer {
    recognizer: *mut NemoSpeechAsrRecognizer,
    stream: *mut NemoSpeechAsrStream,
    model: NemoModel,
    language: CString,
    keyterms: Vec<String>,
    buffered_audio: Vec<f32>,
    sample_rate: Option<u32>,
    active: bool,
    _not_sync: PhantomData<Cell<()>>,
}

// SAFETY: NeMo recognizer and stream handles are owned by this value. Awaz moves
// the value to one worker thread and never accesses a handle from two threads.
unsafe impl Send for NemoRecognizer {}

impl NemoRecognizer {
    pub fn load(
        model_path: impl AsRef<Path>,
        model: NemoModel,
        language: &str,
    ) -> Result<Self, RecognizerError> {
        let path = CString::new(model_path.as_ref().to_string_lossy().as_bytes())
            .map_err(|error| RecognizerError::Unavailable(error.to_string()))?;
        let language = CString::new(language)
            .map_err(|error| RecognizerError::Unavailable(error.to_string()))?;
        let backend = NemoSpeechAsrBackendConfig {
            size: size_of::<NemoSpeechAsrBackendConfig>(),
            gpu: if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
                0
            } else {
                -1
            },
        };
        let model_config = NemoSpeechAsrModelConfig {
            size: size_of::<NemoSpeechAsrModelConfig>(),
            path: path.as_ptr(),
            name: ptr::null(),
        };
        let config = NemoSpeechAsrRecognizerConfig {
            size: size_of::<NemoSpeechAsrRecognizerConfig>(),
            backend: &backend,
            model: &model_config,
            streaming: ptr::null(),
            decoder: ptr::null(),
            vad: ptr::null(),
            endpointing: ptr::null(),
            postproc: ptr::null(),
            diar: ptr::null(),
            batching: ptr::null(),
        };
        let mut recognizer = ptr::null_mut();
        // SAFETY: All config pointers and C strings are valid for the call. The
        // SDK copies startup configuration and writes one owned handle to `out`.
        check(
            unsafe { nemo_speech_asr_create(&config, &mut recognizer) },
            true,
        )?;
        if recognizer.is_null() {
            return Err(RecognizerError::Unavailable(
                "NeMo Speech returned an empty recognizer".into(),
            ));
        }

        Ok(Self {
            recognizer,
            stream: ptr::null_mut(),
            model,
            language,
            keyterms: Vec::new(),
            buffered_audio: Vec::new(),
            sample_rate: None,
            active: false,
            _not_sync: PhantomData,
        })
    }

    pub fn library_version() -> String {
        // SAFETY: The SDK returns a static null-terminated version string.
        let version = unsafe { nemo_speech_asr_version() };
        c_string(version).unwrap_or_else(|| "unknown".into())
    }

    fn with_options<T>(
        &self,
        interim_results: bool,
        operation: impl FnOnce(&NemoSpeechAsrRecognitionOptions) -> T,
    ) -> Result<T, RecognizerError> {
        let phrases = self
            .keyterms
            .iter()
            .map(|term| CString::new(term.as_str()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| RecognizerError::Operation(error.to_string()))?;
        let phrase_ptrs = phrases
            .iter()
            .map(|phrase| phrase.as_ptr())
            .collect::<Vec<*const c_char>>();
        let context = NemoSpeechAsrSpeechContext {
            size: size_of::<NemoSpeechAsrSpeechContext>(),
            phrases: phrase_ptrs.as_ptr(),
            phrase_count: phrase_ptrs.len(),
            boost: 2.5,
        };

        // SAFETY: This function has no input pointers and returns a POD value.
        let mut options = unsafe { nemo_speech_asr_recognition_options_default() };
        options.language_code = self.language.as_ptr();
        options.interim_results = interim_results;
        options.enable_automatic_punctuation = true;
        if !phrase_ptrs.is_empty() {
            options.speech_contexts = &context;
            options.speech_context_count = 1;
        }
        Ok(operation(&options))
    }

    fn next_events(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
        let mut events = Vec::new();
        loop {
            let mut result = ptr::null_mut();
            // SAFETY: `stream` is live and exclusively used on this thread. The
            // SDK writes either null or one result handle owned by the caller.
            check(
                unsafe { nemo_speech_asr_stream_next(self.stream, &mut result) },
                false,
            )?;
            if result.is_null() {
                break;
            }
            if let Some(event) = result_event(result) {
                events.push(event);
            }
            // SAFETY: The preceding call transferred this result handle to us.
            unsafe { nemo_speech_asr_result_destroy(result) };
        }
        Ok(events)
    }

    fn close_stream(&mut self) {
        if !self.stream.is_null() {
            // SAFETY: This value owns the live stream and closes it once.
            unsafe { nemo_speech_asr_stream_close(self.stream) };
            self.stream = ptr::null_mut();
        }
    }

    fn set_sample_rate(&mut self, sample_rate: u32) -> Result<(), RecognizerError> {
        if let Some(current) = self.sample_rate
            && current != sample_rate
        {
            return Err(RecognizerError::Operation(format!(
                "NeMo sample rate changed within an utterance: {current} to {sample_rate}"
            )));
        }
        self.sample_rate = Some(sample_rate);
        Ok(())
    }
}

impl Recognizer for NemoRecognizer {
    fn start(&mut self) -> Result<(), RecognizerError> {
        if self.active {
            return Ok(());
        }
        self.buffered_audio.clear();
        self.sample_rate = None;

        if self.model.supports_streaming() {
            let mut stream = ptr::null_mut();
            let status = self.with_options(true, |options| {
                // SAFETY: The recognizer is live, option pointers remain valid
                // for the call, and the SDK copies per-stream configuration.
                unsafe {
                    nemo_speech_asr_streaming_recognize(self.recognizer, options, &mut stream)
                }
            })?;
            check(status, false)?;
            if stream.is_null() {
                return Err(RecognizerError::Operation(
                    "NeMo Speech returned an empty stream".into(),
                ));
            }
            self.stream = stream;
        }
        self.active = true;
        Ok(())
    }

    fn push_audio(&mut self, chunk: &AudioChunk) -> Result<(), RecognizerError> {
        if !self.active {
            return Ok(());
        }
        self.set_sample_rate(chunk.sample_rate)?;
        if self.model.supports_streaming() {
            // SAFETY: The stream is live and samples remain valid for the call.
            check(
                unsafe {
                    nemo_speech_asr_stream_push_f32(
                        self.stream,
                        chunk.samples.as_ptr(),
                        chunk.samples.len(),
                        chunk.sample_rate as i32,
                    )
                },
                false,
            )
        } else {
            self.buffered_audio.extend_from_slice(&chunk.samples);
            Ok(())
        }
    }

    fn poll(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
        if !self.active || !self.model.supports_streaming() {
            return Ok(Vec::new());
        }
        self.next_events()
    }

    fn finish(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
        if !self.active {
            return Ok(vec![SpeechEvent::Final(String::new())]);
        }

        let events = if self.model.supports_streaming() {
            // SAFETY: The owned stream is active and receives finish once.
            check(unsafe { nemo_speech_asr_stream_finish(self.stream) }, false)?;
            let mut events = self.next_events()?;
            self.close_stream();
            if !events
                .iter()
                .any(|event| matches!(event, SpeechEvent::Final(_)))
            {
                events.push(SpeechEvent::Final(String::new()));
            }
            events
        } else if self.buffered_audio.is_empty() {
            vec![SpeechEvent::Final(String::new())]
        } else {
            let mut result = ptr::null_mut();
            let sample_rate = self.sample_rate.unwrap_or(16_000) as i32;
            let status = self.with_options(false, |options| {
                // SAFETY: The recognizer is live. Audio and options remain valid
                // for this synchronous call, which transfers any result to us.
                unsafe {
                    nemo_speech_asr_recognize_f32(
                        self.recognizer,
                        options,
                        self.buffered_audio.as_ptr(),
                        self.buffered_audio.len(),
                        sample_rate,
                        &mut result,
                    )
                }
            })?;
            check(status, false)?;
            let text = result_text(result).unwrap_or_default();
            if !result.is_null() {
                // SAFETY: The recognition call transferred this result to us.
                unsafe { nemo_speech_asr_result_destroy(result) };
            }
            vec![SpeechEvent::Final(text)]
        };

        self.active = false;
        self.buffered_audio.clear();
        self.sample_rate = None;
        Ok(events)
    }

    fn cancel(&mut self) -> Result<(), RecognizerError> {
        self.close_stream();
        self.buffered_audio.clear();
        self.sample_rate = None;
        self.active = false;
        Ok(())
    }

    fn set_keyterms(&mut self, terms: &[String]) -> Result<(), RecognizerError> {
        if !self.model.supports_streaming() {
            return Err(RecognizerError::Unsupported(
                "key terms with Parakeet TDT v3",
            ));
        }
        if self.active {
            return Err(RecognizerError::Operation(
                "cannot change NeMo key terms during an utterance".into(),
            ));
        }
        self.keyterms = terms.to_vec();
        Ok(())
    }
}

impl Drop for NemoRecognizer {
    fn drop(&mut self) {
        self.close_stream();
        if !self.recognizer.is_null() {
            // SAFETY: This value owns the live recognizer and destroys it once.
            unsafe { nemo_speech_asr_destroy(self.recognizer) };
        }
    }
}

fn result_event(result: *mut NemoSpeechAsrResult) -> Option<SpeechEvent> {
    let text = result_text(result)?;
    // SAFETY: `result` is live for both accessor calls.
    if unsafe { nemo_speech_asr_result_is_final(result) } {
        Some(SpeechEvent::Final(text))
    } else {
        Some(SpeechEvent::Partial(text))
    }
}

fn result_text(result: *mut NemoSpeechAsrResult) -> Option<String> {
    if result.is_null() {
        return None;
    }
    // SAFETY: `result` is live and owned by the caller.
    if unsafe { nemo_speech_asr_result_alternative_count(result) } == 0 {
        return Some(String::new());
    }
    // SAFETY: Alternative zero exists and its string lives until result destroy.
    c_string(unsafe { nemo_speech_asr_result_transcript(result, 0) })
        .map(|text| text.trim().to_owned())
}

fn check(status: i32, loading: bool) -> Result<(), RecognizerError> {
    if status == NEMO_SPEECH_ASR_OK {
        return Ok(());
    }
    // SAFETY: The SDK returns a thread-local null-terminated error string.
    let message = c_string(unsafe { nemo_speech_asr_last_error() })
        .unwrap_or_else(|| format!("NeMo Speech error {status}"));
    if loading {
        Err(RecognizerError::Unavailable(message))
    } else {
        Err(RecognizerError::Operation(message))
    }
}

fn c_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        None
    } else {
        // SAFETY: Callers pass SDK-owned pointers documented as null-terminated.
        Some(
            unsafe { CStr::from_ptr(value) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_metadata_matches_pinned_index() {
        assert_eq!(NemoModel::Nemotron35.size(), 741_548_352);
        assert!(NemoModel::Nemotron35.supports_streaming());
        assert_eq!(NemoModel::ParakeetTdtV3.size(), 713_975_456);
        assert!(!NemoModel::ParakeetTdtV3.supports_streaming());
        assert!(
            NemoModel::ParakeetTdtV3
                .download_url()
                .starts_with("https://")
        );
    }
}
