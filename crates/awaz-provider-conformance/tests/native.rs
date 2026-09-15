use awaz_core::AudioChunk;
use awaz_moonshine::{ModelSize, MoonshineRecognizer};
use awaz_nemo::{NemoModel, NemoRecognizer};
use awaz_provider_conformance::{
    CapabilityExpectation::{Supported, Unsupported},
    ProviderExpectations, run_provider_conformance,
};
use std::{path::PathBuf, sync::Mutex};

static NATIVE_TEST: Mutex<()> = Mutex::new(());
const STREAMING: ProviderExpectations = ProviderExpectations {
    keyterms: Supported,
    context: Unsupported,
};
const NO_CUSTOMIZATION: ProviderExpectations = ProviderExpectations {
    keyterms: Unsupported,
    context: Unsupported,
};

#[test]
#[ignore = "requires AWAZ_TEST_MOONSHINE_MODEL_DIR"]
fn moonshine_conforms() {
    let _guard = NATIVE_TEST.lock().unwrap();
    let path = required_path("AWAZ_TEST_MOONSHINE_MODEL_DIR");
    let mut recognizer = MoonshineRecognizer::load(path, ModelSize::Small).unwrap();
    run_provider_conformance(
        &mut recognizer,
        &fixture_audio(),
        ProviderExpectations {
            keyterms: Supported,
            context: Supported,
        },
    )
    .unwrap();
}

#[test]
#[ignore = "requires AWAZ_TEST_NEMOTRON_MODEL_PATH"]
fn nemotron_conforms() {
    let _guard = NATIVE_TEST.lock().unwrap();
    let path = required_path("AWAZ_TEST_NEMOTRON_MODEL_PATH");
    let mut recognizer = NemoRecognizer::load(path, NemoModel::Nemotron35, "auto").unwrap();
    run_provider_conformance(&mut recognizer, &fixture_audio(), STREAMING).unwrap();
}

#[test]
#[ignore = "requires AWAZ_TEST_PARAKEET_MODEL_PATH"]
fn parakeet_conforms() {
    let _guard = NATIVE_TEST.lock().unwrap();
    let path = required_path("AWAZ_TEST_PARAKEET_MODEL_PATH");
    let mut recognizer = NemoRecognizer::load(path, NemoModel::ParakeetTdtV3, "").unwrap();
    run_provider_conformance(&mut recognizer, &fixture_audio(), NO_CUSTOMIZATION).unwrap();
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "requires macOS Speech assets and permission"]
fn apple_speech_conforms() {
    let _guard = NATIVE_TEST.lock().unwrap();
    let mut recognizer = awaz_apple_speech::AppleSpeechRecognizer::load("en").unwrap();
    run_provider_conformance(&mut recognizer, &fixture_audio(), NO_CUSTOMIZATION).unwrap();
}

fn required_path(variable: &str) -> PathBuf {
    let path = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {variable} to run this provider test"));
    assert!(
        path.exists(),
        "{variable} does not exist: {}",
        path.display()
    );
    path
}

fn fixture_audio() -> Vec<AudioChunk> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/jfk.wav");
    let mut reader = hound::WavReader::open(path).unwrap();
    let spec = reader.spec();
    assert_eq!(spec.channels, 1);
    assert_eq!(spec.bits_per_sample, 16);
    let samples = reader
        .samples::<i16>()
        .map(|sample| sample.unwrap() as f32 / 32_768.0)
        .collect::<Vec<_>>();
    samples
        .chunks((spec.sample_rate as usize / 10).max(1))
        .map(|samples| AudioChunk {
            samples: samples.to_vec(),
            sample_rate: spec.sample_rate,
        })
        .collect()
}
