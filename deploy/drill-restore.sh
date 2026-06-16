#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
CONFIG="${POLYMARKET_CONFIG:-$BOT_DIR/config/production.toml}"
BOT="$BOT_DIR/target/release/polymarket-bot"
DRILL_DIR="$(mktemp -d)"
SUMMARY="${DRILL_SUMMARY:-$BOT_DIR/data-production/drills/restore-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"
trap 'rm -rf "$DRILL_DIR"' EXIT

echo "== Non-live restore drill =="
echo "config=$CONFIG"

if [[ ! -x "$BOT" ]]; then
  echo "Bot binary is missing or not executable: $BOT" >&2
  exit 1
fi

backup="$DRILL_DIR/source-backup.db"
restored="$DRILL_DIR/restored.db"

echo
echo "== Create backup =="
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$CONFIG" "$BOT" backup --destination "$backup"
)

echo
echo "== Restore backup into temporary database =="
POLYMARKET_BOT_BIN="$BOT" \
POLYMARKET_DATABASE="$restored" \
POLYMARKET_RESTORE_DRY_RUN=1 \
POLYMARKET_CONFIRM_RESTORE=DRY_RUN_RESTORE_ONLY \
  "$ROOT/deploy/restore-production-backup.sh" "$backup"

echo
echo "== Verify restored database =="
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$CONFIG" "$BOT" verify-database --path "$restored" \
    | tee "$DRILL_DIR/verify-restored.json"
)

SUMMARY_PATH="$SUMMARY" \
VERIFY_JSON_PATH="$DRILL_DIR/verify-restored.json" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary = {
    "drill_type": "restore",
    "generated_at_ms": int(time.time() * 1000),
    "ok": True,
    "submitted": False,
    "live_credentials_required": False,
    "restored_database_verify": json.loads(Path(os.environ["VERIFY_JSON_PATH"]).read_text()),
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Drill summary: {summary_path}")
PY

echo
echo "PASS: restore drill completed against a temporary database"
