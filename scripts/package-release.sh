#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/moonshine-config.sh"
source "$ROOT/scripts/nemo-config.sh"
OUT="${1:-$ROOT/dist/awaz}"

rm -rf "$OUT"
mkdir -p "$OUT/lib"

bin="$ROOT/target/release/awaz"
[[ -f "$bin.exe" ]] && bin="$bin.exe"
[[ -f "$bin" ]] || { echo "Release binary missing: run cargo build --release -p awaz-cli" >&2; exit 3; }
cp "$bin" "$OUT/"
if [[ "$(uname -s)" == Darwin ]]; then
  apple_helper="$ROOT/target/release/awaz-apple-speech"
  [[ -f "$apple_helper" ]] || { echo "Apple Speech helper missing: rebuild awaz-cli" >&2; exit 3; }
  cp "$apple_helper" "$OUT/"
fi

if [[ -d "$ROOT/vendor/moonshine/lib" ]]; then
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
      # Windows searches the executable directory for runtime DLLs. Static/import
      # libraries are build-time inputs and do not belong in the release archive.
      find "$ROOT/vendor/moonshine/lib" -maxdepth 1 -type f -iname '*.dll' -exec cp {} "$OUT/" \;
      ;;
    Darwin)
      # Moonshine's portable macOS archive is static; nothing is needed at runtime.
      rmdir "$OUT/lib" 2>/dev/null || true
      ;;
    *)
      cp -a "$ROOT/vendor/moonshine/lib"/. "$OUT/lib/"
      ;;
  esac
fi
rmdir "$OUT/lib" 2>/dev/null || true

if [[ -d "$ROOT/vendor/nemo" ]]; then
  case "$(uname -s)" in
    MINGW*|MSYS*|CYGWIN*)
      find "$ROOT/vendor/nemo/bin" -maxdepth 1 -type f -iname '*.dll' -exec cp {} "$OUT/" \;
      ;;
    *)
      mkdir -p "$OUT/lib"
      cp -a "$ROOT/vendor/nemo/lib"/. "$OUT/lib/"
      ;;
  esac
fi
rmdir "$OUT/lib" 2>/dev/null || true

# Models are not bundled: `awaz` downloads the selected model on first use
# into the user cache (~/.cache/awaz or %LOCALAPPDATA%\awaz).

cp "$ROOT/LICENSE" "$ROOT/THIRD_PARTY.md" "$ROOT/README.md" "$ROOT/moonshine.version" "$ROOT/moonshine.models" "$ROOT/nemo.version" "$OUT/"
if [[ -f "$ROOT/vendor/moonshine/LICENSE" ]]; then
  mkdir -p "$OUT/THIRD_PARTY_LICENSES"
  cp "$ROOT/vendor/moonshine/LICENSE" "$OUT/THIRD_PARTY_LICENSES/MOONSHINE-$MOONSHINE_TAG-LICENSE"
fi
if [[ -d "$ROOT/vendor/nemo/share/licenses/nemo-speech" ]]; then
  mkdir -p "$OUT/THIRD_PARTY_LICENSES/NEMO-SPEECH-$NEMO_TAG"
  cp -a "$ROOT/vendor/nemo/share/licenses/nemo-speech"/. \
    "$OUT/THIRD_PARTY_LICENSES/NEMO-SPEECH-$NEMO_TAG/"
fi
mkdir -p "$OUT/docs" "$OUT/integrations"
cp -a "$ROOT/docs"/. "$OUT/docs/"
cp -a "$ROOT/integrations/pi" "$OUT/integrations/pi"
