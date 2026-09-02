# Development

## Supported targets

Awaz supports these source and release targets:

- Linux x86_64 and arm64;
- macOS 26 or newer on Apple Silicon;
- Windows x86_64.

The Cargo configuration sets the macOS deployment target to 26.0. The Moonshine provider rejects Intel macOS builds. Its prebuilt static library requires the macOS Clang runtime. `awaz-moonshine/build.rs` locates and links this runtime.

## Checks

Run these checks before a commit:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p awaz-cli
```
