# Third-party runtime

Awaz's first speech-to-text provider is Moonshine Voice by moonshine-ai.
Moonshine is not vendored in this source archive. Release packaging may bundle
its platform-specific native runtime and must preserve Moonshine's license and
notices.

- Project: https://github.com/moonshine-ai/moonshine
- License: MIT for the runtime and English streaming models at the time this
  repository was prepared. Verify upstream licensing when adding other models.
- Release version: see [`moonshine.version`](moonshine.version).
- Bundled models: see [`moonshine.models`](moonshine.models).
- Native API: Moonshine C API (`moonshine-c-api.h`), header version 30000.

Awaz talks to Moonshine through a narrow provider boundary so another recognizer can be selected without changing the audio core, CLI protocol, or integrations.

## NeMo Speech

Awaz can bundle the NeMo-Speech.cpp native SDK from NVIDIA. The source repository does not vendor it. Release packaging preserves its license, notice, third-party notices, and bundled dependency licenses.

- Project: https://github.com/NVIDIA/NeMo-Speech.cpp
- Runtime license: Apache-2.0.
- Release version: see [`nemo.version`](nemo.version).
- Native API: stable `nemo_speech/asr.h` C ABI v1.
- Nemotron 3.5 license: NVIDIA Open Model License (OpenMDW 1.1).
- Parakeet TDT 0.6B v3 license: CC-BY-4.0.

Model weights are not part of Awaz release archives. Awaz downloads pinned GGUF artifacts from their NVIDIA Hugging Face repositories. Users remain responsible for the separate model terms.

## Speech test fixture

`tests/fixtures/jfk.wav` is the JFK sample distributed by `whisper.cpp`. It is
used for local provider smoke tests and is not included in Awaz release archives.
See `tests/fixtures/README.md` for its source and checksum.
