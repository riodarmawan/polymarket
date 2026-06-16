#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SECRET_FILE="${POLYMARKET_SECRET_FILE:-${XDG_CONFIG_HOME:-$HOME/.config}/polymarket/production.env}"

if [[ ! -f "$SECRET_FILE" ]]; then
  echo "ERROR: secret file not found: $SECRET_FILE" >&2
  echo "Run ./deploy/init-production-secrets.sh first." >&2
  exit 1
fi

# Source the secret file (contains private key — never print to stdout)
set -a
# shellcheck disable=SC1090
source "$SECRET_FILE"
set +a

OWNER_ADDRESS="${POLYMARKET_OWNER_ADDRESS:?POLYMARKET_OWNER_ADDRESS not set}"
PRIVATE_KEY="${POLYMARKET_PRIVATE_KEY:?POLYMARKET_PRIVATE_KEY not set}"
RELAYER_API_KEY="${POLYMARKET_RELAYER_API_KEY:?POLYMARKET_RELAYER_API_KEY not set}"
RELAYER_API_KEY_ADDRESS="${POLYMARKET_RELAYER_API_KEY_ADDRESS:?POLYMARKET_RELAYER_API_KEY_ADDRESS not set}"
RELAYER_URL="${POLYMARKET_RELAYER_URL:-https://polygon.api.relayer.polymarket.com}"
RPC_URL="${POLYMARKET_RPC_URL:-https://rpc-mainnet.matic.quiknode.pro}"
CHAIN_ID="${POLYMARKET_CHAIN_ID:-137}"
DEPOSIT_WALLET_ADDRESS="${POLYMARKET_DEPOSIT_WALLET_ADDRESS:-}"
FACTORY_ADDRESS="0x00000000000Fb5C9ADea0298D729A0CB3823Cc07"

echo "=== Polymarket Deposit Wallet Deployment ==="
echo "Owner address:       $OWNER_ADDRESS"
echo "Relayer URL:         $RELAYER_URL"
echo "Chain ID:            $CHAIN_ID"
echo ""

derive_expected_address_python() {
  python3.10 -c "
import os, sys
sys.path.insert(0, os.path.expanduser('~/.local/lib/python3.10/site-packages'))
from py_builder_relayer_client.builder.derive import derive_uups_deposit_wallet
from py_builder_relayer_client.config import get_contract_config

private_key = os.environ['POLYMARKET_PRIVATE_KEY']
from eth_account import Account
account = Account.from_key(private_key)
owner_address = account.address

config = get_contract_config(${CHAIN_ID})
deposit_wallet = derive_uups_deposit_wallet(
    owner_address,
    config.deposit_wallet_factory,
    config.deposit_wallet_implementation,
)
print(deposit_wallet)
"
}

derive_expected_address_python_with_rpc() {
  python3.10 -c "
import os, sys
sys.path.insert(0, os.path.expanduser('~/.local/lib/python3.10/site-packages'))
from py_builder_relayer_client.client import RelayClient

relayer = RelayClient(
    '${RELAYER_URL}',
    ${CHAIN_ID},
    os.environ['POLYMARKET_PRIVATE_KEY'],
    rpc_url='${RPC_URL}',
)

deposit_wallet = relayer.get_expected_deposit_wallet()
print(deposit_wallet)
"
}

deploy_python_sdk() {
  python3.10 -c "
import os, sys, json, time
sys.path.insert(0, os.path.expanduser('~/.local/lib/python3.10/site-packages'))
from py_builder_relayer_client.client import RelayClient

class RelayerApiKeyHeaders:
    def __init__(self):
        self.headers = {
            'RELAYER_API_KEY': os.environ['POLYMARKET_RELAYER_API_KEY'],
            'RELAYER_API_KEY_ADDRESS': os.environ['POLYMARKET_RELAYER_API_KEY_ADDRESS'],
        }

    def generate_builder_headers(self, method, request_path, body=None):
        return self

    def to_dict(self):
        return self.headers

relayer = RelayClient(
    '${RELAYER_URL}',
    ${CHAIN_ID},
    os.environ['POLYMARKET_PRIVATE_KEY'],
    builder_config=RelayerApiKeyHeaders(),
    rpc_url='${RPC_URL}',
)

deposit_wallet = relayer.get_expected_deposit_wallet()
print(f'Expected deposit wallet: {deposit_wallet}')

response = relayer.deploy_deposit_wallet()
print(f'Transaction ID: {response.transaction_id}')

result = response.wait()
if result is not None:
    print(f'Deployment confirmed. Transaction hash: {result.transaction_hash if hasattr(result, \"transaction_hash\") else result}')
else:
    print('WARNING: Transaction submitted but confirmation timed out.')
    print('Check status manually or re-run this script.')

print(f'DEPOSIT_WALLET={deposit_wallet}')
"
}

deploy_curl() {
  echo "Raw /submit deployment is intentionally disabled." >&2
  echo "The current Relayer /submit API requires an encoded transaction, nonce," >&2
  echo "signature, signatureParams, and proxyWallet. Use the Python SDK path so" >&2
  echo "the request body is built and signed correctly, while authentication uses" >&2
  echo "RELAYER_API_KEY + RELAYER_API_KEY_ADDRESS." >&2
  return 1
}

update_env_file() {
  local new_address="$1"
  local tmp_file
  tmp_file=$(mktemp)

  if grep -q "^POLYMARKET_DEPOSIT_WALLET_ADDRESS=" "$SECRET_FILE"; then
    sed "s|^POLYMARKET_DEPOSIT_WALLET_ADDRESS=.*|POLYMARKET_DEPOSIT_WALLET_ADDRESS=${new_address}|" \
      "$SECRET_FILE" > "$tmp_file"
  else
    {
      echo ""
      echo "# Auto-set by deploy-deposit-wallet.sh"
      echo "POLYMARKET_DEPOSIT_WALLET_ADDRESS=${new_address}"
    } >> "$SECRET_FILE"
    echo "Added POLYMARKET_DEPOSIT_WALLET_ADDRESS to $SECRET_FILE"
    return
  fi

  local old_mode
  old_mode=$(stat -c '%a' "$SECRET_FILE")
  cat "$tmp_file" > "$SECRET_FILE"
  chmod "$old_mode" "$SECRET_FILE"
  rm -f "$tmp_file"
  echo "Updated POLYMARKET_DEPOSIT_WALLET_ADDRESS in $SECRET_FILE"
}

echo "== Step 1: Derive expected deposit wallet address =="
EXPECTED_ADDRESS=""

if command -v python3.10 >/dev/null 2>&1; then
  if python3.10 -c "from py_builder_relayer_client.client import RelayClient; from eth_account import Account" 2>/dev/null; then
    EXPECTED_ADDRESS=$(derive_expected_address_python) || true
    if [[ -n "$EXPECTED_ADDRESS" ]]; then
      echo "Expected deposit wallet (local derivation): $EXPECTED_ADDRESS"
    else
      echo "Local derivation failed, trying RPC-assisted derivation..."
      EXPECTED_ADDRESS=$(derive_expected_address_python_with_rpc) || true
      if [[ -n "$EXPECTED_ADDRESS" ]]; then
        echo "Expected deposit wallet (RPC-assisted): $EXPECTED_ADDRESS"
      fi
    fi
  fi
fi

if [[ -z "$EXPECTED_ADDRESS" ]]; then
  echo "WARNING: Could not derive deposit wallet address." >&2
  echo "Ensure python3.10 with py-builder-relayer-client and eth-account are installed." >&2
  echo "The deposit wallet address will need to be determined after deployment." >&2
fi

echo ""
echo "== Step 2: Deploy deposit wallet =="

if [[ "$DEPOSIT_WALLET_ADDRESS" != "0xreplace_me" && "$DEPOSIT_WALLET_ADDRESS" != "" && "$DEPOSIT_WALLET_ADDRESS" != "$OWNER_ADDRESS" ]]; then
  echo "Deposit wallet appears already deployed: $DEPOSIT_WALLET_ADDRESS"
  echo "Skipping deployment. To force redeploy, set POLYMARKET_DEPOSIT_WALLET_ADDRESS=0xreplace_me"
  echo ""
  echo "Current deposit wallet: $DEPOSIT_WALLET_ADDRESS"
  echo ""
  echo "Next steps:"
  echo "  1. Fund the deposit wallet with pUSD on Polygon"
  echo "  2. Set conditional-token and exchange approvals via the Relayer V2 batch API"
  echo "  3. Derive CLOB L2 credentials (api-key, secret, passphrase) for this identity"
  echo "  4. Fill in CLOB credentials in $SECRET_FILE"
  exit 0
fi

NET_OK=0
curl --silent --connect-timeout 5 -o /dev/null "$RELAYER_URL" 2>/dev/null && NET_OK=1 || true
if [[ "$NET_OK" -eq 0 ]]; then
  echo "ERROR: Cannot reach $RELAYER_URL from this host." >&2
  echo "" >&2
  echo "The Relayer endpoint is unreachable (common in WSL)." >&2
  echo "Options:" >&2
  echo "  1. Run this script from a machine that can reach relayer-v2.polymarket.com" >&2
  echo "  2. Set POLYMARKET_RELAYER_URL to a reachable proxy" >&2
  echo "  3. Use a VPN or configure WSL networking (e.g., mirrored mode)" >&2
  echo "" >&2
  echo "The expected deposit wallet address is: $EXPECTED_ADDRESS" >&2
  echo "You can also deploy manually via the Polymarket web dashboard." >&2
  exit 1
fi

DEPLOYED_ADDRESS=""
DEPLOYED=0

if command -v python3.10 >/dev/null 2>&1; then
  if python3.10 -c "from py_builder_relayer_client.client import RelayClient" 2>/dev/null; then
    echo "Using Python SDK with Relayer API Key auth..."
    if deploy_python_sdk; then
      DEPLOYED=1
      DEPLOYED_ADDRESS="$EXPECTED_ADDRESS"
    fi
  fi
fi

if [[ "$DEPLOYED" -eq 0 ]]; then
  echo "Falling back to raw API (curl)..."
  if deploy_curl; then
    DEPLOYED=1
    DEPLOYED_ADDRESS="${EXPECTED_ADDRESS:-}"
  fi
fi

if [[ "$DEPLOYED" -eq 0 ]]; then
  echo "" >&2
  echo "ERROR: Deposit wallet deployment failed." >&2
  echo "Ensure all Relayer credentials are set in $SECRET_FILE" >&2
  exit 1
fi

echo ""
echo "== Step 3: Update deposit wallet address =="

if [[ -n "$DEPLOYED_ADDRESS" ]]; then
  update_env_file "$DEPLOYED_ADDRESS"
  echo ""
  echo "=== SUCCESS ==="
  echo "Deposit wallet deployed: $DEPLOYED_ADDRESS"
  echo ""
  echo "Next steps:"
  echo "  1. Fund the deposit wallet with pUSD on Polygon"
  echo "  2. Set conditional-token and exchange approvals via the Relayer V2 batch API"
  echo "  3. Derive CLOB L2 credentials (api-key, secret, passphrase) for this identity"
  echo "  4. Fill in CLOB credentials in $SECRET_FILE"
  echo "  5. Verify with: ./deploy/check-production.sh"
else
  echo "WARNING: Deposit wallet was submitted but address could not be derived locally." >&2
  echo "Check the Relayer dashboard or transaction status for your deposit wallet address." >&2
  echo "Then manually update POLYMARKET_DEPOSIT_WALLET_ADDRESS in $SECRET_FILE"
fi
