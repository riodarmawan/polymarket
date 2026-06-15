# Polymarket Production Runbook

## Current Status

The dashboard is production-observable but execution remains paper-only. This is
intentional. No live order code may be enabled until every gate below passes.

Implementation sequence, architecture changes, and acceptance criteria are
defined in [PRODUCTION_IMPLEMENTATION_PLAN.md](PRODUCTION_IMPLEMENTATION_PLAN.md).

The wallet private key, mnemonic, CLOB credentials, and relayer key previously
shared in chat must be treated as compromised. Never fund or reuse them. Revoke
the associated API keys and create a new signer.

Polymarket production uses CLOB V2 as of April 28, 2026. New API integrations
should use a deposit wallet with signature type `3` (`POLY_1271`) and pUSD
collateral. Old V1 examples, USDC.e assumptions, and legacy SDKs are invalid.

## Architecture

1. **Owner/session signer**
   - New EOA used only to sign deposit-wallet batches and CLOB orders.
   - Private key lives in a secret manager or root-readable environment file.
   - Never log, print, commit, or put the mnemonic on the server.

2. **Deposit wallet**
   - Deploy through Relayer V2.
   - Holds pUSD and outcome tokens.
   - Must be the CLOB funder/maker/signer for signature type `3`.

3. **Trading service**
   - Reads Gamma/CLOB market data.
   - Runs 5m and 15m models independently.
   - Uses FAK/FOK marketable limit orders only after all live guards pass.
   - Sends heartbeat every 5 seconds and cancels all orders on shutdown.

4. **Dashboard**
   - Must remain bound to localhost or protected by authentication/TLS.
   - Must never expose secrets or an endpoint that enables live trading.

## One-Time Wallet Setup

1. Revoke all previously shared CLOB and relayer API keys.
2. Generate a completely new owner/session signer offline.
3. Create a new Relayer API key from Polymarket Settings.
4. Deploy a deposit wallet through Relayer V2 using `WALLET-CREATE`.
5. Fund the **deposit wallet**, not the owner EOA, with pUSD.
6. Execute required pUSD and conditional-token approvals from the deposit wallet
   through a signed Relayer `WALLET` batch.
7. Sync CLOB balances/allowances using signature type `3`.
8. Derive new CLOB V2 L2 API credentials for the new identity.
9. Store secrets outside Git using `production.env.example` as a field list.

For a $2 experiment, initially deposit only $2 pUSD. Keep a separate wallet for
any larger funds.

## Mandatory Preflight

Inspect implementation readiness first:

```bash
POLYMARKET_CONFIG=/absolute/path/to/config/production.toml \
  ./target/release/polymarket-bot production-readiness
```

This report must continue to show live execution as `BLOCKED` until every
implementation phase is complete.

Run from the deployment host:

```bash
set -a
source /secure/path/polymarket-production.env
set +a
./target/release/polymarket-bot production-check
```

Set `POLYMARKET_CONFIG` to the absolute path of `config/production.toml`. The
service must refuse to start if the expected production config is unavailable.

The check intentionally fails unless:

- all required credentials are present and correctly shaped;
- the server IP is not blocked by `https://polymarket.com/api/geoblock`;
- CLOB V2 production is reachable;
- the explicit live confirmation is set.

Do not bypass a failed geoblock check or use a VPN to evade restrictions.

## $2 Capital Guards

- One open position across both strategies.
- First live order: maximum `$0.10`.
- Maximum daily realized loss: `$0.30`.
- Maximum daily orders: `3`.
- Stop after `3` consecutive losses.
- No martingale, averaging down, or automatic risk increase.
- Never place an order unless current book `min_order_size`, tick size, spread,
  balance, allowance, and fee rate have been fetched and validated.
- If the market minimum cannot be satisfied with `$0.10`, skip the market.
- Do not assume the backtest fee. Fetch the current fee rate for every token.

## Go-Live Gates

1. At least 200 forward-test paper trades using real CLOB asks.
2. Win rate at least 68%, profit factor at least 1.40, drawdown at most 20%.
3. No settlement mismatch between dashboard and official Polymarket outcome.
4. Preflight passes on the actual deployment host.
5. Deposit wallet pUSD balance and allowances confirmed.
6. A signed order can be created locally without submission.
7. Cancel-all and heartbeat failure behavior tested.
8. First live order requires a manual one-time canary command.

## Live Execution Requirements

The live executor must implement all of these before it can be considered ready:

- CLOB V2 SDK or exact V2 EIP-712/POLY_1271 signing.
- Authenticated user WebSocket for fills and order state.
- Idempotency: one client order key per market/timeframe.
- Fetch tick size, min order size, neg-risk flag, and fee rate before submission.
- FAK/FOK handling with partial-fill accounting.
- Heartbeat every 5 seconds; fail closed if heartbeat fails.
- Cancel all orders on SIGINT/SIGTERM and startup recovery.
- Reconcile local state against CLOB open orders, trades, and positions.
- Redeem winning positions after resolution.
- Structured audit log without secrets.

## Deployment

- Run the Gamma proxy and dashboard as separate supervised services.
- On a native Linux server, bind the dashboard to localhost or protect it with
  an authenticated reverse proxy. The WSL/Codex desktop setup binds the
  dashboard to `0.0.0.0:3001` so Windows localhost forwarding can reach it;
  restrict port `3001` with the host firewall.
- Use a dedicated non-root OS user.
- Restrict the production secret file to mode `0600`.
- Persist trade/audit state on durable storage.
- Alert on process exit, heartbeat failure, geoblock change, repeated API
  errors, and daily-loss stop.

## Rollback

1. Disable the live-trading environment switch.
2. Cancel all open orders.
3. Stop the trading service.
4. Reconcile CLOB positions and balances.
5. Redeem or manually close remaining positions.
6. Rotate credentials if compromise is suspected.

## Official References

- https://docs.polymarket.com/v2-migration
- https://docs.polymarket.com/trading/quickstart
- https://docs.polymarket.com/trading/deposit-wallets
- https://docs.polymarket.com/api-reference/authentication
- https://docs.polymarket.com/api-reference/geoblock
- https://docs.polymarket.com/trading/orders/create
- https://docs.polymarket.com/trading/orders/overview
