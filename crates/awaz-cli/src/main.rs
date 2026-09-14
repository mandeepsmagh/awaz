use anyhow::{Context, Result, anyhow};
use awaz_apple_speech::AppleSpeechRecognizer;
use awaz_audio::{AudioCapture, CaptureConfig, list_input_devices};
use awaz_core::{Command, Event, Recognizer, RecognizerError, SpeechEvent, VoiceState};
use awaz_moonshine::{ModelSize, MoonshineRecognizer, default_model_dir};
use awaz_nemo::{NemoModel, NemoRecognizer, default_model_path as default_nemo_model_path};
use clap::{Args, Parser, Subcommand, ValueEnum};
use crossbeam_channel::{Receiver, Sender, TrySendError, bounded, select, select_biased, tick};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashSet, VecDeque},
    fs::{File, OpenOptions},
    io::{self, BufRead, BufReader, IsTerminal, Read, Write},
    path::{Component, Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, SystemTime},
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, ValueEnum)]
enum ProviderArg {
    #[default]
    Moonshine,
    Apple,
    Nemo,
}

impl ProviderArg {
    fn name(self) -> &'static str {
        match self {
            Self::Moonshine => "moonshine",
            Self::Apple => "apple",
            Self::Nemo => "nemo",
        }
    }
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

#[derive(Debug, Clone, Copy, Default, ValueEnum)]
enum NemoModelArg {
    #[default]
    #[value(name = "nemotron-3.5")]
    Nemotron35,
    #[value(name = "parakeet-tdt-v3")]
    ParakeetTdtV3,
}

impl From<NemoModelArg> for NemoModel {
    fn from(value: NemoModelArg) -> Self {
        match value {
            NemoModelArg::Nemotron35 => Self::Nemotron35,
            NemoModelArg::ParakeetTdtV3 => Self::ParakeetTdtV3,
        }
    }
}

#[derive(Args, Debug, Clone)]
struct CommonArgs {
    #[arg(
        long,
        value_enum,
        default_value_t,
        env = "AWAZ_PROVIDER",
        help = "Speech provider"
    )]
    provider: ProviderArg,
    #[arg(long, env = "AWAZ_LANGUAGE", help = "Language code, for example `en`")]
    language: Option<String>,
    #[arg(
        long,
        value_enum,
        env = "AWAZ_MODEL",
        help = "Moonshine model size; downloaded on first use"
    )]
    model: Option<ModelArg>,
    #[arg(
        long,
        env = "AWAZ_MODEL_DIR",
        help = "Use a pre-staged Moonshine model directory instead of the cache"
    )]
    model_dir: Option<PathBuf>,
    #[arg(
        long,
        value_enum,
        env = "AWAZ_NEMO_MODEL",
        help = "NeMo Speech model; downloaded on first use"
    )]
    nemo_model: Option<NemoModelArg>,
    #[arg(
        long,
        env = "AWAZ_NEMO_MODEL_PATH",
        help = "Use a pre-staged NeMo Speech GGUF instead of the cache"
    )]
    nemo_model_path: Option<PathBuf>,
}

#[derive(Args)]
struct MicArgs {
    #[command(flatten)]
    common: CommonArgs,
    #[arg(long, help = "Microphone device name; see `awaz devices`")]
    device: Option<String>,
    #[arg(
        long,
        env = "AWAZ_SAVE_WAV",
        help = "Write captured audio to a mono WAV; replay it with `awaz transcribe`"
    )]
    save_wav: Option<PathBuf>,
}

#[derive(Args)]
struct TranscribeArgs {
    #[command(flatten)]
    common: CommonArgs,
    /// Mono WAV file to transcribe.
    path: PathBuf,
}

#[derive(Args)]
struct ServeArgs {
    #[command(flatten)]
    common: CommonArgs,
    #[arg(long, help = "Microphone device name; see `awaz devices`")]
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

fn nemo_model_path(common: &CommonArgs, model: NemoModel) -> Result<PathBuf> {
    if let Some(path) = &common.nemo_model_path {
        if !path.is_file() {
            return Err(anyhow!("NeMo Speech model not found at {}", path.display()));
        }
        return Ok(path.clone());
    }

    if let Ok(exe) = std::env::current_exe()
        && let Some(root) = exe.parent()
    {
        let bundled = root
            .join("models/nemo")
            .join(model.slug())
            .join(model.filename());
        if bundled.is_file() {
            return Ok(bundled);
        }
    }

    let path = default_nemo_model_path(model)
        .ok_or_else(|| anyhow!("cannot determine NeMo model path; pass --nemo-model-path"))?;
    ensure_nemo_model(model, &path)?;
    Ok(path)
}

struct LoadedRecognizer {
    provider: &'static str,
    recognizer: Box<dyn Recognizer>,
}

fn load_recognizer(common: &CommonArgs) -> Result<LoadedRecognizer> {
    let recognizer: Box<dyn Recognizer> = match common.provider {
        ProviderArg::Moonshine => {
            if common.nemo_model.is_some() || common.nemo_model_path.is_some() {
                return Err(anyhow!(
                    "--nemo-model and --nemo-model-path apply only to the NeMo provider"
                ));
            }
            let (language, path, size) = model_path(common)?;
            if common.model_dir.is_some() {
                if !path.is_dir() {
                    return Err(anyhow!("Moonshine model not found at {}", path.display()));
                }
            } else if default_model_dir(&language, size).as_deref() == Some(path.as_path()) {
                ensure_model(&language, size, &path)?;
            }
            Box::new(MoonshineRecognizer::load(&path, size).map_err(anyhow::Error::from)?)
        }
        ProviderArg::Apple => {
            if common.model.is_some()
                || common.model_dir.is_some()
                || common.nemo_model.is_some()
                || common.nemo_model_path.is_some()
            {
                return Err(anyhow!(
                    "model options do not apply to Apple Speech; macOS manages its models"
                ));
            }
            let language = common.language.as_deref().unwrap_or("en");
            Box::new(AppleSpeechRecognizer::load(language).map_err(anyhow::Error::from)?)
        }
        ProviderArg::Nemo => {
            if common.model.is_some() || common.model_dir.is_some() {
                return Err(anyhow!(
                    "--model and --model-dir apply only to the Moonshine provider"
                ));
            }
            let model = common.nemo_model.unwrap_or_default().into();
            let path = nemo_model_path(common, model)?;
            let language = common
                .language
                .as_deref()
                .unwrap_or(if model.supports_streaming() {
                    "auto"
                } else {
                    ""
                });
            Box::new(NemoRecognizer::load(&path, model, language).map_err(anyhow::Error::from)?)
        }
    };
    Ok(LoadedRecognizer {
        provider: common.provider.name(),
        recognizer,
    })
}

#[derive(Debug)]
struct ModelFile {
    name: String,
    url: String,
    size: Option<u64>,
    sha256: Option<String>,
}

struct DownloadLock(PathBuf);

impl Drop for DownloadLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn ensure_nemo_model(model: NemoModel, path: &Path) -> Result<()> {
    let dest = path
        .parent()
        .context("NeMo model path has no parent directory")?;
    let file = ModelFile {
        name: model.filename().into(),
        url: model.download_url(),
        size: Some(model.size()),
        sha256: Some(model.sha256().into()),
    };
    if model_file_complete(dest, &file) {
        return Ok(());
    }

    std::fs::create_dir_all(dest)?;
    let _lock = acquire_download_lock(dest)?;
    if model_file_complete(dest, &file) {
        return Ok(());
    }

    eprintln!("downloading NeMo Speech {} model…", model.slug());
    download_model_file(dest, &file)?;
    if !model_file_complete(dest, &file) {
        return Err(anyhow!("NeMo Speech model download is incomplete"));
    }
    Ok(())
}

fn ensure_model(language: &str, size: ModelSize, dest: &Path) -> Result<()> {
    let manifest =
        MoonshineRecognizer::model_manifest(language, size).map_err(anyhow::Error::from)?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest)?;
    let files = manifest_files(&manifest)?;
    if files.iter().all(|file| model_file_complete(dest, file)) {
        return Ok(());
    }

    std::fs::create_dir_all(dest)?;
    let _lock = acquire_download_lock(dest)?;
    if files.iter().all(|file| model_file_complete(dest, file)) {
        return Ok(());
    }

    eprintln!("downloading Moonshine {language} {} model…", size.slug());
    for file in &files {
        if model_file_complete(dest, file) {
            continue;
        }
        download_model_file(dest, file)?;
    }

    if !files.iter().all(|file| model_file_complete(dest, file)) {
        return Err(anyhow!("Moonshine model download is incomplete"));
    }
    Ok(())
}

fn manifest_files(manifest: &serde_json::Value) -> Result<Vec<ModelFile>> {
    let groups = manifest
        .get("groups")
        .and_then(serde_json::Value::as_array)
        .context("model manifest has no groups")?;
    let mut out = Vec::new();
    for group in groups {
        let Some(files) = group.get("files").and_then(serde_json::Value::as_array) else {
            continue;
        };
        for file in files {
            let name = file
                .get("name")
                .and_then(serde_json::Value::as_str)
                .context("manifest entry missing name")?;
            let path = Path::new(name);
            if path.as_os_str().is_empty()
                || path
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
            {
                return Err(anyhow!("unsafe model manifest path: {name}"));
            }
            out.push(ModelFile {
                name: name.to_owned(),
                url: file
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .context("manifest entry missing url")?
                    .to_owned(),
                size: file.get("size").and_then(serde_json::Value::as_u64),
                sha256: None,
            });
        }
    }
    if out.is_empty() {
        return Err(anyhow!("model manifest has no files"));
    }
    Ok(out)
}

fn model_file_complete(dest: &Path, file: &ModelFile) -> bool {
    let len = dest.join(&file.name).metadata().map(|meta| meta.len()).ok();
    match (len, file.size) {
        (Some(actual), Some(expected)) => actual == expected,
        (Some(actual), None) => actual > 0,
        (None, _) => false,
    }
}

fn acquire_download_lock(dest: &Path) -> Result<DownloadLock> {
    let path = dest.join(".download.lock");
    let mut announced = false;
    for _ in 0..600 {
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                let lock = DownloadLock(path);
                writeln!(file, "{}", std::process::id())?;
                return Ok(lock);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                let stale = path
                    .metadata()
                    .and_then(|metadata| metadata.modified())
                    .ok()
                    .and_then(|modified| SystemTime::now().duration_since(modified).ok())
                    .is_some_and(|age| age > Duration::from_secs(30 * 60));
                if stale {
                    let _ = std::fs::remove_file(&path);
                    continue;
                }
                if !announced {
                    eprintln!("waiting for another Awaz model download…");
                    announced = true;
                }
                thread::sleep(Duration::from_secs(1));
            }
            Err(error) => return Err(error).context("create model download lock"),
        }
    }
    Err(anyhow!("timed out waiting for the model download lock"))
}

fn download_model_file(dest: &Path, file: &ModelFile) -> Result<()> {
    let target = dest.join(&file.name);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file_name = target
        .file_name()
        .and_then(|name| name.to_str())
        .context("model manifest has an invalid file name")?;
    let part = target.with_file_name(format!(".{file_name}.part-{}", std::process::id()));
    let status = std::process::Command::new("curl")
        .args(["-fL", "--retry", "3", "--progress-bar", "-o"])
        .arg(&part)
        .arg(&file.url)
        .status()
        .context("failed to run curl; install curl or pre-stage the model with --model-dir")?;
    if !status.success() {
        let _ = std::fs::remove_file(&part);
        return Err(anyhow!("curl failed while downloading {}", file.name));
    }

    let actual = part.metadata().map(|metadata| metadata.len())?;
    let valid = file.size.map_or(actual > 0, |expected| actual == expected);
    if !valid {
        let _ = std::fs::remove_file(&part);
        return Err(anyhow!(
            "downloaded {} has size {actual}, expected {}",
            file.name,
            file.size
                .map(|size| size.to_string())
                .unwrap_or_else(|| "a non-empty file".into())
        ));
    }
    if let Some(expected) = &file.sha256 {
        let actual = file_sha256(&part)?;
        if !actual.eq_ignore_ascii_case(expected) {
            let _ = std::fs::remove_file(&part);
            return Err(anyhow!(
                "downloaded {} has SHA-256 {actual}, expected {expected}",
                file.name
            ));
        }
    }

    match std::fs::rename(&part, &target) {
        Ok(()) => Ok(()),
        Err(_) if target.exists() => {
            std::fs::remove_file(&target)
                .with_context(|| format!("replace invalid model file {}", file.name))?;
            std::fs::rename(&part, &target).with_context(|| format!("finalize {}", file.name))
        }
        Err(error) => Err(error).with_context(|| format!("finalize {}", file.name)),
    }
}

fn file_sha256(path: &Path) -> Result<String> {
    let file = File::open(path).with_context(|| format!("open {} for checksum", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
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

    match common.provider {
        ProviderArg::Moonshine => {
            let (_, path, size) = model_path(&common)?;
            eprintln!("  model              {} ({})", size.slug(), path.display());
        }
        ProviderArg::Apple => eprintln!("  model              managed by macOS"),
        ProviderArg::Nemo => {
            let model: NemoModel = common.nemo_model.unwrap_or_default().into();
            let path = nemo_model_path(&common, model)?;
            eprintln!("  model              {} ({})", model.slug(), path.display());
        }
    }

    let loaded = load_recognizer(&common)?;
    drop(loaded.recognizer);
    match common.provider {
        ProviderArg::Moonshine => eprintln!(
            "  moonshine          ready (library {})",
            MoonshineRecognizer::library_version()
        ),
        ProviderArg::Apple => eprintln!("  apple speech        ready"),
        ProviderArg::Nemo => eprintln!(
            "  nemo speech         ready (library {})",
            NemoRecognizer::library_version()
        ),
    }
    eprintln!("  status             ready");
    Ok(())
}

fn mic(args: MicArgs) -> Result<()> {
    if !io::stdin().is_terminal() {
        return Err(anyhow!(
            "`awaz mic` requires an interactive terminal; use `awaz transcribe` for a file or `awaz serve` for machine control"
        ));
    }
    let ansi = io::stderr().is_terminal();
    let LoadedRecognizer { mut recognizer, .. } = load_recognizer(&args.common)?;
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
    let audio_rx = capture.receiver();
    while audio_rx.try_recv().is_ok() {}
    let dropped_at_start = capture.dropped_chunks();
    recognizer.start().map_err(anyhow::Error::from)?;
    eprintln!("listening…");
    let audio_errors = capture.error_receiver();
    let poll_tick = tick(Duration::from_millis(80));
    let mut saved = save_wav.as_ref().map(|_| Vec::<f32>::new());

    loop {
        select! {
            recv(rx) -> _ => break,
            recv(audio_errors) -> message => {
                return Err(anyhow!(
                    "audio input failed while listening: {}",
                    message.unwrap_or_else(|_| "audio error channel closed".into())
                ));
            }
            recv(audio_rx) -> message => {
                if let Ok(chunk) = message {
                    if let Some(buffer) = saved.as_mut() {
                        buffer.extend_from_slice(&chunk.samples);
                    }
                    recognizer.push_audio(&chunk).map_err(anyhow::Error::from)?;
                }
            }
            recv(poll_tick) -> _ => {
                // Catch up before inference. A slow previous poll can leave audio
                // queued, and polling that stale prefix adds avoidable live latency.
                for _ in 0..audio_rx.len() {
                    let Ok(chunk) = audio_rx.try_recv() else {
                        break;
                    };
                    if let Some(buffer) = saved.as_mut() {
                        buffer.extend_from_slice(&chunk.samples);
                    }
                    recognizer.push_audio(&chunk).map_err(anyhow::Error::from)?;
                }
                for event in recognizer.poll().map_err(anyhow::Error::from)? {
                    if let SpeechEvent::Partial(text) = event {
                        if ansi {
                            eprint!("\r{text}\x1b[K");
                            let _ = io::stderr().flush();
                        } else {
                            eprintln!("{text}");
                        }
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

    let dropped = capture.dropped_chunks().saturating_sub(dropped_at_start);
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

    let LoadedRecognizer { mut recognizer, .. } = load_recognizer(&args.common)?;
    recognizer.start().map_err(anyhow::Error::from)?;
    let chunk_size = (spec.sample_rate as usize / 10).max(1);
    let mut chunk = Vec::with_capacity(chunk_size);
    {
        let mut push_sample = |sample: f32| -> Result<()> {
            chunk.push(sample);
            if chunk.len() == chunk_size {
                recognizer
                    .push_audio(&awaz_core::AudioChunk {
                        samples: std::mem::take(&mut chunk),
                        sample_rate: spec.sample_rate,
                    })
                    .map_err(anyhow::Error::from)?;
                chunk = Vec::with_capacity(chunk_size);
            }
            Ok(())
        };

        match spec.sample_format {
            hound::SampleFormat::Float => {
                for sample in reader.samples::<f32>() {
                    push_sample(sample?)?;
                }
            }
            hound::SampleFormat::Int if spec.bits_per_sample <= 16 => {
                let scale = signed_pcm_scale(spec.bits_per_sample)?;
                for sample in reader.samples::<i16>() {
                    push_sample(sample? as f32 / scale)?;
                }
            }
            hound::SampleFormat::Int => {
                let scale = signed_pcm_scale(spec.bits_per_sample)?;
                for sample in reader.samples::<i32>() {
                    push_sample(sample? as f32 / scale)?;
                }
            }
        }
    }
    if !chunk.is_empty() {
        recognizer
            .push_audio(&awaz_core::AudioChunk {
                samples: chunk,
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

enum RecognizerCommand {
    Start(u64),
    Push(awaz_core::AudioChunk),
    Poll(u64),
    Finish(u64, Arc<AtomicBool>),
    Cancel(u64),
    SetKeyterms(Vec<String>),
    SetContext(String),
    Shutdown,
}

enum RecognizerResponse {
    Started(u64, std::result::Result<(), RecognizerError>),
    Speech(u64, std::result::Result<Vec<SpeechEvent>, RecognizerError>),
    Finished(u64, std::result::Result<Vec<SpeechEvent>, RecognizerError>),
    Cancelled(u64, std::result::Result<(), RecognizerError>),
    Configured(std::result::Result<(), RecognizerError>),
    Fault(RecognizerError),
}

struct ServeSession {
    state: VoiceState,
    utterance: u64,
    cancelled: HashSet<u64>,
    cancel_token: Arc<AtomicBool>,
    poll_pending: bool,
    preroll: VecDeque<f32>,
    worker_dropped: u64,
    dropped_at_start: u64,
}

fn recognizer_worker(
    mut recognizer: Box<dyn Recognizer>,
    commands: Receiver<RecognizerCommand>,
    responses: Sender<RecognizerResponse>,
) {
    while let Ok(command) = commands.recv() {
        let response = match command {
            RecognizerCommand::Start(id) => RecognizerResponse::Started(id, recognizer.start()),
            RecognizerCommand::Push(chunk) => {
                if let Err(error) = recognizer.push_audio(&chunk) {
                    if responses.send(RecognizerResponse::Fault(error)).is_err() {
                        break;
                    }
                }
                continue;
            }
            RecognizerCommand::Poll(id) => RecognizerResponse::Speech(id, recognizer.poll()),
            RecognizerCommand::Finish(id, cancelled) => {
                RecognizerResponse::Finished(id, recognizer.finish_cancellable(&cancelled))
            }
            RecognizerCommand::Cancel(id) => RecognizerResponse::Cancelled(id, recognizer.cancel()),
            RecognizerCommand::SetKeyterms(terms) => {
                RecognizerResponse::Configured(recognizer.set_keyterms(&terms))
            }
            RecognizerCommand::SetContext(text) => {
                RecognizerResponse::Configured(recognizer.set_context(&text))
            }
            RecognizerCommand::Shutdown => {
                let _ = recognizer.cancel();
                break;
            }
        };
        if responses.send(response).is_err() {
            break;
        }
    }
}

fn serve(args: ServeArgs) -> Result<()> {
    let LoadedRecognizer {
        provider,
        recognizer,
    } = load_recognizer(&args.common)?;
    let capture = AudioCapture::start(CaptureConfig {
        device_name: args.device,
        ..Default::default()
    })
    .map_err(anyhow::Error::from)?;
    let audio_rx = capture.receiver();
    let audio_errors = capture.error_receiver();
    let command_rx = command_reader();
    let poll_tick = tick(Duration::from_millis(80));
    let (recognizer_tx, worker_commands) = bounded(1024);
    let (worker_responses, recognizer_rx) = bounded(16);
    thread::spawn(move || recognizer_worker(recognizer, worker_commands, worker_responses));

    let preroll_capacity = ((capture.sample_rate as u64 * args.preroll_ms as u64) / 1000) as usize;
    let mut session = ServeSession {
        state: VoiceState::Idle,
        utterance: 0,
        cancelled: HashSet::new(),
        cancel_token: Arc::new(AtomicBool::new(false)),
        poll_pending: false,
        preroll: VecDeque::with_capacity(preroll_capacity.max(1)),
        worker_dropped: 0,
        dropped_at_start: 0,
    };

    emit(&Event::Ready {
        version: env!("CARGO_PKG_VERSION").into(),
        provider: provider.into(),
    })?;
    emit(&Event::Capabilities {
        stt: true,
        tts: false,
    })?;

    loop {
        select_biased! {
            recv(command_rx) -> command => {
                let Ok(command) = command else {
                    let _ = recognizer_tx.try_send(RecognizerCommand::Shutdown);
                    break;
                };
                match command {
                    Ok(command) => {
                        if handle_command(
                            command,
                            &mut session,
                            &audio_rx,
                            &capture,
                            &recognizer_tx,
                        )? {
                            break;
                        }
                    }
                    Err(message) => emit_error("bad_json", &message, session.state, false)?,
                }
            }
            recv(audio_errors) -> message => {
                let message = message.unwrap_or_else(|_| "audio error channel closed".into());
                emit_error("audio_error", &message, session.state, true)?;
                let _ = recognizer_tx.try_send(RecognizerCommand::Shutdown);
                return Err(anyhow!("audio input failed: {message}"));
            }
            recv(audio_rx) -> message => {
                let Ok(chunk) = message else {
                    emit_error("audio_error", "audio input stopped", session.state, true)?;
                    return Err(anyhow!("audio input stopped"));
                };
                if session.state == VoiceState::Listening {
                    forward_audio(&recognizer_tx, chunk, &mut session.worker_dropped)?;
                } else {
                    retain_preroll(&mut session.preroll, preroll_capacity, chunk.samples);
                }
            }
            recv(recognizer_rx) -> response => {
                let response = response.context("recognizer worker stopped")?;
                match response {
                    RecognizerResponse::Started(id, result) => {
                        if let Err(error) = result {
                            emit_error(
                                "recognizer_error",
                                &error.to_string(),
                                session.state,
                                true,
                            )?;
                            return Err(error.into());
                        }
                        if id == session.utterance
                            && session.state == VoiceState::Listening
                            && !session.cancelled.contains(&id)
                        {
                            emit(&Event::ListenStarted)?;
                        }
                    }
                    RecognizerResponse::Speech(id, result) => {
                        if id == session.utterance {
                            session.poll_pending = false;
                        }
                        match result {
                            Ok(events)
                                if id == session.utterance
                                    && session.state == VoiceState::Listening =>
                            {
                                for event in events {
                                    emit_speech(event)?;
                                }
                            }
                            Ok(_) => {}
                            Err(error) => {
                                emit_error(
                                    "recognizer_error",
                                    &error.to_string(),
                                    session.state,
                                    true,
                                )?;
                                return Err(error.into());
                            }
                        }
                    }
                    RecognizerResponse::Finished(id, result) => match result {
                        Ok(_) if session.cancelled.contains(&id) => {}
                        Ok(events)
                            if id == session.utterance
                                && session.state == VoiceState::Finalizing =>
                        {
                            session.poll_pending = false;
                            emit_finalized(events)?;
                            session.state.transition(VoiceState::Idle)?;
                            warn_dropped_audio(
                                &capture,
                                session.worker_dropped,
                                session.dropped_at_start,
                            );
                        }
                        Ok(_) => {}
                        Err(error) => {
                            emit_error(
                                "recognizer_error",
                                &error.to_string(),
                                session.state,
                                true,
                            )?;
                            return Err(error.into());
                        }
                    },
                    RecognizerResponse::Cancelled(id, result) => {
                        if let Err(error) = result {
                            emit_error(
                                "recognizer_error",
                                &error.to_string(),
                                session.state,
                                true,
                            )?;
                            return Err(error.into());
                        }
                        session.cancelled.remove(&id);
                    }
                    RecognizerResponse::Configured(result) => {
                        if let Err(error) = result {
                            emit_provider_error(&error, session.state)?;
                        }
                    }
                    RecognizerResponse::Fault(error) => {
                        emit_error(
                            "recognizer_error",
                            &error.to_string(),
                            session.state,
                            true,
                        )?;
                        return Err(error.into());
                    }
                }
            }
            recv(poll_tick) -> _ => {
                if session.state == VoiceState::Listening {
                    drain_audio_to_worker(
                        &audio_rx,
                        &recognizer_tx,
                        &mut session.worker_dropped,
                    )?;
                    if !session.poll_pending {
                        match recognizer_tx.try_send(RecognizerCommand::Poll(session.utterance)) {
                            Ok(()) => session.poll_pending = true,
                            Err(TrySendError::Full(_)) => {}
                            Err(TrySendError::Disconnected(_)) => {
                                return Err(anyhow!("recognizer worker stopped"));
                            }
                        }
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
    session: &mut ServeSession,
    audio_rx: &Receiver<awaz_core::AudioChunk>,
    capture: &AudioCapture,
    recognizer_tx: &Sender<RecognizerCommand>,
) -> Result<bool> {
    match command {
        Command::Hello => emit(&Event::Capabilities {
            stt: true,
            tts: false,
        })?,
        Command::ListenStart => {
            if session.state != VoiceState::Idle {
                emit_error("invalid_state", "already busy", session.state, false)?;
                return Ok(false);
            }

            session.utterance = session.utterance.wrapping_add(1);
            session.cancel_token = Arc::new(AtomicBool::new(false));
            session.dropped_at_start = capture
                .dropped_chunks()
                .saturating_add(session.worker_dropped);
            recognizer_tx
                .send(RecognizerCommand::Start(session.utterance))
                .context("recognizer worker stopped")?;
            session.state.transition(VoiceState::Listening)?;
            if !session.preroll.is_empty() {
                let samples = session.preroll.drain(..).collect();
                forward_audio(
                    recognizer_tx,
                    awaz_core::AudioChunk {
                        samples,
                        sample_rate: capture.sample_rate,
                    },
                    &mut session.worker_dropped,
                )?;
            }
        }
        Command::ListenStop => {
            if session.state != VoiceState::Listening {
                emit_error("invalid_state", "not listening", session.state, false)?;
                return Ok(false);
            }

            session.state.transition(VoiceState::Finalizing)?;
            drain_audio_to_worker(audio_rx, recognizer_tx, &mut session.worker_dropped)?;
            recognizer_tx
                .send(RecognizerCommand::Finish(
                    session.utterance,
                    Arc::clone(&session.cancel_token),
                ))
                .context("recognizer worker stopped")?;
        }
        Command::ListenCancel => {
            if session.state == VoiceState::Listening || session.state == VoiceState::Finalizing {
                let id = session.utterance;
                session.cancelled.insert(id);
                session.cancel_token.store(true, Ordering::Release);
                session.poll_pending = false;
                session.state = VoiceState::Idle;
                recognizer_tx
                    .send(RecognizerCommand::Cancel(id))
                    .context("recognizer worker stopped")?;
            }
            emit(&Event::ListenCancelled)?;
        }
        Command::KeytermsSet { terms } => recognizer_tx
            .send(RecognizerCommand::SetKeyterms(terms))
            .context("recognizer worker stopped")?,
        Command::ContextSet { text } => recognizer_tx
            .send(RecognizerCommand::SetContext(text))
            .context("recognizer worker stopped")?,
        Command::SpeakStart
        | Command::SpeakText { .. }
        | Command::SpeakEnd
        | Command::SpeakCancel => {
            emit_error(
                "unsupported",
                "TTS is reserved by the protocol but not implemented in Awaz v1",
                session.state,
                false,
            )?;
        }
        Command::Shutdown => {
            let _ = recognizer_tx.try_send(RecognizerCommand::Shutdown);
            emit(&Event::Shutdown)?;
            return Ok(true);
        }
    }
    Ok(false)
}

fn forward_audio(
    recognizer_tx: &Sender<RecognizerCommand>,
    chunk: awaz_core::AudioChunk,
    worker_dropped: &mut u64,
) -> Result<()> {
    match recognizer_tx.try_send(RecognizerCommand::Push(chunk)) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => {
            *worker_dropped += 1;
            Ok(())
        }
        Err(TrySendError::Disconnected(_)) => Err(anyhow!("recognizer worker stopped")),
    }
}

fn drain_audio_to_worker(
    audio_rx: &Receiver<awaz_core::AudioChunk>,
    recognizer_tx: &Sender<RecognizerCommand>,
    worker_dropped: &mut u64,
) -> Result<()> {
    // Use a snapshot so a live producer cannot keep this loop from reaching inference.
    for _ in 0..audio_rx.len() {
        let Ok(chunk) = audio_rx.try_recv() else {
            break;
        };
        forward_audio(recognizer_tx, chunk, worker_dropped)?;
    }
    Ok(())
}

fn warn_dropped_audio(capture: &AudioCapture, worker_dropped: u64, baseline: u64) {
    let total = capture.dropped_chunks().saturating_add(worker_dropped);
    let dropped = total.saturating_sub(baseline);
    if dropped > 0 {
        eprintln!("warning: dropped {dropped} audio chunks during the utterance");
    }
}

fn emit_finalized(events: Vec<SpeechEvent>) -> Result<()> {
    let mut final_text = String::new();
    for event in events {
        match event {
            SpeechEvent::Partial(text) => emit(&Event::TranscriptPartial { text })?,
            SpeechEvent::Final(text) => final_text = text,
        }
    }
    emit(&Event::TranscriptFinal { text: final_text })
}

fn emit_provider_error(error: &RecognizerError, state: VoiceState) -> Result<()> {
    let code = match error {
        RecognizerError::Unsupported(_) => "unsupported",
        _ => "recognizer_error",
    };
    emit_error(code, &error.to_string(), state, false)
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

    #[test]
    fn manifest_rejects_paths_outside_the_model_directory() {
        let manifest = serde_json::json!({
            "groups": [{"files": [{"name": "../escape", "url": "https://example.invalid"}]}]
        });
        assert!(manifest_files(&manifest).is_err());
    }

    #[test]
    fn manifest_requires_at_least_one_file() {
        let manifest = serde_json::json!({"groups": []});
        assert!(manifest_files(&manifest).is_err());
    }

    #[test]
    fn model_file_completion_checks_the_declared_size() {
        let root = std::env::temp_dir().join(format!(
            "awaz-model-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("model.bin"), b"1234").unwrap();
        let mut file = ModelFile {
            name: "model.bin".into(),
            url: "https://example.invalid".into(),
            size: Some(4),
            sha256: None,
        };
        assert!(model_file_complete(&root, &file));
        file.size = Some(5);
        assert!(!model_file_complete(&root, &file));
        std::fs::remove_dir_all(root).unwrap();
    }

    struct FakeRecognizer {
        actions: std::sync::Arc<std::sync::Mutex<Vec<&'static str>>>,
    }

    impl Recognizer for FakeRecognizer {
        fn start(&mut self) -> std::result::Result<(), RecognizerError> {
            self.actions.lock().unwrap().push("start");
            Ok(())
        }

        fn push_audio(
            &mut self,
            _chunk: &awaz_core::AudioChunk,
        ) -> std::result::Result<(), RecognizerError> {
            self.actions.lock().unwrap().push("audio");
            Ok(())
        }

        fn poll(&mut self) -> std::result::Result<Vec<SpeechEvent>, RecognizerError> {
            Ok(Vec::new())
        }

        fn finish(&mut self) -> std::result::Result<Vec<SpeechEvent>, RecognizerError> {
            self.actions.lock().unwrap().push("finish");
            Ok(vec![SpeechEvent::Final("done".into())])
        }

        fn cancel(&mut self) -> std::result::Result<(), RecognizerError> {
            self.actions.lock().unwrap().push("cancel");
            Ok(())
        }
    }

    #[test]
    fn recognizer_worker_preserves_finish_then_cancel_order() {
        let actions = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let recognizer = Box::new(FakeRecognizer {
            actions: actions.clone(),
        });
        let (command_tx, command_rx) = bounded(8);
        let (response_tx, response_rx) = bounded(8);
        let worker = thread::spawn(move || recognizer_worker(recognizer, command_rx, response_tx));

        command_tx.send(RecognizerCommand::Start(7)).unwrap();
        command_tx
            .send(RecognizerCommand::Finish(
                7,
                Arc::new(AtomicBool::new(false)),
            ))
            .unwrap();
        command_tx.send(RecognizerCommand::Cancel(7)).unwrap();
        command_tx.send(RecognizerCommand::Shutdown).unwrap();

        assert!(matches!(
            response_rx.recv().unwrap(),
            RecognizerResponse::Started(7, Ok(()))
        ));
        assert!(matches!(
            response_rx.recv().unwrap(),
            RecognizerResponse::Finished(7, Ok(_))
        ));
        assert!(matches!(
            response_rx.recv().unwrap(),
            RecognizerResponse::Cancelled(7, Ok(()))
        ));
        worker.join().unwrap();
        assert_eq!(
            actions.lock().unwrap().as_slice(),
            &["start", "finish", "cancel", "cancel"]
        );
    }

    #[test]
    fn provider_defaults_to_moonshine() {
        let cli = Cli::try_parse_from(["awaz", "transcribe", "audio.wav"]).unwrap();
        let CliCommand::Transcribe(args) = cli.command else {
            panic!("expected transcribe command");
        };
        assert_eq!(args.common.provider, ProviderArg::Moonshine);
    }

    #[test]
    fn apple_provider_is_selectable() {
        let cli = Cli::try_parse_from(["awaz", "transcribe", "--provider", "apple", "audio.wav"])
            .unwrap();
        let CliCommand::Transcribe(args) = cli.command else {
            panic!("expected transcribe command");
        };
        assert_eq!(args.common.provider, ProviderArg::Apple);
    }

    #[test]
    fn nemo_provider_and_parakeet_are_selectable() {
        let cli = Cli::try_parse_from([
            "awaz",
            "transcribe",
            "--provider",
            "nemo",
            "--nemo-model",
            "parakeet-tdt-v3",
            "audio.wav",
        ])
        .unwrap();
        let CliCommand::Transcribe(args) = cli.command else {
            panic!("expected transcribe command");
        };
        assert_eq!(args.common.provider, ProviderArg::Nemo);
        assert!(matches!(
            args.common.nemo_model,
            Some(NemoModelArg::ParakeetTdtV3)
        ));
    }

    #[test]
    fn sha256_matches_known_value() {
        let path = std::env::temp_dir().join(format!("awaz-sha256-{}", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            file_sha256(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        std::fs::remove_file(path).unwrap();
    }
}
