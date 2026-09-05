#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/moonshine-config.sh"
OUT="${1:-$ROOT/dist/awaz}"

rm -rf "$OUT"
mkdir -p "$OUT/lib"

bin="$ROOT/target/release/awaz"
[[ -f "$bin.exe" ]] && bin="$bin.exe"
[[ -f "$bin" ]] || { echo "Release binary missing: run cargo build --release -p awaz-cli" >&2; exit 3; }
cp "$bin" "$OUT/"

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

# Models are not bundled: `awaz` downloads the selected model on first use
# into the user cache (~/.cache/awaz or %LOCALAPPDATA%\awaz).

cp "$ROOT/LICENSE" "$ROOT/THIRD_PARTY.md" "$ROOT/README.md" "$ROOT/moonshine.version" "$ROOT/moonshine.models" "$OUT/"
if [[ -f "$ROOT/vendor/moonshine/LICENSE" ]]; then
  mkdir -p "$OUT/THIRD_PARTY_LICENSES"
  cp "$ROOT/vendor/moonshine/LICENSE" "$OUT/THIRD_PARTY_LICENSES/MOONSHINE-$MOONSHINE_TAG-LICENSE"
fi
mkdir -p "$OUT/docs" "$OUT/integrations"
cp -a "$ROOT/docs"/. "$OUT/docs/"
cp -a "$ROOT/integrations/pi" "$OUT/integrations/pi"
