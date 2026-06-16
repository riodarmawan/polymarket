#!/usr/bin/env bash
set -euo pipefail

SECRET_FILE="${POLYMARKET_SECRET_FILE:-${XDG_CONFIG_HOME:-$HOME/.config}/polymarket/production.env}"

if [[ -L "$SECRET_FILE" ]]; then
  echo "Refusing to inspect a symlinked secret file: $SECRET_FILE" >&2
  exit 1
fi

if [[ ! -f "$SECRET_FILE" ]]; then
  echo "Missing production secret file: $SECRET_FILE" >&2
  exit 1
fi

file_mode="$(stat -c '%a' "$SECRET_FILE")"
dir_mode="$(stat -c '%a' "$(dirname "$SECRET_FILE")")"
failed=0

if [[ "$file_mode" != "600" ]]; then
  echo "FAIL: secret file mode is $file_mode; expected 600" >&2
  failed=1
else
  echo "PASS: secret file mode is 600"
fi

if [[ "$dir_mode" != "700" ]]; then
  echo "FAIL: secret directory mode is $dir_mode; expected 700" >&2
  failed=1
else
  echo "PASS: secret directory mode is 700"
fi

if grep -Eq '=(replace_me|0xreplace_me|https://polygon-rpc\.example)$' "$SECRET_FILE"; then
  echo "INFO: placeholders remain; this is expected until wallet onboarding is complete"
else
  echo "PASS: no known placeholders remain"
fi

if grep -q '^POLYMARKET_LIVE_TRADING_ENABLED=disabled$' "$SECRET_FILE"; then
  echo "PASS: live trading remains disabled"
else
  echo "WARN: live switch is not disabled; live execution is still unavailable" >&2
fi

exit "$failed"
