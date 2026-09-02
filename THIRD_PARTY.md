# Third-party runtime

Awaz's first speech-to-text provider is Moonshine Voice by moonshine-ai.
Moonshine is not vendored in this source archive. Release packaging may bundle
its platform-specific native runtime and must preserve Moonshine's license and
notices.

- Project: https://github.com/moonshine-ai/moonshine
- License: MIT for the runtime and English streaming models at the time this
  repository was prepared. Verify upstream licensing when adding other models.
- Native API: Moonshine C API (`moonshine-c-api.h`), header version 30000.

Awaz intentionally talks to Moonshine through a narrow provider boundary so a
future recognizer can replace it without changing the audio core, CLI protocol,
or integrations.
