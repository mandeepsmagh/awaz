# Handoff

## Status

Moonshine is the working STT provider. Its release version and bundled models are centrally configured. Model setup supports Unix and Windows shell behavior. CI pins resolvable Node 24 action patch tags and disables unused uv caching. macOS targets version 26 or newer on Apple Silicon. The next phase adds Apple Speech as an optional provider.

## Next

1. Confirm the v0.1.0 release workflow, then complete the Moonshine microphone and Pi hardware checks.
2. Add CLI provider selection: `--provider moonshine|apple`.
3. Add `awaz-apple-speech` for macOS 26 on Apple Silicon.
4. Use `SpeechAnalyzer` and `SpeechTranscriber` through a small Swift bridge.
5. Feed provider-neutral audio from `awaz-audio`. The Apple provider must not open the microphone.
6. Adapt Apple callback events to the existing `Recognizer` polling contract with a bounded queue.
7. Handle speech authorization and unavailable on-device language assets.
8. Add shared provider behavior checks before changing any platform default.
9. Compare latency, accuracy, memory, package size, and language support.
10. Keep Moonshine available on every supported platform. Keep it as the default until measurements support a change.

## Gotchas

- Apple Speech is a second provider, not a Moonshine replacement.
- The NDJSON protocol and Pi integration must remain provider-neutral.
- Apple framework APIs are asynchronous. Do not block audio capture while waiting for them.
- The current engineering rule confines unsafe Rust to Moonshine FFI. Review that rule before adding an Apple FFI boundary. Prefer a narrow provider-local boundary and document every unsafe operation.
- Apple on-device recognition depends on language assets managed by macOS.
