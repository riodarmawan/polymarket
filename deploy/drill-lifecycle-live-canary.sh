#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
CONFIG="${POLYMARKET_CONFIG:-$BOT_DIR/config/production.toml}"
BOT="$BOT_DIR/target/release/polymarket-bot"
DRILL_DIR="$(mktemp -d)"
SUMMARY="${DRILL_SUMMARY:-$BOT_DIR/data-production/drills/lifecycle-live-canary-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"
trap 'rm -rf "$DRILL_DIR"' EXIT

echo "== lifecycle-live-canary drill =="
echo "PLACEHOLDER: This script is a stub for canary order submission."
echo "An actual canary order can only be submitted with live credentials after promotion."
echo "In paper mode, canary submission is correctly blocked."
echo "This gate will remain PENDING until a real canary order is submitted."

echo
echo "== Verify canary submission path is blocked in paper mode =="
(
  cd "$BOT_DIR"
  set +e
  POLYMARKET_CONFIG="$CONFIG" "$BOT" production-readiness --json --require-canary-ready \
    >"$DRILL_DIR/canary-readiness.json" 2>"$DRILL_DIR/canary-readiness.err"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "FAIL: canary readiness unexpectedly passed in paper mode" >&2
    cat "$DRILL_DIR/canary-readiness.json" >&2
    exit 1
  fi
  echo "PASS: canary readiness is correctly blocked in paper mode"
  cat "$DRILL_DIR/canary-readiness.err"
)

echo
echo "== Attempt submit-canary without authorization (must fail) =="
(
  cd "$BOT_DIR"
  set +e
  POLYMARKET_CONFIG="$CONFIG" "$BOT" submit-canary \
    --authorization-id "drill-placeholder-not-real" \
    --client-key "drill-placeholder-not-real" \
    --confirm "SUBMIT_AUTHORIZED_CANARY" \
    >"$DRILL_DIR/submit-canary.json" 2>"$DRILL_DIR/submit-canary.err"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "FAIL: submit-canary unexpectedly succeeded without authorization" >&2
    cat "$DRILL_DIR/submit-canary.json" >&2
    exit 1
  fi
  echo "PASS: submit-canary correctly rejected without valid authorization"
  cat "$DRILL_DIR/submit-canary.err"
)

echo
echo "== Write drill summary =="
CANARY_BLOCKED=true
SUBMIT_REJECTED=true

CANARY_BLOCKED="$CANARY_BLOCKED" \
SUBMIT_REJECTED="$SUBMIT_REJECTED" \
SUMMARY_PATH="$SUMMARY" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary = {
    "drill_type": "lifecycle-live-canary",
    "generated_at_ms": int(time.time() * 1000),
    "ok": False,
    "submitted": False,
    "canary_gate_blocked_in_paper_mode": os.environ["CANARY_BLOCKED"] == "true",
    "submit_canary_rejected_without_authorization": os.environ["SUBMIT_REJECTED"] == "true",
    "placeholder_notice": "This drill can only reach ok=true after an actual canary order is submitted with live credentials. The gate remains PENDING until then.",
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Drill summary: {summary_path}")
PY

echo
echo "BLOCKED: lifecycle-live-canary drill correctly reports no canary submission."
echo "         The production readiness gate for this drill type requires submitted=true,"
echo "         which can only occur after a real canary order round-trip."
echo "         This gate will remain PENDING until live canary testing is performed."