#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
CONFIG="$BOT_DIR/config/production.toml"
SECRET_FILE="${POLYMARKET_SECRET_FILE:-$HOME/.config/polymarket/production.env}"
LOG_DIR="${POLYMARKET_LOG_DIR:-$BOT_DIR/data-production/logs}"

if [[ -f "$HOME/.cargo/env" ]]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

if [[ ! -f "$SECRET_FILE" ]]; then
  echo "Missing production secret file: $SECRET_FILE" >&2
  exit 1
fi

command -v cargo >/dev/null || {
  echo "cargo is not available; install Rust or add it to PATH" >&2
  exit 1
}

mkdir -p "$LOG_DIR"

cargo build --release --locked --manifest-path "$ROOT/Cargo.toml"
cargo build --release --locked --manifest-path "$BOT_DIR/Cargo.toml"

(
  cd "$BOT_DIR"
  set -a
  # shellcheck disable=SC1090
  source "$SECRET_FILE"
  set +a
  POLYMARKET_CONFIG="$CONFIG" "$BOT_DIR/target/release/polymarket-bot" reconcile \
    >"$LOG_DIR/reconcile-startup.log" 2>&1
)

pkill -f 'target/release/polymarket-bot web --port 3001' 2>/dev/null || true
pkill -f 'target/release/polymarket-gamma' 2>/dev/null || true

nohup "$ROOT/target/release/polymarket-gamma" \
  >"$LOG_DIR/gamma.log" 2>&1 &

(
  cd "$BOT_DIR"
  set -a
  # shellcheck disable=SC1090
  source "$SECRET_FILE"
  set +a
  nohup env POLYMARKET_CONFIG="$CONFIG" POLYMARKET_DASHBOARD_BIND="0.0.0.0:3001" \
    "$BOT_DIR/target/release/polymarket-bot" web --port 3001 \
    >"$LOG_DIR/dashboard.log" 2>&1 &
)

for _ in $(seq 1 30); do
  if curl --silent --fail http://localhost:3001/api/health >/dev/null; then
    echo "Production operator dashboard is running at http://localhost:3001/"
    echo "Runtime remains controlled by production.toml; this runner only exposes authenticated account status to the dashboard."
    exit 0
  fi
  sleep 1
done

echo "Dashboard did not become healthy. Check $LOG_DIR/dashboard.log" >&2
exit 1
