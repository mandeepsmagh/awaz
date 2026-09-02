# Validation status

Awaz is designed so the repository can be checked at several layers before a physical microphone is involved.

## Performed while preparing this source archive

- compared the handwritten Moonshine FFI surface and struct layout against the pinned Moonshine Voice v0.1.5 C header;
- checked the v0.1.5 native release target names used by the bootstrap/release scripts;
- checked CPAL 0.18.2 API assumptions used by the audio layer;
- parsed all TOML, JSON and GitHub Actions YAML files;
- shell-syntax checked all `scripts/*.sh` files;
- type/syntax checked the Pi TypeScript adapter against a temporary minimal declaration of the Pi extension surface it uses;
- checked the source tree for conflict markers, stale Ctrl+R instructions and accidental generated/runtime files;
- verified the final ZIP archive structure and CRC integrity.

## Intentionally delegated to native CI / real machines

This artifact environment does not contain Rust/Cargo/Nix or physical audio devices, so it cannot truthfully claim the following checks were run here:

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build --release -p awaz-cli
awaz doctor
physical microphone capture
```

The included GitHub Actions workflows perform the Rust format/lint/test/build matrix after the repository is pushed. Physical microphone smoke tests still need real NixOS/Linux and macOS machines.

## First hardware acceptance pass

For the first real machine, validate in this order:

1. `./scripts/dev-setup.sh`
2. `cargo build --release -p awaz-cli`
3. `./target/release/awaz doctor`
4. `./target/release/awaz mic`
5. `./target/release/awaz serve` with `listen.start` / `listen.stop`
6. install `integrations/pi` and verify Alt+R inserts, but does not submit, the final transcript
7. repeat several utterances to confirm the model stays warm and no process is relaunched per turn

Any failure in those checks should be fixed at its owning boundary (audio, provider, protocol, integration) rather than by adding another framework layer.
