# Speech fixtures

## `jfk.wav`

This file is the JFK sample distributed by the `whisper.cpp` project:

- Source: https://github.com/ggerganov/whisper.cpp/blob/master/samples/jfk.wav
- Content: excerpt from John F. Kennedy's 1961 inaugural address;
- Format: 16 kHz, mono, signed 16-bit PCM WAV;
- Duration: 11 seconds;
- SHA-256: `59dfb9a4acb36fe2a2affc14bacbee2920ff435cb13cc314a08c13f66ba7860e`.

Use this file for local provider smoke tests. Providers can use different punctuation and capitalization, so tests must not require byte-for-byte transcript equality.
