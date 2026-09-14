# Release matrix

GitHub Actions publishes self-contained archives for the native Moonshine and NeMo Speech runtime targets Awaz can stage automatically:

- Linux x86_64
- Linux arm64
- macOS 26 or newer on Apple Silicon
- Windows x86_64

Each release archive contains:

```text
awaz[.exe]
awaz-apple-speech          # macOS only
lib/                       # Linux and macOS dynamic provider libraries
*.dll                      # Windows runtime DLLs live beside awaz.exe
integrations/pi/
docs/
README.md
LICENSE
moonshine.version
moonshine.models
nemo.version
THIRD_PARTY.md
THIRD_PARTY_LICENSES/MOONSHINE-v<version>-LICENSE
THIRD_PARTY_LICENSES/NEMO-SPEECH-v<version>/
```

## Installation

Extract the complete archive and keep its directory tree intact. Add the directory that contains `awaz` or `awaz.exe` to `PATH`. Run `awaz doctor` before the first transcription. Install the bundled Pi adapter from `integrations/pi`.

Do not move only the executable. Awaz resolves its platform libraries and the macOS Apple Speech helper relative to it. Speech models are not bundled: on first use Awaz downloads the selected model into the user cache (`~/.cache/awaz` on Linux and macOS, `%LOCALAPPDATA%\awaz` on Windows), which requires `curl` and a network connection. The archives do not require a source setup, Rust, Python, or uv.

The macOS and Windows archives are not signed. The operating system can require manual approval. macOS can also request microphone permission on the first run.

## Support notes

Awaz does not support Intel macOS. Windows arm64 remains a future target.

The root `moonshine.version` and `nemo.version` files select the native runtime releases. The first entry in `moonshine.models` is the default Moonshine model. NeMo model revisions and checksums are pinned in `awaz-nemo` from the selected SDK model index.

macOS signing/notarization and Windows Authenticode signing are separate distribution-hardening steps and require project-owned signing identities/secrets; the workflow is intentionally usable before those secrets exist.
