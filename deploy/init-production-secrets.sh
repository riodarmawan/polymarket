#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TEMPLATE="$ROOT/polymarket-bot/production.env.example"
DEST="${POLYMARKET_SECRET_FILE:-${XDG_CONFIG_HOME:-$HOME/.config}/polymarket/production.env}"
DEST_DIR="$(dirname "$DEST")"

if [[ -e "$DEST" || -L "$DEST" ]]; then
  echo "Refusing to overwrite existing secret file: $DEST" >&2
  exit 1
fi

umask 077
mkdir -p "$DEST_DIR"
chmod 700 "$DEST_DIR"

sed \
  "s|POLYMARKET_CONFIG=/absolute/path/to/polymarket-bot/config/production.toml|POLYMARKET_CONFIG=$ROOT/polymarket-bot/config/production.toml|" \
  "$TEMPLATE" > "$DEST"
chmod 600 "$DEST"

echo "Created locked production secret template: $DEST"
echo "No wallet or private key was generated."
echo "Fill the placeholders locally and keep POLYMARKET_LIVE_TRADING_ENABLED=disabled."
echo "Read: $ROOT/docs/WALLET_ONBOARDING.md"
