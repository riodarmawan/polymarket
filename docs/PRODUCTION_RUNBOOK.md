# Polymarket Production Runbook

## Current Status

The supervised dashboard defaults to paper mode. Operator-live execution is
available only when the local live environment confirmations are set and the
hard production gates pass. Forward promotion and canary evidence are
observational review inputs, not blockers for the auto-live path.

Implementation sequence, architecture changes, and acceptance criteria are
defined in [PRODUCTION_IMPLEMENTATION_PLAN.md](PRODUCTION_IMPLEMENTATION_PLAN.md).

The wallet private key, mnemonic, CLOB credentials, and relayer key previously
shared in chat must be treated as compromised. Never fund or reuse them. Revoke
the associated API keys and create a new signer.

Before every commit, release, or canary review, run:

```bash
./deploy/scan-repo-secrets.sh
```

This scans tracked and untracked repository files for private keys, mnemonics,
CLOB secrets, passphrases, and relayer keys. It is separate from
`check-production-secrets.sh`, which validates the locked local secret file
permissions outside Git.

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

For the current small operator profile, initially deposit only about `$7.50` to
`$8.00` pUSD. Keep a separate wallet for any larger funds.

## Mandatory Preflight

Inspect implementation readiness first:

```bash
POLYMARKET_CONFIG=/absolute/path/to/config/production.toml \
  ./target/release/polymarket-bot production-readiness
```

For CI/operator tooling, use the JSON form:

```bash
POLYMARKET_CONFIG=/absolute/path/to/config/production.toml \
  ./target/release/polymarket-bot production-readiness --json
```

For an actual canary promotion gate, require a non-zero exit when any required
gate is still blocked:

```bash
POLYMARKET_CONFIG=/absolute/path/to/config/production.toml \
  ./target/release/polymarket-bot production-readiness --json --require-canary-ready
```

This report reads the production database, forward-test metrics, runtime state,
open incidents, reconciliation status, live switch, and build provenance. It
must continue to show live execution as `BLOCKED` until every implementation
phase and external acceptance gate is complete.

`deploy/check-production.sh` runs the non-failing inspection by default. Set
`REQUIRE_CANARY_READY=true` only when intentionally checking whether the host is
allowed to proceed to a reviewed canary.

Run the full non-live production-paper drill after deployment changes:

```bash
./deploy/drill-production-paper.sh
```

The drill runs the repository secret scan, health/readiness inspection, one
forward monitor pass, backup plus database integrity verification, and confirms
the canary gate is still blocked. It does not need wallet credentials and does
not submit orders. By default it writes a JSON summary under
`polymarket-bot/data-production/drills/`; set `DRILL_SUMMARY=/path/file.json`
to choose another artifact path.

Run the non-live restore drill after backup/restore or database changes:

```bash
./deploy/drill-restore.sh
```

This creates a fresh backup, restores it into a temporary database through
`restore-production-backup.sh` dry-run mode, verifies integrity, and writes a
`drill_type=restore` summary artifact. Production readiness reads these drill
artifacts and keeps Phase 8 blocked until `production-paper`, `restore`,
`rollback`, `credential-rotation`, `alerts`, and `reboot` drills have all passed.

Run the non-live rollback drill after changing restore, deployment, or database
backup behavior:

```bash
./deploy/drill-rollback.sh
```

This creates a known-good backup, mutates only a temporary database, restores it
through the dry-run restore path, checks that the restored checksum matches the
known-good backup, verifies SQLite integrity, and writes a `drill_type=rollback`
summary artifact.

Run the non-live alerts/check drill after changing deployment checks or health
reporting:

```bash
./deploy/drill-alerts.sh
```

This proves the deployment check passes against the healthy local dashboard and
fails closed against an unreachable health endpoint, then writes a
`drill_type=alerts` summary artifact.

Run the non-live credential rotation drill after changing secret handling:

```bash
./deploy/drill-credential-rotation.sh
```

This uses only placeholder template values in a temporary directory, verifies
`700` directory and `600` file permissions, rotates the placeholder file without
overwriting it, confirms the new file still keeps live trading disabled, and
writes a `drill_type=credential-rotation` summary artifact.

Run the reboot drill only on the intended production host:

```bash
./deploy/prepare-reboot-drill.sh
sudo reboot
./deploy/run-production-paper.sh
./deploy/complete-reboot-drill.sh
```

The completion script compares Linux boot IDs before and after reboot, runs the
production check, and writes a `drill_type=reboot` artifact only if the host
actually rebooted and the dashboard is healthy again.

Run the non-live lifecycle drill after changing execution, recovery,
reconciliation, or redemption code:

```bash
./deploy/drill-lifecycle.sh
```

This runs the lifecycle, live-executor fail-closed, recovery, user-stream, and
redemption-planning tests, then writes a `drill_type=lifecycle-non-live`
artifact. This is necessary evidence, but it does not complete Phase 6 by
itself; Phase 6 still requires a reviewed `lifecycle-live-canary` artifact after
wallet onboarding, dry-sign validation, reconciliation, and explicit operator
authorization.

Run the non-live dry-sign safety drill after changing signer or secret loading
code:

```bash
./deploy/drill-dry-sign-safety.sh
```

This intentionally runs `dry-sign` with wallet and CLOB credentials removed and
passes only if the command fails closed before any signing or submission path.
It writes `drill_type=dry-sign-safety`. This is necessary evidence, but it does
not complete Phase 5 by itself; Phase 5 still requires a redacted
`drill_type=new-wallet-dry-sign` artifact produced on the production host with a
new uncompromised wallet, POLY_1271 signature type `3`, and `submitted=false`.

Run the non-live reconciliation safety drill after changing reconciliation,
secret loading, or incident handling:

```bash
./deploy/drill-reconciliation-safety.sh
```

This runs `reconcile` without wallet/CLOB credentials against a temporary
database and passes only if the command fails closed, leaves runtime halted, and
opens an incident in that temporary database. It writes
`drill_type=reconciliation-safety`. This does not complete Phase 7; Phase 7
still requires a fresh authenticated reconciliation against the real production
database after wallet onboarding.

Before issuing any canary authorization, generate a redacted operator review
packet for the exact durable execution intent:

```bash
POLYMARKET_CONFIG=/absolute/path/to/config/production.toml \
  ./target/release/polymarket-bot canary-review --client-key <client_key>
```

The packet includes the intent, active strategy manifest, readiness evidence,
intent checks, and the next manual commands. It does not issue an authorization,
does not sign an order, and does not submit anything. `review_ready` must be
`true` before the operator may proceed to `authorize-canary`.

Run from the deployment host:

```bash
./deploy/scan-repo-secrets.sh
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

## $7.50 Capital Guards

- One open position across both strategies.
- Live order size cap: `$4.00`, sized to satisfy 5-share markets only when the
  ask is within the configured entry-price ceiling.
- Maximum daily realized loss: `$0.30`.
- Maximum daily orders: `3`.
- Stop after `3` consecutive losses.
- No martingale, averaging down, or automatic risk increase.
- Never place an order unless current book `min_order_size`, tick size, spread,
  balance, allowance, and fee rate have been fetched and validated.
- If the market minimum cannot be satisfied with the configured order cap, skip
  the market.
- Do not assume the backtest fee. Fetch the current fee rate for every token.

## Go-Live Evidence

1. Forward-test paper trades using real CLOB asks.
2. Win rate, profit factor, and drawdown from the current forward report.
3. No settlement mismatch between dashboard and official Polymarket outcome.
4. Preflight passes on the actual deployment host.
5. Deposit wallet pUSD balance and allowances confirmed.
6. A signed order can be created locally without submission.
7. Cancel-all and heartbeat failure behavior tested.
8. Auto-live remains armed only by explicit local environment confirmations.

## Forward-Test Evaluation

Every actionable 5m/15m signal is recorded once per market window with its real
CLOB executable quote, spread, fee rate, central-risk decision, and eventual
official Polymarket outcome. Rejected and unfilled opportunities remain in the
report so promotion metrics cannot ignore them.

CLI report:

```bash
POLYMARKET_CONFIG=/absolute/path/to/config/production.toml \
  ./target/release/polymarket-bot forward-report
```

Forward quality monitor:

```bash
POLYMARKET_CONFIG=/absolute/path/to/config/production.toml \
  ./target/release/polymarket-bot monitor-forward --max-iterations 1
```

The monitor records an audit event on every run. It remains in `collecting`
until all promotion gates pass, prints `promotion_ready` only after the forward
sample clears the required thresholds, and fails closed by setting runtime
state to `halted` plus opening `forward-monitor-halt` if settlement mismatches
or a drawdown breach are detected.

For production-paper hosts, install
`deploy/polymarket-forward-monitor.service.example` and
`deploy/polymarket-forward-monitor.timer.example` as systemd units so this
check runs every five minutes.

Dashboard API:

```text
GET /api/forward-report
```

The report includes daily and rolling metrics, raw direction accuracy,
executable trade accuracy, fill ratio, PnL, profit factor, drawdown, Brier
score, settlement mismatch rate, rejection reasons, and segmentation by
timeframe, UTC date/hour, ask, spread, and strategy regime. `promotion_ready`
must remain false until every stated gate passes.

## Build Provenance

Every startup and `production-readiness` run prints the package version, git
revision, dirty flag, and build timestamp. A canary/live runtime must be built
from a known clean git revision. Dirty builds are allowed only for explicit
development-only drills and are never acceptable for production promotion.

Strategy parameters are exported separately so an operator can verify which
model thresholds are attached to a forward-test report or canary review:

```bash
POLYMARKET_CONFIG=/absolute/path/to/config/production.toml \
  ./target/release/polymarket-bot strategy-manifest
```

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

## Production Control Commands

All commands below are fail-closed. `dry-sign`, `reconcile`, status, incidents,
and backup never submit an order.

```bash
./target/release/polymarket-bot operational-status
./target/release/polymarket-bot backup
./target/release/polymarket-bot verify-database --path /path/to/backup.db
./target/release/polymarket-bot dry-sign --token-id TOKEN --price 0.50 --size 5
./target/release/polymarket-bot reconcile
```

Canary authorization commands are retained for reviewed manual drills. They are
not the primary operator-live path:

```bash
./target/release/polymarket-bot authorize-canary \
  --client-key EXPECTED_CLIENT_KEY \
  --max-usd 0.10 \
  --confirm AUTHORIZE_ONE_LIVE_CANARY

./target/release/polymarket-bot submit-canary \
  --authorization-id AUTHORIZATION_ID \
  --client-key EXPECTED_CLIENT_KEY \
  --confirm SUBMIT_AUTHORIZED_CANARY
```

Emergency and lifecycle commands:

```bash
./target/release/polymarket-bot monitor-user-stream
./target/release/polymarket-bot plan-redemptions
./target/release/polymarket-bot incidents
./target/release/polymarket-bot cancel-all-live --confirm CANCEL_ALL_OPEN_ORDERS
```

After any manual canary attempt, the runtime returns to `HALTED`; reconcile and
review the incident/order history before doing anything else.

`plan-redemptions` only inventories redeemable positions and persists a durable
plan. It does not submit a redemption transaction. POLY_1271 deposit-wallet
redemption must go through an operator-reviewed Relayer V2 batch until that
path has its own signed drill and recovery tests.

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
- Install the example trader, reconciliation, and backup services/timers after
  replacing paths and the service account. Keep the trader disabled until all
  canary gates pass.

## Rollback

1. Disable the live-trading environment switch.
2. Cancel all open orders.
3. Stop the trading service.
4. Reconcile CLOB positions and balances.
5. Run `plan-redemptions`, then redeem or manually close remaining positions
   through an operator-reviewed Relayer V2 batch.
6. Rotate credentials if compromise is suspected.

## Official References

- https://docs.polymarket.com/v2-migration
- https://docs.polymarket.com/trading/quickstart
- https://docs.polymarket.com/trading/deposit-wallets
- https://docs.polymarket.com/api-reference/authentication
- https://docs.polymarket.com/api-reference/geoblock
- https://docs.polymarket.com/trading/orders/create
- https://docs.polymarket.com/trading/orders/overview
