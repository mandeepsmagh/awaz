#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
source "$ROOT/scripts/moonshine-config.sh"
OUT="${1:-$ROOT/dist/awaz}"

rm -rf "$OUT"
mkdir -p "$OUT/lib" "$OUT/models/moonshine"

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
while read -r language model_size; do
  slug="$(moonshine_model_slug "$model_size")"
  model="$cache_root/models/moonshine/$language/$slug"
  bundled_model="$OUT/models/moonshine/$language/$slug"
  [[ -d "$model" ]] || { echo "Model missing: $language $slug. Run scripts/dev-setup-model.sh." >&2; exit 4; }
  mkdir -p "$bundled_model"
  cp -aL "$model"/. "$bundled_model/"
done < <(moonshine_models)

cp "$ROOT/LICENSE" "$ROOT/THIRD_PARTY.md" "$ROOT/README.md" "$ROOT/moonshine.version" "$ROOT/moonshine.models" "$OUT/"
if [[ -f "$ROOT/vendor/moonshine/LICENSE" ]]; then
  mkdir -p "$OUT/THIRD_PARTY_LICENSES"
  cp "$ROOT/vendor/moonshine/LICENSE" "$OUT/THIRD_PARTY_LICENSES/MOONSHINE-$MOONSHINE_TAG-LICENSE"
fi
mkdir -p "$OUT/docs" "$OUT/integrations"
cp -a "$ROOT/docs"/. "$OUT/docs/"
cp -a "$ROOT/integrations/pi" "$OUT/integrations/pi"
