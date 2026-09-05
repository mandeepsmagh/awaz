# Changelog

All notable changes to Awaz are documented here.

## 0.2.1 - Larger audio queue

- Raise the capture queue capacity from 64 to 1024 chunks, so a slow decoder during a cold start or on older CPUs does not drop audio.

## 0.2.0 - On-demand model download

- Download the selected Moonshine model on first use into the user cache, using the manifest returned by the Moonshine library. No Python or uv is required.
- Stop bundling model weights in release archives. Archives ship the binary and runtime library only.
- Use English Small Streaming as the default model.
- Add `awaz mic --save-wav` to dump captured audio for replay and debugging.
- Warn on stderr when audio chunks are dropped in `mic` and `serve`.
- Drain queued audio before finalizing a `mic` utterance.
- Download to a `.part` file and verify size before rename, so an interrupted download cannot look complete.
- Unify the model cache under `~/.cache/awaz` on Linux and macOS.

## 0.1.2 - English Medium default

- Use English Medium Streaming as the default Moonshine model.

## 0.1.1 - macOS audio diagnostics

- Fixed a panic when a macOS audio device reports no readable description.
- Report a clear no-input-device error instead of a raw CoreAudio OSStatus.

## 0.1.0 - Initial source release

- Added Rust 2024 portable core, explicit voice state and stable NDJSON protocol.
- Added CPAL microphone capture with bounded queues, mono PCM conversion and 450 ms pre-roll.
- Added Moonshine Voice v0.1.5 streaming STT provider through a small handwritten C ABI layer.
- Added `mic`, `transcribe`, `devices`, `doctor` and long-lived `serve` commands.
- Added runtime key-term/context biasing and provider-neutral STT contracts.
- Centralized the Moonshine version and bundled language/model manifest.
- Added authoritative state and fatal status to protocol errors.
- Added Pi integration with Alt+R push-to-talk and editable transcript insertion.
- Added Nix development shell plus Linux arm64/x86_64, macOS 26 arm64 and Windows x86_64 CI/release workflows.
- Reserved TTS protocol/state semantics without implementing synthesis in v1.
