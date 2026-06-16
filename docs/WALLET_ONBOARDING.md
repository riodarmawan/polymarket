# Production Wallet Onboarding

The application is ready for production-paper operation, not live-money
execution. Live order placement remains intentionally unavailable until the
remaining implementation phases and go-live gates pass.

## Required Identities And Credentials

| Item | Purpose | Secret? |
|---|---|---|
| Owner/session signer EOA address | Public identity that signs wallet batches and orders | No |
| Owner/session signer private key | Signs actions for the dedicated trading identity | Yes |
| Polymarket deposit wallet address | Holds pUSD and outcome tokens; fund this address | No |
| Relayer API key and key address | Deploys and operates the deposit wallet gaslessly | Yes / No |
| CLOB L2 key, secret, and passphrase | Authenticates order API requests | Yes |
| Polygon RPC URL | Connects to Polygon; provider token may be sensitive | Usually |

Do not store a mnemonic on the production laptop. Do not use a personal wallet
that holds other funds. Never reuse any key or credential previously committed,
posted in chat, or otherwise exposed.

## Safe Setup Sequence

1. Install and verify the application in production-paper mode.
2. Create the locked local template:

   ```bash
   ./deploy/init-production-secrets.sh
   ./deploy/check-production-secrets.sh
   ```

3. Generate a dedicated owner/session signer using trusted offline wallet
   tooling. Record its public address in the template and place its private key
   only in the locked local secret file.
4. In Polymarket Settings, create a new Relayer API key.
5. Deploy a new deposit wallet through Relayer V2 using `WALLET-CREATE`.
6. Derive new CLOB V2 L2 credentials for this signer/deposit-wallet identity.
7. Complete deposit-wallet pUSD and conditional-token approvals, then sync CLOB
   balances and allowances.
8. Only after all live implementation phases pass, fund the **deposit wallet**
   with the intended test amount. Do not fund the owner signer EOA.

For the planned `$2` experiment, wait to fund the deposit wallet until the live
executor, reconciliation, canary, and rollback gates are complete. Gasless
relayed actions do not require keeping POL on the signer, but trading requires
pUSD in the deposit wallet.

## Secret File

The default location is:

```text
~/.config/polymarket/production.env
```

The initializer creates its directory with mode `0700` and file with mode
`0600`. It refuses to overwrite an existing file and never generates or prints
a private key.

To use a different secure location:

```bash
POLYMARKET_SECRET_FILE=/secure/path/polymarket-production.env \
  ./deploy/init-production-secrets.sh
```

Keep `POLYMARKET_LIVE_TRADING_ENABLED=disabled`. The secret file is not needed
for production-paper mode.

## Official References

- https://docs.polymarket.com/trading/deposit-wallets
- https://docs.polymarket.com/trading/quickstart
- https://docs.polymarket.com/trading/gasless
- https://docs.polymarket.com/v2-migration
- https://docs.polymarket.com/trading/bridge/deposit
