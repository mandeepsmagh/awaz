#!/usr/bin/env bash

config_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version_file="$config_root/moonshine.version"
MOONSHINE_MODELS_FILE="$config_root/moonshine.models"

if [[ ! -s "$version_file" ]]; then
  echo "Moonshine version file is missing or empty: $version_file" >&2
  exit 2
fi
if [[ ! -s "$MOONSHINE_MODELS_FILE" ]]; then
  echo "Moonshine model manifest is missing or empty: $MOONSHINE_MODELS_FILE" >&2
  exit 2
fi

IFS= read -r pinned_moonshine_version < "$version_file"
MOONSHINE_VERSION="${MOONSHINE_VERSION:-$pinned_moonshine_version}"
MOONSHINE_VERSION="${MOONSHINE_VERSION#v}"
if [[ -z "$MOONSHINE_VERSION" ]]; then
  echo "Moonshine version must not be empty." >&2
  exit 2
fi
MOONSHINE_TAG="v$MOONSHINE_VERSION"

moonshine_model_arch() {
  case "$1" in
    tiny) printf '2\n' ;;
    small) printf '4\n' ;;
    medium) printf '5\n' ;;
    *) echo "Unsupported Moonshine model size in $MOONSHINE_MODELS_FILE: $1" >&2; return 2 ;;
  esac
}

moonshine_model_slug() {
  printf '%s-streaming\n' "$1"
}

moonshine_models() {
  local line language model extra count=0
  while IFS= read -r line || [[ -n "$line" ]]; do
    line="${line%%#*}"
    read -r language model extra <<< "$line"
    [[ -z "${language:-}" ]] && continue
    if [[ -z "${model:-}" || -n "${extra:-}" ]]; then
      echo "Invalid Moonshine model entry: $line" >&2
      return 2
    fi
    moonshine_model_arch "$model" >/dev/null || return
    printf '%s %s\n' "$language" "$model"
    count=$((count + 1))
  done < "$MOONSHINE_MODELS_FILE"
  if [[ "$count" -eq 0 ]]; then
    echo "Moonshine model manifest has no entries: $MOONSHINE_MODELS_FILE" >&2
    return 2
  fi
}

moonshine_models >/dev/null
