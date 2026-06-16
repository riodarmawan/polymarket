#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
DRILL_DIR="$BOT_DIR/data-production/drills"
PENDING="$DRILL_DIR/reboot-drill-pending.json"
SUMMARY="${DRILL_SUMMARY:-$DRILL_DIR/reboot-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"

if [[ ! -f "$PENDING" ]]; then
  echo "Missing reboot drill marker. Run ./deploy/prepare-reboot-drill.sh before rebooting." >&2
  exit 1
fi

pre_boot_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["pre_reboot_boot_id"])' "$PENDING")"
post_boot_id="$(cat /proc/sys/kernel/random/boot_id)"
if [[ "$pre_boot_id" == "$post_boot_id" ]]; then
  echo "Reboot drill is not complete: host boot ID did not change." >&2
  exit 1
fi

echo "== Post-reboot production check =="
"$ROOT/deploy/check-production.sh"
health_json="$(curl --fail --silent --show-error http://localhost:3001/api/health)"

SUMMARY_PATH="$SUMMARY" \
PENDING_PATH="$PENDING" \
PRE_BOOT_ID="$pre_boot_id" \
POST_BOOT_ID="$post_boot_id" \
HEALTH_JSON="$health_json" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
health = json.loads(os.environ["HEALTH_JSON"])
summary = {
    "drill_type": "reboot",
    "generated_at_ms": int(time.time() * 1000),
    "ok": True,
    "submitted": False,
    "live_credentials_required": False,
    "pre_reboot_boot_id": os.environ["PRE_BOOT_ID"],
    "post_reboot_boot_id": os.environ["POST_BOOT_ID"],
    "boot_id_changed": os.environ["PRE_BOOT_ID"] != os.environ["POST_BOOT_ID"],
    "health": health,
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
Path(os.environ["PENDING_PATH"]).unlink()
print(f"Drill summary: {summary_path}")
PY

echo
echo "PASS: reboot drill completed after a real host reboot"
