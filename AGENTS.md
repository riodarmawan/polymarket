# Production Operator Notes

This repository's production branch is `main`.

## Safety Contract

- Production currently means a release build using the production config in
  **paper mode**. Live order placement is intentionally unavailable.
- Never reuse credentials previously committed or shared in chat.
- Never commit `.env`, `production.env`, databases, logs, private keys,
  mnemonics, API secrets, or wallet credentials.
- Do not bypass failed geoblock, freshness, depth, fee, minimum-size, or
  production-preflight checks.

## Fast Path

From the repository root:

```bash
./deploy/run-production-paper.sh
./deploy/check-production.sh
```

Inspect honest forward-test promotion metrics:

```bash
POLYMARKET_CONFIG="$PWD/polymarket-bot/config/production.toml" \
  ./polymarket-bot/target/release/polymarket-bot forward-report
```

Inspect the production control plane and create a consistent backup:

```bash
POLYMARKET_CONFIG="$PWD/polymarket-bot/config/production.toml" \
  ./polymarket-bot/target/release/polymarket-bot operational-status
POLYMARKET_CONFIG="$PWD/polymarket-bot/config/production.toml" \
  ./polymarket-bot/target/release/polymarket-bot backup
```

Prepare a locked local secret template for future live onboarding:

```bash
./deploy/init-production-secrets.sh
./deploy/check-production-secrets.sh
```

Dashboard:

```text
http://localhost:3001/
```

The local Gamma/CLOB proxy listens on port `3000`. The release dashboard listens
on `0.0.0.0:3001` so Windows and the Codex in-app browser can reach WSL.

## Required Verification After Changes

```bash
cargo check
(cd polymarket-bot && cargo check && cargo test)
git diff --check
```

Read `docs/PRODUCTION_INSTALL.md` for installation and
`docs/PRODUCTION_RUNBOOK.md` before touching production configuration. Read
`docs/WALLET_ONBOARDING.md` before creating or funding a trading identity.
