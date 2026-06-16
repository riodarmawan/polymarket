#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
BOT="$BOT_DIR/target/release/polymarket-bot"
PROD_CONFIG="$BOT_DIR/config/production.toml"
DRILL_DIR="$(mktemp -d)"
SUMMARY="${DRILL_SUMMARY:-$BOT_DIR/data-production/drills/reconciliation-safety-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"
trap 'rm -rf "$DRILL_DIR"' EXIT

echo "== Non-live reconciliation safety drill =="
echo "This drill uses a temporary database and does not use wallet credentials."

if [[ ! -x "$BOT" ]]; then
  echo "Bot binary is missing or not executable: $BOT" >&2
  exit 1
fi

temp_config="$DRILL_DIR/production-reconciliation-safety.toml"
temp_db="$DRILL_DIR/reconciliation-safety.db"
temp_backups="$DRILL_DIR/backups"

PROD_CONFIG="$PROD_CONFIG" \
TEMP_CONFIG="$temp_config" \
TEMP_DB="$temp_db" \
TEMP_BACKUPS="$temp_backups" \
python3 - <<'PY'
import os
from pathlib import Path

text = Path(os.environ["PROD_CONFIG"]).read_text()
text = text.replace('database_path = "data-production/trading.db"', f'database_path = "{os.environ["TEMP_DB"]}"')
text = text.replace('backup_directory = "data-production/backups"', f'backup_directory = "{os.environ["TEMP_BACKUPS"]}"')
text = text.replace('data_dir = "data-production"', f'data_dir = "{os.path.dirname(os.environ["TEMP_DB"])}"')
Path(os.environ["TEMP_CONFIG"]).write_text(text)
PY

echo
echo "== Reconcile without credentials must fail closed =="
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
      POLYMARKET_CONFIG="$temp_config" \
      "$BOT" reconcile \
      >"$DRILL_DIR/reconcile.stdout" 2>"$DRILL_DIR/reconcile.stderr"
)
status=$?
set -e

if [[ "$status" -eq 0 ]]; then
  echo "Reconcile unexpectedly succeeded without credentials" >&2
  cat "$DRILL_DIR/reconcile.stdout" >&2
  exit 1
fi
if ! grep -q 'POLYMARKET_PRIVATE_KEY is required' "$DRILL_DIR/reconcile.stderr"; then
  echo "Reconcile did not fail for the expected missing private key guard" >&2
  cat "$DRILL_DIR/reconcile.stderr" >&2
  exit 1
fi

echo
echo "== Temporary runtime must be halted with an incident =="
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$temp_config" "$BOT" operational-status \
    >"$DRILL_DIR/operational-status.json"
)

SUMMARY_PATH="$SUMMARY" \
STATUS_JSON_PATH="$DRILL_DIR/operational-status.json" \
EXIT_STATUS="$status" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

status = json.loads(Path(os.environ["STATUS_JSON_PATH"]).read_text())
runtime_halted = status.get("startup_state") == "halted"
incident_opened = status.get("open_incidents", 0) > 0
if not runtime_halted:
    raise SystemExit(f"runtime was not halted after failed reconciliation: {status}")
if not incident_opened:
    raise SystemExit(f"failed reconciliation did not open an incident: {status}")

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary = {
    "drill_type": "reconciliation-safety",
    "generated_at_ms": int(time.time() * 1000),
    "ok": True,
    "submitted": False,
    "live_credentials_required": False,
    "failed_closed_without_credentials": True,
    "runtime_halted_after_failure": runtime_halted,
    "incident_opened_after_failure": incident_opened,
    "reconcile_exit_status": int(os.environ["EXIT_STATUS"]),
    "temporary_operational_status": status,
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Drill summary: {summary_path}")
PY

echo
echo "PASS: reconciliation failed closed against a temporary database"
