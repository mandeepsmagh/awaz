use anyhow::{Context, Result, anyhow};
use awaz_audio::{AudioCapture, CaptureConfig, list_input_devices};
use awaz_core::{Command, Event, Recognizer, RecognizerError, SpeechEvent, VoiceState};
use awaz_moonshine::{ModelSize, MoonshineRecognizer, default_model_dir};
use clap::{Args, Parser, Subcommand, ValueEnum};
use crossbeam_channel::{Receiver, bounded, select, tick};
use std::{
    collections::VecDeque,
    io::{self, BufRead, Write},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

const MOONSHINE_MODELS: &str = include_str!("../../../moonshine.models");

#[derive(Parser)]
#[command(
    name = "awaz",
    version,
    about = "Fast, local, provider-neutral voice I/O"
)]
struct Cli {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand)]
enum CliCommand {
    /// Push-to-talk transcription using the default microphone.
    Mic(MicArgs),
    /// Transcribe a WAV file.
    Transcribe(TranscribeArgs),
    /// List microphone devices.
    Devices,
    /// Check audio, model and Moonshine readiness.
    Doctor(CommonArgs),
    /// Run the stable NDJSON machine protocol over stdin/stdout.
    Serve(ServeArgs),
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum ModelArg {
    Tiny,
    Small,
    Medium,
}

impl From<ModelArg> for ModelSize {
    fn from(value: ModelArg) -> Self {
        match value {
            ModelArg::Tiny => Self::Tiny,
            ModelArg::Small => Self::Small,
            ModelArg::Medium => Self::Medium,
        }
    }
}

#[derive(Args, Debug, Clone)]
struct CommonArgs {
    #[arg(long, env = "AWAZ_LANGUAGE")]
    language: Option<String>,
    #[arg(long, value_enum, env = "AWAZ_MODEL")]
    model: Option<ModelArg>,
    #[arg(long, env = "AWAZ_MODEL_DIR")]
    model_dir: Option<PathBuf>,
}

#[derive(Args)]
struct MicArgs {
    #[command(flatten)]
    common: CommonArgs,
    #[arg(long)]
    device: Option<String>,
    #[arg(long, env = "AWAZ_SAVE_WAV")]
    save_wav: Option<PathBuf>,
}

#[derive(Args)]
struct TranscribeArgs {
    #[command(flatten)]
    common: CommonArgs,
    path: PathBuf,
}

#[derive(Args)]
struct ServeArgs {
    #[command(flatten)]
    common: CommonArgs,
    #[arg(long)]
    device: Option<String>,
    #[arg(long, default_value_t = 450)]
    preroll_ms: u32,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        CliCommand::Mic(args) => mic(args),
        CliCommand::Transcribe(args) => transcribe(args),
        CliCommand::Devices => devices(),
        CliCommand::Doctor(args) => doctor(args),
        CliCommand::Serve(args) => serve(args),
    }
}

fn configured_models() -> Result<Vec<(String, ModelSize)>> {
    let mut models = Vec::new();
    for (index, raw_line) in MOONSHINE_MODELS.lines().enumerate() {
        let line = raw_line.split('#').next().unwrap_or_default().trim();
        if line.is_empty() {
            continue;
        }

        let mut fields = line.split_whitespace();
        let language = fields.next().unwrap_or_default();
        let model = fields.next().unwrap_or_default();
        if language.is_empty() || model.is_empty() || fields.next().is_some() {
            return Err(anyhow!(
                "invalid moonshine.models entry on line {}",
                index + 1
            ));
        }
        let size = model
            .parse::<ModelSize>()
            .map_err(|error| anyhow!("moonshine.models line {}: {error}", index + 1))?;
        models.push((language.to_owned(), size));
    }

    if models.is_empty() {
        return Err(anyhow!("moonshine.models has no model entries"));
    }
    Ok(models)
}

fn model_selection(common: &CommonArgs) -> Result<(String, ModelSize)> {
    let configured = configured_models()?;
    let default = configured
        .first()
        .ok_or_else(|| anyhow!("moonshine.models has no model entries"))?;
    let language = common.language.clone().unwrap_or_else(|| default.0.clone());
    let size = common
        .model
        .map(ModelSize::from)
        .or_else(|| {
            configured
                .iter()
                .find(|(configured_language, _)| configured_language == &language)
                .map(|(_, size)| *size)
        })
        .ok_or_else(|| anyhow!("no model configured for {language}; pass --model"))?;
    Ok((language, size))
}

fn model_path(common: &CommonArgs) -> Result<(String, PathBuf, ModelSize)> {
    let (language, size) = model_selection(common)?;
    if let Some(path) = &common.model_dir {
        return Ok((language, path.clone(), size));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent() {
            let bundled = root
                .join("models/moonshine")
                .join(&language)
                .join(size.slug());
            if bundled.exists() {
                return Ok((language, bundled, size));
            }
        }
    }

    let path = default_model_dir(&language, size)
        .ok_or_else(|| anyhow!("cannot determine model directory; pass --model-dir"))?;
    Ok((language, path, size))
}

fn load_recognizer(common: &CommonArgs) -> Result<MoonshineRecognizer> {
    let (language, path, size) = model_path(common)?;
    if !path.exists() {
        if common.model_dir.is_some() {
            return Err(anyhow!("Moonshine model not found at {}.", path.display()));
        }
        let cache = default_model_dir(&language, size)
            .ok_or_else(|| anyhow!("cannot determine model directory"))?;
        download_model(&language, size, &cache)?;
        return MoonshineRecognizer::load(&cache, size).map_err(anyhow::Error::from);
    }
    MoonshineRecognizer::load(&path, size).map_err(anyhow::Error::from)
}

fn download_model(language: &str, size: ModelSize, dest: &Path) -> Result<()> {
    eprintln!("downloading Moonshine {language} {} model…", size.slug());
    let manifest =
        MoonshineRecognizer::model_manifest(language, size).map_err(anyhow::Error::from)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)?;
    std::fs::create_dir_all(dest)?;

    let Some(groups) = manifest.get("groups").and_then(serde_json::Value::as_array) else {
        return Err(anyhow!("model manifest has no groups"));
    };
    for group in groups {
        let Some(files) = group.get("files").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for file in files {
            let name = file
                .get("name")
                .and_then(serde_json::Value::as_str)
                .context("manifest entry missing name")?;
            let url = file
                .get("url")
                .and_then(serde_json::Value::as_str)
                .context("manifest entry missing url")?;
            let target = dest.join(name);
            let len = target.metadata().map(|meta| meta.len()).unwrap_or(0);
            let expected = file.get("size").and_then(serde_json::Value::as_u64);
            let complete = match expected {
                Some(size) => len == size,
                None => len > 0,
            };
            if complete {
                continue;
            }

            // Download to a temporary name and rename only after success, so an
            // interrupted download never leaves a file that looks complete.
            let part = dest.join(format!("{name}.part"));
            let status = std::process::Command::new("curl")
                .args(["-fsSL", "--retry", "3", "-o"])
                .arg(&part)
                .arg(url)
                .status()
                .context("failed to run curl")?;
            if !status.success() {
                let _ = std::fs::remove_file(&part);
                return Err(anyhow!("curl failed while downloading {name}"));
            }
            std::fs::rename(&part, &target).with_context(|| format!("finalize {name}"))?;
        }
    }
    Ok(())
}

fn devices() -> Result<()> {
    for device in list_input_devices().map_err(anyhow::Error::from)? {
        println!(
            "{}{}",
            if device.is_default { "* " } else { "  " },
            device.name
        );
    }
    Ok(())
}

fn doctor(common: CommonArgs) -> Result<()> {
    eprintln!("Awaz doctor");
    let devices = list_input_devices().map_err(anyhow::Error::from)?;
    let default = devices
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.name.as_str())
        .unwrap_or("none");
    eprintln!("  audio devices      {}", devices.len());
    eprintln!("  default microphone {default}");
    let capture = AudioCapture::start(CaptureConfig::default()).map_err(anyhow::Error::from)?;
    eprintln!("  audio capture      ready ({})", capture.device_name);
    drop(capture);

    let (_, path, size) = model_path(&common)?;
    eprintln!("  model              {} ({})", size.slug(), path.display());

    let recognizer = load_recognizer(&common)?;
    drop(recognizer);
    eprintln!(
        "  moonshine          ready (library {})",
        MoonshineRecognizer::library_version()
    );
    eprintln!("  status             ready");
    Ok(())
}

fn mic(args: MicArgs) -> Result<()> {
    let mut recognizer = load_recognizer(&args.common)?;
    let save_wav = args.save_wav;
    let capture = AudioCapture::start(CaptureConfig {
        device_name: args.device,
        ..Default::default()
    })
    .map_err(anyhow::Error::from)?;

    eprintln!(
        "Awaz ready on {}. Press Enter to start, Enter again to stop.",
        capture.device_name
    );
    let (tx, rx) = bounded::<()>(2);
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        let _ = lines.next();
        let _ = tx.send(());
        let _ = lines.next();
        let _ = tx.send(());
    });

    rx.recv()?;
    recognizer.start().map_err(anyhow::Error::from)?;
    eprintln!("listening…");
    let audio_rx = capture.receiver();
    let mut saved = save_wav.as_ref().map(|_| Vec::<f32>::new());

    loop {
        select! {
            recv(rx) -> _ => break,
            recv(audio_rx) -> message => {
                if let Ok(chunk) = message {
                    if let Some(buffer) = saved.as_mut() {
                        buffer.extend_from_slice(&chunk.samples);
                    }
                    recognizer.push_audio(&chunk).map_err(anyhow::Error::from)?;
                }
            }
            default(Duration::from_millis(80)) => {
                for event in recognizer.poll().map_err(anyhow::Error::from)? {
                    if let SpeechEvent::Partial(text) = event {
                        eprint!("\r{text}\x1b[K");
                        let _ = io::stderr().flush();
                    }
                }
            }
        }
    }

    eprintln!();

    // Drain audio still queued from the capture callback before finalizing, so
    // the tail of the utterance is not lost when Enter stops the loop.
    while let Ok(chunk) = audio_rx.try_recv() {
        if let Some(buffer) = saved.as_mut() {
            buffer.extend_from_slice(&chunk.samples);
        }
        recognizer.push_audio(&chunk).map_err(anyhow::Error::from)?;
    }

    let mut final_text = String::new();
    for event in recognizer.finish().map_err(anyhow::Error::from)? {
        if let SpeechEvent::Final(text) = event {
            final_text = text;
        }
    }
    println!("{final_text}");

    if let (Some(path), Some(samples)) = (save_wav.as_ref(), saved.as_ref()) {
        write_wav(path, samples, capture.sample_rate)?;
        eprintln!("saved captured audio to {}", path.display());
    }

    let dropped = capture.dropped_chunks();
    if dropped > 0 {
        eprintln!("warning: dropped {dropped} audio chunks while listening");
    }
    Ok(())
}

fn write_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(path, spec).context("create save WAV")?;
    for &sample in samples {
        let clipped = sample.clamp(-1.0, 1.0);
        writer
            .write_sample((clipped * i16::MAX as f32) as i16)
            .context("write save WAV sample")?;
    }
    writer.finalize().context("finalize save WAV")?;
    Ok(())
}

fn transcribe(args: TranscribeArgs) -> Result<()> {
    let mut reader = hound::WavReader::open(&args.path).context("open WAV")?;
    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(anyhow!("v1 WAV transcription expects mono audio"));
    }
    if spec.sample_rate == 0 {
        return Err(anyhow!("WAV sample rate must be greater than zero"));
    }

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int if spec.bits_per_sample <= 16 => {
            let scale = signed_pcm_scale(spec.bits_per_sample)?;
            reader
                .samples::<i16>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<_, _>>()?
        }
        hound::SampleFormat::Int => {
            let scale = signed_pcm_scale(spec.bits_per_sample)?;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<_, _>>()?
        }
    };

    let mut recognizer = load_recognizer(&args.common)?;
    recognizer.start().map_err(anyhow::Error::from)?;
    for samples in samples.chunks((spec.sample_rate as usize / 10).max(1)) {
        recognizer
            .push_audio(&awaz_core::AudioChunk {
                samples: samples.to_vec(),
                sample_rate: spec.sample_rate,
            })
            .map_err(anyhow::Error::from)?;
    }

    let mut final_text = String::new();
    for event in recognizer.finish().map_err(anyhow::Error::from)? {
        if let SpeechEvent::Final(text) = event {
            final_text = text;
        }
    }
    println!("{final_text}");
    Ok(())
}

fn signed_pcm_scale(bits_per_sample: u16) -> Result<f32> {
    if !(2..=32).contains(&bits_per_sample) {
        return Err(anyhow!(
            "unsupported integer WAV bit depth: {bits_per_sample}"
        ));
    }
    Ok((1_i64 << (bits_per_sample - 1)) as f32)
}

fn serve(args: ServeArgs) -> Result<()> {
    let mut recognizer = load_recognizer(&args.common)?;
    let capture = AudioCapture::start(CaptureConfig {
        device_name: args.device,
        ..Default::default()
    })
    .map_err(anyhow::Error::from)?;
    let audio_rx = capture.receiver();
    let command_rx = command_reader();
    let poll_tick = tick(Duration::from_millis(80));
    let preroll_capacity = ((capture.sample_rate as u64 * args.preroll_ms as u64) / 1000) as usize;
    let mut preroll = VecDeque::<f32>::with_capacity(preroll_capacity.max(1));
    let mut state = VoiceState::Idle;

    emit(&Event::Ready {
        version: env!("CARGO_PKG_VERSION").into(),
        provider: "moonshine".into(),
    })?;
    emit(&Event::Capabilities {
        stt: true,
        tts: false,
    })?;

    loop {
        select! {
            recv(command_rx) -> command => {
                let Ok(command) = command else {
                    break;
                };
                match command {
                    Ok(command) => {
                        if handle_command(
                            command,
                            &mut state,
                            &mut recognizer,
                            &mut preroll,
                            &audio_rx,
                            &capture,
                        )? {
                            break;
                        }
                    }
                    Err(message) => emit_error("bad_json", &message, state, false)?,
                }
            }
            recv(audio_rx) -> message => {
                let Ok(chunk) = message else {
                    break;
                };
                if state == VoiceState::Listening {
                    require_recognizer(recognizer.push_audio(&chunk), state)?;
                } else {
                    retain_preroll(&mut preroll, preroll_capacity, chunk.samples);
                }
            }
            recv(poll_tick) -> _ => {
                if state == VoiceState::Listening {
                    for event in require_recognizer(recognizer.poll(), state)? {
                        emit_speech(event)?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn retain_preroll(preroll: &mut VecDeque<f32>, capacity: usize, samples: Vec<f32>) {
    if capacity == 0 {
        return;
    }
    for sample in samples {
        if preroll.len() == capacity {
            preroll.pop_front();
        }
        preroll.push_back(sample);
    }
}

fn command_reader() -> Receiver<std::result::Result<Command, String>> {
    let (tx, rx) = bounded(64);
    thread::spawn(move || {
        let stdin = io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if line.trim().is_empty() {
                continue;
            }
            let parsed = serde_json::from_str::<Command>(&line).map_err(|err| err.to_string());
            if tx.send(parsed).is_err() {
                break;
            }
        }
    });
    rx
}

fn handle_command(
    command: Command,
    state: &mut VoiceState,
    recognizer: &mut MoonshineRecognizer,
    preroll: &mut VecDeque<f32>,
    audio_rx: &Receiver<awaz_core::AudioChunk>,
    capture: &AudioCapture,
) -> Result<bool> {
    match command {
        Command::Hello => emit(&Event::Capabilities {
            stt: true,
            tts: false,
        })?,
        Command::ListenStart => {
            if *state != VoiceState::Idle {
                emit_error("invalid_state", "already busy", *state, false)?;
                return Ok(false);
            }

            require_recognizer(recognizer.start(), *state)?;
            state.transition(VoiceState::Listening)?;
            if !preroll.is_empty() {
                let samples = preroll.drain(..).collect();
                require_recognizer(
                    recognizer.push_audio(&awaz_core::AudioChunk {
                        samples,
                        sample_rate: capture.sample_rate,
                    }),
                    *state,
                )?;
            }
            emit(&Event::ListenStarted)?;
        }
        Command::ListenStop => {
            if *state != VoiceState::Listening {
                emit_error("invalid_state", "not listening", *state, false)?;
                return Ok(false);
            }

            state.transition(VoiceState::Finalizing)?;
            drain_pending_audio(recognizer, audio_rx, *state)?;
            for event in require_recognizer(recognizer.finish(), *state)? {
                emit_speech(event)?;
            }
            state.transition(VoiceState::Idle)?;

            let dropped = capture.dropped_chunks();
            if dropped > 0 {
                eprintln!("warning: dropped {dropped} audio chunks since start");
            }
        }
        Command::ListenCancel => {
            if *state == VoiceState::Listening || *state == VoiceState::Finalizing {
                require_recognizer(recognizer.cancel(), *state)?;
                while audio_rx.try_recv().is_ok() {}
                preroll.clear();
                *state = VoiceState::Idle;
            }
            emit(&Event::ListenCancelled)?;
        }
        Command::KeytermsSet { terms } => {
            if let Err(error) = recognizer.set_keyterms(&terms) {
                emit_error("recognizer_error", &error.to_string(), *state, false)?;
            }
        }
        Command::ContextSet { text } => {
            if let Err(error) = recognizer.set_context(&text) {
                emit_error("recognizer_error", &error.to_string(), *state, false)?;
            }
        }
        Command::SpeakStart
        | Command::SpeakText { .. }
        | Command::SpeakEnd
        | Command::SpeakCancel => {
            emit_error(
                "unsupported",
                "TTS is reserved by the protocol but not implemented in Awaz v1",
                *state,
                false,
            )?;
        }
        Command::Shutdown => {
            if *state == VoiceState::Listening || *state == VoiceState::Finalizing {
                let _ = recognizer.cancel();
            }
            emit(&Event::Shutdown)?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn drain_pending_audio(
    recognizer: &mut MoonshineRecognizer,
    audio_rx: &Receiver<awaz_core::AudioChunk>,
    state: VoiceState,
) -> Result<()> {
    while let Ok(chunk) = audio_rx.try_recv() {
        require_recognizer(recognizer.push_audio(&chunk), state)?;
    }
    Ok(())
}

fn emit_speech(event: SpeechEvent) -> Result<()> {
    match event {
        SpeechEvent::Partial(text) => emit(&Event::TranscriptPartial { text }),
        SpeechEvent::Final(text) => emit(&Event::TranscriptFinal { text }),
    }
}

fn emit(event: &Event) -> Result<()> {
    let stdout = io::stdout();
    let mut output = stdout.lock();
    serde_json::to_writer(&mut output, event)?;
    writeln!(output)?;
    output.flush()?;
    Ok(())
}

fn emit_error(code: &str, message: &str, state: VoiceState, fatal: bool) -> Result<()> {
    emit(&Event::Error {
        code: code.into(),
        message: message.into(),
        state,
        fatal,
    })
}

fn require_recognizer<T>(
    result: std::result::Result<T, RecognizerError>,
    state: VoiceState,
) -> Result<T> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            emit_error("recognizer_error", &error.to_string(), state, true)?;
            Err(error.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_pcm_scale_rejects_invalid_bit_depths() {
        assert!(signed_pcm_scale(0).is_err());
        assert!(signed_pcm_scale(1).is_err());
        assert!(signed_pcm_scale(33).is_err());
    }

    #[test]
    fn preroll_keeps_only_the_latest_samples() {
        let mut preroll = VecDeque::new();
        retain_preroll(&mut preroll, 3, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(preroll.into_iter().collect::<Vec<_>>(), vec![2.0, 3.0, 4.0]);
    }
}
