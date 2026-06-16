# Production Operator Setup

This is the exact handoff guide for running the current `main` branch in the
same operator shape as this workspace.

## Clean Host Bootstrap

```bash
git clone --branch main git@github.com:riodarmawan/polymarket.git
cd polymarket

sudo apt-get update
sudo apt-get install -y git curl build-essential pkg-config libssl-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

cargo check
(cd polymarket-bot && cargo check && cargo test)
cargo build --release --locked
```

Create the local secret file outside Git:

```bash
./deploy/init-production-secrets.sh
```

Fill only the local locked file:

```text
~/.config/polymarket/production.env
```

Never commit `.env`, `production.env`, databases, logs, private keys,
mnemonics, CLOB secrets, passphrases, relayer credentials, or wallet
credentials.

## Operator Dashboard

Run the dashboard exactly like this:

```bash
./deploy/run-production-operator-dashboard.sh
```

Open:

```text
http://localhost:3001/
```

The local Gamma/CLOB proxy listens on `http://localhost:3000`. The dashboard
binds to `0.0.0.0:3001` so Windows and the Codex in-app browser can reach WSL.

## Current Production Assumptions

- Config file: `polymarket-bot/config/production.toml`
- Starting strategy capital: `$7.50`
- Shared capital pool: `5m` and `15m` both consume the same risk budget
- Minimum order size: `$0.50`
- Maximum order size: `$4.00`
- Maximum risk fraction: `50%` for this small-balance operator canary profile
- Enabled timeframes: `5m` and `15m`
- Backtest CLI default for crypto production checks now uses `$0.50` minimum
  order size, matching the live operator config.

Live submission is deliberately gated by local operator environment variables
and the production hard checks. To arm the live executor on a host that has
completed wallet onboarding, set these only in the locked local secret file or
process environment:

```bash
POLYMARKET_LIVE_TRADING_ENABLED=I_UNDERSTAND_LIVE_TRADING
POLYMARKET_AUTO_LIVE_EXECUTION=I_UNDERSTAND_AUTO_LIVE_EXECUTION
```

Do not bypass failed geoblock, freshness, depth, fee, minimum-size,
reconciliation, or production-preflight checks. If one of those fails, the
correct behavior is no order plus an audit entry explaining why.

## Audit Workflow

Use the dashboard's Execution Audit and Rejection Summary before changing model
logic. Every detected opportunity should preserve:

- timeframe, market, side, intended size, and estimated price
- approval or rejection status
- rejection reason and failed guard
- sizing context
- market metadata used by the decision

The raw endpoint is:

```text
http://localhost:3001/api/execution-audit
```

If opportunities appear but no orders execute, check the audit table first.
Common valid reasons are `min_order`, `depth`, `freshness`, `fee`,
`preflight`, `reconciliation`, or no aligned strategy signal.

## Backtest Commands

Continuous 7-day test with the same shared-capital assumption:

```bash
cd polymarket-bot
POLYMARKET_CONFIG="$PWD/config/production.toml" \
  cargo run --release -- crypto-backtest \
  --symbol btc \
  --period 7 \
  --capital 7.5 \
  --timeframes 5m,15m \
  --source-interval 1
```

Single-day diagnostic:

```bash
cd polymarket-bot
POLYMARKET_CONFIG="$PWD/config/production.toml" \
  cargo run --release -- crypto-backtest \
  --symbol btc \
  --date 2026-06-14 \
  --capital 7.5 \
  --timeframes 5m,15m \
  --source-interval 1
```

Important: daily `--date` runs reset capital at the beginning of that day.
They are not equivalent to one continuous `--period 7` equity curve. In a
continuous run, early losses can trigger the drawdown kill switch before later
days get a chance to trade.

Do not reintroduce the old bug where `5m` was capped below the `$0.50` minimum
order size. Multi-timeframe candidates must be applied chronologically against
one shared capital curve.

## Required Checks Before Push

```bash
./deploy/scan-repo-secrets.sh
cargo check
(cd polymarket-bot && cargo check && cargo test)
git diff --check
cargo build --release --locked
```

If these fail, fix the cause before shipping.
