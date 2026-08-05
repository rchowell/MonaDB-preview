#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/scripts"

if command -v uv >/dev/null 2>&1; then
  exec uv run e2e "$@"
fi

export PYTHONPATH="$ROOT/scripts${PYTHONPATH:+:$PYTHONPATH}"
exec python3 -m e2e "$@"
