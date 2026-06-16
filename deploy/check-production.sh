#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BOT_DIR="$ROOT/polymarket-bot"
CONFIG="$BOT_DIR/config/production.toml"
HEALTH_URL="${POLYMARKET_HEALTH_URL:-http://localhost:3001/api/health}"

echo "== Repository secret scan =="
"$ROOT/deploy/scan-repo-secrets.sh"

echo
echo "== Process and ports =="
ps -ef | grep -E '[p]olymarket-gamma|[p]olymarket-bot web' || true
ss -ltnp | grep -E ':3000|:3001' || true

echo
echo "== Dashboard health =="
health_json="$(curl --fail --silent --show-error "$HEALTH_URL")"
printf '%s\n' "$health_json"
HEALTH_JSON="$health_json" python3 - <<'PY'
import json
import os
import sys

health = json.loads(os.environ["HEALTH_JSON"])
failures = []
if health.get("overall") != "ready":
    failures.append(f"overall={health.get('overall')}")
if health.get("database", {}).get("status") != "ready":
    failures.append(f"database={health.get('database', {}).get('status')}")
if health.get("production_control", {}).get("open_incidents") != 0:
    failures.append(f"open_incidents={health.get('production_control', {}).get('open_incidents')}")

if failures:
    print("FAIL: dashboard health is not production-ready: " + ", ".join(failures), file=sys.stderr)
    sys.exit(1)
print("PASS: dashboard health is ready")
PY
echo

echo
echo "== Production readiness =="
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$CONFIG" ./target/release/polymarket-bot production-readiness
)

if [[ "${REQUIRE_CANARY_READY:-false}" == "true" ]]; then
  echo
  echo "== Canary readiness gate =="
  (
    cd "$BOT_DIR"
    POLYMARKET_CONFIG="$CONFIG" \
      ./target/release/polymarket-bot production-readiness --json --require-canary-ready
  )
fi

echo
echo "== Operational status =="
(
  cd "$BOT_DIR"
  POLYMARKET_CONFIG="$CONFIG" ./target/release/polymarket-bot operational-status
)
