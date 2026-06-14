# Backtesting Terminal UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add backtesting with real historical price data from CLOB API and a ratatui terminal UI showing equity curve, trade log, and performance metrics.

**Architecture:** 
1. Extend the Rust proxy (`polymarket-gamma`) with a `/api/prices-history` route that proxies to CLOB API
2. Enhance backtesting engine to accept historical price series and produce detailed trade-by-trade results
3. Build ratatui TUI that renders equity curve (ASCII chart), trade log table, and summary stats

**Tech Stack:** Rust, ratatui, crossterm, reqwest, tokio

---

## File Structure

```
polymarket-gamma/src/
├── main.rs                          # Add /api/prices-history route
├── gamma/
│   ├── mod.rs
│   ├── routes.rs                    # Add price_history handler
│   └── client.rs                    # (existing)

polymarket-bot/
├── Cargo.toml                       # Add ratatui, crossterm
├── src/
│   ├── api/
│   │   └── clob.rs                  # Add fetch_price_history method
│   ├── backtesting/
│   │   ├── mod.rs                   # Add ui module
│   │   ├── engine.rs                # Enhance to produce trade log
│   │   ├── report.rs                # Keep for terminal summary
│   │   └── ui.rs                    # NEW: ratatui TUI
│   └── main.rs                      # Update backtest command
```

---

## Task 1: Add CLOB Price History to Proxy

**Files:**
- Modify: `polymarket-gamma/src/gamma/routes.rs`
- Modify: `polymarket-gamma/src/gamma/client.rs` (if needed, but client already uses reqwest)

**Context:** The proxy currently only routes to Gamma API. We need to add a route that proxies to CLOB API `/prices-history` endpoint. The CLOB API endpoint format is:
```
GET https://clob.polymarket.com/prices-history?market={token_id}&interval={interval}&fidelity={fidelity}
```

Response format:
```json
{"history": [{"t": 1781326817, "p": 0.51}, ...]}
```

- [ ] **Step 1: Add price_history handler to routes.rs**

Add to `polymarket-gamma/src/gamma/routes.rs`:

```rust
#[derive(Debug, Deserialize)]
pub struct PriceHistoryQuery {
    pub market: String,
    #[serde(default = "default_interval")]
    pub interval: String,
    #[serde(default = "default_fidelity")]
    pub fidelity: u32,
}

fn default_interval() -> String {
    "1d".to_string()
}

fn default_fidelity() -> u32 {
    60
}
```

Add route to router():
```rust
.route("/api/prices-history", get(price_history))
```

Add handler:
```rust
#[instrument(skip(state))]
async fn price_history(
    Query(q): Query<PriceHistoryQuery>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .gamma
        .fetch_price_history(&q.market, &q.interval, q.fidelity)
        .await
        .map_err(internal)
        .and_then(json)
}
```

- [ ] **Step 2: Add fetch_price_history to client**

Add to `polymarket-gamma/src/gamma/client.rs`:

```rust
use std::net::{IpAddr, Ipv4Addr, SocketAddr};

const CLOB_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(104, 18, 34, 205)); // Same Cloudflare IP

impl GammaClient {
    // ... existing methods ...

    #[instrument(skip(self))]
    pub async fn fetch_price_history(
        &self,
        market: &str,
        interval: &str,
        fidelity: u32,
    ) -> color_eyre::Result<serde_json::Value> {
        let url = format!(
            "https://clob.polymarket.com/prices-history?market={market}&interval={interval}&fidelity={fidelity}"
        );
        tracing::debug!(%url, "fetching price history from CLOB");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body)
    }
}
```

Note: The proxy already resolves to Cloudflare IP `104.18.34.205`. CLOB API uses the same Cloudflare infrastructure, so the same `resolve_to_addrs` approach should work. If CLOB has a different IP, we'll need to add a second client or use DNS resolution.

- [ ] **Step 3: Test the endpoint**

```bash
cd polymarket-gamma && cargo build --release
./target/release/polymarket-gamma &
sleep 2
curl -s "http://localhost:3000/api/prices-history?market=98022490269692409998126496127597032490334070080325855126491859374983463996227&interval=1d&fidelity=60"
```

Expected: JSON with `{"history": [...]}`

- [ ] **Step 4: Commit**

```bash
git add src/gamma/routes.rs src/gamma/client.rs
git commit -m "feat: add CLOB price history proxy endpoint"
```

---

## Task 2: Add Price History Client to Bot

**Files:**
- Modify: `polymarket-bot/src/api/clob.rs`

- [ ] **Step 1: Add fetch_price_history method**

Add to `polymarket-bot/src/api/clob.rs`:

```rust
#[derive(Debug, Clone, Deserialize)]
pub struct PricePoint {
    pub t: u64,
    pub p: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PriceHistory {
    pub history: Vec<PricePoint>,
}

impl ClobClient {
    // ... existing methods ...

    pub async fn fetch_price_history(
        &self,
        token_id: &str,
        interval: &str,
        fidelity: u32,
    ) -> Result<PriceHistory, BotError> {
        let url = format!(
            "{}/prices-history?market={}&interval={}&fidelity={}",
            self.base_url, token_id, interval, fidelity
        );
        let response = self.client.get(&url).send().await?;
        let history: PriceHistory = response.json().await?;
        Ok(history)
    }
}
```

- [ ] **Step 2: Test compilation**

```bash
cd polymarket-bot && cargo check
```

Expected: OK

- [ ] **Step 3: Commit**

```bash
git add src/api/clob.rs
git commit -m "feat: add price history client method"
```

---

## Task 3: Enhance Backtest Engine with Trade Log

**Files:**
- Modify: `polymarket-bot/src/backtesting/engine.rs`

- [ ] **Step 1: Write failing test**

```rust
// In tests/integration/backtest_test.rs or existing test file
use polymarket_bot::backtesting::engine::{BacktestEngine, TradeRecord};

#[test]
fn test_backtest_produces_trade_log() {
    let mut engine = BacktestEngine::new(1000.0);
    let prices = vec![0.5, 0.52, 0.55, 0.53, 0.58, 0.6, 0.57, 0.55];
    let result = engine.run_with_prices(&prices, 0.6);
    assert!(!result.trades.is_empty(), "Should produce trade records");
    assert!(result.equity_curve.len() > 1, "Should track equity over time");
}

#[test]
fn test_trade_record_fields() {
    let mut engine = BacktestEngine::new(1000.0);
    let prices = vec![0.5, 0.55, 0.6];
    let result = engine.run_with_prices(&prices, 0.6);
    let trade = &result.trades[0];
    assert!(trade.entry_price > 0.0);
    assert!(trade.size_usd > 0.0);
    assert!(!trade.action.is_empty());
}
```

- [ ] **Step 2: Run test to verify it fails**

```bash
cd polymarket-bot && cargo test test_backtest_produces_trade_log
```

Expected: FAIL with "no method named `run_with_prices`"

- [ ] **Step 3: Enhance engine.rs**

Replace `polymarket-bot/src/backtesting/engine.rs` with:

```rust
use crate::models::expected_value::EVCalculator;
use crate::models::position_sizing::PositionSizer;

#[derive(Debug, Clone)]
pub struct TradeRecord {
    pub step: usize,
    pub action: String, // "BUY_YES", "BUY_NO", "SELL", "HOLD", "SKIP"
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub size_usd: f64,
    pub pnl: f64,
    pub reason: String,
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
    pub trades: Vec<TradeRecord>,
    pub equity_curve: Vec<(usize, f64)>,
}

#[derive(Debug, Clone)]
pub struct BacktestEngine {
    pub initial_capital: f64,
    pub capital: f64,
    ev_calculator: EVCalculator,
    position_sizer: PositionSizer,
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

    /// Run backtest with a series of prices (simple approach)
    pub fn run_with_prices(&mut self, prices: &[f64], q_model: f64) -> BacktestResult {
        let mut trades = Vec::new();
        let mut equity_curve = vec![(0, self.initial_capital)];
        let mut total_trades = 0;
        let mut winning_trades = 0;
        let mut losing_trades = 0;
        let mut peak_capital = self.initial_capital;
        let mut max_drawdown = 0.0;

        for (i, &price) in prices.iter().enumerate() {
            if self
                .ev_calculator
                .has_positive_edge(q_model, price)
                && self
                    .position_sizer
                    .should_trade(q_model, price, self.capital)
            {
                let size = self
                    .position_sizer
                    .calculate_size(q_model, price, self.capital);
                let payout = 1.0 / price;
                let return_pct = payout - 1.0;

                self.capital += size * return_pct;
                total_trades += 1;

                let pnl = size * return_pct;
                if return_pct > 0.0 {
                    winning_trades += 1;
                } else {
                    losing_trades += 1;
                }

                trades.push(TradeRecord {
                    step: i,
                    action: "BUY_YES".to_string(),
                    entry_price: price,
                    exit_price: None,
                    size_usd: size,
                    pnl,
                    reason: format!("Edge: {:.1}%", (q_model - price) * 100.0),
                });
            } else {
                trades.push(TradeRecord {
                    step: i,
                    action: "SKIP".to_string(),
                    entry_price: price,
                    exit_price: None,
                    size_usd: 0.0,
                    pnl: 0.0,
                    reason: "No edge or position too small".to_string(),
                });
            }

            if self.capital > peak_capital {
                peak_capital = self.capital;
            }
            let drawdown = (peak_capital - self.capital) / peak_capital;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }

            equity_curve.push((i + 1, self.capital));
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
            trades,
            equity_curve,
        }
    }

    /// Run backtest with StoredMarket data (existing interface)
    pub fn run(&mut self, markets: &[crate::storage::types::StoredMarket], q_model: f64) -> BacktestResult {
        let prices: Vec<f64> = markets.iter().map(|m| m.yes_price).collect();
        self.run_with_prices(&prices, q_model)
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

```bash
cd polymarket-bot && cargo test test_backtest
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/backtesting/engine.rs
git commit -m "feat: enhance backtest engine with trade log and equity curve"
```

---

## Task 4: Add ratatui Dependency

**Files:**
- Modify: `polymarket-bot/Cargo.toml`

- [ ] **Step 1: Add dependencies**

Add to `polymarket-bot/Cargo.toml`:

```toml
[dependencies]
# ... existing ...
ratatui = "0.29"
crossterm = "0.28"
```

- [ ] **Step 2: Verify compilation**

```bash
cd polymarket-bot && cargo check
```

Expected: OK (may take a while to download ratatui)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add ratatui and crossterm for terminal UI"
```

---

## Task 5: Create Backtest TUI

**Files:**
- Create: `polymarket-bot/src/backtesting/ui.rs`
- Modify: `polymarket-bot/src/backtesting/mod.rs`

- [ ] **Step 1: Write the TUI module**

Create `polymarket-bot/src/backtesting/ui.rs`:

```rust
use crate::backtesting::engine::BacktestResult;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;

pub fn run_backtest_ui(result: &BacktestResult) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| ui(f, result))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press && key.code == KeyCode::Char('q') {
                    break;
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn ui(f: &mut Frame, result: &BacktestResult) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Length(10), // Summary
            Constraint::Min(10),   // Equity curve + trade log
        ])
        .split(f.area());

    // Header
    let header = Paragraph::new("POLYMARKET BACKTEST RESULTS  [q] quit")
        .style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(header, chunks[0]);

    // Summary stats
    render_summary(f, result, chunks[1]);

    // Bottom section: equity curve + trade log
    let bottom = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(chunks[2]);

    render_equity_curve(f, result, bottom[0]);
    render_trade_log(f, result, bottom[1]);
}

fn render_summary(f: &mut Frame, result: &BacktestResult, area: Rect) {
    let ret = (result.final_capital - result.initial_capital) / result.initial_capital * 100.0;
    let win_rate = if result.total_trades > 0 {
        result.winning_trades as f64 / result.total_trades as f64 * 100.0
    } else {
        0.0
    };

    let lines = vec![
        Line::from(vec![
            Span::styled("Capital: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("${:.2} → ${:.2}", result.initial_capital, result.final_capital),
                if ret >= 0.0 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
            Span::raw("  "),
            Span::styled("Return: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:+.2}%", ret),
                if ret >= 0.0 {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default().fg(Color::Red)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("Trades: ", Style::default().fg(Color::White)),
            Span::styled(result.total_trades.to_string(), Style::default().fg(Color::Yellow)),
            Span::raw("  "),
            Span::styled("Win: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{} ({:.0}%)", result.winning_trades, win_rate),
                Style::default().fg(Color::Green),
            ),
            Span::raw("  "),
            Span::styled("Loss: ", Style::default().fg(Color::White)),
            Span::styled(result.losing_trades.to_string(), Style::default().fg(Color::Red)),
        ]),
        Line::from(vec![
            Span::styled("Max Drawdown: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:.2}%", result.max_drawdown * 100.0),
                Style::default().fg(Color::Red),
            ),
            Span::raw("  "),
            Span::styled("Sharpe: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:.2}", result.sharpe_ratio),
                Style::default().fg(Color::Cyan),
            ),
        ]),
    ];

    let summary = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Summary"));
    f.render_widget(summary, area);
}

fn render_equity_curve(f: &mut Frame, result: &BacktestResult, area: Rect) {
    let height = area.height as usize;
    let width = area.width as usize;

    if result.equity_curve.is_empty() || height < 3 || width < 3 {
        return;
    }

    let values: Vec<f64> = result.equity_curve.iter().map(|(_, v)| *v).collect();
    let min_val = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_val = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let range = max_val - min_val;

    let chart_height = height - 2; // borders
    let chart_width = width - 2;

    let mut lines: Vec<Line> = Vec::new();

    for row in (0..chart_height).rev() {
        let mut spans = Vec::new();
        let threshold = min_val + (range * row as f64 / chart_height as f64);

        for col in 0..chart_width {
            let idx = (col * values.len()) / chart_width;
            let val = values[idx];

            if (val - threshold).abs() < range / chart_height as f64 * 0.5 {
                spans.push(Span::styled("█", Style::default().fg(Color::Green)));
            } else if val > threshold {
                spans.push(Span::styled("█", Style::default().fg(Color::DarkGray)));
            } else {
                spans.push(Span::raw(" "));
            }
        }
        lines.push(Line::from(spans));
    }

    let chart = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Equity Curve"));
    f.render_widget(chart, area);
}

fn render_trade_log(f: &mut Frame, result: &BacktestResult, area: Rect) {
    let items: Vec<ListItem> = result
        .trades
        .iter()
        .rev()
        .take(area.height as usize - 2)
        .map(|t| {
            let (symbol, color) = match t.action.as_str() {
                "BUY_YES" => ("▲", Color::Green),
                "BUY_NO" => ("▼", Color::Red),
                "SKIP" => ("─", Color::DarkGray),
                _ => ("?", Color::White),
            };
            let pnl_str = if t.pnl != 0.0 {
                format!(" {:+.2}", t.pnl)
            } else {
                String::new()
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{:>3} ", t.step), Style::default().fg(Color::DarkGray)),
                Span::styled(symbol, Style::default().fg(color)),
                Span::styled(
                    format!(" @ {:.3}", t.entry_price),
                    Style::default().fg(color),
                ),
                Span::styled(pnl_str, Style::default().fg(color)),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("Trade Log (newest first)"),
    );
    f.render_widget(list, area);
}
```

- [ ] **Step 2: Update mod.rs**

Update `polymarket-bot/src/backtesting/mod.rs`:

```rust
pub mod engine;
pub mod report;
pub mod ui;
```

- [ ] **Step 3: Test compilation**

```bash
cd polymarket-bot && cargo check
```

Expected: OK

- [ ] **Step 4: Commit**

```bash
git add src/backtesting/ui.rs src/backtesting/mod.rs
git commit -m "feat: add ratatui backtest TUI with equity curve and trade log"
```

---

## Task 6: Wire Up Backtest Command

**Files:**
- Modify: `polymarket-bot/src/main.rs`

- [ ] **Step 1: Update backtest command in main.rs**

Replace the existing `Commands::Backtest` arm in `main.rs`:

```rust
Commands::Backtest { period, strategy } => {
    let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
    let db = storage::database::Database::new(&db_path).await?;
    let markets = db.get_markets().await?;

    let period_days: u32 = period.trim_end_matches('d').parse().unwrap_or(30);
    tracing::info!(
        "Backtesting {} markets with period: {} days, strategy: {}",
        markets.len(),
        period_days,
        strategy
    );

    let mut engine = backtesting::engine::BacktestEngine::new(config.backtesting.initial_capital);
    let result = engine.run(&markets, 0.6);

    // Run TUI
    backtesting::ui::run_backtest_ui(&result)?;

    // Also print summary to terminal after TUI exits
    backtesting::report::print_report(&result);
}
```

- [ ] **Step 2: Test compilation**

```bash
cd polymarket-bot && cargo check
```

Expected: OK

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: wire up backtest TUI in backtest command"
```

---

## Task 7: End-to-End Test

**Files:**
- None (test command only)

- [ ] **Step 1: Build everything**

```bash
# Build proxy
cd /home/kucingsakti/polymarket && cargo build --release -p polymarket-gamma

# Build bot
cd /home/kucingsakti/polymarket/polymarket-bot && cargo build --release
```

- [ ] **Step 2: Start proxy and collect data**

```bash
# Start proxy
cd /home/kucingsakti/polymarket && ./target/release/polymarket-gamma &

# Collect some markets
cd /home/kucingsakti/polymarket/polymarket-bot && ./target/release/polymarket-bot collect
```

- [ ] **Step 3: Run backtest**

```bash
./target/release/polymarket-bot backtest --period 30d
```

Expected: Terminal UI opens with equity curve, trade log, and summary. Press `q` to quit.

- [ ] **Step 4: Commit (if any fixes needed)**

---

## Summary

| Task | Description | Est. Time |
|------|-------------|-----------|
| 1 | Add CLOB price history to proxy | 15 min |
| 2 | Add price history client to bot | 5 min |
| 3 | Enhance backtest engine | 20 min |
| 4 | Add ratatui dependency | 5 min |
| 5 | Create backtest TUI | 30 min |
| 6 | Wire up command | 5 min |
| 7 | End-to-end test | 10 min |
| **Total** | | **~90 min** |
