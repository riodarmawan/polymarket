# Production-Paper Installation

This is the canonical installation path for a new operator or LLM. It deploys
the release binaries with the production configuration while keeping all order
execution paper-only.

## 1. Host Requirements

- Ubuntu 22.04 or WSL2 Ubuntu 22.04
- Git
- Rust stable toolchain with Cargo
- `curl`
- Optional: systemd for persistent services

Install basic dependencies:

```bash
sudo apt-get update
sudo apt-get install -y git curl build-essential pkg-config libssl-dev
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

## 2. Clone The Production Branch

```bash
git clone --branch main git@github.com:riodarmawan/polymarket.git
cd polymarket
```

Do not clone or deploy the historical `master` branch. It is retained only for
legacy development history.

## 3. Build And Test

```bash
cargo build --release
(cd polymarket-bot && cargo build --release && cargo test)
```

## 4. Run Production-Paper

```bash
./deploy/run-production-paper.sh
./deploy/check-production.sh
```

Verify a backup before restore:

```bash
(cd polymarket-bot && ./target/release/polymarket-bot verify-database --path /path/to/backup.db)
```

Open:

```text
http://localhost:3001/
```

Runtime characteristics:

- environment: `production`
- mode: `paper`
- dashboard database: `polymarket-bot/data-production/trading.db`
- public market-data proxy: `http://localhost:3000`
- live order submission: blocked

No wallet or API credentials are required for production-paper.

Stop local services:

```bash
pkill -f 'target/release/polymarket-bot web --port 3001' || true
pkill -f 'target/release/polymarket-gamma' || true
```

## 5. Optional Systemd Installation

Build first, then adjust `User`, `Group`, `WorkingDirectory`, and paths in:

- `deploy/polymarket-gamma.service.example`
- `deploy/polymarket-dashboard.service.example`
- `deploy/polymarket-reconcile.service.example`
- `deploy/polymarket-reconcile.timer.example`
- `deploy/polymarket-backup.service.example`
- `deploy/polymarket-backup.timer.example`
- `deploy/polymarket-trader.service.example`

Install them:

```bash
sudo cp deploy/polymarket-gamma.service.example /etc/systemd/system/polymarket-gamma.service
sudo cp deploy/polymarket-dashboard.service.example /etc/systemd/system/polymarket-dashboard.service
sudo cp deploy/polymarket-reconcile.service.example /etc/systemd/system/polymarket-reconcile.service
sudo cp deploy/polymarket-reconcile.timer.example /etc/systemd/system/polymarket-reconcile.timer
sudo cp deploy/polymarket-backup.service.example /etc/systemd/system/polymarket-backup.service
sudo cp deploy/polymarket-backup.timer.example /etc/systemd/system/polymarket-backup.timer
sudo cp deploy/polymarket-trader.service.example /etc/systemd/system/polymarket-trader.service
sudo systemctl daemon-reload
sudo systemctl enable --now polymarket-gamma polymarket-dashboard
sudo systemctl enable --now polymarket-reconcile.timer polymarket-backup.timer
```

Do not enable `polymarket-trader.service` until all live canary acceptance
criteria pass. Its stop hook attempts authenticated cancel-all.

Check:

```bash
systemctl status polymarket-gamma polymarket-dashboard
curl --fail http://localhost:3001/api/health
```

## 6. Live Trading Is Not Installation Work

Do not add wallet credentials merely to run production-paper. Live execution
remains blocked until all phases in `docs/PRODUCTION_IMPLEMENTATION_PLAN.md`
are complete and `production-check` passes with a completely new wallet.

To prepare a locked, unpopulated secret file for future onboarding:

```bash
./deploy/init-production-secrets.sh
./deploy/check-production-secrets.sh
```

Then follow `docs/WALLET_ONBOARDING.md`. The scripts do not generate private
keys and keep live trading disabled.
