# Awaz

**Awaz** is a fast, local-first, provider-neutral voice I/O utility.

The first release does one thing deliberately well: **speech-to-text**. It owns microphone capture and exposes stable transcript events to humans, shells, editors, and agents. Moonshine Voice is the first recognizer provider; Pi is the first editor/agent integration. Neither is baked into the core architecture.

```text
microphone
   ↓
awaz-audio (CPAL)
   ↓
awaz-core
   ↓
SpeechRecognizer
   ↓
Moonshine / Apple Speech / NeMo Speech
   ↓
transcript events
   ├── CLI
   ├── stdio protocol
   └── Pi adapter      ← replaceable integration
```

## Design goals

- Local by default: microphone audio never needs to leave the machine.
- Fast: native audio and native Moonshine inference; the model stays warm in `awaz serve`.
- Invisible in daily use: one hotkey, speak, transcript appears.
- Portable: NixOS/Linux and macOS 26 or newer on Apple Silicon are first-class; Windows is kept architecturally portable.
- Replaceable providers: a future recognizer can replace Moonshine without changing integrations.
- Duplex-ready: v1 is STT only, while the protocol/state machine reserves clean TTS and interruption semantics.
- Boring reliability: bounded queues, pre-roll, explicit state, no HTTP server, no background system daemon.

## What is implemented

- Rust 2024 workspace.
- CPAL microphone capture with PipeWire preference on Linux and native platform backends elsewhere.
- Bounded capture and recognizer queues; capture and protocol control do not block on inference.
- 450 ms configurable pre-roll in machine/agent mode.
- Moonshine streaming STT through its documented C ABI.
- Apple Speech streaming STT through a provider-local Swift helper on macOS 26+.
- NeMo Speech through its native C ABI, with Nemotron 3.5 and Parakeet TDT v3.
- `--provider moonshine|apple|nemo` selection; Moonshine remains the portable default.
- Runtime key-term and free-form context biasing.
- `awaz mic` push-to-talk CLI.
- `awaz transcribe FILE.wav` with streaming WAV input; offline-only models buffer the utterance.
- On-demand model download on first use, cached under `~/.cache/awaz`.
- `--save-wav` capture dump and dropped-audio-chunk warnings for debugging.
- `awaz devices` and `awaz doctor`.
- `awaz serve` stable NDJSON protocol over stdin/stdout.
- Pi TypeScript integration: `Alt+R` toggles listening and inserts the final transcript into Pi's editor without auto-submitting.
- Future TTS command names reserved in the protocol; v1 reports `tts: false`.
- Nix development shell.
- GitHub Actions verification and release workflows.

## Quick start from a release

Download the archive for your platform from GitHub Releases. Extract the complete `awaz` directory and keep its contents together.

On Linux or macOS:

```bash
tar -xzf awaz-<platform>.tar.gz
export PATH="$PWD/awaz:$PATH"
awaz doctor
awaz mic
pi install "$PWD/awaz/integrations/pi"
```

Add the extracted directory to your shell's persistent `PATH` after validation.

On Windows, extract `awaz-windows-x86_64.zip`, add the directory containing `awaz.exe` to `PATH`, and run:

```powershell
awaz doctor
awaz mic
pi install .\awaz\integrations\pi
```

Do not copy only the executable. Awaz ships platform libraries in the extracted directory, but speech models are **not** bundled: the first time you select a model, Awaz downloads it into your user cache (`~/.cache/awaz` on Linux and macOS, `%LOCALAPPDATA%\awaz` on Windows) and reuses it from then on. The first run therefore needs a network connection and `curl`. Release archives do not require Rust, Python, or uv. macOS may request microphone permission on the first run. The current macOS and Windows archives are unsigned, so the operating system can require manual approval.

## Quick start from source

Prerequisites for a source build are Rust and native audio development libraries; macOS builds also require the Xcode command line tools. `curl` is needed only when Awaz downloads a speech model on first use. `scripts/dev-setup.sh` stages the Moonshine and NeMo Speech runtime libraries for linking; model files are fetched on demand by the binary, not by the build.

### NixOS

```bash
nix develop
./scripts/dev-setup.sh
cargo build --release -p awaz-cli
./target/release/awaz doctor
./target/release/awaz mic
```

### Linux / macOS 26+ on Apple Silicon

With Rust 1.98 and the platform audio development libraries installed:

```bash
./scripts/dev-setup.sh
cargo build --release -p awaz-cli
./target/release/awaz doctor
./target/release/awaz mic
```

## CLI

```bash
awaz devices
awaz doctor
awaz mic
awaz mic --device "My Microphone"
awaz mic --provider apple
awaz mic --provider nemo
awaz transcribe recording.wav
awaz transcribe --provider apple recording.wav
awaz transcribe --provider nemo --nemo-model parakeet-tdt-v3 recording.wav
awaz serve
```

Moonshine is the default provider. Select Apple Speech on macOS 26 or newer with `--provider apple`, or select NeMo Speech with `--provider nemo`. The `AWAZ_PROVIDER` variable accepts the same names. macOS manages Apple language assets. Awaz downloads Moonshine and NeMo models on demand.

The first entry in `moonshine.models` is the default Moonshine model. English Small Streaming is the current default. Select a Moonshine model with `--model` (or `--language`); Awaz downloads that model on first use and caches it:

```bash
awaz mic --model tiny
awaz mic --model medium
awaz serve --language es --model small
```

Model files live under `~/.cache/awaz/models/moonshine/<language>/<size>-streaming/` (`%LOCALAPPDATA%\awaz\...` on Windows). Only published language and model pairs are available; the current Moonshine catalog does not publish Hindi or Punjabi STT models.

For fully offline Moonshine use, pre-stage a model directory and point at it:

```bash
AWAZ_MODEL_DIR=/path/to/model awaz mic
```

NeMo Speech uses Nemotron 3.5 by default because it supports streaming and file transcription. Select Parakeet TDT v3 for fast full-utterance recognition:

```bash
awaz transcribe --provider nemo --nemo-model parakeet-tdt-v3 recording.wav
awaz mic --provider nemo --nemo-model parakeet-tdt-v3
```

Parakeet TDT v3 buffers an utterance and returns text only after stop. It does not provide partial transcripts or cooperative cancellation during inference. Its Q8 model is approximately 681 MiB. Nemotron 3.5 streams partial results and supports key-term biasing. Its Q8 model is approximately 707 MiB. Use `--nemo-model-path` or `AWAZ_NEMO_MODEL_PATH` to select a pre-staged GGUF file.

To dump the audio a `mic` session actually captured, add `--save-wav utter.wav` (or `AWAZ_SAVE_WAV`), then replay it with `awaz transcribe utter.wav`.

## Machine protocol

Run:

```bash
awaz serve
```

Then write one JSON object per line to stdin:

```json
{"type":"listen.start"}
{"type":"listen.stop"}
```

Awaz emits one JSON object per line on stdout:

```json
{"type":"transcript.partial","text":"why is this"}
{"type":"transcript.final","text":"Why is this not working?"}
```

Diagnostics go to stderr. See [`docs/PROTOCOL.md`](docs/PROTOCOL.md).

## Pi integration

After building/installing `awaz` so it is on `PATH`:

```bash
pi install ./integrations/pi
```

Then start Pi normally. The first `Alt+R` (or `/awaz`) starts Awaz lazily and begins listening once ready.

- `Alt+R`: start/stop push-to-talk; press again while starting or finalizing to cancel
- `/awaz`: start/stop push-to-talk; run it again while starting or finalizing to cancel
- `/awaz cancel`: discard the current utterance
- `/awaz unload`: stop Awaz and free the loaded model

The final transcript is pasted into Pi's input editor so it can be corrected before Enter is pressed.

Environment overrides (read once per session, when Awaz starts):

```text
AWAZ_BIN       path to the awaz binary (default: awaz on PATH)
AWAZ_PROVIDER        moonshine | apple | nemo (default: moonshine)
AWAZ_LANGUAGE        language code (provider default when unset)
AWAZ_MODEL           tiny | small | medium (Moonshine only)
AWAZ_MODEL_DIR       pre-staged Moonshine model directory
AWAZ_NEMO_MODEL      nemotron-3.5 | parakeet-tdt-v3
AWAZ_NEMO_MODEL_PATH pre-staged NeMo Speech GGUF
AWAZ_DEVICE    microphone device name (see awaz devices)
```

## Source versus release packaging

A source checkout intentionally does **not** commit native provider binaries or model weights. `scripts/dev-setup.sh` stages the Moonshine and NeMo Speech runtime libraries for linking. The binary downloads model weights on first use.

The release workflow stages the Moonshine and NeMo Speech runtime libraries beside the `awaz` binary but ships **no** model weights. At runtime, Moonshine resolves a model directory in this order:

1. `--model-dir` / `AWAZ_MODEL_DIR`
2. a `models/moonshine/<language>/<model>` directory beside the executable
3. the user cache directory

NeMo Speech resolves a GGUF in this order:

1. `--nemo-model-path` / `AWAZ_NEMO_MODEL_PATH`
2. `models/nemo/<model>/<filename>` beside the executable
3. the user cache directory

Awaz uses the Moonshine library manifest for Moonshine files. It uses pinned SDK index metadata for NeMo files. Downloads use a lock and temporary file. NeMo downloads also verify SHA-256 before installation.

## Platform status

Target architecture:

| Platform | Audio | Provider | Intended status |
|---|---|---|---|
| NixOS / Linux x86_64 | CPAL → PipeWire/ALSA | Moonshine or NeMo Speech | first-class |
| Linux arm64 | CPAL → PipeWire/ALSA | Moonshine or NeMo Speech | release target |
| macOS 26+ on Apple Silicon | CPAL → CoreAudio | Moonshine, Apple Speech, or NeMo Speech | first-class |
| Windows x86_64 | CPAL → WASAPI | Moonshine or NeMo Speech | portable / CI target |
| Windows arm64 | CPAL → WASAPI | provider-dependent | future release target |

Real microphone behavior still needs validation on physical hardware for each platform; CI can fully exercise protocol/state logic and file/fixture transcription, but hosted runners do not substitute for device testing.

## Future TTS

The public protocol already reserves:

```text
speak.start
speak.text
speak.end
speak.cancel
```

and the voice state includes a reserved `Speaking` state. V1 does not implement synthesis. When TTS is added, it should be a provider under the same Awaz audio ownership boundary, not a second unrelated service.

## Validation philosophy

Before a release is considered solid:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- deterministic WAV fixture → recognizer smoke test
- shared lifecycle conformance suite for each provider
- `awaz doctor`
- physical microphone smoke tests on NixOS/Linux and macOS 26+ on Apple Silicon
- packaged archive smoke test

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/VALIDATION.md`](docs/VALIDATION.md).

## License

Awaz is MIT licensed. Provider runtimes and model licenses are separate; see [`THIRD_PARTY.md`](THIRD_PARTY.md).
