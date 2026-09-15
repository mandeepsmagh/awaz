#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/nemo-config.sh"
DEST="$ROOT/vendor/nemo"
BASE="https://github.com/NVIDIA/NeMo-Speech.cpp/releases/download/$NEMO_TAG"

os="$(uname -s)"
arch="$(uname -m)"
case "$os/$arch" in
  Linux/x86_64) asset="nemo-speech-$NEMO_VERSION-linux-x86_64-cpu.tar.gz" ;;
  Linux/aarch64|Linux/arm64) asset="nemo-speech-$NEMO_VERSION-linux-aarch64-cpu.tar.gz" ;;
  Darwin/arm64) asset="nemo-speech-$NEMO_VERSION-macos-aarch64-metal.tar.gz" ;;
  Darwin/x86_64) echo "Awaz supports macOS 26 or newer on Apple Silicon only." >&2; exit 2 ;;
  MINGW*/x86_64|MSYS*/x86_64|CYGWIN*/x86_64) asset="nemo-speech-$NEMO_VERSION-windows-x86_64-cpu.zip" ;;
  *) echo "Unsupported platform for the prebuilt NeMo Speech runtime: $os/$arch" >&2; exit 2 ;;
esac

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
curl -fL --retry 3 "$BASE/$asset" -o "$tmp/$asset"
curl -fL --retry 3 "$BASE/$asset.sha256" -o "$tmp/$asset.sha256"
expected="$(awk 'NR == 1 { print $1 }' "$tmp/$asset.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$tmp/$asset" | awk '{ print $1 }')"
elif command -v shasum >/dev/null 2>&1; then
  actual="$(shasum -a 256 "$tmp/$asset" | awk '{ print $1 }')"
elif command -v certutil.exe >/dev/null 2>&1; then
  actual="$(certutil.exe -hashfile "$(cygpath -w "$tmp/$asset")" SHA256 | tr -d ' \r' | awk 'NR == 2 { print }')"
else
  echo "No SHA-256 utility is available to verify $asset" >&2
  exit 3
fi
actual="$(printf '%s' "$actual" | tr '[:upper:]' '[:lower:]')"
expected="$(printf '%s' "$expected" | tr '[:upper:]' '[:lower:]')"
if [[ "$actual" != "$expected" ]]; then
  echo "NeMo Speech runtime checksum mismatch for $asset" >&2
  exit 3
fi

if [[ "$asset" == *.zip ]]; then
  archive_path="$(cygpath -w "$tmp/$asset")"
  destination_path="$(cygpath -w "$tmp")"
  powershell.exe -NoProfile -NonInteractive -Command \
    "Expand-Archive -LiteralPath '$archive_path' -DestinationPath '$destination_path' -Force"
else
  tar -xzf "$tmp/$asset" -C "$tmp"
fi
header="$(find "$tmp" -type f -path '*/include/nemo_speech/asr.h' | head -n1 || true)"
if [[ -z "$header" ]]; then
  echo "Could not locate the NeMo Speech SDK inside $asset" >&2
  exit 3
fi
sdk_root="$(cd "$(dirname "$header")/../.." && pwd)"
rm -rf "$DEST"
mkdir -p "$DEST"
cp -a "$sdk_root/include" "$sdk_root/lib" "$DEST/"
[[ -d "$sdk_root/bin" ]] && cp -a "$sdk_root/bin" "$DEST/"
if [[ "$os" == Linux ]]; then
  # Moonshine requires GLIBCXX_3.4.29, while NeMo's arm64 archive bundles a
  # libstdc++ that stops at 3.4.28. Both providers use the newer host runtime.
  rm -f "$DEST/lib"/libstdc++.so*
fi
mkdir -p "$DEST/link"
if [[ "$os" == MINGW* || "$os" == MSYS* || "$os" == CYGWIN* ]]; then
  cp "$DEST/lib/nemo_speech_asr_c.lib" "$DEST/link/"
  # Keep SDK C++ runtime DLLs out of the build-time PATH. They can shadow the
  # runner's newer runtime and prevent libclang from loading during bindgen.
  mkdir -p "$DEST/runtime"
  for dll in "$DEST/bin"/ggml*.dll "$DEST/bin"/nemo_speech_asr*.dll "$DEST/bin"/vcomp140.dll; do
    [[ -f "$dll" ]] && cp "$dll" "$DEST/runtime/"
  done
else
  # Do not put the SDK's bundled libstdc++ in Cargo's link search path. The
  # Moonshine runtime can require newer GLIBCXX symbols than that copy exports.
  for library in "$DEST/lib"/libggml* "$DEST/lib"/libnemo_speech_asr*; do
    [[ -e "$library" || -L "$library" ]] && cp -a "$library" "$DEST/link/"
  done
fi
mkdir -p "$DEST/share/nemo-speech" "$DEST/share/licenses"
cp "$sdk_root/share/nemo-speech/model-index.json" "$DEST/share/nemo-speech/"
cp -a "$sdk_root/share/licenses/nemo-speech" "$DEST/share/licenses/"

echo "NeMo Speech runtime staged in $DEST"
