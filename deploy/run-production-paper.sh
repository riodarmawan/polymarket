#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
CONFIG="$BOT_DIR/config/production.toml"
LOG_DIR="${POLYMARKET_LOG_DIR:-$BOT_DIR/data-production/logs}"

mkdir -p "$LOG_DIR"

cargo build --release --manifest-path "$ROOT/Cargo.toml"
cargo build --release --manifest-path "$BOT_DIR/Cargo.toml"

pkill -f 'target/release/polymarket-bot web --port 3001' 2>/dev/null || true
pkill -f 'target/release/polymarket-gamma' 2>/dev/null || true

nohup "$ROOT/target/release/polymarket-gamma" \
  >"$LOG_DIR/gamma.log" 2>&1 &

(
  cd "$BOT_DIR"
  nohup env POLYMARKET_CONFIG="$CONFIG" \
    "$BOT_DIR/target/release/polymarket-bot" web --port 3001 \
    >"$LOG_DIR/dashboard.log" 2>&1 &
)

for _ in $(seq 1 30); do
  if curl --silent --fail http://localhost:3001/api/health >/dev/null; then
    echo "Production-paper dashboard is running at http://localhost:3001/"
    exit 0
  fi
  sleep 1
done

echo "Dashboard did not become healthy. Check $LOG_DIR/dashboard.log" >&2
exit 1
