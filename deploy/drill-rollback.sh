#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
CONFIG="${POLYMARKET_CONFIG:-$BOT_DIR/config/production.toml}"
BOT="$BOT_DIR/target/release/polymarket-bot"
DRILL_DIR="$(mktemp -d)"
SUMMARY="${DRILL_SUMMARY:-$BOT_DIR/data-production/drills/rollback-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"
trap 'rm -rf "$DRILL_DIR"' EXIT

echo "== Non-live rollback drill =="
echo "config=$CONFIG"

if [[ ! -x "$BOT" ]]; then
  echo "Bot binary is missing or not executable: $BOT" >&2
  exit 1
fi

golden="$DRILL_DIR/golden-backup.db"
target="$DRILL_DIR/rollback-target.db"

echo
echo "== Create known-good backup =="
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$CONFIG" "$BOT" backup --destination "$golden"
)
cp --preserve=mode,timestamps "$golden" "$target"

echo
echo "== Simulate a bad local database change =="
TARGET_DB="$target" python3 - <<'PY'
import os
import sqlite3

db = os.environ["TARGET_DB"]
with sqlite3.connect(db) as conn:
    conn.execute("CREATE TABLE IF NOT EXISTS rollback_drill_marker (id INTEGER PRIMARY KEY, note TEXT NOT NULL)")
    conn.execute("INSERT INTO rollback_drill_marker (note) VALUES ('temporary rollback drill mutation')")
PY

mutated_checksum="$(sha256sum "$target" | awk '{print $1}')"
golden_checksum="$(sha256sum "$golden" | awk '{print $1}')"
if [[ "$mutated_checksum" == "$golden_checksum" ]]; then
  echo "Rollback drill mutation did not change the target checksum" >&2
  exit 1
fi

echo
echo "== Roll back temporary target from known-good backup =="
POLYMARKET_BOT_BIN="$BOT" \
POLYMARKET_DATABASE="$target" \
POLYMARKET_RESTORE_DRY_RUN=1 \
POLYMARKET_CONFIRM_RESTORE=DRY_RUN_RESTORE_ONLY \
  "$ROOT/deploy/restore-production-backup.sh" "$golden"

before_count="$(find "$DRILL_DIR" -maxdepth 1 -name 'rollback-target.db.before-restore-*' | wc -l | tr -d ' ')"
restored_checksum="$(sha256sum "$target" | awk '{print $1}')"
if [[ "$restored_checksum" != "$golden_checksum" ]]; then
  echo "Rollback target checksum does not match known-good backup" >&2
  exit 1
fi
if [[ "$before_count" -lt 1 ]]; then
  echo "Rollback did not preserve a before-restore copy" >&2
  exit 1
fi

echo
echo "== Verify rolled-back database =="
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$CONFIG" "$BOT" verify-database --path "$target" \
    | tee "$DRILL_DIR/verify-rollback.json"
)

SUMMARY_PATH="$SUMMARY" \
VERIFY_JSON_PATH="$DRILL_DIR/verify-rollback.json" \
GOLDEN_CHECKSUM="$golden_checksum" \
RESTORED_CHECKSUM="$restored_checksum" \
BEFORE_COUNT="$before_count" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary = {
    "drill_type": "rollback",
    "generated_at_ms": int(time.time() * 1000),
    "ok": True,
    "submitted": False,
    "live_credentials_required": False,
    "checksum_matches_known_good": os.environ["GOLDEN_CHECKSUM"] == os.environ["RESTORED_CHECKSUM"],
    "before_restore_copy_count": int(os.environ["BEFORE_COUNT"]),
    "rolled_back_database_verify": json.loads(Path(os.environ["VERIFY_JSON_PATH"]).read_text()),
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Drill summary: {summary_path}")
PY

echo
echo "PASS: rollback drill restored a temporary database to a known-good backup"
