#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
DRILL_DIR="$(mktemp -d)"
SUMMARY="${DRILL_SUMMARY:-$BOT_DIR/data-production/drills/lifecycle-non-live-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"
trap 'rm -rf "$DRILL_DIR"' EXIT

echo "== Non-live lifecycle drill =="
echo "This drill does not use wallet credentials and does not submit orders."

run_test_group() {
  local name="$1"
  local filter="$2"
  echo
  echo "== $name =="
  (
    cd "$BOT_DIR"
    cargo test --workspace "$filter" 2>&1 | tee "$DRILL_DIR/$name.log"
  )
}

run_test_group "lifecycle-state-machine" "execution::lifecycle::tests"
run_test_group "live-executor-fail-closed" "execution::live::tests"
run_test_group "recovery-reconciliation" "execution::recovery::tests"
run_test_group "user-stream-status-mapping" "execution::user_stream::tests"
run_test_group "redemption-planning" "redemption"

SUMMARY_PATH="$SUMMARY" python3 - <<'PY'
import json
import os
import time
from pathlib import Path

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary = {
    "drill_type": "lifecycle-non-live",
    "generated_at_ms": int(time.time() * 1000),
    "ok": True,
    "submitted": False,
    "live_credentials_required": False,
    "test_suite_passed": True,
    "covered": [
        "order lifecycle state machine",
        "FOK rejection/cancel/partial-fill handling",
        "ambiguous failure fail-closed behavior",
        "restart recovery and duplicate retry prevention",
        "remote/local reconciliation mismatch handling",
        "user stream status mapping",
        "redemption planning durability",
    ],
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Drill summary: {summary_path}")
PY

echo
echo "PASS: non-live lifecycle drill completed without wallet credentials or order submission"
