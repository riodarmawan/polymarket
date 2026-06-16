#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DATABASE="${POLYMARKET_DATABASE:-$ROOT/polymarket-bot/data-production/trading.db}"
BOT_BIN="${POLYMARKET_BOT_BIN:-$ROOT/polymarket-bot/target/release/polymarket-bot}"
BACKUP="${1:-}"
DRY_RUN="${POLYMARKET_RESTORE_DRY_RUN:-0}"
CONFIRMATION="${POLYMARKET_CONFIRM_RESTORE:-}"

if [[ -z "$BACKUP" || ! -f "$BACKUP" ]]; then
  echo "Usage: $0 /absolute/path/to/backup.db" >&2
  exit 1
fi
if [[ "$DRY_RUN" == "1" ]]; then
  expected_confirmation="DRY_RUN_RESTORE_ONLY"
else
  expected_confirmation="RESTORE_PRODUCTION_DATABASE"
fi
if [[ "$CONFIRMATION" != "$expected_confirmation" ]]; then
  echo "Set POLYMARKET_CONFIRM_RESTORE=$expected_confirmation after reviewing the target database." >&2
  exit 1
fi
if [[ "$DRY_RUN" != "1" ]] && pgrep -f 'polymarket-bot web|polymarket-bot monitor-user-stream' >/dev/null; then
  echo "Refusing restore while dashboard or lifecycle monitor is running." >&2
  exit 1
fi
if [[ ! -x "$BOT_BIN" ]]; then
  echo "Bot binary is missing or not executable: $BOT_BIN" >&2
  exit 1
fi
"$BOT_BIN" verify-database --path "$BACKUP" >/dev/null

mkdir -p "$(dirname "$DATABASE")"
if [[ -f "$DATABASE" ]]; then
  cp --preserve=mode,timestamps "$DATABASE" "$DATABASE.before-restore-$(date -u +%Y%m%dT%H%M%SZ)"
fi
cp --preserve=mode,timestamps "$BACKUP" "$DATABASE"
"$BOT_BIN" verify-database --path "$DATABASE" >/dev/null
if [[ "$DRY_RUN" == "1" ]]; then
  echo "Restore dry-run complete: $DATABASE"
else
  echo "Restore complete. Run authenticated reconciliation before starting the trader."
fi
