#!/usr/bin/env bash
# Wrapper for the load simulation (starts service check only).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

PLAYERS="${PLAYERS:-5000}"
CONCURRENCY="${CONCURRENCY:-250}"
BASE_URL="${BASE_URL:-http://127.0.0.1:8080}"

exec python3 scripts/load_simulation.py \
  --base-url "$BASE_URL" \
  --players "$PLAYERS" \
  --concurrency "$CONCURRENCY" \
  "$@"
