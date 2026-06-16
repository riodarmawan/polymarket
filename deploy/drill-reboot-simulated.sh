#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
DRILL_DIR="$BOT_DIR/data-production/drills"
PENDING="$DRILL_DIR/reboot-drill-pending.json"
SUMMARY="${DRILL_SUMMARY:-$DRILL_DIR/reboot-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"

echo "== Simulated Reboot Drill (WSL-safe) =="

if [[ -f "$PENDING" ]]; then
    pre_boot_id="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["pre_reboot_boot_id"])' "$PENDING")"
    echo "Found pending marker with pre_reboot_boot_id=$pre_boot_id"
else
    pre_boot_id="$(cat /proc/sys/kernel/random/boot_id)"
    echo "No pending marker; using current boot_id as pre_reboot_boot_id=$pre_boot_id"
fi

echo
echo "== Checking production health =="
if ! health_json="$(curl --fail --silent --show-error http://localhost:3001/api/health)"; then
    echo "FAIL: health endpoint not reachable" >&2
    exit 1
fi
echo "Health endpoint OK"

overall="$(echo "$health_json" | python3 -c 'import json,sys; print(json.load(sys.stdin).get("overall","unknown"))')"
if [[ "$overall" != "ready" ]]; then
    echo "WARN: overall health is '$overall' (expected 'ready'), continuing anyway"
fi

echo
echo "== Checking service configuration =="
service_found=false
for f in \
    "$ROOT/deploy/polymarket-bot.service.example" \
    "$ROOT/deploy/polymarket-bot.service" \
    "$ROOT/polymarket-bot/deploy/polymarket-bot.service.example"; do
    if [[ -f "$f" ]]; then
        echo "Service file found: $f"
        service_found=true
    fi
done
if [[ "$service_found" == "false" ]]; then
    echo "WARN: no systemd service example file found"
fi

post_boot_id="$(cat /proc/sys/kernel/random/boot_id)"
boot_changed=false
if [[ "$pre_boot_id" != "$post_boot_id" ]]; then
    boot_changed=true
fi

SUMMARY_PATH="$SUMMARY" \
PENDING_PATH="$PENDING" \
PRE_BOOT_ID="$pre_boot_id" \
POST_BOOT_ID="$post_boot_id" \
BOOT_CHANGED="$boot_changed" \
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
    "drill_type": "reboot-simulated",
    "generated_at_ms": int(time.time() * 1000),
    "ok": False,
    "submitted": False,
    "live_credentials_required": False,
    "pre_reboot_boot_id": os.environ["PRE_BOOT_ID"],
    "post_reboot_boot_id": os.environ["POST_BOOT_ID"],
    "boot_id_changed": os.environ["BOOT_CHANGED"] == "true",
    "health": health,
    "simulated": True,
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")

pending = Path(os.environ["PENDING_PATH"])
if pending.exists():
    pending.unlink()

print(f"Drill summary: {summary_path}")
PY

echo
echo "INCOMPLETE: simulated reboot drill completed. This is useful local evidence,"
echo "            but production readiness still requires a real reboot drill."
