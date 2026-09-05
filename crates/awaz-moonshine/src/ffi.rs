use std::ffi::{c_char, c_void};

pub const MOONSHINE_HEADER_VERSION: i32 = 30_000;
pub const MODEL_TINY_STREAMING: u32 = 2;
pub const MODEL_SMALL_STREAMING: u32 = 4;
pub const MODEL_MEDIUM_STREAMING: u32 = 5;

#[repr(C)]
pub struct MoonshineOption {
    pub name: *const c_char,
    pub value: *const c_char,
}

#[repr(C)]
pub struct TranscriptWord {
    pub text: *const c_char,
    pub start: f32,
    pub end: f32,
    pub confidence: f32,
}

#[repr(C)]
pub struct SpeakerSpan {
    pub start_time: f32,
    pub duration: f32,
    pub speaker_id: u64,
    pub speaker_index: u32,
    pub start_char: u64,
    pub end_char: u64,
}

#[repr(C)]
pub struct TranscriptLine {
    pub text: *const c_char,
    pub audio_data: *const f32,
    pub audio_data_count: usize,
    pub start_time: f32,
    pub duration: f32,
    pub id: u64,
    pub is_complete: i8,
    pub is_updated: i8,
    pub is_new: i8,
    pub has_text_changed: i8,
    pub have_speakers_changed: i8,
    pub speaker_spans: *const SpeakerSpan,
    pub speaker_span_count: u64,
    pub last_transcription_latency_ms: u32,
    pub words: *const TranscriptWord,
    pub word_count: u64,
}

#[repr(C)]
pub struct Transcript {
    pub lines: *mut TranscriptLine,
    pub line_count: u64,
}

unsafe extern "C" {
    pub fn moonshine_get_version() -> i32;
    pub fn moonshine_error_to_string(error: i32) -> *const c_char;
    pub fn moonshine_load_transcriber_from_files(
        path: *const c_char,
        model_arch: u32,
        options: *const MoonshineOption,
        options_count: u64,
        moonshine_version: i32,
    ) -> i32;
    pub fn moonshine_free_transcriber(transcriber_handle: i32);
    pub fn moonshine_transcriber_set_keyterms(
        transcriber_handle: i32,
        keyterms: *const c_char,
    ) -> i32;
    pub fn moonshine_transcriber_set_context(
        transcriber_handle: i32,
        context: *const c_char,
        max_terms: i32,
    ) -> i32;

    pub fn moonshine_create_stream(transcriber_handle: i32, flags: u32) -> i32;
    pub fn moonshine_free_stream(transcriber_handle: i32, stream_handle: i32) -> i32;
    pub fn moonshine_start_stream(transcriber_handle: i32, stream_handle: i32) -> i32;
    pub fn moonshine_stop_stream(transcriber_handle: i32, stream_handle: i32) -> i32;
    pub fn moonshine_transcribe_add_audio_to_stream(
        transcriber_handle: i32,
        stream_handle: i32,
        new_audio_data: *const f32,
        audio_length: u64,
        sample_rate: i32,
        flags: u32,
    ) -> i32;
    pub fn moonshine_transcribe_stream(
        transcriber_handle: i32,
        stream_handle: i32,
        flags: u32,
        out_transcript: *mut *mut Transcript,
    ) -> i32;
    pub fn moonshine_get_stt_dependencies(
        language: *const c_char,
        options: *const MoonshineOption,
        options_count: u64,
        out_dependencies_json: *mut *mut c_char,
    ) -> i32;
}

unsafe extern "C" {
    pub fn free(ptr: *mut c_void);
}

#[cfg(all(test, target_pointer_width = "64"))]
mod abi_layout_tests {
    use super::*;
    use std::mem::{align_of, size_of};

    #[test]
    fn transcript_structs_match_moonshine_v3_abi() {
        assert_eq!(size_of::<TranscriptWord>(), 24);
        assert_eq!(size_of::<SpeakerSpan>(), 40);
        assert_eq!(size_of::<TranscriptLine>(), 88);
        assert_eq!(size_of::<Transcript>(), 16);
        assert_eq!(align_of::<TranscriptLine>(), 8);
    }
}
