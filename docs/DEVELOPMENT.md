# Development

## Supported targets

Awaz supports these source and release targets:

- Linux x86_64 and arm64;
- macOS 26 or newer on Apple Silicon;
- Windows x86_64.

The Cargo configuration sets the macOS deployment target to 26.0. The Moonshine provider rejects Intel macOS builds. Its prebuilt static library requires the macOS Clang runtime. `awaz-moonshine/build.rs` locates and links this runtime.

## Moonshine updates

Change `moonshine.version` to select a new Moonshine release. The runtime download, model setup, and release packaging scripts read this file. Git keeps the version and model files in LF format, and the loader also accepts CRLF input. Add published language and model pairs to `moonshine.models`. The first entry is the runtime default. Setup and packaging process all entries, but one Awaz process loads one model. `MOONSHINE_HEADER_VERSION` is a separate C ABI value. Update it only after comparing `moonshine-c-api.h` with the handwritten FFI declarations.

## Error protocol

Each protocol error includes the authoritative voice state. It also states whether the process must exit. Integrations must synchronize to that state after recoverable errors.

## Native code

Keep unsafe Rust in `awaz-moonshine`. Document the safety contract at each unsafe operation. Do not expose native pointers or handles through its safe API.

## Checks

Run these checks before a commit:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p awaz-cli
```
