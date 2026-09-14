#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
"$ROOT/scripts/fetch-moonshine-runtime.sh"
"$ROOT/scripts/fetch-nemo-runtime.sh"
echo "Development runtimes ready. Run: cargo run -p awaz-cli -- doctor" >&2
