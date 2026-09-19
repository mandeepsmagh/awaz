# Handoff

## Status

Awaz CLI 0.5.3 is released. The tag `v0.5.3` is published with all four platform archives.
Awaz UI 0.2.1 bundles this version through `AWAZ_ENGINE_REF`.

0.5.3 fixes the first-dictation-after-idle failure in `serve`. A microphone stream that fails
while idle no longer stops the process. The engine marks the stream for rebuild, stops reading
its channels so the protocol loop cannot spin, and builds a fresh stream from the current
default device before the next `listen.start`. A failed first `play` rebuilds once. An audio
failure while listening stays fatal. `AudioSession` in `awaz-cli` owns this policy. The
`Microphone` trait isolates it from a real device, so the unit tests run without one. Tests
cover the fatal policy, the stale-stream rebuild, the failed-start rebuild, the healthy
no-rebuild path, and the no-spin channels.

CLI reliability hardening is complete. The shared `tests/fixtures/jfk.wav` file provides a documented local smoke test for Moonshine, Apple Speech, and NeMo Speech. Model downloads now repair partial caches, serialize concurrent writers, and validate file sizes before installation. File transcription streams input to providers; offline-only Parakeet buffers one utterance because its SDK call requires contiguous audio. `serve` uses a bounded recognizer worker, keeps protocol control responsive, isolates audio across utterances, suppresses cancelled results, and reports runtime audio failures. Unsupported provider customization is explicit. Apple helper operations have timeouts. Interactive microphone output does not write ANSI sequences to redirected streams. The reusable provider conformance suite checks inactive operations, silence, cancellation, immediate restart, repeated utterances, stale events, final-event count, and customization capabilities. Moonshine, Apple Speech, Nemotron, and Parakeet pass on Apple Silicon. The suite found and fixed an Apple Speech timeout when an utterance contained no audio.

Moonshine, Apple Speech, and NeMo Speech are working STT providers. Select them with `--provider moonshine|apple|nemo` or `AWAZ_PROVIDER`. Moonshine remains the portable default. NeMo uses the native SDK C ABI and supports Nemotron 3.5 streaming plus Parakeet TDT v3 full-utterance recognition. Awaz downloads pinned NeMo GGUF files with size and SHA-256 verification. On an M2 Pro, all providers transcribed the 11-second JFK fixture correctly. Warm release runs, including process and model load, took 0.35 seconds for Apple Speech, 0.56 seconds for Moonshine, 0.56 seconds for Parakeet, and 1.59 seconds for streaming Nemotron. Peak RSS was 19 MiB, 625 MiB, 804 MiB, and 1,009 MiB respectively. These are smoke measurements, not a default-provider decision. macOS downloads Apple language assets on demand. Apple file transcription and the packaged macOS archive are verified on macOS 26; microphone tests remain. The Pi integration forwards `AWAZ_PROVIDER`. Its `Alt+R` shortcut cancels a recording while Awaz starts or finalizes. The Moonshine live loop now polls on a fixed tick and feeds queued audio before inference, which prevents continuous capture or a slow poll from adding avoidable latency. Release 0.5.2 changes the `serve` input stream lifecycle: the stream now runs only from `listen.start` until stop or cancel. The recognizer stays warm while idle. This prevents a persistent macOS microphone privacy indicator. Release 0.5.1 added provider conformance checks and the Apple Speech empty-utterance fix.

## Next

1. Confirm on physical hardware that the first dictation after a long idle pause works, and that
   the macOS microphone privacy indicator stays off while idle and clears after stop and cancel.
   The Awaz UI 0.2.1 release is the integration check.
2. Re-verify Lenovo Moonshine dictation and the Pi lazy-start/unload flow.
3. Run the native provider conformance suite on Linux arm64/x86_64 and Windows x86_64.
4. Add full stdio protocol tests with fake audio capture and recognizers.
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
- NeMo release archives use Metal on Apple Silicon and CPU on the current Linux and Windows packages. Keep NeMo's bundled C++ libraries out of build-time loader and linker paths; they can shadow libclang dependencies and conflict with Moonshine's required GLIBCXX symbols. Cargo links through the filtered `vendor/nemo/link` directory. Linux staging removes NeMo's older bundled `libstdc++`; both providers use the host runtime. Extract the Windows SDK ZIP with PowerShell because Git Bash `tar` does not support it.
- Parakeet TDT v3 has no streaming partials, key-term boosting, or cooperative cancellation during an active offline recognition call.
