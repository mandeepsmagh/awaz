# Awaz Architecture

## Core rule

Awaz owns audio and lifecycle. Speech engines are providers. Editors, agents, and applications are integrations.

```text
              integrations
          Pi / CLI / future apps
                    │
                    ▼
               awaz-core
              /         \
      awaz-audio       provider contract
          │                  │
         CPAL                speech providers
          │             /          |          \
 PipeWire/CoreAudio  Moonshine  Apple Speech  NeMo Speech
     /WASAPI          C ABI    Swift helper     C ABI
          │              │          │            │
   mic / speaker   native model  macOS assets    GGUF
```

## Authoritative audio timeline

The microphone is captured once by `awaz-audio`. Providers receive provider-neutral mono `f32` PCM chunks with their source sample rate. A recognizer is not allowed to open the microphone itself.

This is intentional:

- changing STT providers does not change device handling;
- TTS can later share the same audio ownership and cancellation rules;
- device recovery belongs in one place;
- integrations never deal with PCM.

## Real-time rule

The CPAL callback may convert/downmix and enqueue audio, but it must never run neural inference, JSON serialization, filesystem work, or a blocking send.

The queues are bounded. If consumers fall behind, Awaz counts dropped chunks rather than blocking the real-time capture callback. The protocol controller sends audio to a dedicated recognizer worker. Neural inference cannot block stdin commands or microphone queue draining.

## Microphone lifecycle

`awaz serve` keeps one microphone stream for the session and pauses it while the voice state
is idle, so the macOS microphone privacy indicator clears. `listen.start` starts the stream.
`listen.stop` and `listen.cancel` pause it. Idle audio is never captured.

A stream that fails while idle, or that fails to start, does not stop the process. The engine
rebuilds the stream from the current default input device before the next utterance. This
covers a device route change and a long idle pause. A stream failure while listening stays
fatal.

## Provider contract

`awaz-core::Recognizer` owns the minimal STT lifecycle:

```text
start
push_audio
poll
finish / cancellable finish
cancel
set_keyterms
set_context
```

Provider-specific concepts do not appear in protocol or integrations.

Moonshine is implemented in `awaz-moonshine` through a small handwritten C ABI binding. This avoids `bindgen` and generated bindings. macOS builds link the Clang runtime required by the prebuilt Moonshine library.

NeMo Speech is implemented in `awaz-nemo` through NVIDIA's stable native C ABI. Nemotron 3.5 uses a NeMo streaming recognition handle. Parakeet TDT v3 uses full-utterance recognition, so the provider buffers only that model's active utterance and emits no partial transcript. The provider never uses NeMo Speech's built-in microphone, CLI, or servers.

Apple Speech is implemented in `awaz-apple-speech`. A small Swift helper adapts `SpeechAnalyzer` and `SpeechTranscriber` to the Rust contract. It receives PCM from Awaz through local pipes and never opens the microphone. macOS downloads and manages its language assets.

## State machine

```text
Idle → Listening → Finalizing → Idle
  └──── future → Speaking → Idle
                    └──────→ Listening   (future interruption)
```

`Speaking` exists only to preserve the duplex architecture. It is unreachable in v1.

## Utterance boundaries

The controller stops forwarding utterance audio when it accepts `listen.stop` or `listen.cancel`. The stream is paused before the queued audio is drained to the recognizer, so the utterance tail is not lost and no live audio enters the completed utterance. Each worker command carries an utterance identity, so a cancelled final result cannot appear in a later utterance.

## Process model

`awaz serve` is a companion process, not a system daemon.

For Pi:

```text
Pi starts
  ↓ spawn
awaz serve
  ↓ load model once
ready
  ↓ repeated listen.start / listen.stop
Pi exits
  ↓ shutdown
Awaz exits
```

No port discovery, HTTP server, socket permissions, or stale background service is required. The controller remains responsive while the recognizer worker polls or finalizes. Cancellation immediately returns the public state to idle and suppresses pending events from the cancelled utterance. Providers can cooperatively interrupt a slow finalization. Apple Speech uses this path to cancel its asynchronous analyzer.

## Protocol rule

stdout is machine data only. stderr is diagnostics only. The stdio protocol is newline-delimited JSON so any language can integrate without linking Rust.

## Model/runtime packaging

Source development stages pinned Moonshine and NeMo Speech runtime libraries for linking. Release archives bundle the native runtimes beside `awaz` but ship no model weights. On first use, `awaz` downloads the selected model into the user cache (`~/.cache/awaz`). Moonshine supplies its own manifest. Awaz pins NeMo model revisions, sizes, and SHA-256 values from the SDK model index.

Provider packaging and model fetching do not appear in `awaz-core`.

## Cross-platform strategy

- Linux/NixOS: CPAL, preferring PipeWire.
- macOS 26 or newer on Apple Silicon: CPAL/CoreAudio.
- Windows: CPAL/WASAPI.

Platform code should stay in audio/packaging layers. Integrations and provider-neutral contracts must not branch on OS.

## Reliability work after the first hardware pass

The architecture explicitly leaves room for:

- transactional provider/model switching;
- model auto-benchmarking;
- TTS provider and playback;
- interruption/barge-in;
- global dictation integrations.

Those should be added only after the core STT path is measured on real hardware.
