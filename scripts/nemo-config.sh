#!/usr/bin/env bash

config_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
version_file="$config_root/nemo.version"

if [[ ! -s "$version_file" ]]; then
  echo "NeMo Speech version file is missing or empty: $version_file" >&2
  exit 2
fi

IFS= read -r pinned_nemo_version < "$version_file"
NEMO_VERSION="${NEMO_VERSION:-$pinned_nemo_version}"
NEMO_VERSION="${NEMO_VERSION%$'\r'}"
NEMO_VERSION="${NEMO_VERSION#v}"
if [[ -z "$NEMO_VERSION" ]]; then
  echo "NeMo Speech version must not be empty." >&2
  exit 2
fi
NEMO_TAG="v$NEMO_VERSION"
