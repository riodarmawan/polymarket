#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
CONFIG="${POLYMARKET_CONFIG:-$BOT_DIR/config/production.toml}"
BOT="$BOT_DIR/target/release/polymarket-bot"
DRILL_DIR="$(mktemp -d)"
SUMMARY="${DRILL_SUMMARY:-$BOT_DIR/data-production/drills/production-paper-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"
trap 'rm -rf "$DRILL_DIR"' EXIT

echo "== Non-live production-paper drill =="
echo "config=$CONFIG"

echo
echo "== Secret scan =="
"$ROOT/deploy/scan-repo-secrets.sh" --self-test
"$ROOT/deploy/scan-repo-secrets.sh"

echo
echo "== Health/readiness inspection =="
"$ROOT/deploy/check-production.sh"
health_json="$(curl --fail --silent --show-error http://localhost:3001/api/health)"

echo
echo "== Forward monitor once =="
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$CONFIG" "$BOT" monitor-forward --max-iterations 1 \
    | tee "$DRILL_DIR/forward-monitor.json"
)

echo
echo "== Backup and verify drill =="
backup="$DRILL_DIR/trading-drill.db"
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$CONFIG" "$BOT" backup --destination "$backup"
  POLYMARKET_CONFIG="$CONFIG" "$BOT" verify-database --path "$backup" \
    | tee "$DRILL_DIR/verify-database.json"
)

echo
echo "== Canary gate must remain blocked =="
(
  cd "$BOT_DIR"
  set +e
  POLYMARKET_CONFIG="$CONFIG" "$BOT" production-readiness --json --require-canary-ready \
    >"$DRILL_DIR/canary-readiness.json" 2>"$DRILL_DIR/canary-readiness.err"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "FAIL: canary readiness unexpectedly passed during paper drill" >&2
    cat "$DRILL_DIR/canary-readiness.json" >&2
    exit 1
  fi
  echo "PASS: canary readiness remains blocked as expected"
  cat "$DRILL_DIR/canary-readiness.err"
)

echo
HEALTH_JSON="$health_json" \
FORWARD_JSON_PATH="$DRILL_DIR/forward-monitor.json" \
VERIFY_JSON_PATH="$DRILL_DIR/verify-database.json" \
SUMMARY_PATH="$SUMMARY" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary = {
    "drill_type": "production-paper",
    "generated_at_ms": int(time.time() * 1000),
    "ok": True,
    "submitted": False,
    "live_credentials_required": False,
    "health": json.loads(os.environ["HEALTH_JSON"]),
    "forward_monitor": json.loads(Path(os.environ["FORWARD_JSON_PATH"]).read_text()),
    "backup_verify": json.loads(Path(os.environ["VERIFY_JSON_PATH"]).read_text()),
    "canary_gate_blocked": True,
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Drill summary: {summary_path}")
PY

echo
echo "PASS: production-paper drill completed without live credentials or order submission"
