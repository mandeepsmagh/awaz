# Changelog

All notable changes to Awaz are documented here.

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
