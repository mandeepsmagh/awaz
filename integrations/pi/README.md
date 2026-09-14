# Awaz integration for Pi

This adapter is deliberately thin. It starts `awaz serve`, toggles push-to-talk,
and inserts the final transcript into Pi's editor. It does not submit the prompt,
so speech recognition mistakes can be corrected before sending.

Install from a checkout:

```sh
pi install ./integrations/pi
```

Then start Pi normally. The first `Alt+R` (or `/awaz`) starts `awaz serve` and
begins listening once ready. `Alt+R` stops an active recording. Press it again
while Awaz starts or finalizes to cancel that recording. `/awaz cancel` provides
the same cancellation; `/awaz unload` stops the process and frees the model. Set
`AWAZ_BIN`, `AWAZ_PROVIDER`, `AWAZ_LANGUAGE`, `AWAZ_MODEL`, `AWAZ_MODEL_DIR`,
`AWAZ_NEMO_MODEL`, `AWAZ_NEMO_MODEL_PATH`, or `AWAZ_DEVICE` to override defaults.
`AWAZ_PROVIDER` accepts `moonshine`, `apple`, or `nemo`; Apple requires macOS 26
or newer. `AWAZ_MODEL` selects a Moonshine size. `AWAZ_NEMO_MODEL` accepts
`nemotron-3.5` or `parakeet-tdt-v3`. Selected native models download into
`~/.cache/awaz` on first use.

Pi is only the first Awaz integration; no Pi behavior lives in `awaz-core`.
