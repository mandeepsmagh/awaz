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
         CPAL          speech providers
          │             /           \
 PipeWire/CoreAudio  Moonshine    Apple Speech
     /WASAPI          C ABI      Swift helper
          │              │            │
   mic / speaker   Moonshine/ONNX  macOS assets
   mic / speaker
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

## Pre-roll

`awaz serve` continuously retains a small rolling PCM window while idle (450 ms by default). When `listen.start` arrives, that pre-roll is fed to the recognizer before live chunks. This protects the first syllable when a user presses a hotkey and speaks nearly simultaneously.

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

Apple Speech is implemented in `awaz-apple-speech`. A small Swift helper adapts `SpeechAnalyzer` and `SpeechTranscriber` to the Rust contract. It receives PCM from Awaz through local pipes and never opens the microphone. macOS downloads and manages its language assets.

## State machine

```text
Idle → Listening → Finalizing → Idle
  └──── future → Speaking → Idle
                    └──────→ Listening   (future interruption)
```

`Speaking` exists only to preserve the duplex architecture. It is unreachable in v1.

## Utterance boundaries

The controller stops forwarding utterance audio when it accepts `listen.stop` or `listen.cancel`. Audio captured while the recognizer finalizes becomes bounded pre-roll for the next utterance. It never enters the utterance that is finalizing. Each worker command carries an utterance identity, so a cancelled final result cannot appear in a later utterance.

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

Source development stages only the pinned Moonshine runtime library for linking. Release archives bundle the native runtime beside `awaz` but ship no model weights. On first use `awaz` downloads the selected model into the user cache (`~/.cache/awaz`) using the manifest returned by the Moonshine library, so the file layout tracks the runtime version.

The provider is still replaceable: packaged files and model fetching are an implementation detail of `awaz-moonshine`, not `awaz-core`.

## Cross-platform strategy

- Linux/NixOS: CPAL, preferring PipeWire.
- macOS 26 or newer on Apple Silicon: CPAL/CoreAudio.
- Windows: CPAL/WASAPI.

Platform code should stay in audio/packaging layers. Integrations and provider-neutral contracts must not branch on OS.

## Reliability work after the first hardware pass

The architecture explicitly leaves room for:

- device route-change recovery;
- reopen-with-backoff without killing the session;
- transactional provider/model switching;
- model auto-benchmarking;
- TTS provider and playback;
- interruption/barge-in;
- global dictation integrations.

Those should be added only after the core STT path is measured on real hardware.
