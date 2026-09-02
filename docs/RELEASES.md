# Release matrix

GitHub Actions publishes self-contained archives for the native Moonshine runtime targets Awaz can currently stage automatically:

- Linux x86_64
- Linux arm64
- macOS 26 or newer on Apple Silicon
- Windows x86_64

Each release archive contains:

```text
awaz[.exe]
lib/                       # Linux native runtime; macOS is statically linked
*.dll                      # Windows runtime DLLs live beside awaz.exe
models/moonshine/en/small-streaming/
integrations/pi/
docs/
README.md
LICENSE
THIRD_PARTY.md
THIRD_PARTY_LICENSES/MOONSHINE-v0.1.5-LICENSE
```

Awaz does not support Intel macOS. Windows arm64 remains a future target.

macOS signing/notarization and Windows Authenticode signing are separate distribution-hardening steps and require project-owned signing identities/secrets; the workflow is intentionally usable before those secrets exist.
