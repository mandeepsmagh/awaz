# Development

## Supported targets

Awaz supports these source and release targets:

- Linux x86_64 and arm64;
- macOS 26 or newer on Apple Silicon;
- Windows x86_64.

The Cargo configuration sets the macOS deployment target to 26.0. The Moonshine provider rejects Intel macOS builds. Its prebuilt static library requires the macOS Clang runtime. `awaz-moonshine/build.rs` locates and links this runtime.

## Moonshine updates

Change `moonshine.version` to select a new Moonshine release. The runtime download and release packaging scripts read this file. Git keeps the version and model files in LF format, and the loader also accepts CRLF input. The first entry in `moonshine.models` is the runtime default model; one Awaz process loads one model. Model weights are downloaded on first use into the user cache (`~/.cache/awaz` on Linux and macOS) using the manifest from `moonshine_get_stt_dependencies`, so the file layout tracks the runtime version and no CDN paths are hardcoded. `MOONSHINE_HEADER_VERSION` is a separate C ABI value. Update it only after comparing `moonshine-c-api.h` with the handwritten FFI declarations.

## Error protocol

Each protocol error includes the authoritative voice state. It also states whether the process must exit. Integrations must synchronize to that state after recoverable errors.

## Native code

Keep unsafe Rust in `awaz-moonshine`. Document the safety contract at each unsafe operation. Do not expose native pointers or handles through its safe API.

## CI

Use current Node 24 action patch tags and GitHub's latest hosted runner aliases. Keep an explicit runner label when GitHub has no architecture-specific latest alias. The workflows stage only the Moonshine runtime; no Python or uv is involved. The release workflow packages no model weights, so model downloads happen on first use.

## Checks

Run these checks before a commit:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p awaz-cli
```
