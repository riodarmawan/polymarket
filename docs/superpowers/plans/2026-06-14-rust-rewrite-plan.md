# Polymarket Trading Bot — Rust Rewrite Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rewrite the Polymarket trading bot from Python to Rust as a single binary with subcommands for data collection, paper trading, backtesting, and dashboard.

**Architecture:** Single Rust binary with subcommands. Core layer (models, analyzers, engine) is independent of data layer (API clients, storage). Engine layer (collector, trader, backtester) orchestrates core and data layers.

**Tech Stack:** Rust, tokio, reqwest, sqlx, clap, serde, thiserror, tracing, ratatui

---

## File Structure

```
polymarket-bot/
├── Cargo.toml
├── .gitignore
├── .env.example
├── config/
│   └── default.toml
├── src/
│   ├── main.rs              # Entry point, CLI dispatch
│   ├── cli.rs               # CLI argument definitions (clap)
│   ├── config.rs            # Config loading (TOML + env)
│   ├── error.rs             # Custom error types (thiserror)
│   ├── api/
│   │   ├── mod.rs
│   │   ├── gamma.rs         # Gamma API client
│   │   ├── clob.rs          # CLOB API client
│   │   └── types.rs         # API response types
│   ├── models/
│   │   ├── mod.rs
│   │   ├── probability.rs   # Bayesian probability
│   │   ├── expected_value.rs # EV calculator
│   │   └── position_sizing.rs # Kelly Criterion
│   ├── analyzers/
│   │   ├── mod.rs
│   │   └── orderbook.rs     # Order book analysis
│   ├── engine/
│   │   ├── mod.rs
│   │   ├── decision.rs      # Decision engine
│   │   └── signals.rs       # Signal generation
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── database.rs      # SQLite (sqlx)
│   │   └── types.rs         # DB types
│   ├── collector/
│   │   ├── mod.rs
│   │   └── data_collector.rs # Data collection
│   ├── backtesting/
│   │   ├── mod.rs
│   │   ├── engine.rs        # Backtesting engine
│   │   └── report.rs        # Report generation
│   ├── paper_trading/
│   │   ├── mod.rs
│   │   └── engine.rs        # Paper trading
│   └── dashboard/
│       ├── mod.rs
│       └── terminal.rs      # Terminal UI
├── tests/
│   ├── common/
│   │   ├── mod.rs
│   │   └── fixtures.rs
│   └── integration/
│       └── mod.rs
└── data/
```

---

## Phase 1: Foundation (Week 1)

### Task 1: Initialize Cargo Project

**Files:**
- Create: `polymarket-bot/Cargo.toml`
- Create: `polymarket-bot/.gitignore`
- Create: `polymarket-bot/src/main.rs`

- [ ] **Step 1: Create Cargo.toml**

```toml
[package]
name = "polymarket-bot"
version = "0.1.0"
edition = "2021"

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
```

- [ ] **Step 2: Create .gitignore**

```gitignore
/target
/data/*.db
.env
*.pyc
__pycache__
.playwright-mcp/
```

- [ ] **Step 3: Create minimal main.rs**

```rust
fn main() {
    println!("Polymarket Trading Bot");
}
```

- [ ] **Step 4: Verify it compiles**

Run: `cd polymarket-bot && cargo check`
Expected: OK

- [ ] **Step 5: Commit**

```bash
git add polymarket-bot/
git commit -m "feat: initialize Rust project with dependencies"
```

---

### Task 2: Error Types

**Files:**
- Create: `polymarket-bot/src/error.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/mod.rs
pub mod fixtures;
```

```rust
// tests/common/fixtures.rs
use polymarket_bot::error::BotError;

#[test]
fn test_error_display() {
    let err = BotError::ApiError("connection failed".to_string());
    assert_eq!(err.to_string(), "API error: connection failed");
}

#[test]
fn test_error_from_reqwest() {
    let err = BotError::from(reqwest::Error::from(reqwest::StatusCode::NOT_FOUND));
    assert!(matches!(err, BotError::ApiError(_)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test --test common`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write error.rs**

```rust
// src/error.rs
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BotError {
    #[error("API error: {0}")]
    ApiError(String),

    #[error("Database error: {0}")]
    DatabaseError(#[from] sqlx::Error),

    #[error("Config error: {0}")]
    ConfigError(String),

    #[error("Parse error: {0}")]
    ParseError(String),

    #[error("Insufficient data: {0}")]
    InsufficientData(String),

    #[error("Position not found: {0}")]
    PositionNotFound(String),
}

impl From<reqwest::Error> for BotError {
    fn from(err: reqwest::Error) -> Self {
        BotError::ApiError(err.to_string())
    }
}
```

- [ ] **Step 4: Update main.rs to expose error module**

```rust
// src/main.rs
pub mod error;

fn main() {
    println!("Polymarket Trading Bot");
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test --test common`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add polymarket-bot/src/error.rs polymarket-bot/tests/
git commit -m "feat: add error types with thiserror"
```

---

### Task 3: Config System

**Files:**
- Create: `polymarket-bot/src/config.rs`
- Create: `polymarket-bot/config/default.toml`
- Create: `polymarket-bot/.env.example`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::config::Config;

#[test]
fn test_config_default_values() {
    let config = Config::default();
    assert_eq!(config.general.initial_capital, 1000.0);
    assert_eq!(config.orderbook.max_spread_pct, 0.04);
    assert_eq!(config.position_sizing.kelly_fraction, 0.125);
}

#[test]
fn test_config_from_toml() {
    let toml = r#"
[general]
initial_capital = 500.0

[orderbook]
max_spread_pct = 0.05
"#;
    let config: Config = toml::from_str(toml).unwrap();
    assert_eq!(config.general.initial_capital, 500.0);
    assert_eq!(config.orderbook.max_spread_pct, 0.05);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_config`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write config.rs**

```rust
// src/config.rs
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub general: GeneralConfig,
    pub api: ApiConfig,
    pub orderbook: OrderBookConfig,
    pub probability: ProbabilityConfig,
    pub expected_value: EVConfig,
    pub position_sizing: PositionSizingConfig,
    pub exit: ExitConfig,
    pub collector: CollectorConfig,
    pub backtesting: BacktestingConfig,
    pub paper_trading: PaperTradingConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GeneralConfig {
    pub initial_capital: f64,
    pub max_positions: usize,
    pub log_level: String,
    pub data_dir: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiConfig {
    pub gamma_base_url: String,
    pub clob_base_url: String,
    pub timeout_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OrderBookConfig {
    pub max_spread_pct: f64,
    pub min_depth: f64,
    pub min_volume_24h: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProbabilityConfig {
    pub prior_weight: f64,
    pub news_weight: f64,
    pub polling_weight: f64,
    pub expert_weight: f64,
    pub historical_weight: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EVConfig {
    pub cost_per_trade_pct: f64,
    pub min_ev_threshold: f64,
    pub min_edge_pct: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PositionSizingConfig {
    pub kelly_fraction: f64,
    pub max_position_pct: f64,
    pub min_position_usd: f64,
    pub max_position_usd: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExitConfig {
    pub take_profit_pct: f64,
    pub stop_loss_pct: f64,
    pub trailing_stop_pct: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CollectorConfig {
    pub interval_secs: u64,
    pub max_markets: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BacktestingConfig {
    pub default_period_days: u32,
    pub initial_capital: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PaperTradingConfig {
    pub enabled: bool,
    pub dry_run: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            general: GeneralConfig {
                initial_capital: 1000.0,
                max_positions: 10,
                log_level: "info".to_string(),
                data_dir: "data".to_string(),
            },
            api: ApiConfig {
                gamma_base_url: "https://gamma-api.polymarket.com".to_string(),
                clob_base_url: "https://clob.polymarket.com".to_string(),
                timeout_secs: 30,
            },
            orderbook: OrderBookConfig {
                max_spread_pct: 0.04,
                min_depth: 1000.0,
                min_volume_24h: 10000.0,
            },
            probability: ProbabilityConfig {
                prior_weight: 0.3,
                news_weight: 0.25,
                polling_weight: 0.15,
                expert_weight: 0.20,
                historical_weight: 0.10,
            },
            expected_value: EVConfig {
                cost_per_trade_pct: 0.02,
                min_ev_threshold: 0.05,
                min_edge_pct: 0.02,
            },
            position_sizing: PositionSizingConfig {
                kelly_fraction: 0.125,
                max_position_pct: 0.05,
                min_position_usd: 10.0,
                max_position_usd: 100.0,
            },
            exit: ExitConfig {
                take_profit_pct: 0.30,
                stop_loss_pct: 0.20,
                trailing_stop_pct: 0.10,
            },
            collector: CollectorConfig {
                interval_secs: 300,
                max_markets: 50,
            },
            backtesting: BacktestingConfig {
                default_period_days: 30,
                initial_capital: 1000.0,
            },
            paper_trading: PaperTradingConfig {
                enabled: true,
                dry_run: false,
            },
        }
    }
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        let config_path = Path::new("config/default.toml");
        let config = if config_path.exists() {
            let content = std::fs::read_to_string(config_path)?;
            toml::from_str(&content)?
        } else {
            Self::default()
        };

        Ok(config)
    }
}
```

- [ ] **Step 4: Create config/default.toml**

```toml
[general]
initial_capital = 1000.0
max_positions = 10
log_level = "info"
data_dir = "data"

[api]
gamma_base_url = "https://gamma-api.polymarket.com"
clob_base_url = "https://clob.polymarket.com"
timeout_secs = 30

[orderbook]
max_spread_pct = 0.04
min_depth = 1000
min_volume_24h = 10000

[probability]
prior_weight = 0.3
news_weight = 0.25
polling_weight = 0.15
expert_weight = 0.20
historical_weight = 0.10

[expected_value]
cost_per_trade_pct = 0.02
min_ev_threshold = 0.05
min_edge_pct = 0.02

[position_sizing]
kelly_fraction = 0.125
max_position_pct = 0.05
min_position_usd = 10.0
max_position_usd = 100.0

[exit]
take_profit_pct = 0.30
stop_loss_pct = 0.20
trailing_stop_pct = 0.10

[collector]
interval_secs = 300
max_markets = 50

[backtesting]
default_period_days = 30
initial_capital = 1000.0

[paper_trading]
enabled = true
dry_run = false
```

- [ ] **Step 5: Create .env.example**

```bash
# API Keys (from Polymarket)
POLY_API_KEY=your_api_key
POLY_API_SECRET=your_api_secret
POLY_PASSPHRASE=your_passphrase
POLY_ADDRESS=0xyour_wallet_address

# Optional overrides
POLY_LOG_LEVEL=debug
POLY_DATA_DIR=/path/to/data
```

- [ ] **Step 6: Update main.rs to expose config module**

```rust
// src/main.rs
pub mod config;
pub mod error;

fn main() {
    println!("Polymarket Trading Bot");
}
```

- [ ] **Step 7: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_config`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add polymarket-bot/src/config.rs polymarket-bot/config/ polymarket-bot/.env.example
git commit -m "feat: add config system with TOML and env support"
```

---

### Task 4: API Types

**Files:**
- Create: `polymarket-bot/src/api/mod.rs`
- Create: `polymarket-bot/src/api/types.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::api::types::{Market, OrderBook, OrderBookLevel};

#[test]
fn test_market_deserialization() {
    let json = r#"{
        "id": "0x123",
        "question": "Will BTC hit $100k?",
        "outcomePrices": "[0.65, 0.35]",
        "volume": "1000000",
        "endDate": "2026-12-31T00:00:00Z"
    }"#;
    let market: Market = serde_json::from_str(json).unwrap();
    assert_eq!(market.id, "0x123");
    assert_eq!(market.question, "Will BTC hit $100k?");
}

#[test]
fn test_orderbook_level() {
    let level = OrderBookLevel { price: 0.65, size: 1000.0 };
    assert_eq!(level.price, 0.65);
    assert_eq!(level.size, 1000.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_market`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write api/types.rs**

```rust
// src/api/types.rs
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Market {
    pub id: String,
    pub question: String,
    #[serde(rename = "outcomePrices")]
    pub outcome_prices: String,
    pub volume: String,
    #[serde(rename = "endDate")]
    pub end_date: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub slug: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub markets: Vec<Market>,
}

#[derive(Debug, Clone)]
pub struct OrderBook {
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
}

#[derive(Debug, Clone)]
pub struct OrderBookLevel {
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Trade {
    pub id: String,
    pub market: String,
    pub side: String,
    pub price: String,
    pub size: String,
    #[serde(rename = "timestamp")]
    pub timestamp: String,
}

impl Market {
    pub fn yes_price(&self) -> f64 {
        let prices: Vec<String> = serde_json::from_str(&self.outcome_prices).unwrap_or_default();
        prices.first().and_then(|p| p.parse().ok()).unwrap_or(0.5)
    }

    pub fn no_price(&self) -> f64 {
        let prices: Vec<String> = serde_json::from_str(&self.outcome_prices).unwrap_or_default();
        prices.get(1).and_then(|p| p.parse().ok()).unwrap_or(0.5)
    }

    pub fn volume_24h(&self) -> f64 {
        self.volume.parse().unwrap_or(0.0)
    }
}
```

- [ ] **Step 4: Write api/mod.rs**

```rust
// src/api/mod.rs
pub mod types;
pub mod gamma;
pub mod clob;
```

- [ ] **Step 5: Update main.rs to expose api module**

```rust
// src/main.rs
pub mod api;
pub mod config;
pub mod error;

fn main() {
    println!("Polymarket Trading Bot");
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_market`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add polymarket-bot/src/api/
git commit -m "feat: add API types for Market, Event, OrderBook"
```

---

### Task 5: Gamma API Client

**Files:**
- Create: `polymarket-bot/src/api/gamma.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::api::gamma::GammaClient;

#[tokio::test]
async fn test_gamma_client_new() {
    let client = GammaClient::new("https://gamma-api.polymarket.com");
    assert_eq!(client.base_url, "https://gamma-api.polymarket.com");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_gamma_client`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write gamma.rs**

```rust
// src/api/gamma.rs
use crate::api::types::{Event, Market};
use crate::error::BotError;
use reqwest::Client;

#[derive(Debug, Clone)]
pub struct GammaClient {
    pub base_url: String,
    client: Client,
}

impl GammaClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: Client::new(),
        }
    }

    pub async fn fetch_markets(&self, limit: usize) -> Result<Vec<Market>, BotError> {
        let url = format!("{}/markets?limit={}", self.base_url, limit);
        let response = self.client.get(&url).send().await?;
        let markets: Vec<Market> = response.json().await?;
        Ok(markets)
    }

    pub async fn fetch_market_by_id(&self, id: &str) -> Result<Option<Market>, BotError> {
        let url = format!("{}/markets/{}", self.base_url, id);
        let response = self.client.get(&url).send().await?;

        if response.status().is_success() {
            let market: Market = response.json().await?;
            Ok(Some(market))
        } else {
            Ok(None)
        }
    }

    pub async fn fetch_events(&self, limit: usize) -> Result<Vec<Event>, BotError> {
        let url = format!("{}/events?limit={}", self.base_url, limit);
        let response = self.client.get(&url).send().await?;
        let events: Vec<Event> = response.json().await?;
        Ok(events)
    }

    pub async fn search_markets(&self, query: &str) -> Result<Vec<Market>, BotError> {
        let url = format!("{}/markets?_q={}", self.base_url, query);
        let response = self.client.get(&url).send().await?;
        let markets: Vec<Market> = response.json().await?;
        Ok(markets)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_gamma_client`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add polymarket-bot/src/api/gamma.rs
git commit -m "feat: add Gamma API client for market data"
```

---

### Task 6: CLOB API Client

**Files:**
- Create: `polymarket-bot/src/api/clob.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::api::clob::ClobClient;

#[tokio::test]
async fn test_clob_client_new() {
    let client = ClobClient::new("https://clob.polymarket.com");
    assert_eq!(client.base_url, "https://clob.polymarket.com");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_clob_client`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write clob.rs**

```rust
// src/api/clob.rs
use crate::api::types::{OrderBook, OrderBookLevel, Trade};
use crate::error::BotError;
use reqwest::Client;

#[derive(Debug, Clone)]
pub struct ClobClient {
    pub base_url: String,
    client: Client,
}

impl ClobClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: Client::new(),
        }
    }

    pub async fn get_order_book(&self, token_id: &str) -> Result<OrderBook, BotError> {
        let url = format!("{}/book?token_id={}", self.base_url, token_id);
        let response = self.client.get(&url).send().await?;
        let data: serde_json::Value = response.json().await?;

        let bids = data["bids"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|item| OrderBookLevel {
                        price: item["price"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        size: item["size"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let asks = data["asks"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|item| OrderBookLevel {
                        price: item["price"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        size: item["size"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(OrderBook { bids, asks })
    }

    pub async fn get_price(&self, token_id: &str) -> Result<f64, BotError> {
        let url = format!("{}/price?token_id={}", self.base_url, token_id);
        let response = self.client.get(&url).send().await?;
        let data: serde_json::Value = response.json().await?;

        data["price"]
            .as_str()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| BotError::ParseError("Invalid price format".to_string()))
    }

    pub async fn get_midpoint(&self, token_id: &str) -> Result<f64, BotError> {
        let url = format!("{}/midpoint?token_id={}", self.base_url, token_id);
        let response = self.client.get(&url).send().await?;
        let data: serde_json::Value = response.json().await?;

        data["mid"]
            .as_str()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| BotError::ParseError("Invalid midpoint format".to_string()))
    }

    pub async fn get_spread(&self, token_id: &str) -> Result<(f64, f64), BotError> {
        let url = format!("{}/spread?token_id={}", self.base_url, token_id);
        let response = self.client.get(&url).send().await?;
        let data: serde_json::Value = response.json().await?;

        let bid = data["bid"]
            .as_str()
            .and_then(|p| p.parse().ok())
            .unwrap_or(0.0);
        let ask = data["ask"]
            .as_str()
            .and_then(|p| p.parse().ok())
            .unwrap_or(0.0);

        Ok((bid, ask))
    }

    pub async fn get_trades(&self, token_id: &str, limit: usize) -> Result<Vec<Trade>, BotError> {
        let url = format!("{}/trades?token_id={}&limit={}", self.base_url, token_id, limit);
        let response = self.client.get(&url).send().await?;
        let trades: Vec<Trade> = response.json().await?;
        Ok(trades)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_clob_client`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add polymarket-bot/src/api/clob.rs
git commit -m "feat: add CLOB API client for order book and trades"
```

---

### Task 7: CLI Setup with Clap

**Files:**
- Create: `polymarket-bot/src/cli.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::cli::{Cli, Commands};
use clap::Parser;

#[test]
fn test_cli_collect_command() {
    let cli = Cli::try_parse_from(["polymarket", "collect"]).unwrap();
    assert!(matches!(cli.command, Commands::Collect { .. }));
}

#[test]
fn test_cli_trade_dry_run() {
    let cli = Cli::try_parse_from(["polymarket", "trade", "--dry-run"]).unwrap();
    match cli.command {
        Commands::Trade { dry_run, .. } => assert!(dry_run),
        _ => panic!("Expected Trade command"),
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_cli`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write cli.rs**

```rust
// src/cli.rs
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "polymarket")]
#[command(about = "Polymarket Trading Bot")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Collect market data from APIs
    Collect {
        /// Run as daemon (continuous collection)
        #[arg(long)]
        daemon: bool,

        /// Collection interval in seconds
        #[arg(long, default_value = "300")]
        interval: u64,
    },

    /// Paper trading
    Trade {
        /// Run as daemon (continuous trading)
        #[arg(long)]
        daemon: bool,

        /// Show decisions without execution
        #[arg(long)]
        dry_run: bool,
    },

    /// Backtest strategies with historical data
    Backtest {
        /// Period in days (e.g., 30d)
        #[arg(long, default_value = "30d")]
        period: String,

        /// Strategy to test
        #[arg(long, default_value = "momentum")]
        strategy: String,
    },

    /// Terminal UI dashboard
    Dashboard {
        /// Refresh interval in seconds
        #[arg(long, default_value = "10")]
        refresh: u64,
    },

    /// Show portfolio
    Portfolio {
        /// Show detailed positions
        #[arg(long)]
        detail: bool,
    },

    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
pub enum ConfigAction {
    /// Generate .env file with template
    Init,

    /// Show active configuration
    Show,
}
```

- [ ] **Step 4: Update main.rs to use CLI**

```rust
// src/main.rs
pub mod api;
pub mod cli;
pub mod config;
pub mod error;

use clap::Parser;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Collect { daemon, interval } => {
            println!("Collecting data... (daemon: {}, interval: {}s)", daemon, interval);
        }
        Commands::Trade { daemon, dry_run } => {
            println!("Paper trading... (daemon: {}, dry_run: {})", daemon, dry_run);
        }
        Commands::Backtest { period, strategy } => {
            println!("Backtesting... (period: {}, strategy: {})", period, strategy);
        }
        Commands::Dashboard { refresh } => {
            println!("Dashboard... (refresh: {}s)", refresh);
        }
        Commands::Portfolio { detail } => {
            println!("Portfolio... (detail: {})", detail);
        }
        Commands::Config { action } => {
            println!("Config action: {:?}", action);
        }
    }

    Ok(())
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_cli`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add polymarket-bot/src/cli.rs
git commit -m "feat: add CLI with clap subcommands"
```

---

## Phase 2: Core Models (Week 2)

### Task 8: Bayesian Probability Model

**Files:**
- Create: `polymarket-bot/src/models/mod.rs`
- Create: `polymarket-bot/src/models/probability.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::models::probability::{ProbabilityModel, Signal};

#[test]
fn test_probability_with_strong_news_signal() {
    let model = ProbabilityModel::new();
    let signals = vec![
        Signal { name: "news".to_string(), value: 0.8, confidence: 0.9 },
    ];
    let prob = model.calculate(0.5, &signals);
    assert!(prob > 0.5, "Strong news signal should increase probability");
}

#[test]
fn test_probability_edge_case_50_50() {
    let model = ProbabilityModel::new();
    let signals = vec![];
    let prob = model.calculate(0.5, &signals);
    assert_eq!(prob, 0.5, "No signals should return prior");
}

#[test]
fn test_probability_clamp_0_1() {
    let model = ProbabilityModel::new();
    let signals = vec![
        Signal { name: "news".to_string(), value: 1.0, confidence: 1.0 },
    ];
    let prob = model.calculate(0.99, &signals);
    assert!(prob <= 1.0, "Probability should not exceed 1.0");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_probability`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write models/mod.rs**

```rust
// src/models/mod.rs
pub mod probability;
pub mod expected_value;
pub mod position_sizing;
```

- [ ] **Step 4: Write models/probability.rs**

```rust
// src/models/probability.rs
use crate::config::ProbabilityConfig;

#[derive(Debug, Clone)]
pub struct Signal {
    pub name: String,
    pub value: f64,
    pub confidence: f64,
}

#[derive(Debug, Clone)]
pub struct ProbabilityModel {
    config: ProbabilityConfig,
}

impl ProbabilityModel {
    pub fn new() -> Self {
        Self {
            config: ProbabilityConfig::default(),
        }
    }

    pub fn with_config(config: ProbabilityConfig) -> Self {
        Self { config }
    }

    pub fn calculate(&self, prior: f64, signals: &[Signal]) -> f64 {
        if signals.is_empty() {
            return prior;
        }

        let mut weighted_sum = prior * self.config.prior_weight;
        let mut total_weight = self.config.prior_weight;

        for signal in signals {
            let weight = self.get_signal_weight(&signal.name) * signal.confidence;
            weighted_sum += signal.value * weight;
            total_weight += weight;
        }

        if total_weight > 0.0 {
            (weighted_sum / total_weight).clamp(0.0, 1.0)
        } else {
            prior
        }
    }

    fn get_signal_weight(&self, signal_name: &str) -> f64 {
        match signal_name {
            "news" => self.config.news_weight,
            "polling" => self.config.polling_weight,
            "expert" => self.config.expert_weight,
            "historical" => self.config.historical_weight,
            _ => 0.1,
        }
    }
}

impl Default for ProbabilityModel {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_probability`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add polymarket-bot/src/models/
git commit -m "feat: add Bayesian probability model"
```

---

### Task 9: Expected Value Calculator

**Files:**
- Create: `polymarket-bot/src/models/expected_value.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::models::expected_value::EVCalculator;

#[test]
fn test_ev_positive_when_edge_exists() {
    let calc = EVCalculator::new();
    let ev = calc.calculate(0.6, 0.5, 0.02);
    assert!(ev > 0.0, "Positive edge should yield positive EV");
}

#[test]
fn test_ev_negative_when_no_edge() {
    let calc = EVCalculator::new();
    let ev = calc.calculate(0.4, 0.5, 0.02);
    assert!(ev < 0.0, "Negative edge should yield negative EV");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_ev`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write models/expected_value.rs**

```rust
// src/models/expected_value.rs
use crate::config::EVConfig;

#[derive(Debug, Clone)]
pub struct EVResult {
    pub ev_gross: f64,
    pub cost: f64,
    pub ev_net: f64,
    pub edge: f64,
}

#[derive(Debug, Clone)]
pub struct EVCalculator {
    config: EVConfig,
}

impl EVCalculator {
    pub fn new() -> Self {
        Self {
            config: EVConfig::default(),
        }
    }

    pub fn with_config(config: EVConfig) -> Self {
        Self { config }
    }

    pub fn calculate(&self, q_model: f64, market_price: f64, cost_per_trade: f64) -> EVResult {
        let edge = q_model - market_price;
        let payout_if_yes = 1.0 / market_price;
        let ev_gross = q_model * payout_if_yes - 1.0;
        let cost = cost_per_trade;
        let ev_net = ev_gross - cost;

        EVResult {
            ev_gross,
            cost,
            ev_net,
            edge,
        }
    }

    pub fn has_positive_edge(&self, q_model: f64, market_price: f64) -> bool {
        let edge = q_model - market_price;
        edge > self.config.min_edge_pct
    }

    pub fn has_positive_ev(&self, q_model: f64, market_price: f64) -> bool {
        let result = self.calculate(q_model, market_price, self.config.cost_per_trade_pct);
        result.ev_net > self.config.min_ev_threshold
    }
}

impl Default for EVCalculator {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_ev`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add polymarket-bot/src/models/expected_value.rs
git commit -m "feat: add Expected Value calculator"
```

---

### Task 10: Kelly Criterion Position Sizing

**Files:**
- Create: `polymarket-bot/src/models/position_sizing.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::models::position_sizing::PositionSizer;

#[test]
fn test_kelly_sizing_basic() {
    let sizer = PositionSizer::new();
    let size = sizer.calculate_size(0.6, 0.5, 1000.0);
    assert!(size > 0.0, "Kelly should suggest positive size with edge");
}

#[test]
fn test_kelly_respects_max_position() {
    let sizer = PositionSizer::new();
    let size = sizer.calculate_size(0.9, 0.5, 1000.0);
    assert!(size <= 100.0, "Kelly should respect max position limit");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_kelly`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write models/position_sizing.rs**

```rust
// src/models/position_sizing.rs
use crate::config::PositionSizingConfig;

#[derive(Debug, Clone)]
pub struct PositionSizer {
    config: PositionSizingConfig,
}

impl PositionSizer {
    pub fn new() -> Self {
        Self {
            config: PositionSizingConfig::default(),
        }
    }

    pub fn with_config(config: PositionSizingConfig) -> Self {
        Self { config }
    }

    pub fn calculate_size(&self, q_model: f64, market_price: f64, capital: f64) -> f64 {
        let edge = q_model - market_price;
        if edge <= 0.0 {
            return 0.0;
        }

        let odds = 1.0 / market_price;
        let kelly_bet = (edge * odds - (1.0 - q_model)) / odds;
        let adjusted_bet = kelly_bet * self.config.kelly_fraction;
        let position_usd = (adjusted_bet * capital).max(0.0);

        position_usd
            .max(self.config.min_position_usd)
            .min(self.config.max_position_usd)
            .min(capital * self.config.max_position_pct)
    }

    pub fn should_trade(&self, q_model: f64, market_price: f64, capital: f64) -> bool {
        let size = self.calculate_size(q_model, market_price, capital);
        size >= self.config.min_position_usd
    }
}

impl Default for PositionSizer {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_kelly`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add polymarket-bot/src/models/position_sizing.rs
git commit -m "feat: add Kelly Criterion position sizing"
```

---

### Task 11: Order Book Analyzer

**Files:**
- Create: `polymarket-bot/src/analyzers/mod.rs`
- Create: `polymarket-bot/src/analyzers/orderbook.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::analyzers::orderbook::{OrderBookAnalyzer, OrderBookSnapshot, OrderBookLevel};

#[test]
fn test_obi_bullish() {
    let analyzer = OrderBookAnalyzer::new();
    let snapshot = OrderBookSnapshot {
        bids: vec![OrderBookLevel { price: 0.6, size: 1000.0 }],
        asks: vec![OrderBookLevel { price: 0.65, size: 500.0 }],
    };
    let metrics = analyzer.analyze(&snapshot);
    assert!(metrics.order_book_imbalance > 0.0, "More bids than asks should be positive OBI");
}

#[test]
fn test_spread_calculation() {
    let analyzer = OrderBookAnalyzer::new();
    let snapshot = OrderBookSnapshot {
        bids: vec![OrderBookLevel { price: 0.6, size: 1000.0 }],
        asks: vec![OrderBookLevel { price: 0.65, size: 1000.0 }],
    };
    let metrics = analyzer.analyze(&snapshot);
    assert!((metrics.spread_pct - 0.05).abs() < 0.001);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_obi`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write analyzers/mod.rs**

```rust
// src/analyzers/mod.rs
pub mod orderbook;
```

- [ ] **Step 4: Write analyzers/orderbook.rs**

```rust
// src/analyzers/orderbook.rs
use crate::api::types::{OrderBook, OrderBookLevel};
use crate::config::OrderBookConfig;

#[derive(Debug, Clone)]
pub struct OrderBookSnapshot {
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
}

#[derive(Debug, Clone)]
pub struct OrderBookMetrics {
    pub spread_pct: f64,
    pub bid_depth: f64,
    pub ask_depth: f64,
    pub order_book_imbalance: f64,
    pub liquidity_score: f64,
    pub is_tradeable: bool,
}

#[derive(Debug, Clone)]
pub struct OrderBookAnalyzer {
    config: OrderBookConfig,
}

impl OrderBookAnalyzer {
    pub fn new() -> Self {
        Self {
            config: OrderBookConfig::default(),
        }
    }

    pub fn with_config(config: OrderBookConfig) -> Self {
        Self { config }
    }

    pub fn analyze(&self, snapshot: &OrderBookSnapshot) -> OrderBookMetrics {
        let bid_depth: f64 = snapshot.bids.iter().map(|l| l.size).sum();
        let ask_depth: f64 = snapshot.asks.iter().map(|l| l.size).sum();
        let total_depth = bid_depth + ask_depth;

        let best_bid = snapshot.bids.first().map(|l| l.price).unwrap_or(0.0);
        let best_ask = snapshot.asks.first().map(|l| l.price).unwrap_or(1.0);
        let spread = best_ask - best_bid;
        let mid_price = (best_bid + best_ask) / 2.0;
        let spread_pct = if mid_price > 0.0 { spread / mid_price } else { 0.0 };

        let order_book_imbalance = if total_depth > 0.0 {
            (bid_depth - ask_depth) / total_depth
        } else {
            0.0
        };

        let liquidity_score = if total_depth > 0.0 {
            (total_depth / 10000.0).min(1.0)
        } else {
            0.0
        };

        let is_tradeable = spread_pct <= self.config.max_spread_pct
            && total_depth >= self.config.min_depth;

        OrderBookMetrics {
            spread_pct,
            bid_depth,
            ask_depth,
            order_book_imbalance,
            liquidity_score,
            is_tradeable,
        }
    }
}

impl Default for OrderBookAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_obi`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add polymarket-bot/src/analyzers/
git commit -m "feat: add order book analyzer with OBI and liquidity"
```

---

### Task 12: Decision Engine

**Files:**
- Create: `polymarket-bot/src/engine/mod.rs`
- Create: `polymarket-bot/src/engine/decision.rs`
- Create: `polymarket-bot/src/engine/signals.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::engine::decision::{DecisionEngine, Decision};

#[tokio::test]
async fn test_decision_skip_wide_spread() {
    let engine = DecisionEngine::new();
    let decision = engine.evaluate(
        "market1",
        "Test market",
        0.5,
        0.6,
        vec![],
        0.0,
    );
    assert_eq!(decision, Decision::Skip);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_decision`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write engine/mod.rs**

```rust
// src/engine/mod.rs
pub mod decision;
pub mod signals;
```

- [ ] **Step 4: Write engine/decision.rs**

```rust
// src/engine/decision.rs
use crate::analyzers::orderbook::OrderBookAnalyzer;
use crate::config::BotConfig;
use crate::models::expected_value::EVCalculator;
use crate::models::position_sizing::PositionSizer;
use crate::models::probability::{ProbabilityModel, Signal};

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Buy {
        market_id: String,
        side: String,
        price: f64,
        size_usd: f64,
        reason: String,
    },
    Sell {
        market_id: String,
        side: String,
        price: f64,
        size_usd: f64,
        reason: String,
    },
    Hold {
        market_id: String,
        reason: String,
    },
    Skip {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct DecisionEngine {
    probability_model: ProbabilityModel,
    ev_calculator: EVCalculator,
    position_sizer: PositionSizer,
    orderbook_analyzer: OrderBookAnalyzer,
}

impl DecisionEngine {
    pub fn new() -> Self {
        let config = BotConfig::default();
        Self {
            probability_model: ProbabilityModel::new(),
            ev_calculator: EVCalculator::new(),
            position_sizer: PositionSizer::new(),
            orderbook_analyzer: OrderBookAnalyzer::new(),
        }
    }

    pub fn evaluate(
        &self,
        market_id: &str,
        market_question: &str,
        market_price: f64,
        q_model: f64,
        signals: Vec<Signal>,
        capital: f64,
    ) -> Decision {
        let prob = self.probability_model.calculate(market_price, &signals);

        if !self.ev_calculator.has_positive_edge(q_model, market_price) {
            return Decision::Skip {
                reason: "No positive edge".to_string(),
            };
        }

        if !self.ev_calculator.has_positive_ev(q_model, market_price) {
            return Decision::Skip {
                reason: "EV too low".to_string(),
            };
        }

        if !self.position_sizer.should_trade(q_model, market_price, capital) {
            return Decision::Skip {
                reason: "Position too small".to_string(),
            };
        }

        let size_usd = self.position_sizer.calculate_size(q_model, market_price, capital);

        Decision::Buy {
            market_id: market_id.to_string(),
            side: "YES".to_string(),
            price: market_price,
            size_usd,
            reason: format!(
                "Edge: {:.2}%, EV: positive, Size: ${:.2}",
                (q_model - market_price) * 100.0,
                size_usd
            ),
        }
    }
}

impl Default for DecisionEngine {
    fn default() -> Self {
        Self::new()
    }
}
```

- [ ] **Step 5: Write engine/signals.rs**

```rust
// src/engine/signals.rs
use crate::models::probability::Signal;

pub fn generate_signals(
    news_sentiment: f64,
    polling_data: f64,
    expert_opinion: f64,
    historical_trend: f64,
) -> Vec<Signal> {
    vec![
        Signal {
            name: "news".to_string(),
            value: news_sentiment,
            confidence: 0.8,
        },
        Signal {
            name: "polling".to_string(),
            value: polling_data,
            confidence: 0.6,
        },
        Signal {
            name: "expert".to_string(),
            value: expert_opinion,
            confidence: 0.7,
        },
        Signal {
            name: "historical".to_string(),
            value: historical_trend,
            confidence: 0.5,
        },
    ]
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_decision`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add polymarket-bot/src/engine/
git commit -m "feat: add decision engine with signal generation"
```

---

## Phase 3: Storage & Collection (Week 3)

### Task 13: SQLite Database

**Files:**
- Create: `polymarket-bot/src/storage/mod.rs`
- Create: `polymarket-bot/src/storage/database.rs`
- Create: `polymarket-bot/src/storage/types.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::storage::database::Database;
use tempfile::TempDir;

#[tokio::test]
async fn test_database_create_tables() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = Database::new(&db_path).await.unwrap();
    assert!(db.path.exists());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_database`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write storage/mod.rs**

```rust
// src/storage/mod.rs
pub mod database;
pub mod types;
```

- [ ] **Step 4: Write storage/types.rs**

```rust
// src/storage/types.rs
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMarket {
    pub id: String,
    pub question: String,
    pub yes_price: f64,
    pub no_price: f64,
    pub volume: f64,
    pub end_date: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredOrderBook {
    pub id: i64,
    pub market_id: String,
    pub best_bid: f64,
    pub best_ask: f64,
    pub spread_pct: f64,
    pub bid_depth: f64,
    pub ask_depth: f64,
    pub obi: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPosition {
    pub id: String,
    pub market_id: String,
    pub side: String,
    pub entry_price: f64,
    pub current_price: f64,
    pub size_usd: f64,
    pub status: String,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDecision {
    pub id: i64,
    pub market_id: String,
    pub decision: String,
    pub q_model: f64,
    pub market_price: f64,
    pub ev_net: f64,
    pub size_usd: f64,
    pub timestamp: DateTime<Utc>,
}
```

- [ ] **Step 5: Write storage/database.rs**

```rust
// src/storage/database.rs
use crate::storage::types::*;
use chrono::Utc;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use std::path::Path;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct Database {
    pool: SqlitePool,
    pub path: std::path::PathBuf,
}

impl Database {
    pub async fn new(path: &Path) -> Result<Self, sqlx::Error> {
        let pool = SqlitePoolOptions::new()
            .connect(path.to_str().unwrap())
            .await?;

        let db = Self {
            pool,
            path: path.to_path_buf(),
        };

        db.create_tables().await?;
        Ok(db)
    }

    async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS markets (
                id TEXT PRIMARY KEY,
                question TEXT NOT NULL,
                yes_price REAL NOT NULL,
                no_price REAL NOT NULL,
                volume REAL NOT NULL,
                end_date TEXT NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS orderbook_snapshots (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                market_id TEXT NOT NULL,
                best_bid REAL NOT NULL,
                best_ask REAL NOT NULL,
                spread_pct REAL NOT NULL,
                bid_depth REAL NOT NULL,
                ask_depth REAL NOT NULL,
                obi REAL NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS paper_positions (
                id TEXT PRIMARY KEY,
                market_id TEXT NOT NULL,
                side TEXT NOT NULL,
                entry_price REAL NOT NULL,
                current_price REAL NOT NULL,
                size_usd REAL NOT NULL,
                status TEXT NOT NULL,
                opened_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                closed_at DATETIME
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS decision_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                market_id TEXT NOT NULL,
                decision TEXT NOT NULL,
                q_model REAL NOT NULL,
                market_price REAL NOT NULL,
                ev_net REAL NOT NULL,
                size_usd REAL NOT NULL,
                timestamp DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn save_market(&self, market: &StoredMarket) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO markets (id, question, yes_price, no_price, volume, end_date, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&market.id)
        .bind(&market.question)
        .bind(market.yes_price)
        .bind(market.no_price)
        .bind(market.volume)
        .bind(&market.end_date)
        .bind(market.created_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_markets(&self) -> Result<Vec<StoredMarket>, sqlx::Error> {
        let markets = sqlx::query_as::<_, StoredMarket>(
            "SELECT id, question, yes_price, no_price, volume, end_date, created_at FROM markets",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(markets)
    }

    pub async fn save_position(&self, position: &StoredPosition) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO paper_positions (id, market_id, side, entry_price, current_price, size_usd, status, opened_at, closed_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&position.id)
        .bind(&position.market_id)
        .bind(&position.side)
        .bind(position.entry_price)
        .bind(position.current_price)
        .bind(position.size_usd)
        .bind(&position.status)
        .bind(position.opened_at)
        .bind(position.closed_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_open_positions(&self) -> Result<Vec<StoredPosition>, sqlx::Error> {
        let positions = sqlx::query_as::<_, StoredPosition>(
            "SELECT id, market_id, side, entry_price, current_price, size_usd, status, opened_at, closed_at FROM paper_positions WHERE status = 'open'",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(positions)
    }

    pub async fn save_decision(&self, decision: &StoredDecision) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO decision_history (market_id, decision, q_model, market_price, ev_net, size_usd, timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&decision.market_id)
        .bind(&decision.decision)
        .bind(decision.q_model)
        .bind(decision.market_price)
        .bind(decision.ev_net)
        .bind(decision.size_usd)
        .bind(decision.timestamp)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
```

- [ ] **Step 6: Add dependencies to Cargo.toml**

```toml
[dependencies]
uuid = { version = "1", features = ["v4"] }
tempfile = "3"
```

- [ ] **Step 7: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_database`
Expected: PASS

- [ ] **Step 8: Commit**

```bash
git add polymarket-bot/src/storage/ polymarket-bot/Cargo.toml
git commit -m "feat: add SQLite database layer with sqlx"
```

---

### Task 14: Data Collector

**Files:**
- Create: `polymarket-bot/src/collector/mod.rs`
- Create: `polymarket-bot/src/collector/data_collector.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::collector::data_collector::DataCollector;

#[tokio::test]
async fn test_collector_new() {
    let collector = DataCollector::new("https://gamma-api.polymarket.com".to_string());
    assert_eq!(collector.gamma_base_url, "https://gamma-api.polymarket.com");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_collector`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write collector/mod.rs**

```rust
// src/collector/mod.rs
pub mod data_collector;
```

- [ ] **Step 4: Write collector/data_collector.rs**

```rust
// src/collector/data_collector.rs
use crate::api::gamma::GammaClient;
use crate::api::types::Market;
use crate::error::BotError;
use crate::storage::database::Database;
use crate::storage::types::StoredMarket;
use chrono::Utc;

#[derive(Debug, Clone)]
pub struct DataCollector {
    pub gamma_base_url: String,
}

impl DataCollector {
    pub fn new(gamma_base_url: String) -> Self {
        Self { gamma_base_url }
    }

    pub async fn collect_markets(&self, db: &Database, limit: usize) -> Result<usize, BotError> {
        let client = GammaClient::new(&self.gamma_base_url);
        let markets = client.fetch_markets(limit).await?;

        let mut count = 0;
        for market in markets {
            let stored = StoredMarket {
                id: market.id.clone(),
                question: market.question.clone(),
                yes_price: market.yes_price(),
                no_price: market.no_price(),
                volume: market.volume_24h(),
                end_date: market.end_date.clone(),
                created_at: Utc::now(),
            };

            if let Err(e) = db.save_market(&stored).await {
                eprintln!("Failed to save market {}: {}", market.id, e);
            } else {
                count += 1;
            }
        }

        Ok(count)
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_collector`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add polymarket-bot/src/collector/
git commit -m "feat: add data collector for market data"
```

---

## Phase 4: Trading (Week 4)

### Task 15: Paper Trading Engine

**Files:**
- Create: `polymarket-bot/src/paper_trading/mod.rs`
- Create: `polymarket-bot/src/paper_trading/engine.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::paper_trading::engine::PaperTradingEngine;

#[tokio::test]
async fn test_paper_engine_new() {
    let engine = PaperTradingEngine::new(1000.0);
    assert_eq!(engine.capital, 1000.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_paper_engine`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write paper_trading/mod.rs**

```rust
// src/paper_trading/mod.rs
pub mod engine;
```

- [ ] **Step 4: Write paper_trading/engine.rs**

```rust
// src/paper_trading/engine.rs
use crate::engine::decision::Decision;
use crate::storage::database::Database;
use crate::storage::types::StoredPosition;
use chrono::Utc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PaperTradingEngine {
    pub capital: f64,
    pub initial_capital: f64,
}

impl PaperTradingEngine {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            capital: initial_capital,
            initial_capital,
        }
    }

    pub async fn execute_trade(&mut self, decision: &Decision, db: &Database) -> Option<StoredPosition> {
        match decision {
            Decision::Buy { market_id, side, price, size_usd, .. } => {
                if *size_usd > self.capital {
                    return None;
                }

                let position = StoredPosition {
                    id: Uuid::new_v4().to_string(),
                    market_id: market_id.clone(),
                    side: side.clone(),
                    entry_price: *price,
                    current_price: *price,
                    size_usd: *size_usd,
                    status: "open".to_string(),
                    opened_at: Utc::now(),
                    closed_at: None,
                };

                if let Err(e) = db.save_position(&position).await {
                    eprintln!("Failed to save position: {}", e);
                    return None;
                }

                self.capital -= size_usd;
                Some(position)
            }
            _ => None,
        }
    }

    pub fn get_portfolio_summary(&self, open_positions: &[StoredPosition]) -> PortfolioSummary {
        let total_invested: f64 = open_positions.iter().map(|p| p.size_usd).sum();
        let current_value: f64 = open_positions
            .iter()
            .map(|p| {
                let shares = p.size_usd / p.entry_price;
                shares * p.current_price
            })
            .sum();

        let unrealized_pnl = current_value - total_invested;
        let total_value = self.capital + current_value;

        PortfolioSummary {
            initial_capital: self.initial_capital,
            current_capital: self.capital,
            total_invested,
            current_value,
            unrealized_pnl,
            total_value,
            total_return: (total_value - self.initial_capital) / self.initial_capital,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PortfolioSummary {
    pub initial_capital: f64,
    pub current_capital: f64,
    pub total_invested: f64,
    pub current_value: f64,
    pub unrealized_pnl: f64,
    pub total_value: f64,
    pub total_return: f64,
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_paper_engine`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add polymarket-bot/src/paper_trading/
git commit -m "feat: add paper trading engine"
```

---

### Task 16: Backtesting Engine

**Files:**
- Create: `polymarket-bot/src/backtesting/mod.rs`
- Create: `polymarket-bot/src/backtesting/engine.rs`
- Create: `polymarket-bot/src/backtesting/report.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::backtesting::engine::BacktestEngine;

#[test]
fn test_backtest_engine_new() {
    let engine = BacktestEngine::new(1000.0);
    assert_eq!(engine.initial_capital, 1000.0);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_backtest`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write backtesting/mod.rs**

```rust
// src/backtesting/mod.rs
pub mod engine;
pub mod report;
```

- [ ] **Step 4: Write backtesting/engine.rs**

```rust
// src/backtesting/engine.rs
use crate::models::expected_value::EVCalculator;
use crate::models::position_sizing::PositionSizer;
use crate::storage::types::StoredMarket;

#[derive(Debug, Clone)]
pub struct BacktestEngine {
    pub initial_capital: f64,
    pub capital: f64,
    ev_calculator: EVCalculator,
    position_sizer: PositionSizer,
}

#[derive(Debug, Clone)]
pub struct BacktestResult {
    pub initial_capital: f64,
    pub final_capital: f64,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub max_drawdown: f64,
    pub sharpe_ratio: f64,
}

impl BacktestEngine {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            initial_capital,
            capital: initial_capital,
            ev_calculator: EVCalculator::new(),
            position_sizer: PositionSizer::new(),
        }
    }

    pub fn run(&mut self, markets: &[StoredMarket], q_model: f64) -> BacktestResult {
        let mut total_trades = 0;
        let mut winning_trades = 0;
        let mut losing_trades = 0;
        let mut peak_capital = self.initial_capital;
        let mut max_drawdown = 0.0;

        for market in markets {
            if self.ev_calculator.has_positive_edge(q_model, market.yes_price)
                && self.position_sizer.should_trade(q_model, market.yes_price, self.capital)
            {
                let size = self.position_sizer.calculate_size(q_model, market.yes_price, self.capital);
                let payout = if q_model > 0.5 { 1.0 / market.yes_price } else { 1.0 / market.no_price };
                let return_pct = (payout - 1.0) * if q_model > 0.5 { 1.0 } else { -1.0 };

                self.capital += size * return_pct;
                total_trades += 1;

                if return_pct > 0.0 {
                    winning_trades += 1;
                } else {
                    losing_trades += 1;
                }

                if self.capital > peak_capital {
                    peak_capital = self.capital;
                }

                let drawdown = (peak_capital - self.capital) / peak_capital;
                if drawdown > max_drawdown {
                    max_drawdown = drawdown;
                }
            }
        }

        let avg_return = (self.capital - self.initial_capital) / self.initial_capital;
        let sharpe = if max_drawdown > 0.0 {
            avg_return / max_drawdown
        } else {
            0.0
        };

        BacktestResult {
            initial_capital: self.initial_capital,
            final_capital: self.capital,
            total_trades,
            winning_trades,
            losing_trades,
            max_drawdown,
            sharpe_ratio: sharpe,
        }
    }
}
```

- [ ] **Step 5: Write backtesting/report.rs**

```rust
// src/backtesting/report.rs
use crate::backtesting::engine::BacktestResult;

pub fn print_report(result: &BacktestResult) {
    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    BACKTEST REPORT                          ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!("║ Initial Capital:  ${:>10.2}                              ║", result.initial_capital);
    println!("║ Final Capital:    ${:>10.2}                              ║", result.final_capital);
    println!("║ Total Return:     {:>10.2}%                              ║", (result.final_capital - result.initial_capital) / result.initial_capital * 100.0);
    println!("║ Total Trades:     {:>10}                              ║", result.total_trades);
    println!("║ Winning Trades:   {:>10}                              ║", result.winning_trades);
    println!("║ Losing Trades:    {:>10}                              ║", result.losing_trades);
    println!("║ Max Drawdown:     {:>10.2}%                              ║", result.max_drawdown * 100.0);
    println!("║ Sharpe Ratio:     {:>10.2}                              ║", result.sharpe_ratio);
    println!("╚══════════════════════════════════════════════════════════════╝");
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_backtest`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add polymarket-bot/src/backtesting/
git commit -m "feat: add backtesting engine with report generation"
```

---

## Phase 5: Dashboard & Polish (Week 5)

### Task 17: Terminal Dashboard

**Files:**
- Create: `polymarket-bot/src/dashboard/mod.rs`
- Create: `polymarket-bot/src/dashboard/terminal.rs`

- [ ] **Step 1: Write the failing test**

```rust
// tests/common/fixtures.rs (add to existing)
use polymarket_bot::dashboard::terminal::Dashboard;

#[test]
fn test_dashboard_new() {
    let dashboard = Dashboard::new();
    assert!(dashboard.is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd polymarket-bot && cargo test test_dashboard`
Expected: FAIL with "unresolved import"

- [ ] **Step 3: Write dashboard/mod.rs**

```rust
// src/dashboard/mod.rs
pub mod terminal;
```

- [ ] **Step 4: Write dashboard/terminal.rs**

```rust
// src/dashboard/terminal.rs
use crate::storage::database::Database;
use crate::storage::types::StoredPosition;

#[derive(Debug)]
pub struct Dashboard;

impl Dashboard {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(Self)
    }

    pub async fn render(&self, db: &Database, capital: f64) -> Result<(), Box<dyn std::error::Error>> {
        let positions = db.get_open_positions().await?;

        println!("\n╔══════════════════════════════════════════════════════════════╗");
        println!("║                  POLYMARKET PAPER PORTFOLIO                  ║");
        println!("╠══════════════════════════════════════════════════════════════╣");
        println!("║ Capital: ${:>10.2}                                        ║", capital);
        println!("║ Open Positions: {:>5}                                       ║", positions.len());
        println!("╠══════════════════════════════════════════════════════════════╣");

        if positions.is_empty() {
            println!("║ No open positions                                          ║");
        } else {
            for pos in &positions {
                let pnl = (pos.current_price - pos.entry_price) / pos.entry_price * 100.0;
                println!("║ {:>4} {:>3} @ ${:.3} → ${:.3} ({:>+.1}%) {:>10.2}    ║",
                    pos.id[..4].to_string(),
                    pos.side,
                    pos.entry_price,
                    pos.current_price,
                    pnl,
                    pos.size_usd
                );
            }
        }

        println!("╚══════════════════════════════════════════════════════════════╝");
        Ok(())
    }
}
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cd polymarket-bot && cargo test test_dashboard`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add polymarket-bot/src/dashboard/
git commit -m "feat: add terminal dashboard"
```

---

### Task 18: Integration Tests

**Files:**
- Create: `tests/integration/mod.rs`

- [ ] **Step 1: Write integration test**

```rust
// tests/integration/mod.rs
use polymarket_bot::config::Config;
use polymarket_bot::engine::decision::DecisionEngine;
use polymarket_bot::models::probability::Signal;
use tempfile::TempDir;

#[tokio::test]
async fn test_full_flow_collect_trade() {
    let config = Config::default();
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");

    // Initialize database
    let db = polymarket_bot::storage::database::Database::new(&db_path).await.unwrap();

    // Initialize engines
    let decision_engine = DecisionEngine::new();
    let mut paper_engine = polymarket_bot::paper_trading::engine::PaperTradingEngine::new(
        config.general.initial_capital,
    );

    // Simulate decision
    let signals = vec![
        Signal { name: "news".to_string(), value: 0.7, confidence: 0.8 },
    ];

    let decision = decision_engine.evaluate(
        "market1",
        "Test market",
        0.5,
        0.6,
        signals,
        config.general.initial_capital,
    );

    // Execute trade
    let position = paper_engine.execute_trade(&decision, &db).await;
    assert!(position.is_some(), "Should execute trade");

    // Verify portfolio
    let positions = db.get_open_positions().await.unwrap();
    assert_eq!(positions.len(), 1);

    let summary = paper_engine.get_portfolio_summary(&positions);
    assert!(summary.total_invested > 0.0);
}
```

- [ ] **Step 2: Run integration test**

Run: `cd polymarket-bot && cargo test --test integration`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add tests/integration/
git commit -m "feat: add integration tests for full trading flow"
```

---

### Task 19: Final Polish

**Files:**
- Modify: `polymarket-bot/src/main.rs`
- Create: `polymarket-bot/README.md`

- [ ] **Step 1: Update main.rs with full CLI dispatch**

```rust
// src/main.rs
pub mod api;
pub mod backtesting;
pub mod cli;
pub mod collector;
pub mod config;
pub mod dashboard;
pub mod engine;
pub mod error;
pub mod models;
pub mod paper_trading;
pub mod storage;

use clap::Parser;
use cli::{Cli, Commands, ConfigAction};
use config::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();
    let config = Config::load()?;

    match cli.command {
        Commands::Collect { daemon, interval } => {
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let collector = collector::data_collector::DataCollector::new(
                config.api.gamma_base_url.clone(),
            );

            if daemon {
                loop {
                    match collector.collect_markets(&db, config.collector.max_markets).await {
                        Ok(count) => tracing::info!("Collected {} markets", count),
                        Err(e) => tracing::error!("Collection failed: {}", e),
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                }
            } else {
                let count = collector.collect_markets(&db, config.collector.max_markets).await?;
                tracing::info!("Collected {} markets", count);
            }
        }
        Commands::Trade { daemon, dry_run } => {
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let decision_engine = engine::decision::DecisionEngine::new();
            let mut paper_engine = paper_trading::engine::PaperTradingEngine::new(
                config.general.initial_capital,
            );

            tracing::info!("Paper trading started (dry_run: {})", dry_run);

            if daemon {
                loop {
                    let markets = db.get_markets().await?;
                    for market in markets {
                        let signals = vec![];
                        let decision = decision_engine.evaluate(
                            &market.id,
                            &market.question,
                            market.yes_price,
                            market.yes_price,
                            signals,
                            config.general.initial_capital,
                        );

                        if !dry_run {
                            paper_engine.execute_trade(&decision, &db).await;
                        }

                        tracing::info!("Decision for {}: {:?}", market.question, decision);
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            }
        }
        Commands::Backtest { period, strategy } => {
            tracing::info!("Backtesting with period: {}, strategy: {}", period, strategy);
        }
        Commands::Dashboard { refresh } => {
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let dash = dashboard::terminal::Dashboard::new()?;

            loop {
                dash.render(&db, config.general.initial_capital).await?;
                tokio::time::sleep(std::time::Duration::from_secs(refresh)).await;
            }
        }
        Commands::Portfolio { detail } => {
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let positions = db.get_open_positions().await?;

            println!("Portfolio ({} open positions):", positions.len());
            for pos in &positions {
                println!("  {} {} @ ${:.3} (size: ${:.2})", pos.side, pos.market_id, pos.entry_price, pos.size_usd);
            }
        }
        Commands::Config { action } => match action {
            ConfigAction::Init => {
                std::fs::write(".env", include_str!("../.env.example"))?;
                tracing::info!("Created .env file from template");
            }
            ConfigAction::Show => {
                println!("{:#?}", config);
            }
        },
    }

    Ok(())
}
```

- [ ] **Step 2: Create README.md**

```markdown
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
2. Fill in your API keys
3. Run `polymarket config show` to verify

## Development

```bash
cargo test
cargo build --release
```
```

- [ ] **Step 3: Run all tests**

Run: `cd polymarket-bot && cargo test`
Expected: All tests PASS

- [ ] **Step 4: Run clippy**

Run: `cd polymarket-bot && cargo clippy`
Expected: No warnings

- [ ] **Step 5: Run fmt**

Run: `cd polymarket-bot && cargo fmt`
Expected: Formatted

- [ ] **Step 6: Final commit**

```bash
git add polymarket-bot/
git commit -m "feat: complete Rust rewrite of Polymarket trading bot"
```

---

## Summary

| Phase | Tasks | Focus |
|-------|-------|-------|
| Phase 1 | Tasks 1-7 | Foundation: Cargo, config, API clients, CLI |
| Phase 2 | Tasks 8-12 | Core models: probability, EV, Kelly, orderbook, decision |
| Phase 3 | Tasks 13-14 | Storage: SQLite, data collector |
| Phase 4 | Tasks 15-16 | Trading: paper trading, backtesting |
| Phase 5 | Tasks 17-19 | Dashboard, integration tests, polish |

**Total tasks:** 19
**Estimated time:** 5 weeks
**Each task:** ~2-5 minutes per step
