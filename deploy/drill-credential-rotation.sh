#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
DRILL_DIR="$(mktemp -d)"
SUMMARY="${DRILL_SUMMARY:-$BOT_DIR/data-production/drills/credential-rotation-drill-$(date -u +%Y%m%dT%H%M%SZ).json}"
trap 'rm -rf "$DRILL_DIR"' EXIT

secret_dir="$DRILL_DIR/secrets"
secret_file="$secret_dir/production.env"
rotated_file="$secret_dir/production.env.rotated-old"

echo "== Non-live credential-rotation drill =="

echo
echo "== Create locked placeholder secret file =="
POLYMARKET_SECRET_FILE="$secret_file" "$ROOT/deploy/init-production-secrets.sh"
POLYMARKET_SECRET_FILE="$secret_file" "$ROOT/deploy/check-production-secrets.sh"

echo
echo "== Rotate placeholder secret file without overwrite =="
mv "$secret_file" "$rotated_file"
chmod 600 "$rotated_file"
POLYMARKET_SECRET_FILE="$secret_file" "$ROOT/deploy/init-production-secrets.sh"
POLYMARKET_SECRET_FILE="$secret_file" "$ROOT/deploy/check-production-secrets.sh"

dir_mode="$(stat -c '%a' "$secret_dir")"
new_mode="$(stat -c '%a' "$secret_file")"
old_mode="$(stat -c '%a' "$rotated_file")"
live_disabled="$(grep -c '^POLYMARKET_LIVE_TRADING_ENABLED=disabled$' "$secret_file")"

if [[ "$dir_mode" != "700" || "$new_mode" != "600" || "$old_mode" != "600" ]]; then
  echo "Credential rotation drill found unsafe file permissions" >&2
  exit 1
fi
if [[ "$live_disabled" -ne 1 ]]; then
  echo "Rotated placeholder secret file does not keep live trading disabled" >&2
  exit 1
fi

SUMMARY_PATH="$SUMMARY" \
DIR_MODE="$dir_mode" \
NEW_MODE="$new_mode" \
OLD_MODE="$old_mode" \
python3 - <<'PY'
import json
import os
import time
from pathlib import Path

summary_path = Path(os.environ["SUMMARY_PATH"])
summary_path.parent.mkdir(parents=True, exist_ok=True)
summary = {
    "drill_type": "credential-rotation",
    "generated_at_ms": int(time.time() * 1000),
    "ok": True,
    "submitted": False,
    "live_credentials_required": False,
    "used_placeholder_template_only": True,
    "secret_directory_mode": os.environ["DIR_MODE"],
    "new_secret_file_mode": os.environ["NEW_MODE"],
    "rotated_secret_file_mode": os.environ["OLD_MODE"],
    "live_switch_disabled_after_rotation": True,
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
print(f"Drill summary: {summary_path}")
PY

echo
echo "PASS: credential-rotation drill completed with locked placeholder files"
