#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/moonshine-config.sh"

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
mkdir -p "$cache_root/models/moonshine" "$upstream_cache"

while read -r language model_size; do
  model_arch="$(moonshine_model_arch "$model_size")"
  slug="$(moonshine_model_slug "$model_size")"
  target="$cache_root/models/moonshine/$language/$slug"

  echo "Downloading Moonshine $language $slug model (one-time developer setup)…" >&2
  if ! output="$(MOONSHINE_VOICE_CACHE="$upstream_cache" uvx "moonshine-voice==$MOONSHINE_VERSION" download --stt --language "$language" --model-arch "$model_arch" 2>&1)"; then
    printf '%s\n' "$output" >&2
    exit 3
  fi
  printf '%s\n' "$output" >&2

  model_path="$(printf '%s\n' "$output" | sed -n 's/^Downloaded model path: //p' | tail -n1)"
  if [[ -z "$model_path" || ! -d "$model_path" ]]; then
    echo "Could not determine the downloaded $language $slug model path." >&2
    exit 3
  fi
  rm -rf "$target"
  mkdir -p "$(dirname "$target")"
  ln -s "$model_path" "$target"
  echo "Awaz model ready: $target -> $model_path" >&2
done < <(moonshine_models)
