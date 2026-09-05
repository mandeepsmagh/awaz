#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$ROOT/scripts/fetch-moonshine-runtime.sh"
echo "Development runtime ready. Run: cargo run -p awaz-cli -- doctor" >&2
