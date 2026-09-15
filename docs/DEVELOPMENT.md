# Development

## Supported targets

Awaz supports these source and release targets:

- Linux x86_64 and arm64;
- macOS 26 or newer on Apple Silicon;
- Windows x86_64.

The Cargo configuration sets the macOS deployment target to 26.0. The Moonshine provider rejects Intel macOS builds. Its prebuilt static library requires the macOS Clang runtime. `awaz-moonshine/build.rs` locates and links this runtime.

## Moonshine updates

Change `moonshine.version` to select a new Moonshine release. The runtime download and release packaging scripts read this file. Git keeps the version and model files in LF format, and the loader also accepts CRLF input. The first entry in `moonshine.models` is the runtime default model; one Awaz process loads one model. Model weights are downloaded on first use into the user cache (`~/.cache/awaz` on Linux and macOS) using the manifest from `moonshine_get_stt_dependencies`, so the file layout tracks the runtime version and no CDN paths are hardcoded. Awaz validates every declared file on each load. It serializes concurrent downloads, validates temporary file sizes, and renames each file only after validation. An interrupted download must repair itself on the next run. `MOONSHINE_HEADER_VERSION` is a separate C ABI value. Update it only after comparing `moonshine-c-api.h` with the handwritten FFI declarations.

## NeMo Speech updates

Change `nemo.version` only after comparing the release `include/nemo_speech/asr.h` with `awaz-nemo/src/ffi.rs`. The v1 ABI uses size-prefixed append-only structures, but Awaz must still verify layouts and behavior before an update. Model metadata in `awaz-nemo/src/lib.rs` comes from the pinned release's `share/nemo-speech/model-index.json`. Update each revision, size, and SHA-256 value together. Release archives retain the complete NeMo Speech license directory.

## Microphone lifecycle

`awaz serve` keeps the recognizer process and model warm, but it keeps the input stream paused while the voice state is idle. Start the stream for `listen.start`. Pause it before draining the final queued audio for `listen.stop`, and pause it when a listening session is cancelled. Do not capture idle pre-roll. On macOS, an active idle stream leaves the system microphone privacy indicator visible and tells the user that Awaz is listening when it is not.

## Error protocol

Each protocol error includes the authoritative voice state. It also states whether the process must exit. Integrations must synchronize to that state after recoverable errors. Audio stream failures are fatal protocol errors. Optional provider operations return `unsupported` when a provider does not implement them.

## Native code

Keep unsafe Rust in provider-specific FFI crates. Document the safety contract at each unsafe operation. Do not expose native pointers or handles through a safe provider API.

`awaz-moonshine` wraps the Moonshine C ABI. `awaz-nemo` wraps the stable NeMo Speech C ABI from the release selected by `nemo.version`. Both use small handwritten declarations instead of generated bindings. `scripts/dev-setup.sh` stages both runtimes. NeMo release builds use the Metal archive on Apple Silicon and the portable CPU archive on Linux and Windows. Awaz captures the microphone and passes PCM to both providers.

`awaz-apple-speech` builds a small Swift helper on macOS. The helper receives mono `f32` PCM from Rust and adapts it to the format required by `SpeechAnalyzer`. It must not open the microphone. Keep its pipe protocol private to the provider. `scripts/package-release.sh` places the helper beside `awaz`.

## CI

Use current Node 24 action patch tags and GitHub's latest hosted runner aliases. Keep an explicit runner label when GitHub has no architecture-specific latest alias. The workflows stage the Moonshine and NeMo Speech runtimes; no Python or uv is involved. Do not put the full NeMo library directory in a build-time loader or linker path. Its bundled C++ runtime can shadow the runner toolchain, prevent libclang from loading, or conflict with Moonshine's required GLIBCXX symbols. The staging script creates a filtered `link` directory for Cargo. It removes NeMo's bundled Linux `libstdc++`, which is older than Moonshine's required GLIBCXX baseline. Both Linux providers use the host C++ runtime. Release builds use relative runpaths only. CI copies each staged package outside the workspace and clears loader-path variables before it runs the smoke test. Windows build jobs extract the SDK ZIP with PowerShell and expose a filtered ASR DLL directory. The release workflow packages no model weights, so model downloads happen on first use.

## Provider conformance

`awaz-provider-conformance` applies one lifecycle suite to every recognizer. Normal workspace tests run the harness against deterministic recognizers. Native tests are ignored unless selected explicitly because they require local models or Apple Speech assets.

```text
AWAZ_TEST_MOONSHINE_MODEL_DIR=/path/to/small-streaming cargo test -p awaz-provider-conformance --test native moonshine_conforms -- --ignored --exact
AWAZ_TEST_NEMOTRON_MODEL_PATH=/path/to/nemotron.gguf cargo test -p awaz-provider-conformance --test native nemotron_conforms -- --ignored --exact
AWAZ_TEST_PARAKEET_MODEL_PATH=/path/to/parakeet.gguf cargo test -p awaz-provider-conformance --test native parakeet_conforms -- --ignored --exact
cargo test -p awaz-provider-conformance --test native apple_speech_conforms -- --ignored --exact
```

Run native tests serially. Each test keeps one model loaded and checks silence, cancellation, restart, repeated utterances, final-event count, stale output, and customization capability errors.

## Checks

Run these checks before a commit. Run the final Swift command only on macOS.

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p awaz-cli
./target/release/awaz transcribe --provider nemo --nemo-model parakeet-tdt-v3 tests/fixtures/jfk.wav
./target/release/awaz transcribe --provider nemo --nemo-model nemotron-3.5 tests/fixtures/jfk.wav
xcrun swiftc -parse-as-library -O -warnings-as-errors crates/awaz-apple-speech/src/AppleSpeechBridge.swift -o /tmp/awaz-apple-speech
```
