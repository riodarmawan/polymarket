#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
DRILL_DIR="$BOT_DIR/data-production/drills"
PENDING="$DRILL_DIR/reboot-drill-pending.json"

mkdir -p "$DRILL_DIR"
boot_id="$(cat /proc/sys/kernel/random/boot_id)"

PENDING_PATH="$PENDING" \
BOOT_ID="$boot_id" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

pending = Path(os.environ["PENDING_PATH"])
summary = {
    "drill_type": "reboot",
    "ok": False,
    "pending": True,
    "submitted": False,
    "live_credentials_required": False,
    "prepared_at_ms": int(time.time() * 1000),
    "pre_reboot_boot_id": os.environ["BOOT_ID"],
}
pending.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Prepared reboot drill marker: {pending}")
PY

echo "Reboot the production host, restart production-paper, then run:"
echo "  ./deploy/complete-reboot-drill.sh"
