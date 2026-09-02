# Awaz integration for Pi

This adapter is deliberately thin. It starts `awaz serve`, toggles push-to-talk,
and inserts the final transcript into Pi's editor. It does not submit the prompt,
so speech recognition mistakes can be corrected before sending.

Install from a checkout:

```sh
pi install ./integrations/pi
```

Then start Pi normally. `Alt+R` toggles recording; `/awaz cancel` discards a
recording. Set `AWAZ_BIN`, `AWAZ_MODEL`, `AWAZ_MODEL_DIR`, or `AWAZ_DEVICE` to
override defaults.

Pi is only the first Awaz integration; no Pi behavior lives in `awaz-core`.
