# Awaz stdio protocol

`awaz serve` reads NDJSON commands from stdin and writes NDJSON events to stdout. One JSON object equals one line. Human diagnostics are written to stderr.

## Startup

Awaz emits:

```json
{"type":"ready","version":"0.1.0","provider":"moonshine"}
{"type":"capabilities","stt":true,"tts":false}
```

## Commands

### `hello`

```json
{"type":"hello"}
```

Returns the current `capabilities` event.

### `listen.start`

```json
{"type":"listen.start"}
```

Starts a fresh utterance. Awaz first feeds its configured pre-roll, then live microphone audio.

Event:

```json
{"type":"listen.started"}
```

### `listen.stop`

```json
{"type":"listen.stop"}
```

Stops/finalizes the utterance and always emits exactly one final transcript event. For silence, `text` is an empty string; this lets integrations deterministically leave their finalizing state.

```json
{"type":"transcript.final","text":"Hello from Awaz."}
```

### `listen.cancel`

```json
{"type":"listen.cancel"}
```

Discards the current utterance and returns to idle.

```json
{"type":"listen.cancelled"}
```

### `keyterms.set`

```json
{"type":"keyterms.set","terms":["Svelte","TypeScript","Kubernetes"]}
```

Provider-neutral request to bias recognition toward exact domain vocabulary. The Moonshine provider applies it to streaming decoding. Moonshine key terms must not contain commas.

### `context.set`

```json
{"type":"context.set","text":"This project uses Svelte, TypeScript, llama.cpp and Moonshine Voice."}
```

Lets a provider derive likely key terms from a free-form passage.

### Reserved TTS commands

The wire contract reserves these names now:

```json
{"type":"speak.start"}
{"type":"speak.text","text":"Hello"}
{"type":"speak.end"}
{"type":"speak.cancel"}
```

V1 replies with an `unsupported` error because `capabilities.tts` is false.

### `shutdown`

```json
{"type":"shutdown"}
```

Awaz emits:

```json
{"type":"shutdown"}
```

and exits cleanly.

## Transcript events

```json
{"type":"transcript.partial","text":"How do I"}
{"type":"transcript.final","text":"How do I make this async?"}
```

Partial text may change. Final text is stable for the utterance.

## Errors

Errors remain protocol data:

```json
{"type":"error","code":"invalid_state","message":"not listening","state":"idle","fatal":false}
```

`state` is the authoritative Awaz state after the error. Integrations must synchronize to it. `fatal` means the recognizer cannot continue and the process will exit.

Malformed input is reported as `bad_json` without terminating the process.
