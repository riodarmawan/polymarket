#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
BOT="$BOT_DIR/target/release/polymarket-bot"
DRILL_DIR="$(mktemp -d)"
SUMMARY="${DRILL_SUMMARY:-$BOT_DIR/data-production/drills/dry-sign-safety-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"
trap 'rm -rf "$DRILL_DIR"' EXIT

echo "== Non-live dry-sign safety drill =="
echo "This drill proves dry-sign fails closed when wallet credentials are absent."

if [[ ! -x "$BOT" ]]; then
  echo "Bot binary is missing or not executable: $BOT" >&2
  exit 1
fi

echo
echo "== Dry-sign without credentials must fail before submission =="
set +e
(
  cd "$BOT_DIR"
  env -u POLYMARKET_PRIVATE_KEY \
      -u POLYMARKET_DEPOSIT_WALLET_ADDRESS \
      -u POLYMARKET_CLOB_API_KEY \
      -u POLYMARKET_CLOB_API_SECRET \
      -u POLYMARKET_CLOB_PASSPHRASE \
      -u POLYMARKET_RELAYER_API_KEY \
      -u POLYMARKET_RELAYER_API_KEY_ADDRESS \
      -u POLYMARKET_LIVE_TRADING_ENABLED \
      POLYMARKET_CONFIG="$BOT_DIR/config/production.toml" \
      "$BOT" dry-sign --token-id 1 --price 0.50 --size 5 \
      >"$DRILL_DIR/dry-sign.stdout" 2>"$DRILL_DIR/dry-sign.stderr"
)
status=$?
set -e

if [[ "$status" -eq 0 ]]; then
  echo "Dry-sign unexpectedly succeeded without credentials" >&2
  cat "$DRILL_DIR/dry-sign.stdout" >&2
  exit 1
fi
if ! grep -q 'POLYMARKET_PRIVATE_KEY is required' "$DRILL_DIR/dry-sign.stderr"; then
  echo "Dry-sign did not fail for the expected missing private key guard" >&2
  cat "$DRILL_DIR/dry-sign.stderr" >&2
  exit 1
fi
if grep -qi 'submitted.*true' "$DRILL_DIR/dry-sign.stdout" "$DRILL_DIR/dry-sign.stderr"; then
  echo "Dry-sign output suggests an order may have been submitted" >&2
  exit 1
fi

SUMMARY_PATH="$SUMMARY" \
EXIT_STATUS="$status" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary = {
    "drill_type": "dry-sign-safety",
    "generated_at_ms": int(time.time() * 1000),
    "ok": True,
    "submitted": False,
    "live_credentials_required": False,
    "failed_closed_without_credentials": True,
    "missing_secret_guard": "POLYMARKET_PRIVATE_KEY",
    "dry_sign_exit_status": int(os.environ["EXIT_STATUS"]),
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Drill summary: {summary_path}")
PY

echo
echo "PASS: dry-sign failed closed without wallet credentials"
