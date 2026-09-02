# Release matrix

GitHub Actions publishes self-contained archives for the native Moonshine runtime targets Awaz can currently stage automatically:

- Linux x86_64
- Linux arm64
- macOS Apple Silicon
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

macOS Intel and Windows arm64 are intentionally not advertised as binary releases until the pinned provider release has matching generic native runtime assets. Awaz core/audio remains portable to them; macOS Intel can also be enabled by building Moonshine from source.

macOS signing/notarization and Windows Authenticode signing are separate distribution-hardening steps and require project-owned signing identities/secrets; the workflow is intentionally usable before those secrets exist.
