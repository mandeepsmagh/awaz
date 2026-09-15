# Handoff

## Status

CLI reliability hardening is complete. The shared `tests/fixtures/jfk.wav` file provides a documented local smoke test for Moonshine, Apple Speech, and NeMo Speech. Model downloads now repair partial caches, serialize concurrent writers, and validate file sizes before installation. File transcription streams input to providers; offline-only Parakeet buffers one utterance because its SDK call requires contiguous audio. `serve` uses a bounded recognizer worker, keeps protocol control responsive, isolates audio across utterances, suppresses cancelled results, and reports runtime audio failures. Unsupported provider customization is explicit. Apple helper operations have timeouts. Interactive microphone output does not write ANSI sequences to redirected streams.

Moonshine, Apple Speech, and NeMo Speech are working STT providers. Select them with `--provider moonshine|apple|nemo` or `AWAZ_PROVIDER`. Moonshine remains the portable default. NeMo uses the native SDK C ABI and supports Nemotron 3.5 streaming plus Parakeet TDT v3 full-utterance recognition. Awaz downloads pinned NeMo GGUF files with size and SHA-256 verification. On an M2 Pro, all providers transcribed the 11-second JFK fixture correctly. Warm release runs, including process and model load, took 0.35 seconds for Apple Speech, 0.56 seconds for Moonshine, 0.56 seconds for Parakeet, and 1.59 seconds for streaming Nemotron. Peak RSS was 19 MiB, 625 MiB, 804 MiB, and 1,009 MiB respectively. These are smoke measurements, not a default-provider decision. macOS downloads Apple language assets on demand. Apple file transcription and the packaged macOS archive are verified on macOS 26; microphone tests remain. The Pi integration forwards `AWAZ_PROVIDER`. Its `Alt+R` shortcut cancels a recording while Awaz starts or finalizes. The Moonshine live loop now polls on a fixed tick and feeds queued audio before inference, which prevents continuous capture or a slow poll from adding avoidable latency. Current release 0.5.0.

## Next

1. Verify Apple `mic` and `serve` on physical hardware.
2. Re-verify Lenovo Moonshine dictation and the Pi lazy-start/unload flow.
3. Verify NeMo runtime staging, loading, and packaged library lookup on Linux arm64/x86_64 and Windows x86_64.
4. Add shared provider behavior checks before changing any platform default.
5. Measure model load, live partial, and stop-to-final latency for each provider on the same saved microphone recordings.
6. Test NeMo `serve` cancellation and immediate restart. Parakeet cannot cooperatively interrupt active offline inference.
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
- Apple has no selectable model size. Reject model options with `--provider apple`.
- NeMo Speech v0.1.0 emits native backend diagnostics to stderr because its C ABI has no log callback. stdout remains clean.
- NeMo release archives use Metal on Apple Silicon and CPU on the current Linux and Windows packages.
- Parakeet TDT v3 has no streaming partials, key-term boosting, or cooperative cancellation during an active offline recognition call.
