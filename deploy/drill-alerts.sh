#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
DRILL_DIR="$(mktemp -d)"
SUMMARY="${DRILL_SUMMARY:-$BOT_DIR/data-production/drills/alerts-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"
trap 'rm -rf "$DRILL_DIR"' EXIT

echo "== Non-live alerts/check drill =="

echo
echo "== Healthy dashboard check must pass =="
"$ROOT/deploy/check-production.sh" >"$DRILL_DIR/check-production-ok.log" 2>"$DRILL_DIR/check-production-ok.err"

echo
echo "== Broken health endpoint must fail closed =="
set +e
POLYMARKET_HEALTH_URL="http://127.0.0.1:9/api/health" \
  "$ROOT/deploy/check-production.sh" >"$DRILL_DIR/check-production-fail.log" 2>"$DRILL_DIR/check-production-fail.err"
failed_status=$?
set -e
if [[ "$failed_status" -eq 0 ]]; then
  echo "Alert drill expected check-production to fail against an unreachable health endpoint" >&2
  exit 1
fi

SUMMARY_PATH="$SUMMARY" \
FAILED_STATUS="$failed_status" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary = {
    "drill_type": "alerts",
    "generated_at_ms": int(time.time() * 1000),
    "ok": True,
    "submitted": False,
    "live_credentials_required": False,
    "healthy_check_passed": True,
    "broken_health_check_failed_closed": True,
    "broken_health_exit_status": int(os.environ["FAILED_STATUS"]),
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Drill summary: {summary_path}")
PY

echo
echo "PASS: alerts/check drill detected an unreachable health endpoint"
