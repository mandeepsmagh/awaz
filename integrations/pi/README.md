# Awaz integration for Pi

This adapter is deliberately thin. It starts `awaz serve`, toggles push-to-talk,
and inserts the final transcript into Pi's editor. It does not submit the prompt,
so speech recognition mistakes can be corrected before sending.

Install from a checkout:

```sh
pi install ./integrations/pi
```

Then start Pi normally. The first `Alt+R` (or `/awaz`) starts `awaz serve` and
begins listening once ready. `/awaz cancel` discards a recording; `/awaz unload`
stops the process and frees the model. Set `AWAZ_BIN`, `AWAZ_LANGUAGE`,
`AWAZ_MODEL`, `AWAZ_MODEL_DIR`, or `AWAZ_DEVICE` to override defaults.
`AWAZ_MODEL` accepts `tiny`, `small`, or `medium` (default `small`); the
selected model downloads into `~/.cache/awaz` on first use.

Pi is only the first Awaz integration; no Pi behavior lives in `awaz-core`.
