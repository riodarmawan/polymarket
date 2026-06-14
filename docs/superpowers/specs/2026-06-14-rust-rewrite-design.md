# Polymarket Trading Bot — Rust Rewrite Design

**Date:** 2026-06-14
**Status:** Approved
**Author:** opencode

---

## 1. Overview

Rewrite the Polymarket trading bot from Python to Rust. Single binary with subcommands for data collection, paper trading, backtesting, and dashboard.

**Goals:**
- Type-safe, performant trading bot
- Single binary deployment
- Proper testing (unit + integration + property-based)
- Clean architecture with clear module boundaries

**Non-goals:**
- Real money trading (paper trading only for now)
- Web dashboard (terminal UI only)
- Mobile app

---

## 2. Project Structure

```
polymarket-bot/
├── Cargo.toml
├── .gitignore
├── .env.example
├── config/
│   └── default.toml
├── src/
│   ├── main.rs
│   ├── cli.rs
│   ├── config.rs
│   ├── error.rs
│   ├── api/
│   │   ├── mod.rs
│   │   ├── gamma.rs
│   │   ├── clob.rs
│   │   └── types.rs
│   ├── models/
│   │   ├── mod.rs
│   │   ├── probability.rs
│   │   ├── expected_value.rs
│   │   └── position_sizing.rs
│   ├── analyzers/
│   │   ├── mod.rs
│   │   └── orderbook.rs
│   ├── engine/
│   │   ├── mod.rs
│   │   ├── decision.rs
│   │   └── signals.rs
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── database.rs
│   │   └── types.rs
│   ├── collector/
│   │   ├── mod.rs
│   │   └── data_collector.rs
│   ├── backtesting/
│   │   ├── mod.rs
│   │   ├── engine.rs
│   │   └── report.rs
│   ├── paper_trading/
│   │   ├── mod.rs
│   │   └── engine.rs
│   └── dashboard/
│       ├── mod.rs
│       └── terminal.rs
├── tests/
│   ├── models/
│   ├── analyzers/
│   ├── engine/
│   ├── integration/
│   └── common/
├── data/
└── docs/
```

---

## 3. Module Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         CLI Layer                           │
│  (polymarket collect | trade | backtest | dashboard)        │
└──────────────────────────────┬──────────────────────────────┘
                               │
┌──────────────────────────────▼──────────────────────────────┐
│                      Engine Layer                           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │  Collector   │  │   Trader    │  │  Backtester  │        │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘        │
└─────────┼────────────────┼────────────────┼─────────────────┘
          │                │                │
┌─────────▼────────────────▼────────────────▼─────────────────┐
│                      Core Layer                             │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │  Probability │  │     EV      │  │   Kelly     │        │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘        │
│         │                │                │                 │
│  ┌──────▼──────────────▼──────────────▼──────┐             │
│  │           Decision Engine                  │             │
│  └──────────────────┬────────────────────────┘             │
└─────────────────────┼───────────────────────────────────────┘
                      │
┌─────────────────────▼───────────────────────────────────────┐
│                    Data Layer                               │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │
│  │   Gamma API  │  │   CLOB API  │  │   SQLite    │        │
│  └─────────────┘  └─────────────┘  └─────────────┘        │
└─────────────────────────────────────────────────────────────┘
```

---

## 4. CLI Commands

```bash
polymarket collect                    # Collect data once
polymarket collect --daemon           # Collect periodically
polymarket collect --interval 300     # Every 5 minutes

polymarket trade                      # Paper trade once
polymarket trade --daemon             # Continuous paper trading
polymarket trade --dry-run            # Show decisions without execution

polymarket backtest                   # Backtest with historical data
polymarket backtest --period 30d      # Last 30 days
polymarket backtest --strategy momentum

polymarket dashboard                  # Terminal UI dashboard
polymarket dashboard --refresh 10     # Refresh every 10 seconds

polymarket portfolio                  # Show portfolio
polymarket portfolio --detail         # Detailed positions

polymarket config init                # Generate .env file
polymarket config show                # Show active config
```

---

## 5. Configuration

**Config loading order:**
1. `config/default.toml` (defaults)
2. `.env` (environment variables)
3. CLI arguments (overrides)

**Key config sections:**
- `general` — initial capital, max positions, log level
- `api` — base URLs, timeout
- `orderbook` — spread threshold, min depth
- `probability` — Bayesian model weights
- `expected_value` — cost per trade, min EV
- `position_sizing` — Kelly fraction, position limits
- `exit` — take profit, stop loss, trailing stop
- `collector` — interval, max markets
- `backtesting` — period, initial capital
- `paper_trading` — enabled, dry run

---

## 6. Data Flow

### Collect
```
Gamma API → Fetch markets → SQLite
CLOB API → Fetch order book → SQLite
```

### Trade
```
SQLite → Load markets → Analyze order book → Generate signals →
Decision Engine → Execute paper trade → Update SQLite
```

### Backtest
```
SQLite → Load historical data → Replay timeline →
Simulate trades → Generate report
```

### Dashboard
```
SQLite → Load portfolio → Terminal UI → Display
```

---

## 7. Trading Model

**Decision formula:**
```
Market Score = Edge + Liquidity + OBI + Momentum + Volume - Spread - Volatility - Resolution Risk
```

**Entry conditions:**
1. Edge > 5% (q_model - market_price)
2. Spread < 4%
3. Liquidity > $10,000
4. EV > 0
5. Confidence > 60%

**Position sizing:**
```
kelly_bet = (edge * odds - (1 - edge)) / odds
adjusted_bet = kelly_bet * 0.125  # 1/8 Kelly
position_usd = min(adjusted_bet * capital, max_position)
```

**Exit conditions:**
- Take profit: +30%
- Stop loss: -20%
- Trailing stop: -10% from peak

---

## 8. Testing Strategy

**Test types:**
- Unit tests: Each model function
- Integration tests: Module interactions
- Property-based tests: Mathematical models (proptest)
- Snapshot tests: Decision outputs (insta)

**Key test cases:**
- Probability clamping [0, 1]
- EV positive/negative edge cases
- Kelly respecting position limits
- OBI calculation accuracy
- Decision engine skip/entry logic
- Database CRUD operations
- Full collect → trade flow

---

## 9. Dependencies

```toml
[dependencies]
tokio = { version = "1", features = ["full"] }
reqwest = { version = "0.12", features = ["json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.8", features = ["sqlite", "runtime-tokio"] }
clap = { version = "4", features = ["derive"] }
thiserror = "2"
anyhow = "1"
dotenvy = "0.15"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tokio-test = "0.4"
proptest = "1"
insta = { version = "1", features = ["json"] }
```

---

## 10. Phased Implementation

### Phase 1: Foundation (Week 1)
- Cargo project setup
- Config loading (TOML + env)
- Error types (thiserror)
- Gamma API client
- CLOB API client
- Basic CLI with clap
- Unit tests for API clients
- .gitignore + .env.example

### Phase 2: Core Models (Week 2)
- Bayesian probability model
- Expected value calculator
- Kelly Criterion position sizing
- Order book analyzer
- Decision engine
- Unit tests for all models
- Property-based tests

### Phase 3: Storage & Collection (Week 3)
- SQLite database schema
- Database operations (CRUD)
- Data collector (periodic fetch)
- Collector daemon mode
- Integration tests

### Phase 4: Trading (Week 4)
- Paper trading engine
- Backtesting engine
- Trade execution (simulated)
- P&L tracking
- Portfolio summary
- Integration tests

### Phase 5: Dashboard & Polish (Week 5)
- Terminal dashboard (ratatui)
- Portfolio view
- Market list view
- Trade history view
- Documentation
- cargo clippy + cargo fmt

---

## 11. Future Considerations

- Web dashboard (Leptos/Yew)
- Real trading (with CLOB API auth)
- LLM integration for news analysis
- Docker deployment
- Monitoring & alerting

---

## 12. Open Questions

- None at this time (all resolved during brainstorming)
