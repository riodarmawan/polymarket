#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
CONFIG="$BOT_DIR/config/production.toml"

echo "== Process and ports =="
ps -ef | grep -E '[p]olymarket-gamma|[p]olymarket-bot web' || true
ss -ltnp | grep -E ':3000|:3001' || true

echo
echo "== Dashboard health =="
curl --fail --show-error http://localhost:3001/api/health
echo

echo
echo "== Production readiness =="
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$CONFIG" ./target/release/polymarket-bot production-readiness
)
