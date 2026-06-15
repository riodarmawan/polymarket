# Polymarket Trading Bot

Rust-based trading bot for Polymarket prediction markets.

## Features

- Bayesian probability modeling
- Expected value calculation
- Kelly Criterion position sizing
- Order book analysis
- Paper trading
- Backtesting
- Terminal dashboard

## Usage

```bash
# Collect market data
polymarket collect

# Paper trade
polymarket trade --dry-run

# Backtest
polymarket backtest --period 30d

# Dashboard
polymarket dashboard
```

## Configuration

1. Copy `.env.example` to `.env`
2. Run `polymarket config show` to verify

Production secrets must not be stored in the repository. See
`../docs/PRODUCTION_INSTALL.md` for the production-paper fast path and
`../docs/PRODUCTION_RUNBOOK.md` before any live-execution implementation is
enabled.

## Development

```bash
cargo test
cargo build --release
```
