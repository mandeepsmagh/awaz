use anyhow::{Context, Result, anyhow};
use awaz_audio::{AudioCapture, CaptureConfig, list_input_devices};
use awaz_core::{Command, Event, Recognizer, SpeechEvent, VoiceState};
use awaz_moonshine::{ModelSize, MoonshineRecognizer, default_model_dir};
use clap::{Args, Parser, Subcommand, ValueEnum};
use crossbeam_channel::{Receiver, bounded, select, tick};
use std::{
    collections::VecDeque,
    io::{self, BufRead, Write},
    path::PathBuf,
    thread,
    time::Duration,
};

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
    #[arg(long, default_value = "en")]
    language: String,
    #[arg(long, value_enum, default_value = "small")]
    model: ModelArg,
    #[arg(long, env = "AWAZ_MODEL_DIR")]
    model_dir: Option<PathBuf>,
}

#[derive(Args)]
struct MicArgs {
    #[command(flatten)]
    common: CommonArgs,
    #[arg(long)]
    device: Option<String>,
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

fn model_path(common: &CommonArgs) -> Result<(PathBuf, ModelSize)> {
    let size = ModelSize::from(common.model);
    if let Some(path) = &common.model_dir {
        return Ok((path.clone(), size));
    }

    if let Ok(exe) = std::env::current_exe() {
        if let Some(root) = exe.parent() {
            let bundled = root
                .join("models/moonshine")
                .join(&common.language)
                .join(size.slug());
            if bundled.exists() {
                return Ok((bundled, size));
            }
        }
    }

    let path = default_model_dir(&common.language, size)
        .ok_or_else(|| anyhow!("cannot determine model directory; pass --model-dir"))?;
    Ok((path, size))
}

fn load_recognizer(common: &CommonArgs) -> Result<MoonshineRecognizer> {
    let (path, size) = model_path(common)?;
    if !path.exists() {
        return Err(anyhow!(
            "Moonshine model not found at {}. Run scripts/dev-setup-model.sh or pass --model-dir.",
            path.display()
        ));
    }
    MoonshineRecognizer::load(&path, size).map_err(anyhow::Error::from)
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

    let (path, size) = model_path(&common)?;
    eprintln!("  model              {} ({})", size.slug(), path.display());
    if !path.exists() {
        return Err(anyhow!("model directory is missing"));
    }

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

    loop {
        select! {
            recv(rx) -> _ => break,
            recv(audio_rx) -> message => {
                if let Ok(chunk) = message {
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
    let mut final_text = String::new();
    for event in recognizer.finish().map_err(anyhow::Error::from)? {
        if let SpeechEvent::Final(text) = event {
            final_text = text;
        }
    }
    println!("{final_text}");
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
            let max = signed_pcm_max(spec.bits_per_sample)? as f32;
            reader
                .samples::<i16>()
                .map(|sample| sample.map(|value| value as f32 / max))
                .collect::<Result<_, _>>()?
        }
        hound::SampleFormat::Int => {
            let max = signed_pcm_max(spec.bits_per_sample)? as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / max))
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

fn signed_pcm_max(bits_per_sample: u16) -> Result<i64> {
    if !(2..=32).contains(&bits_per_sample) {
        return Err(anyhow!(
            "unsupported integer WAV bit depth: {bits_per_sample}"
        ));
    }
    Ok((1_i64 << (bits_per_sample - 1)) - 1)
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
    let preroll_capacity =
        ((capture.sample_rate as u64 * args.preroll_ms as u64) / 1000) as usize;
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
                            capture.sample_rate,
                        )? {
                            break;
                        }
                    }
                    Err(message) => emit_error("bad_json", &message)?,
                }
            }
            recv(audio_rx) -> message => {
                let Ok(chunk) = message else {
                    break;
                };
                if state == VoiceState::Listening {
                    recognizer.push_audio(&chunk).map_err(anyhow::Error::from)?;
                } else {
                    retain_preroll(&mut preroll, preroll_capacity, chunk.samples);
                }
            }
            recv(poll_tick) -> _ => {
                if state == VoiceState::Listening {
                    for event in recognizer.poll().map_err(anyhow::Error::from)? {
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
    sample_rate: u32,
) -> Result<bool> {
    match command {
        Command::Hello => emit(&Event::Capabilities {
            stt: true,
            tts: false,
        })?,
        Command::ListenStart => {
            if *state != VoiceState::Idle {
                emit_error("invalid_state", "already busy")?;
                return Ok(false);
            }

            recognizer.start().map_err(anyhow::Error::from)?;
            state.transition(VoiceState::Listening)?;
            if !preroll.is_empty() {
                let samples = preroll.drain(..).collect();
                recognizer
                    .push_audio(&awaz_core::AudioChunk {
                        samples,
                        sample_rate,
                    })
                    .map_err(anyhow::Error::from)?;
            }
            emit(&Event::ListenStarted)?;
        }
        Command::ListenStop => {
            if *state != VoiceState::Listening {
                emit_error("invalid_state", "not listening")?;
                return Ok(false);
            }

            state.transition(VoiceState::Finalizing)?;
            drain_pending_audio(recognizer, audio_rx)?;
            for event in recognizer.finish().map_err(anyhow::Error::from)? {
                emit_speech(event)?;
            }
            state.transition(VoiceState::Idle)?;
        }
        Command::ListenCancel => {
            if *state == VoiceState::Listening || *state == VoiceState::Finalizing {
                recognizer.cancel().map_err(anyhow::Error::from)?;
                while audio_rx.try_recv().is_ok() {}
                preroll.clear();
                *state = VoiceState::Idle;
            }
            emit(&Event::ListenCancelled)?;
        }
        Command::KeytermsSet { terms } => recognizer
            .set_keyterms(&terms)
            .map_err(anyhow::Error::from)?,
        Command::ContextSet { text } => recognizer
            .set_context(&text)
            .map_err(anyhow::Error::from)?,
        Command::SpeakStart
        | Command::SpeakText { .. }
        | Command::SpeakEnd
        | Command::SpeakCancel => {
            emit(&Event::Error {
                code: "unsupported".into(),
                message: "TTS is reserved by the protocol but not implemented in Awaz v1".into(),
            })?;
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
) -> Result<()> {
    while let Ok(chunk) = audio_rx.try_recv() {
        recognizer
            .push_audio(&chunk)
            .map_err(anyhow::Error::from)?;
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

fn emit_error(code: &str, message: &str) -> Result<()> {
    emit(&Event::Error {
        code: code.into(),
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_pcm_scale_rejects_invalid_bit_depths() {
        assert!(signed_pcm_max(0).is_err());
        assert!(signed_pcm_max(1).is_err());
        assert!(signed_pcm_max(33).is_err());
    }

    #[test]
    fn preroll_keeps_only_the_latest_samples() {
        let mut preroll = VecDeque::new();
        retain_preroll(&mut preroll, 3, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(preroll.into_iter().collect::<Vec<_>>(), vec![2.0, 3.0, 4.0]);
    }
}
