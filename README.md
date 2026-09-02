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
awaz-moonshine  ← replaceable provider
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
- Bounded audio queue; capture does not block on inference.
- 450 ms configurable pre-roll in machine/agent mode.
- Moonshine streaming STT through its documented C ABI.
- Runtime key-term and free-form context biasing.
- `awaz mic` push-to-talk CLI.
- `awaz transcribe FILE.wav` for mono WAV input.
- `awaz devices` and `awaz doctor`.
- `awaz serve` stable NDJSON protocol over stdin/stdout.
- Pi TypeScript integration: `Alt+R` toggles listening and inserts the final transcript into Pi's editor without auto-submitting.
- Future TTS command names reserved in the protocol; v1 reports `tts: false`.
- Nix development shell.
- GitHub Actions verification and release workflows.

## Quick start from source

Prerequisites for a source build are Rust, native audio development libraries, `curl`, and `uv` for the one-time Moonshine model bootstrap. macOS builds also require the Xcode command line tools. **Released Awaz archives are intended to bundle the Moonshine runtime and default model so users do not need Python/uv.**

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

`dev-setup.sh` stages the pinned Moonshine native library and downloads the English Small Streaming model once. The model is cached under your user cache directory. Awaz supports macOS 26 or newer on Apple Silicon only.

## CLI

```bash
awaz devices
awaz doctor
awaz mic
awaz mic --device "My Microphone"
awaz transcribe recording.wav
awaz serve
```

The default recognizer is English Small Streaming. Override it when benchmarking:

```bash
awaz mic --model tiny
awaz mic --model medium
```

Or provide an explicit model directory:

```bash
AWAZ_MODEL_DIR=/path/to/model awaz mic
```

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

Then start Pi normally. Awaz is launched as a Pi-owned companion process and the model remains warm for the session.

- `Alt+R`: start/stop push-to-talk
- `/awaz`: start/stop push-to-talk
- `/awaz cancel`: discard the current utterance

The final transcript is pasted into Pi's input editor so it can be corrected before Enter is pressed.

Environment overrides:

```text
AWAZ_BIN
AWAZ_MODEL
AWAZ_MODEL_DIR
AWAZ_DEVICE
```

## Source versus release packaging

A source checkout intentionally does **not** commit Moonshine binaries or model weights. `scripts/dev-setup.sh` fetches those for development.

The release workflow stages the pinned Moonshine runtime and default English Small Streaming model, then packages them beside the `awaz` binary. At runtime Awaz searches, in order:

1. `--model-dir` / `AWAZ_MODEL_DIR`
2. a bundled `models/moonshine/<language>/<model>` directory beside the executable
3. the user cache directory

This keeps normal release use self-contained while preserving clean source licensing and provider replacement.

## Platform status

Target architecture:

| Platform | Audio | Provider | Intended status |
|---|---|---|---|
| NixOS / Linux x86_64 | CPAL → PipeWire/ALSA | Moonshine native | first-class |
| Linux arm64 | CPAL → PipeWire/ALSA | Moonshine native | release target |
| macOS 26+ on Apple Silicon | CPAL → CoreAudio | Moonshine native | first-class |
| Windows x86_64 | CPAL → WASAPI | Moonshine native | portable / CI target |
| Windows arm64 | CPAL → WASAPI | provider-dependent | future release target |

Real microphone behavior still needs validation on physical hardware for each platform; CI can fully exercise protocol/state logic and file/fixture transcription, but hosted runners do not substitute for device testing.

## Why no Python, ffmpeg, Pipecat, or HTTP daemon?

They are not required on Awaz's hot path. The native process captures audio directly, calls the provider directly, and talks to integrations over stdio. Python/`uvx` is used only as a convenient source-development model downloader until Awaz has its own provider catalog/downloader.

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
- `awaz doctor`
- physical microphone smoke tests on NixOS/Linux and macOS 26+ on Apple Silicon
- packaged archive smoke test

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) and [`docs/VALIDATION.md`](docs/VALIDATION.md).

## License

Awaz is MIT licensed. Moonshine Voice and its model licensing are separate; see [`THIRD_PARTY.md`](THIRD_PARTY.md).
