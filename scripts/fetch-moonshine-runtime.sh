#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/vendor/moonshine"
MOONSHINE_VERSION="${MOONSHINE_VERSION:-v0.1.5}"
BASE="https://github.com/moonshine-ai/moonshine/releases/download/$MOONSHINE_VERSION"

os="$(uname -s)"
arch="$(uname -m)"
case "$os/$arch" in
  Linux/x86_64) asset="moonshine-voice-linux-x86_64.tar.gz" ;;
  Linux/aarch64|Linux/arm64) asset="moonshine-voice-linux-arm64.tar.gz" ;;
  Darwin/arm64) asset="moonshine-voice-macos-arm64.tar.gz" ;;
  Darwin/x86_64) echo "Awaz supports macOS 26 or newer on Apple Silicon only." >&2; exit 2 ;;
  MINGW*/x86_64|MSYS*/x86_64|CYGWIN*/x86_64) asset="moonshine-voice-windows-x86_64.tar.gz" ;;
  *) echo "Unsupported platform for the prebuilt Moonshine runtime: $os/$arch" >&2; exit 2 ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fL "$BASE/$asset" -o "$tmp/runtime.tar.gz"
tar -xzf "$tmp/runtime.tar.gz" -C "$tmp"
mkdir -p "$DEST/lib" "$DEST/include"

libfile="$(find "$tmp" -type f \( -name 'libmoonshine.so' -o -name 'libmoonshine.a' -o -name 'moonshine.lib' \) | head -n1 || true)"
if [[ -z "$libfile" ]]; then
  echo "Could not locate the Moonshine native library inside $asset" >&2
  exit 3
fi
libdir="$(dirname "$libfile")"
cp -a "$libdir"/. "$DEST/lib/"
header="$(find "$tmp" -type f -name 'moonshine-c-api.h' | head -n1 || true)"
[[ -n "$header" ]] && cp "$header" "$DEST/include/"

# Keep the exact upstream license beside the staged runtime so release archives
# can satisfy third-party redistribution requirements without vendoring binaries
# or model weights in the source repository.
curl -fL "https://raw.githubusercontent.com/moonshine-ai/moonshine/$MOONSHINE_VERSION/LICENSE" -o "$DEST/LICENSE"

echo "Moonshine runtime staged in $DEST"
