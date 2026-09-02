mod ffi;

use awaz_core::{AudioChunk, Recognizer, RecognizerError, SpeechEvent};
use ffi::*;
use std::{
    ffi::{CStr, CString},
    path::{Path, PathBuf},
    ptr, slice,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSize {
    Tiny,
    Small,
    Medium,
}

impl ModelSize {
    pub fn arch(self) -> u32 {
        match self {
            Self::Tiny => MODEL_TINY_STREAMING,
            Self::Small => MODEL_SMALL_STREAMING,
            Self::Medium => MODEL_MEDIUM_STREAMING,
        }
    }

    pub fn slug(self) -> &'static str {
        match self {
            Self::Tiny => "tiny-streaming",
            Self::Small => "small-streaming",
            Self::Medium => "medium-streaming",
        }
    }
}

impl std::str::FromStr for ModelSize {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "tiny" | "tiny-streaming" => Ok(Self::Tiny),
            "small" | "small-streaming" => Ok(Self::Small),
            "medium" | "medium-streaming" => Ok(Self::Medium),
            _ => Err(format!("unknown Moonshine model size: {s}")),
        }
    }
}

pub fn default_model_dir(language: &str, size: ModelSize) -> Option<PathBuf> {
    directories::BaseDirs::new().map(|dirs| {
        dirs.cache_dir()
            .join("awaz/models/moonshine")
            .join(language)
            .join(size.slug())
    })
}

pub struct MoonshineRecognizer {
    transcriber: i32,
    stream: i32,
    last_rendered: String,
    active: bool,
}

impl MoonshineRecognizer {
    pub fn load(model_dir: impl AsRef<Path>, model: ModelSize) -> Result<Self, RecognizerError> {
        let path = CString::new(model_dir.as_ref().to_string_lossy().as_bytes())
            .map_err(|err| RecognizerError::Unavailable(err.to_string()))?;
        let transcriber = unsafe {
            moonshine_load_transcriber_from_files(
                path.as_ptr(),
                model.arch(),
                ptr::null(),
                0,
                MOONSHINE_HEADER_VERSION,
            )
        };
        if transcriber < 0 {
            return Err(RecognizerError::Unavailable(error_string(transcriber)));
        }

        let stream = unsafe { moonshine_create_stream(transcriber, 0) };
        if stream < 0 {
            unsafe { moonshine_free_transcriber(transcriber) };
            return Err(RecognizerError::Unavailable(error_string(stream)));
        }

        Ok(Self {
            transcriber,
            stream,
            last_rendered: String::new(),
            active: false,
        })
    }

    pub fn library_version() -> i32 {
        unsafe { moonshine_get_version() }
    }

    fn read_transcript(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
        let mut transcript: *mut Transcript = ptr::null_mut();
        let code = unsafe {
            moonshine_transcribe_stream(self.transcriber, self.stream, 0, &mut transcript)
        };
        check(code)?;

        if transcript.is_null() {
            return Ok(Vec::new());
        }

        let transcript = unsafe { &*transcript };
        let lines = if transcript.lines.is_null() || transcript.line_count == 0 {
            &[][..]
        } else {
            unsafe { slice::from_raw_parts(transcript.lines, transcript.line_count as usize) }
        };

        let mut complete = Vec::new();
        let mut incomplete = Vec::new();
        for line in lines {
            if line.text.is_null() {
                continue;
            }

            let text = unsafe { CStr::from_ptr(line.text) }
                .to_string_lossy()
                .trim()
                .to_owned();
            if text.is_empty() {
                continue;
            }

            if line.is_complete != 0 {
                complete.push(text);
            } else {
                incomplete.push(text);
            }
        }

        let mut rendered = complete.join(" ");
        if !incomplete.is_empty() {
            if !rendered.is_empty() {
                rendered.push(' ');
            }
            rendered.push_str(&incomplete.join(" "));
        }

        if rendered.is_empty() || rendered == self.last_rendered {
            return Ok(Vec::new());
        }

        self.last_rendered = rendered.clone();
        Ok(vec![SpeechEvent::Partial(rendered)])
    }
}

impl Recognizer for MoonshineRecognizer {
    fn start(&mut self) -> Result<(), RecognizerError> {
        if self.active {
            return Ok(());
        }

        check(unsafe { moonshine_start_stream(self.transcriber, self.stream) })?;
        self.last_rendered.clear();
        self.active = true;
        Ok(())
    }

    fn push_audio(&mut self, chunk: &AudioChunk) -> Result<(), RecognizerError> {
        if !self.active {
            return Ok(());
        }

        check(unsafe {
            moonshine_transcribe_add_audio_to_stream(
                self.transcriber,
                self.stream,
                chunk.samples.as_ptr(),
                chunk.samples.len() as u64,
                chunk.sample_rate as i32,
                0,
            )
        })
    }

    fn poll(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
        if !self.active {
            return Ok(Vec::new());
        }
        self.read_transcript()
    }

    fn finish(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
        if !self.active {
            return Ok(vec![SpeechEvent::Final(String::new())]);
        }

        check(unsafe { moonshine_stop_stream(self.transcriber, self.stream) })?;
        let mut events = self.read_transcript()?;
        let final_text = self.last_rendered.trim().to_owned();

        // One stop always terminates one utterance. Emit exactly one Final event,
        // including an empty string for silence, so integrations can leave their
        // finalizing state deterministically.
        events.retain(|event| !matches!(event, SpeechEvent::Partial(_)));
        events.push(SpeechEvent::Final(final_text));
        self.active = false;
        self.last_rendered.clear();
        Ok(events)
    }

    fn cancel(&mut self) -> Result<(), RecognizerError> {
        if self.active {
            check(unsafe { moonshine_stop_stream(self.transcriber, self.stream) })?;
            // Drain and discard residual data so the stream can be started fresh.
            let _ = self.read_transcript();
        }
        self.active = false;
        self.last_rendered.clear();
        Ok(())
    }

    fn set_keyterms(&mut self, terms: &[String]) -> Result<(), RecognizerError> {
        let joined = terms.join(",");
        let keyterms =
            CString::new(joined).map_err(|err| RecognizerError::Operation(err.to_string()))?;
        check(unsafe { moonshine_transcriber_set_keyterms(self.transcriber, keyterms.as_ptr()) })
    }

    fn set_context(&mut self, context: &str) -> Result<(), RecognizerError> {
        let context =
            CString::new(context).map_err(|err| RecognizerError::Operation(err.to_string()))?;
        check(unsafe { moonshine_transcriber_set_context(self.transcriber, context.as_ptr(), 200) })
    }
}

impl Drop for MoonshineRecognizer {
    fn drop(&mut self) {
        unsafe {
            let _ = moonshine_free_stream(self.transcriber, self.stream);
            moonshine_free_transcriber(self.transcriber);
        }
    }
}

fn check(code: i32) -> Result<(), RecognizerError> {
    if code == 0 {
        Ok(())
    } else {
        Err(RecognizerError::Operation(error_string(code)))
    }
}

fn error_string(code: i32) -> String {
    let message = unsafe { moonshine_error_to_string(code) };
    if message.is_null() {
        format!("Moonshine error {code}")
    } else {
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    }
}
