mod ffi;

use awaz_core::{AudioChunk, Recognizer, RecognizerError, SpeechEvent};
use ffi::*;
use std::{
    cell::Cell,
    ffi::{CStr, CString, c_char, c_void},
    marker::PhantomData,
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

/// Root directory for cached files. Unix (including macOS) uses
/// `$XDG_CACHE_HOME` or `~/.cache`; Windows uses `%LOCALAPPDATA%`.
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

pub fn default_model_dir(language: &str, size: ModelSize) -> Option<PathBuf> {
    cache_root().map(|root| {
        root.join("awaz/models/moonshine")
            .join(language)
            .join(size.slug())
    })
}

pub struct MoonshineRecognizer {
    transcriber: i32,
    stream: i32,
    last_rendered: String,
    active: bool,
    // Native handles use mutable state and require synchronized access.
    _not_sync: PhantomData<Cell<()>>,
}

impl MoonshineRecognizer {
    pub fn load(model_dir: impl AsRef<Path>, model: ModelSize) -> Result<Self, RecognizerError> {
        let path = CString::new(model_dir.as_ref().to_string_lossy().as_bytes())
            .map_err(|err| RecognizerError::Unavailable(err.to_string()))?;
        // SAFETY: `path` is a live C string. Null options are valid when the count is zero.
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

        // SAFETY: A nonnegative load result is a live transcriber handle.
        let stream = unsafe { moonshine_create_stream(transcriber, 0) };
        if stream < 0 {
            // SAFETY: Stream creation failed, but the transcriber handle remains live.
            unsafe { moonshine_free_transcriber(transcriber) };
            return Err(RecognizerError::Unavailable(error_string(stream)));
        }

        Ok(Self {
            transcriber,
            stream,
            last_rendered: String::new(),
            active: false,
            _not_sync: PhantomData,
        })
    }

    pub fn library_version() -> i32 {
        // SAFETY: This function takes no pointers and reads library metadata.
        unsafe { moonshine_get_version() }
    }

    /// Returns the download manifest JSON for `language` and `model` from the
    /// Moonshine library. The caller parses it to fetch model files.
    pub fn model_manifest(language: &str, model: ModelSize) -> Result<String, RecognizerError> {
        let language =
            CString::new(language).map_err(|err| RecognizerError::Operation(err.to_string()))?;
        let arch = CString::new(model.arch().to_string())
            .map_err(|err| RecognizerError::Operation(err.to_string()))?;
        let option_name = CString::new("model_arch")
            .map_err(|err| RecognizerError::Operation(err.to_string()))?;
        let options = [MoonshineOption {
            name: option_name.as_ptr(),
            value: arch.as_ptr(),
        }];

        let mut out: *mut c_char = ptr::null_mut();
        // SAFETY: All C strings are live for the call. `out` is a valid output
        // pointer; on success Moonshine writes a malloc'd, NUL-terminated string.
        check(unsafe {
            moonshine_get_stt_dependencies(
                language.as_ptr(),
                options.as_ptr(),
                options.len() as u64,
                &mut out,
            )
        })?;
        if out.is_null() {
            return Err(RecognizerError::Unavailable("empty model manifest".into()));
        }

        // SAFETY: `out` is non-null and NUL-terminated after a successful call.
        let json = unsafe { CStr::from_ptr(out) }
            .to_string_lossy()
            .into_owned();
        // SAFETY: Moonshine allocated `out` with malloc; free() releases it.
        unsafe { free(out as *mut c_void) };
        Ok(json)
    }

    fn read_transcript(&mut self) -> Result<Vec<SpeechEvent>, RecognizerError> {
        let mut transcript: *mut Transcript = ptr::null_mut();
        // SAFETY: Both handles are live and `transcript` is writable for the call.
        let code = unsafe {
            moonshine_transcribe_stream(self.transcriber, self.stream, 0, &mut transcript)
        };
        check(code)?;

        if transcript.is_null() {
            return Ok(Vec::new());
        }

        // SAFETY: A successful call returned a non-null transcript owned by the transcriber.
        // No other transcriber call occurs while this reference is in use.
        let transcript = unsafe { &*transcript };
        let lines = if transcript.lines.is_null() || transcript.line_count == 0 {
            &[][..]
        } else {
            // SAFETY: Moonshine guarantees `line_count` initialized entries when `lines` is set.
            unsafe { slice::from_raw_parts(transcript.lines, transcript.line_count as usize) }
        };

        let mut complete = Vec::new();
        let mut incomplete = Vec::new();
        for line in lines {
            if line.text.is_null() {
                continue;
            }

            // SAFETY: Moonshine owns a null-terminated string valid until the next API call.
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

        // SAFETY: Both handles remain live for the recognizer lifetime.
        check(unsafe { moonshine_start_stream(self.transcriber, self.stream) })?;
        self.last_rendered.clear();
        self.active = true;
        Ok(())
    }

    fn push_audio(&mut self, chunk: &AudioChunk) -> Result<(), RecognizerError> {
        if !self.active {
            return Ok(());
        }

        // SAFETY: Both handles are live. The sample pointer is valid for `len` elements.
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

        // SAFETY: Both handles are live and the stream is active.
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
            // SAFETY: Both handles are live and the stream is active.
            check(unsafe { moonshine_stop_stream(self.transcriber, self.stream) })?;
            // Drain and discard residual data so the stream can be started fresh.
            let _ = self.read_transcript();
        }
        self.active = false;
        self.last_rendered.clear();
        Ok(())
    }

    fn set_keyterms(&mut self, terms: &[String]) -> Result<(), RecognizerError> {
        if terms.iter().any(|term| term.contains(',')) {
            return Err(RecognizerError::Operation(
                "Moonshine key terms must not contain commas".into(),
            ));
        }

        let joined = terms.join(",");
        let keyterms =
            CString::new(joined).map_err(|err| RecognizerError::Operation(err.to_string()))?;
        // SAFETY: The handle is live and `keyterms` remains valid for the call.
        check(unsafe { moonshine_transcriber_set_keyterms(self.transcriber, keyterms.as_ptr()) })
    }

    fn set_context(&mut self, context: &str) -> Result<(), RecognizerError> {
        let context =
            CString::new(context).map_err(|err| RecognizerError::Operation(err.to_string()))?;
        // SAFETY: The handle is live and `context` remains valid for the call.
        check(unsafe { moonshine_transcriber_set_context(self.transcriber, context.as_ptr(), 200) })
    }
}

impl Drop for MoonshineRecognizer {
    fn drop(&mut self) {
        // SAFETY: This recognizer owns both live handles and drops them exactly once.
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
    // SAFETY: The function accepts every Moonshine status code.
    let message = unsafe { moonshine_error_to_string(code) };
    if message.is_null() {
        format!("Moonshine error {code}")
    } else {
        // SAFETY: Moonshine returned a non-null, null-terminated error string.
        unsafe { CStr::from_ptr(message) }
            .to_string_lossy()
            .into_owned()
    }
}
