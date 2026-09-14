# Handoff

## Status

CLI reliability hardening is complete. Model downloads now repair partial caches, serialize concurrent writers, and validate file sizes before installation. File transcription uses bounded memory. `serve` uses a bounded recognizer worker, keeps protocol control responsive, isolates audio across utterances, suppresses cancelled results, and reports runtime audio failures. Unsupported provider customization is explicit. Apple helper operations have timeouts. Interactive microphone output does not write ANSI sequences to redirected streams.

Moonshine and Apple Speech are working STT providers. Select them with `--provider moonshine|apple` or `AWAZ_PROVIDER`. Moonshine remains the portable default and downloads its selected model on demand. macOS downloads Apple language assets on demand. Apple file transcription is verified on macOS 26; microphone and packaged-release tests remain. The Pi integration forwards `AWAZ_PROVIDER`. The Moonshine live loop now polls on a fixed tick and feeds queued audio before inference, which prevents continuous capture or a slow poll from adding avoidable latency. Current release 0.3.0.

## Next

1. Verify Apple `mic` and `serve` on physical hardware.
2. Smoke-test the packaged macOS archive and confirm the helper stays beside `awaz`.
3. Re-verify Lenovo Moonshine dictation and the Pi lazy-start/unload flow.
4. Add shared provider behavior checks before changing any platform default.
5. Measure model load, live partial, and stop-to-final latency for each provider.
6. Evaluate `parakeet-rs` as a later provider. Download model files on demand and expose only supported model names.
7. Keep Moonshine available on every supported platform. Keep it as the default until measurements support a change.

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
- Apple framework APIs are asynchronous. The provider uses a Swift helper so no Apple FFI or unsafe Rust is needed.
- `awaz-audio` remains the sole microphone owner. The helper receives PCM through a private local pipe.
- Apple on-device recognition depends on language assets managed by macOS.
- Apple has no selectable model size. Reject `--model` and `--model-dir` with `--provider apple`.
