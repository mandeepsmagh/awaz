#!/usr/bin/env bash
set -euo pipefail

MOONSHINE_VERSION="${MOONSHINE_VERSION:-0.1.5}"
LANGUAGE="${AWAZ_LANGUAGE:-en}"
MODEL_ARCH="${AWAZ_MODEL_ARCH:-4}" # 4 = Small Streaming
slug="small-streaming"
case "$MODEL_ARCH" in
  2) slug="tiny-streaming" ;;
  4) slug="small-streaming" ;;
  5) slug="medium-streaming" ;;
  *) echo "AWAZ_MODEL_ARCH must be 2, 4, or 5" >&2; exit 2 ;;
esac

case "$(uname -s)" in
  Darwin) cache_root="$HOME/Library/Caches/awaz" ;;
  MINGW*|MSYS*|CYGWIN*)
    if [[ -n "${LOCALAPPDATA:-}" ]] && command -v cygpath >/dev/null 2>&1; then
      cache_root="$(cygpath -u "$LOCALAPPDATA")/awaz"
    else
      cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/awaz"
    fi
    ;;
  *) cache_root="${XDG_CACHE_HOME:-$HOME/.cache}/awaz" ;;
esac
upstream_cache="$cache_root/moonshine-upstream"
target="$cache_root/models/moonshine/$LANGUAGE/$slug"
mkdir -p "$(dirname "$target")" "$upstream_cache"

echo "Downloading Moonshine $slug model (one-time developer setup)…" >&2
output="$(MOONSHINE_VOICE_CACHE="$upstream_cache" uvx "moonshine-voice==$MOONSHINE_VERSION" download --stt --language "$LANGUAGE" --model-arch "$MODEL_ARCH" 2>&1 | tee /dev/stderr)"
model_path="$(printf '%s\n' "$output" | sed -n 's/^Downloaded model path: //p' | tail -n1)"
if [[ -z "$model_path" || ! -d "$model_path" ]]; then
  echo "Could not determine downloaded model path. Set AWAZ_MODEL_DIR manually." >&2
  exit 3
fi
rm -rf "$target"
ln -s "$model_path" "$target"
echo "Awaz model ready: $target -> $model_path" >&2
