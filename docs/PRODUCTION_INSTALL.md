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

Stop local services:

```bash
pkill -f 'target/release/polymarket-bot web --port 3001' || true
pkill -f 'target/release/polymarket-gamma' || true
```

## 5. Optional Systemd Installation

Build first, then adjust `User`, `Group`, `WorkingDirectory`, and paths in:

- `deploy/polymarket-gamma.service.example`
- `deploy/polymarket-dashboard.service.example`

Install them:

```bash
sudo cp deploy/polymarket-gamma.service.example /etc/systemd/system/polymarket-gamma.service
sudo cp deploy/polymarket-dashboard.service.example /etc/systemd/system/polymarket-dashboard.service
sudo systemctl daemon-reload
sudo systemctl enable --now polymarket-gamma polymarket-dashboard
```

Check:

```bash
systemctl status polymarket-gamma polymarket-dashboard
curl --fail http://localhost:3001/api/health
```

## 6. Live Trading Is Not Installation Work

Do not add wallet credentials merely to run production-paper. Live execution
remains blocked until all phases in `docs/PRODUCTION_IMPLEMENTATION_PLAN.md`
are complete and `production-check` passes with a completely new wallet.
