# Awaz engineering rules

Awaz is portable local voice infrastructure, not a Moonshine wrapper and not a
Pi plugin.

## Boundaries

- `awaz-core` contains portable domain types and provider contracts only.
- `awaz-audio` owns microphone/device capture. Speech providers never own the mic.
- `awaz-moonshine` is the first STT provider and contains the only Moonshine FFI.
- `awaz-cli` owns orchestration and the stable stdio protocol.
- `integrations/*` adapt Awaz to external applications and must not leak into core.

## Runtime invariants

- Audio capture must never wait for neural inference.
- Queues are bounded. Prefer dropping diagnostic/partial work over blocking capture.
- Keep the recognizer loaded/warm for the lifetime of `awaz serve --stdio`.
- The microphone timeline is authoritative; recognizers are projections of it.
- Push-to-talk is the v1 interaction. Do not add external VAD/turn frameworks.
- Reserve TTS protocol/state capability, but do not implement TTS in v1.
- Keep unsafe Rust confined to `awaz-moonshine` FFI.
- stdout is machine/user data; diagnostics belong on stderr.
