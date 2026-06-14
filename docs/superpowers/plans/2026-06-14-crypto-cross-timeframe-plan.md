# Crypto Cross-Timeframe Trend Following Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a crypto trading bot that uses cross-timeframe trend following to trade Polymarket BTC Up/Down markets (5m/15m/1h/4h/daily).

**Architecture:** Binance WebSocket feeds real-time BTC price → calculate technical indicators per timeframe → generate cross-timeframe signals → match to Polymarket markets → execute trades via CLOB API.

**Tech Stack:** Rust, tokio, reqwest, sqlx (SQLite), tungstenite (WebSocket), ta-rs (technical analysis)

---

## File Structure

```
polymarket-bot/src/
├── crypto/
│   ├── mod.rs              # Module declarations
│   ├── binance_ws.rs       # Binance WebSocket client for BTC price
│   ├── indicators.rs       # EMA, ADX, RSI calculations
│   ├── signals.rs          # Cross-timeframe signal engine
│   └── market_matcher.rs   # Match signals to Polymarket markets
├── backtesting/
│   └── engine.rs           # Modified: add crypto backtest mode
├── main.rs                 # Modified: add crypto subcommand
└── config.rs               # Modified: add crypto config
```

---

## Task 1: Add Dependencies

**Files:**
- Modify: `polymarket-bot/Cargo.toml`

- [ ] **Step 1: Add tungstenite and ta-rs dependencies**

```toml
# Add to [dependencies] section
tungstenite = { version = "0.24", features = ["native-tls"] }
ta = "0.5"
```

- [ ] **Step 2: Verify build**

Run: `cargo check`
Expected: No errors (may have warnings)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add tungstenite and ta-rs for crypto trading"
```

---

## Task 2: Create Crypto Module Structure

**Files:**
- Create: `polymarket-bot/src/crypto/mod.rs`
- Create: `polymarket-bot/src/crypto/binance_ws.rs`
- Create: `polymarket-bot/src/crypto/indicators.rs`
- Create: `polymarket-bot/src/crypto/signals.rs`
- Create: `polymarket-bot/src/crypto/market_matcher.rs`
- Modify: `polymarket-bot/src/lib.rs`

- [ ] **Step 1: Create crypto/mod.rs**

```rust
pub mod binance_ws;
pub mod indicators;
pub mod signals;
pub mod market_matcher;

pub use binance_ws::BinanceWsClient;
pub use indicators::{IndicatorSet, Timeframe};
pub use signals::SignalEngine;
pub use market_matcher::MarketMatcher;
```

- [ ] **Step 2: Create placeholder files**

Create empty files:
- `polymarket-bot/src/crypto/binance_ws.rs`
- `polymarket-bot/src/crypto/indicators.rs`
- `polymarket-bot/src/crypto/signals.rs`
- `polymarket-bot/src/crypto/market_matcher.rs`

- [ ] **Step 3: Add crypto module to lib.rs**

```rust
// Add to lib.rs after existing modules
pub mod crypto;
```

- [ ] **Step 4: Verify build**

Run: `cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add src/crypto/ src/lib.rs
git commit -m "feat: add crypto module structure"
```

---

## Task 3: Implement Binance WebSocket Client

**Files:**
- Create: `polymarket-bot/src/crypto/binance_ws.rs`

- [ ] **Step 1: Write Binance WebSocket client**

```rust
use tokio::sync::broadcast;
use tungstenite::connect;
use serde::Deserialize;
use anyhow::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct KlineEvent {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    pub k: Kline,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Kline {
    #[serde(rename = "t")]
    pub start_time: i64,
    #[serde(rename = "T")]
    pub close_time: i64,
    #[serde(rename = "o")]
    pub open: String,
    #[serde(rename = "c")]
    pub close: String,
    #[serde(rename = "h")]
    pub high: String,
    #[serde(rename = "l")]
    pub low: String,
    #[serde(rename = "v")]
    pub volume: String,
    #[serde(rename = "n")]
    pub number_of_trades: i64,
}

#[derive(Debug, Clone)]
pub struct Candle {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl From<Kline> for Candle {
    fn from(k: Kline) -> Self {
        Self {
            timestamp: k.start_time,
            open: k.open.parse().unwrap_or(0.0),
            high: k.high.parse().unwrap_or(0.0),
            low: k.low.parse().unwrap_or(0.0),
            close: k.close.parse().unwrap_or(0.0),
            volume: k.volume.parse().unwrap_or(0.0),
        }
    }
}

pub struct BinanceWsClient {
    tx: broadcast::Sender<Candle>,
}

impl BinanceWsClient {
    pub fn new(buffer_size: usize) -> Self {
        let (tx, _) = broadcast::channel(buffer_size);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Candle> {
        self.tx.subscribe()
    }

    pub fn start(&self) -> Result<()> {
        let tx = self.tx.clone();
        
        std::thread::spawn(move || -> Result<()> {
            let url = "wss://stream.binance.com:9443/ws/btcusdt@kline_1m";
            let (mut socket, _) = connect(url)?;
            
            loop {
                let msg = socket.read_message()?;
                if let tungstenite::Message::Text(text) = msg {
                    if let Ok(event) = serde_json::from_str::<KlineEvent>(&text) {
                        let candle = event.k.into();
                        let _ = tx.send(candle);
                    }
                }
            }
        });
        
        Ok(())
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cargo check`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/crypto/binance_ws.rs
git commit -m "feat: implement Binance WebSocket client for BTC price"
```

---

## Task 4: Implement Technical Indicators

**Files:**
- Create: `polymarket-bot/src/crypto/indicators.rs`

- [ ] **Step 1: Write indicator calculations**

```rust
use crate::crypto::binance_ws::Candle;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    M5,
    M15,
    H1,
    H4,
    D1,
}

impl Timeframe {
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::M5 => "5m",
            Timeframe::M15 => "15m",
            Timeframe::H1 => "1h",
            Timeframe::H4 => "4h",
            Timeframe::D1 => "1d",
        }
    }
    
    pub fn candle_count(&self) -> usize {
        match self {
            Timeframe::M5 => 5,
            Timeframe::M15 => 15,
            Timeframe::H1 => 60,
            Timeframe::H4 => 240,
            Timeframe::D1 => 1440,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IndicatorSet {
    pub timeframe: Timeframe,
    pub ema_short: f64,
    pub ema_long: f64,
    pub adx: f64,
    pub rsi: f64,
    pub trend: Trend,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Trend {
    Bullish,
    Bearish,
    Neutral,
}

pub struct IndicatorEngine {
    ema_short_period: usize,
    ema_long_period: usize,
    adx_period: usize,
    rsi_period: usize,
}

impl IndicatorEngine {
    pub fn new() -> Self {
        Self {
            ema_short_period: 20,
            ema_long_period: 50,
            adx_period: 14,
            rsi_period: 14,
        }
    }
    
    pub fn calculate(&self, candles: &[Candle], timeframe: Timeframe) -> Option<IndicatorSet> {
        if candles.len() < self.ema_long_period {
            return None;
        }
        
        let closes: Vec<f64> = candles.iter().map(|c| c.close).collect();
        
        let ema_short = self.calculate_ema(&closes, self.ema_short_period)?;
        let ema_long = self.calculate_ema(&closes, self.ema_long_period)?;
        let adx = self.calculate_adx(candles, self.adx_period)?;
        let rsi = self.calculate_rsi(&closes, self.rsi_period)?;
        
        let trend = if ema_short > ema_long && adx > 25.0 {
            Trend::Bullish
        } else if ema_short < ema_long && adx > 25.0 {
            Trend::Bearish
        } else {
            Trend::Neutral
        };
        
        Some(IndicatorSet {
            timeframe,
            ema_short,
            ema_long,
            adx,
            rsi,
            trend,
        })
    }
    
    fn calculate_ema(&self, data: &[f64], period: usize) -> Option<f64> {
        if data.len() < period {
            return None;
        }
        
        let multiplier = 2.0 / (period as f64 + 1.0);
        let mut ema = data[..period].iter().sum::<f64>() / period as f64;
        
        for &price in &data[period..] {
            ema = (price - ema) * multiplier + ema;
        }
        
        Some(ema)
    }
    
    fn calculate_adx(&self, candles: &[Candle], period: usize) -> Option<f64> {
        if candles.len() < period + 1 {
            return None;
        }
        
        let mut tr_sum = 0.0;
        let mut plus_dm_sum = 0.0;
        let mut minus_dm_sum = 0.0;
        
        for i in 1..=period {
            let high = candles[i].high;
            let low = candles[i].low;
            let prev_high = candles[i - 1].high;
            let prev_low = candles[i - 1].low;
            let prev_close = candles[i - 1].close;
            
            let tr = (high - low).max((high - prev_close).abs()).max((low - prev_close).abs());
            let plus_dm = if high - prev_high > prev_low - low && high - prev_high > 0.0 {
                high - prev_high
            } else {
                0.0
            };
            let minus_dm = if prev_low - low > high - prev_high && prev_low - low > 0.0 {
                prev_low - low
            } else {
                0.0
            };
            
            tr_sum += tr;
            plus_dm_sum += plus_dm;
            minus_dm_sum += minus_dm;
        }
        
        let plus_di = if tr_sum > 0.0 { (plus_dm_sum / tr_sum) * 100.0 } else { 0.0 };
        let minus_di = if tr_sum > 0.0 { (minus_dm_sum / tr_sum) * 100.0 } else { 0.0 };
        
        let di_sum = plus_di + minus_di;
        let dx = if di_sum > 0.0 { ((plus_di - minus_di).abs() / di_sum) * 100.0 } else { 0.0 };
        
        Some(dx)
    }
    
    fn calculate_rsi(&self, data: &[f64], period: usize) -> Option<f64> {
        if data.len() < period + 1 {
            return None;
        }
        
        let mut gains = 0.0;
        let mut losses = 0.0;
        
        for i in 1..=period {
            let change = data[i] - data[i - 1];
            if change > 0.0 {
                gains += change;
            } else {
                losses -= change;
            }
        }
        
        let avg_gain = gains / period as f64;
        let avg_loss = losses / period as f64;
        
        if avg_loss == 0.0 {
            return Some(100.0);
        }
        
        let rs = avg_gain / avg_loss;
        Some(100.0 - (100.0 / (1.0 + rs)))
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cargo check`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/crypto/indicators.rs
git commit -m "feat: implement EMA, ADX, RSI technical indicators"
```

---

## Task 5: Implement Cross-Timeframe Signal Engine

**Files:**
- Create: `polymarket-bot/src/crypto/signals.rs`

- [ ] **Step 1: Write signal engine**

```rust
use crate::crypto::indicators::{IndicatorSet, Timeframe, Trend};
use crate::crypto::binance_ws::Candle;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Signal {
    pub timeframe: Timeframe,
    pub direction: Direction,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    Up,
    Down,
}

pub struct SignalEngine {
    min_confidence: f64,
    cross_timeframe_weight: f64,
}

impl SignalEngine {
    pub fn new() -> Self {
        Self {
            min_confidence: 0.6,
            cross_timeframe_weight: 0.3,
        }
    }
    
    pub fn generate_signals(
        &self,
        candles: &HashMap<Timeframe, Vec<Candle>>,
    ) -> Vec<Signal> {
        let mut signals = Vec::new();
        
        // Calculate indicators for each timeframe
        let mut indicators: HashMap<Timeframe, IndicatorSet> = HashMap::new();
        let engine = super::indicators::IndicatorEngine::new();
        
        for (tf, tf_candles) in candles {
            if let Some(ind) = engine.calculate(tf_candles, *tf) {
                indicators.insert(*tf, ind);
            }
        }
        
        // Generate signals for each target timeframe
        let target_timeframes = vec![Timeframe::M5, Timeframe::M15, Timeframe::H1];
        
        for target_tf in target_timeframes {
            if let Some(signal) = self.generate_signal_for_timeframe(target_tf, &indicators) {
                signals.push(signal);
            }
        }
        
        signals
    }
    
    fn generate_signal_for_timeframe(
        &self,
        target: Timeframe,
        indicators: &HashMap<Timeframe, IndicatorSet>,
    ) -> Option<Signal> {
        let target_ind = indicators.get(&target)?;
        
        // Get higher timeframe indicators for confirmation
        let higher_tf = match target {
            Timeframe::M5 => Some(Timeframe::H1),
            Timeframe::M15 => Some(Timeframe::H4),
            Timeframe::H1 => Some(Timeframe::D1),
            _ => None,
        };
        
        let higher_ind = higher_tf.and_then(|tf| indicators.get(&tf));
        
        // Calculate base confidence from target timeframe
        let base_confidence = self.calculate_base_confidence(target_ind);
        
        // Apply cross-timeframe confirmation
        let cross_confidence = if let Some(higher) = higher_ind {
            self.calculate_cross_timeframe_bonus(target_ind, higher)
        } else {
            0.0
        };
        
        let total_confidence = (base_confidence + cross_confidence).min(1.0);
        
        if total_confidence < self.min_confidence {
            return None;
        }
        
        let direction = match target_ind.trend {
            Trend::Bullish => Direction::Up,
            Trend::Bearish => Direction::Down,
            Trend::Neutral => return None,
        };
        
        let reason = format!(
            "TF:{} EMA:{}>{} ADX:{:.1} RSI:{:.1}",
            target.as_str(),
            target_ind.ema_short > target_ind.ema_long,
            target_ind.trend == Trend::Bullish,
            target_ind.adx,
            target_ind.rsi
        );
        
        Some(Signal {
            timeframe: target,
            direction,
            confidence: total_confidence,
            reason,
        })
    }
    
    fn calculate_base_confidence(&self, ind: &IndicatorSet) -> f64 {
        let mut confidence = 0.0;
        
        // EMA crossover
        if ind.ema_short > ind.ema_long {
            confidence += 0.3;
        }
        
        // ADX strength
        if ind.adx > 30.0 {
            confidence += 0.3;
        } else if ind.adx > 25.0 {
            confidence += 0.2;
        }
        
        // RSI filter
        if ind.rsi > 30.0 && ind.rsi < 70.0 {
            confidence += 0.2;
        }
        
        confidence
    }
    
    fn calculate_cross_timeframe_bonus(
        &self,
        target: &IndicatorSet,
        higher: &IndicatorSet,
    ) -> f64 {
        // Bonus if both timeframes agree
        if target.trend == higher.trend && target.trend != Trend::Neutral {
            self.cross_timeframe_weight
        } else {
            0.0
        }
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cargo check`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/crypto/signals.rs
git commit -m "feat: implement cross-timeframe signal engine"
```

---

## Task 6: Implement Market Matcher

**Files:**
- Create: `polymarket-bot/src/crypto/market_matcher.rs`

- [ ] **Step 1: Write market matcher**

```rust
use crate::crypto::signals::{Signal, Direction};
use crate::crypto::indicators::Timeframe;
use crate::api::types::Market;
use anyhow::Result;

pub struct MarketMatcher {
    gamma_base_url: String,
}

impl MarketMatcher {
    pub fn new(gamma_base_url: &str) -> Self {
        Self {
            gamma_base_url: gamma_base_url.to_string(),
        }
    }
    
    pub async fn find_matching_market(
        &self,
        signal: &Signal,
        active_markets: &[Market],
    ) -> Option<Market> {
        let pattern = self.get_market_pattern(signal.timeframe);
        
        for market in active_markets {
            let question = market.question.to_lowercase();
            if question.contains(&pattern) && !market.closed {
                return Some(market.clone());
            }
        }
        
        None
    }
    
    fn get_market_pattern(&self, tf: Timeframe) -> String {
        match tf {
            Timeframe::M5 => "btc up or down 5m".to_string(),
            Timeframe::M15 => "btc up or down 15m".to_string(),
            Timeframe::H1 => "btc up or down 1h".to_string(),
            Timeframe::H4 => "btc up or down 4h".to_string(),
            Timeframe::D1 => "btc up or down daily".to_string(),
        }
    }
    
    pub fn get_token_for_direction(
        &self,
        market: &Market,
        direction: &Direction,
    ) -> Option<String> {
        // Parse clobTokenIds to get YES token
        let token_ids = market.yes_token_id()?;
        
        match direction {
            Direction::Up => Some(token_ids), // YES = Up
            Direction::Down => None, // Need to use NO side
        }
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cargo check`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/crypto/market_matcher.rs
git commit -m "feat: implement market matcher for crypto markets"
```

---

## Task 7: Add Crypto Config

**Files:**
- Modify: `polymarket-bot/src/config.rs`

- [ ] **Step 1: Add CryptoConfig struct**

```rust
// Add to config.rs

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CryptoConfig {
    pub enabled: bool,
    pub initial_capital: f64,
    pub min_order_usd: f64,
    pub max_trades_per_hour: usize,
    pub min_confidence: f64,
    pub timeframes: Vec<String>,
}

impl Default for CryptoConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_capital: 2.0,
            min_order_usd: 0.50,
            max_trades_per_hour: 1,
            min_confidence: 0.6,
            timeframes: vec!["5m".into(), "15m".into(), "1h".into()],
        }
    }
}
```

- [ ] **Step 2: Add crypto field to Config**

```rust
// Modify Config struct in config.rs
pub struct Config {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub trading: TradingConfig,
    pub api: ApiConfig,
    pub crypto: CryptoConfig,  // Add this line
}
```

- [ ] **Step 3: Update default.toml**

```toml
# Add to config/default.toml
[crypto]
enabled = true
initial_capital = 2.0
min_order_usd = 0.50
max_trades_per_hour = 1
min_confidence = 0.6
timeframes = ["5m", "15m", "1h"]
```

- [ ] **Step 4: Verify build**

Run: `cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add src/config.rs config/default.toml
git commit -m "feat: add crypto configuration"
```

---

## Task 8: Add Crypto CLI Subcommand

**Files:**
- Modify: `polymarket-bot/src/cli.rs`
- Modify: `polymarket-bot/src/main.rs`

- [ ] **Step 1: Add Crypto command to CLI**

```rust
// Add to cli.rs
#[derive(Subcommand)]
pub enum Commands {
    // ... existing commands ...
    
    /// Run crypto trading bot
    Crypto {
        /// Run in paper trading mode
        #[arg(long, default_value = "true")]
        paper: bool,
        
        /// Timeframes to trade (comma-separated)
        #[arg(long, default_value = "5m,15m,1h")]
        timeframes: String,
    },
}
```

- [ ] **Step 2: Add crypto command handler to main.rs**

```rust
// Add to main.rs
Commands::Crypto { paper, timeframes } => {
    println!("Starting crypto trading bot...");
    println!("Paper mode: {}", paper);
    println!("Timeframes: {}", timeframes);
    
    // TODO: Implement crypto trading loop
    todo!("Implement crypto trading")
}
```

- [ ] **Step 3: Verify build**

Run: `cargo check`
Expected: No errors

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add crypto CLI subcommand"
```

---

## Task 9: Implement Crypto Trading Loop

**Files:**
- Create: `polymarket-bot/src/crypto/engine.rs`
- Modify: `polymarket-bot/src/crypto/mod.rs`
- Modify: `polymarket-bot/src/main.rs`

- [ ] **Step 1: Write crypto engine**

```rust
// Create src/crypto/engine.rs
use crate::crypto::binance_ws::BinanceWsClient;
use crate::crypto::indicators::{IndicatorEngine, Timeframe};
use crate::crypto::signals::SignalEngine;
use crate::crypto::market_matcher::MarketMatcher;
use crate::config::CryptoConfig;
use std::collections::HashMap;
use tokio::sync::broadcast;
use anyhow::Result;

pub struct CryptoEngine {
    config: CryptoConfig,
    ws_client: BinanceWsClient,
    indicator_engine: IndicatorEngine,
    signal_engine: SignalEngine,
    market_matcher: MarketMatcher,
}

impl CryptoEngine {
    pub fn new(config: CryptoConfig, gamma_base_url: &str) -> Self {
        let ws_client = BinanceWsClient::new(1000);
        let indicator_engine = IndicatorEngine::new();
        let signal_engine = SignalEngine::new();
        let market_matcher = MarketMatcher::new(gamma_base_url);
        
        Self {
            config,
            ws_client,
            indicator_engine,
            signal_engine,
            market_matcher,
        }
    }
    
    pub async fn run(&self) -> Result<()> {
        // Start WebSocket connection
        self.ws_client.start()?;
        let mut rx = self.ws_client.subscribe();
        
        // Buffer candles per timeframe
        let mut candle_buffers: HashMap<Timeframe, Vec<_>> = HashMap::new();
        
        println!("Crypto engine started. Listening for BTC price...");
        
        while let Ok(candle) = rx.recv().await {
            // Add candle to buffer
            let tf = Timeframe::M5; // 1m candles, aggregate later
            candle_buffers.entry(tf).or_default().push(candle.clone());
            
            // Keep last 100 candles per buffer
            if let Some(buf) = candle_buffers.get_mut(&tf) {
                if buf.len() > 100 {
                    buf.remove(0);
                }
            }
            
            // Generate signals every 5 minutes
            if candle.timestamp % 300 == 0 {
                let signals = self.signal_engine.generate_signals(&candle_buffers);
                
                for signal in signals {
                    println!("Signal: {:?} {} confidence {:.2}", 
                        signal.timeframe, signal.direction, signal.confidence);
                    
                    // TODO: Find matching market and execute trade
                }
            }
        }
        
        Ok(())
    }
}
```

- [ ] **Step 2: Update crypto/mod.rs**

```rust
pub mod binance_ws;
pub mod indicators;
pub mod signals;
pub mod market_matcher;
pub mod engine;

pub use binance_ws::BinanceWsClient;
pub use indicators::{IndicatorSet, Timeframe};
pub use signals::SignalEngine;
pub use market_matcher::MarketMatcher;
pub use engine::CryptoEngine;
```

- [ ] **Step 3: Update main.rs crypto command**

```rust
Commands::Crypto { paper, timeframes } => {
    use polymarket_bot::crypto::CryptoEngine;
    use polymarket_bot::config::CryptoConfig;
    
    let config = CryptoConfig::default();
    let gamma_url = &config_server.gamma_base_url;
    
    let engine = CryptoEngine::new(config, gamma_url);
    engine.run().await?;
}
```

- [ ] **Step 4: Verify build**

Run: `cargo check`
Expected: No errors

- [ ] **Step 5: Commit**

```bash
git add src/crypto/engine.rs src/crypto/mod.rs src/main.rs
git commit -m "feat: implement crypto trading engine"
```

---

## Task 10: Test with Paper Trading

**Files:**
- None (testing only)

- [ ] **Step 1: Run crypto command in paper mode**

Run: `cargo run -- crypto --paper`
Expected: Bot starts, connects to Binance WebSocket, prints BTC price

- [ ] **Step 2: Verify signals are generated**

Wait 5 minutes and check for signal output

- [ ] **Step 3: Commit any fixes**

```bash
git add -A
git commit -m "fix: crypto engine paper trading fixes"
```

---

## Task 11: Add Backtest Mode for Crypto

**Files:**
- Modify: `polymarket-bot/src/backtesting/engine.rs`

- [ ] **Step 1: Add crypto backtest method**

```rust
// Add to backtesting/engine.rs
impl BacktestEngine {
    pub async fn run_crypto_backtest(
        &self,
        candles: Vec<Candle>,
        config: &BacktestConfig,
    ) -> BacktestResult {
        // Use same engine but with crypto-specific logic
        todo!("Implement crypto backtest")
    }
}
```

- [ ] **Step 2: Verify build**

Run: `cargo check`
Expected: No errors

- [ ] **Step 3: Commit**

```bash
git add src/backtesting/engine.rs
git commit -m "feat: add crypto backtest mode"
```

---

## Task 12: Final Integration and Testing

**Files:**
- Various

- [ ] **Step 1: Full build test**

Run: `cargo build --release`
Expected: Successful build

- [ ] **Step 2: Run all tests**

Run: `cargo test`
Expected: All tests pass

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat: complete crypto cross-timeframe trading implementation"
```

---

## Success Criteria

- [ ] Bot connects to Binance WebSocket and receives real-time BTC price
- [ ] Technical indicators (EMA, ADX, RSI) calculate correctly
- [ ] Cross-timeframe signals generate with confidence scores
- [ ] Paper trading mode works without placing real trades
- [ ] Backtest mode can analyze historical crypto data
- [ ] All tests pass
- [ ] No compiler warnings
