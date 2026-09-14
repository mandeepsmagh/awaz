use std::ffi::{c_char, c_float, c_int};

pub const NEMO_SPEECH_ASR_OK: c_int = 0;

#[repr(C)]
pub struct NemoSpeechAsrRecognizer {
    _private: [u8; 0],
}

#[repr(C)]
pub struct NemoSpeechAsrStream {
    _private: [u8; 0],
}

#[repr(C)]
pub struct NemoSpeechAsrResult {
    _private: [u8; 0],
}

#[repr(C)]
pub struct NemoSpeechAsrBackendConfig {
    pub size: usize,
    pub gpu: i32,
}

#[repr(C)]
pub struct NemoSpeechAsrModelConfig {
    pub size: usize,
    pub path: *const c_char,
    pub name: *const c_char,
}

#[repr(C)]
pub struct NemoSpeechAsrRecognizerConfig {
    pub size: usize,
    pub backend: *const NemoSpeechAsrBackendConfig,
    pub model: *const NemoSpeechAsrModelConfig,
    pub streaming: *const u8,
    pub decoder: *const u8,
    pub vad: *const u8,
    pub endpointing: *const u8,
    pub postproc: *const u8,
    pub diar: *const u8,
    pub batching: *const u8,
}

#[repr(C)]
pub struct NemoSpeechAsrSpeechContext {
    pub size: usize,
    pub phrases: *const *const c_char,
    pub phrase_count: usize,
    pub boost: c_float,
}

#[repr(C)]
pub struct NemoSpeechAsrRecognitionOptions {
    pub size: usize,
    pub request_id: *const c_char,
    pub language_code: *const c_char,
    pub interim_results: bool,
    pub enable_word_time_offsets: bool,
    pub enable_automatic_punctuation: bool,
    pub verbatim_transcripts: bool,
    pub profanity_filter: bool,
    pub stop_history_eou_ms: i32,
    pub speech_contexts: *const NemoSpeechAsrSpeechContext,
    pub speech_context_count: usize,
    pub max_alternatives: i32,
    pub enable_speaker_diarization: bool,
    pub max_speaker_count: i32,
}

#[link(name = "nemo_speech_asr_c")]
unsafe extern "C" {
    pub fn nemo_speech_asr_recognition_options_default() -> NemoSpeechAsrRecognitionOptions;
    pub fn nemo_speech_asr_create(
        config: *const NemoSpeechAsrRecognizerConfig,
        out: *mut *mut NemoSpeechAsrRecognizer,
    ) -> c_int;
    pub fn nemo_speech_asr_destroy(recognizer: *mut NemoSpeechAsrRecognizer);
    pub fn nemo_speech_asr_recognize_f32(
        recognizer: *mut NemoSpeechAsrRecognizer,
        options: *const NemoSpeechAsrRecognitionOptions,
        samples: *const c_float,
        sample_count: usize,
        sample_rate: i32,
        out: *mut *mut NemoSpeechAsrResult,
    ) -> c_int;
    pub fn nemo_speech_asr_streaming_recognize(
        recognizer: *mut NemoSpeechAsrRecognizer,
        options: *const NemoSpeechAsrRecognitionOptions,
        out: *mut *mut NemoSpeechAsrStream,
    ) -> c_int;
    pub fn nemo_speech_asr_stream_push_f32(
        stream: *mut NemoSpeechAsrStream,
        samples: *const c_float,
        sample_count: usize,
        sample_rate: i32,
    ) -> c_int;
    pub fn nemo_speech_asr_stream_finish(stream: *mut NemoSpeechAsrStream) -> c_int;
    pub fn nemo_speech_asr_stream_next(
        stream: *mut NemoSpeechAsrStream,
        out: *mut *mut NemoSpeechAsrResult,
    ) -> c_int;
    pub fn nemo_speech_asr_stream_close(stream: *mut NemoSpeechAsrStream);
    pub fn nemo_speech_asr_result_is_final(result: *const NemoSpeechAsrResult) -> bool;
    pub fn nemo_speech_asr_result_alternative_count(result: *const NemoSpeechAsrResult) -> usize;
    pub fn nemo_speech_asr_result_transcript(
        result: *const NemoSpeechAsrResult,
        alternative: usize,
    ) -> *const c_char;
    pub fn nemo_speech_asr_result_destroy(result: *mut NemoSpeechAsrResult);
    pub fn nemo_speech_asr_last_error() -> *const c_char;
    pub fn nemo_speech_asr_version() -> *const c_char;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handwritten_structs_match_v1_c_abi() {
        assert_eq!(size_of::<NemoSpeechAsrRecognizerConfig>(), 80);
        assert_eq!(size_of::<NemoSpeechAsrRecognitionOptions>(), 72);
        assert_eq!(size_of::<NemoSpeechAsrSpeechContext>(), 32);
    }
}
