# Handoff

## Status

Moonshine is the working STT provider. Models download on first use into `~/.cache/awaz` (default `en small`); releases ship no model weights. The capture queue holds 1024 chunks so slow decoders do not drop audio. The Pi integration starts lazily on the first `Alt+R` and supports `/awaz unload`. Current release 0.2.2. The next phase adds Apple Speech as an optional provider.

## Next

1. Re-verify Lenovo dictation after the queue fix, and validate the Pi lazy-start/unload flow.
2. Add CLI provider selection: `--provider moonshine|apple`.
3. Add `awaz-apple-speech` for macOS 26 on Apple Silicon.
4. Use `SpeechAnalyzer` and `SpeechTranscriber` through a small Swift bridge.
5. Feed provider-neutral audio from `awaz-audio`. The Apple provider must not open the microphone.
6. Adapt Apple callback events to the existing `Recognizer` polling contract with a bounded queue.
7. Handle speech authorization and unavailable on-device language assets.
8. Add shared provider behavior checks before changing any platform default.
9. Compare latency, accuracy, memory, package size, and language support.
10. Keep Moonshine available on every supported platform. Keep it as the default until measurements support a change.

## Parked: keyterm/context biasing

Dictation misreads programming terms. Moonshine biases decoding toward jargon: `context` (raw text; the library auto-extracts ≤200 unusual terms, replacing the list each call) versus `keyterms` (a short explicit override). The bias is a nudge, not a guarantee, and overloading hurts general accuracy.

The `serve` path already handles `context.set`/`keyterms.set` end to end. Only the Pi extension is missing: send `context.set` (editor text plus the most recently read/edited files, tracked from `tool_call` paths and read via `node:fs`, trimmed to ~4 KB) just before `listen.start`.

Open: how many recent files; whole file vs window; a manual `/awaz context <path>` override; an `AWAZ_KEYTERMS` env override. Not decided.

## Gotchas

- Model download on first use needs `curl` and a network connection; releases bundle the runtime library but no model weights.
- The model cache is `~/.cache/awaz` on Linux and macOS (not `~/Library/Caches`); Windows uses `%LOCALAPPDATA%\awaz`.
- The download manifest comes from the Moonshine library, so the file layout tracks the runtime version; do not hardcode CDN paths.
- Apple Speech is a second provider, not a Moonshine replacement.
- The NDJSON protocol and Pi integration must remain provider-neutral.
- Apple framework APIs are asynchronous. Do not block audio capture while waiting for them.
- The current engineering rule confines unsafe Rust to Moonshine FFI. Review that rule before adding an Apple FFI boundary. Prefer a narrow provider-local boundary and document every unsafe operation.
- Apple on-device recognition depends on language assets managed by macOS.
