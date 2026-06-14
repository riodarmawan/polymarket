# Live Trading Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a real-time TUI dashboard for paper trading BTC Up/Down markets on Polymarket using M15 signals.

**Architecture:** New `live` module under `src/crypto/` with three components: PaperTradingEngine (virtual positions, PnL), TuiRenderer (ratatui UI), and LiveDashboard (orchestrator). Reuses existing BinanceWsClient and SignalEngine.

**Tech Stack:** Rust, ratatui, crossterm, tokio, tungstenite (all already available or easy to add)

---

## File Structure

```
src/crypto/
├── live/
│   ├── mod.rs              # LiveDashboard orchestrator
│   ├── paper_trading.rs    # PaperTradingEngine
│   └── tui.rs              # TuiRenderer (ratatui)
├── mod.rs                  # Add `pub mod live;`
src/cli.rs                  # Add `Live` command
src/main.rs                 # Add `Commands::Live` handler
Cargo.toml                  # Add ratatui, crossterm deps
```

---

## Task 1: Add Dependencies

**Files:**
- Modify: `/home/kucingsakti/polymarket/polymarket-bot/Cargo.toml`

- [ ] **Step 1: Add ratatui and crossterm to Cargo.toml**

Add after the existing `[dependencies]` section:

```toml
ratatui = "0.29"
crossterm = "0.28"
```

- [ ] **Step 2: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors (warnings OK)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add ratatui and crossterm for TUI dashboard"
```

---

## Task 2: Create PaperTradingEngine

**Files:**
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/crypto/live/paper_trading.rs`

- [ ] **Step 1: Create paper_trading.rs with struct definitions**

```rust
use crate::crypto::signals::Direction;
use crate::crypto::indicators::Timeframe;

#[derive(Debug, Clone)]
pub struct Trade {
    pub timestamp: i64,
    pub direction: Direction,
    pub timeframe: Timeframe,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub size_usd: f64,
    pub pnl: Option<f64>,
    pub status: TradeStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TradeStatus {
    Open,
    Closed,
    Timeout,
}

#[derive(Debug, Clone)]
pub struct PaperTradingConfig {
    pub initial_capital: f64,
    pub max_order_usd: f64,
    pub fee_pct: f64,
    pub stop_loss_pct: f64,
    pub take_profit_pct: f64,
    pub timeout_minutes: u32,
}

impl Default for PaperTradingConfig {
    fn default() -> Self {
        Self {
            initial_capital: 2.0,
            max_order_usd: 0.50,
            fee_pct: 0.02,
            stop_loss_pct: 0.005,
            take_profit_pct: 0.01,
            timeout_minutes: 15,
        }
    }
}

pub struct PaperTradingEngine {
    config: PaperTradingConfig,
    pub capital: f64,
    pub peak_capital: f64,
    pub trades: Vec<Trade>,
    pub open_trade: Option<Trade>,
}
```

- [ ] **Step 2: Add PaperTradingEngine::new() and execute_signal()**

```rust
impl PaperTradingEngine {
    pub fn new(config: PaperTradingConfig) -> Self {
        Self {
            capital: config.initial_capital,
            peak_capital: config.initial_capital,
            config,
            trades: Vec::new(),
            open_trade: None,
        }
    }

    pub fn execute_signal(
        &mut self,
        direction: Direction,
        timeframe: Timeframe,
        price: f64,
        timestamp: i64,
    ) -> Option<Trade> {
        // Don't execute if we already have an open trade
        if self.open_trade.is_some() {
            return None;
        }

        // Calculate position size with drawdown scaling
        let size = self.calculate_position_size();

        // Check minimum order
        if size < 0.10 || self.capital < size {
            return None;
        }

        // Create trade
        let trade = Trade {
            timestamp,
            direction,
            timeframe,
            entry_price: price,
            exit_price: None,
            size_usd: size,
            pnl: None,
            status: TradeStatus::Open,
        };

        self.capital -= size;
        self.open_trade = Some(trade.clone());
        self.trades.push(trade.clone());

        Some(trade)
    }

    fn calculate_position_size(&self) -> f64 {
        let base_fraction = 0.10;
        let current_drawdown = if self.peak_capital > 0.0 {
            (self.peak_capital - self.capital) / self.peak_capital
        } else {
            0.0
        };

        let drawdown_scale = if current_drawdown > 0.25 {
            0.5
        } else if current_drawdown > 0.15 {
            0.75
        } else {
            1.0
        };

        (self.capital * base_fraction * drawdown_scale)
            .max(0.10)
            .min(self.config.max_order_usd)
    }

    pub fn check_exit(&mut self, current_price: i64, timestamp: i64) -> Option<Trade> {
        let trade = self.open_trade.as_ref()?;

        let price_change = match trade.direction {
            Direction::Up => (current_price as f64 - trade.entry_price) / trade.entry_price,
            Direction::Down => (trade.entry_price - current_price as f64) / trade.entry_price,
        };

        let minutes_elapsed = (timestamp - trade.timestamp) / 60000;

        let should_exit = price_change >= self.config.take_profit_pct
            || price_change <= -self.config.stop_loss_pct
            || minutes_elapsed >= self.config.timeout_minutes as i64;

        if should_exit {
            let mut closed_trade = self.open_trade.take().unwrap();
            closed_trade.exit_price = Some(current_price as f64);
            closed_trade.status = if minutes_elapsed >= self.config.timeout_minutes as i64 {
                TradeStatus::Timeout
            } else {
                TradeStatus::Closed
            };

            // Calculate PnL
            let pnl = match closed_trade.direction {
                Direction::Up => {
                    if price_change > 0.0 {
                        closed_trade.size_usd * (price_change / self.config.take_profit_pct)
                    } else {
                        -closed_trade.size_usd
                    }
                }
                Direction::Down => {
                    if price_change < 0.0 {
                        closed_trade.size_usd * (price_change.abs() / self.config.take_profit_pct)
                    } else {
                        -closed_trade.size_usd
                    }
                }
            };

            let fee = closed_trade.size_usd * self.config.fee_pct;
            let net_pnl = pnl - fee;

            closed_trade.pnl = Some(net_pnl);
            self.capital += closed_trade.size_usd + net_pnl;

            if self.capital > self.peak_capital {
                self.peak_capital = self.capital;
            }

            // Update the trade in history
            if let Some(last_trade) = self.trades.last_mut() {
                last_trade.exit_price = closed_trade.exit_price;
                last_trade.pnl = closed_trade.pnl;
                last_trade.status = closed_trade.status.clone();
            }

            Some(closed_trade)
        } else {
            None
        }
    }

    pub fn stats(&self) -> Stats {
        let completed: Vec<&Trade> = self.trades.iter()
            .filter(|t| t.status != TradeStatus::Open)
            .collect();

        let total = completed.len();
        let wins = completed.iter().filter(|t| t.pnl.map_or(false, |p| p > 0.0)).count();
        let losses = total - wins;

        let total_pnl: f64 = completed.iter()
            .filter_map(|t| t.pnl)
            .sum();

        let avg_win = if wins > 0 {
            completed.iter()
                .filter_map(|t| t.pnl)
                .filter(|p| *p > 0.0)
                .sum::<f64>() / wins as f64
        } else {
            0.0
        };

        let avg_loss = if losses > 0 {
            completed.iter()
                .filter_map(|t| t.pnl)
                .filter(|p| *p < 0.0)
                .sum::<f64>() / losses as f64
        } else {
            0.0
        };

        let profit_factor = if avg_loss != 0.0 {
            (avg_win * wins as f64) / (avg_loss.abs() * losses as f64)
        } else {
            0.0
        };

        let max_drawdown = (self.peak_capital - self.capital) / self.peak_capital;

        Stats {
            total_trades: total,
            wins,
            losses,
            win_rate: if total > 0 { wins as f64 / total as f64 } else { 0.0 },
            total_pnl,
            avg_win,
            avg_loss,
            profit_factor,
            max_drawdown,
            current_capital: self.capital,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Stats {
    pub total_trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
    pub current_capital: f64,
}
```

- [ ] **Step 3: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 4: Commit**

```bash
git add src/crypto/live/paper_trading.rs
git commit -m "feat: add PaperTradingEngine for virtual position management"
```

---

## Task 3: Create TuiRenderer

**Files:**
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/crypto/live/tui.rs`

- [ ] **Step 1: Create tui.rs with basic TUI layout**

```rust
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use crate::crypto::live::paper_trading::{Stats, Trade, TradeStatus};
use crate::crypto::signals::Direction;

pub struct TuiRenderer {
    last_signal: Option<String>,
    last_signal_time: Option<i64>,
    regime: String,
    current_price: f64,
    price_change_pct: f64,
    uptime_seconds: u64,
}

impl TuiRenderer {
    pub fn new() -> Self {
        Self {
            last_signal: None,
            last_signal_time: None,
            regime: "UNKNOWN".to_string(),
            current_price: 0.0,
            price_change_pct: 0.0,
            uptime_seconds: 0,
        }
    }

    pub fn update_price(&mut self, price: f64, prev_price: f64) {
        self.current_price = price;
        self.price_change_pct = if prev_price > 0.0 {
            ((price - prev_price) / prev_price) * 100.0
        } else {
            0.0
        };
    }

    pub fn update_signal(&mut self, signal: Option<String>, timestamp: i64) {
        self.last_signal = signal;
        self.last_signal_time = Some(timestamp);
    }

    pub fn update_regime(&mut self, regime: String) {
        self.regime = regime;
    }

    pub fn update_uptime(&mut self, seconds: u64) {
        self.uptime_seconds = seconds;
    }

    pub fn render(&self, frame: &mut Frame, stats: &Stats, open_trade: &Option<Trade>, trades: &[Trade]) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),  // Header
                Constraint::Length(3),  // Price
                Constraint::Length(6),  // Signal
                Constraint::Length(5),  // Open Position
                Constraint::Length(6),  // Stats
                Constraint::Min(5),    // Trade History
                Constraint::Length(2),  // Footer
            ])
            .split(frame.area());

        self.render_header(frame, chunks[0], stats);
        self.render_price(frame, chunks[1]);
        self.render_signal(frame, chunks[2]);
        self.render_open_position(frame, chunks[3], open_trade);
        self.render_stats(frame, chunks[4], stats);
        self.render_trade_history(frame, chunks[5], trades);
        self.render_footer(frame, chunks[6]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect, stats: &Stats) {
        let header = Paragraph::new(Line::from(vec![
            Span::styled("  LIVE TRADING DASHBOARD  ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
            Span::raw("                    "),
            Span::styled(
                format!("Capital: ${:.2}", stats.current_capital),
                Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
            ),
        ]))
        .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::Cyan)));
        frame.render_widget(header, area);
    }

    fn render_price(&self, frame: &mut Frame, area: Rect) {
        let arrow = if self.price_change_pct >= 0.0 { "▲" } else { "▼" };
        let color = if self.price_change_pct >= 0.0 { Color::Green } else { Color::Red };

        let price_line = Line::from(vec![
            Span::raw("  BTC/USDT: "),
            Span::styled(format!("${:.2}", self.current_price), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
            Span::raw("  "),
            Span::styled(format!("{} {:+.2}%", arrow, self.price_change_pct), Style::default().fg(color)),
            Span::raw("    Regime: "),
            Span::styled(&self.regime, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
        ]);

        let price_widget = Paragraph::new(price_line)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(price_widget, area);
    }

    fn render_signal(&self, frame: &mut Frame, area: Rect) {
        let signal_text = match &self.last_signal {
            Some(sig) => vec![
                Line::from(vec![
                    Span::styled("  CURRENT SIGNAL:  ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                ]),
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(sig.as_str(), Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)),
                ]),
            ],
            None => vec![
                Line::from(vec![
                    Span::styled("  Waiting for signal...", Style::default().fg(Color::DarkGray)),
                ]),
            ],
        };

        let signal_widget = Paragraph::new(signal_text)
            .block(Block::default().title("Signal").borders(Borders::ALL).border_style(Style::default().fg(Color::Yellow)));
        frame.render_widget(signal_widget, area);
    }

    fn render_open_position(&self, frame: &mut Frame, area: Rect, trade: &Option<Trade>) {
        let pos_text = match trade {
            Some(t) => {
                let dir_color = match t.direction {
                    Direction::Up => Color::Green,
                    Direction::Down => Color::Red,
                };
                vec![
                    Line::from(vec![
                        Span::raw("  "),
                        Span::styled(
                            format!("{:?} @ ${:.2}", t.direction, t.entry_price),
                            Style::default().fg(dir_color).add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!("  Size: ${:.2}", t.size_usd)),
                    ]),
                ]
            }
            None => vec![
                Line::from(vec![
                    Span::styled("  No open position", Style::default().fg(Color::DarkGray)),
                ]),
            ],
        };

        let pos_widget = Paragraph::new(pos_text)
            .block(Block::default().title("Open Position").borders(Borders::ALL).border_style(Style::default().fg(Color::Blue)));
        frame.render_widget(pos_widget, area);
    }

    fn render_stats(&self, frame: &mut Frame, area: Rect, stats: &Stats) {
        let pnl_color = if stats.total_pnl >= 0.0 { Color::Green } else { Color::Red };
        let dd_color = if stats.max_drawdown > 0.25 { Color::Red } else { Color::Yellow };

        let stats_text = vec![
            Line::from(vec![
                Span::raw("  Trades: "),
                Span::styled(stats.total_trades.to_string(), Style::default().fg(Color::White).add_modifier(Modifier::BOLD)),
                Span::raw(format!("  Win: {} ({:.1}%)  PnL: ", stats.wins, stats.win_rate * 100.0)),
                Span::styled(format!("${:+.2} ({:+.1}%)", stats.total_pnl, (stats.total_pnl / stats.current_capital) * 100.0), Style::default().fg(pnl_color).add_modifier(Modifier::BOLD)),
            ]),
            Line::from(vec![
                Span::raw(format!("  Drawdown: ")),
                Span::styled(format!("{:.1}%", stats.max_drawdown * 100.0), Style::default().fg(dd_color)),
                Span::raw(format!("  PF: {:.2}  Avg Win: ${:.2}  Avg Loss: ${:.2}", stats.profit_factor, stats.avg_win, stats.avg_loss)),
            ]),
        ];

        let stats_widget = Paragraph::new(stats_text)
            .block(Block::default().title("Stats").borders(Borders::ALL).border_style(Style::default().fg(Color::Magenta)));
        frame.render_widget(stats_widget, area);
    }

    fn render_trade_history(&self, frame: &mut Frame, area: Rect, trades: &[Trade]) {
        let recent: Vec<&Trade> = trades.iter()
            .rev()
            .take(10)
            .collect();

        let mut lines: Vec<Line> = vec![
            Line::from(vec![Span::styled("  TRADE HISTORY (last 10):", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))]),
        ];

        for trade in recent {
            let (status_icon, pnl_text) = match trade.status {
                TradeStatus::Open => ("●", "...".to_string()),
                TradeStatus::Closed | TradeStatus::Timeout => {
                    match trade.pnl {
                        Some(pnl) => {
                            let icon = if pnl >= 0.0 { "✓" } else { "✗" };
                            (icon, format!("${:+.2}", pnl))
                        }
                        None => ("?", "?".to_string()),
                    }
                }
            };

            let dir_color = match trade.direction {
                Direction::Up => Color::Green,
                Direction::Down => Color::Red,
            };

            let exit_str = match trade.exit_price {
                Some(p) => format!("${:.2}", p),
                None => "...".to_string(),
            };

            lines.push(Line::from(vec![
                Span::raw(format!("  {} ", status_icon)),
                Span::styled(
                    format!("{:?} @ ${:.2} → {}", trade.direction, trade.entry_price, exit_str),
                    Style::default().fg(dir_color),
                ),
                Span::raw(format!("  {}", pnl_text)),
            ]));
        }

        let history_widget = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)));
        frame.render_widget(history_widget, area);
    }

    fn render_footer(&self, frame: &mut Frame, area: Rect) {
        let elapsed = self.uptime_seconds;
        let hours = elapsed / 3600;
        let minutes = (elapsed % 3600) / 60;
        let seconds = elapsed % 60;

        let footer = Paragraph::new(Line::from(vec![
            Span::styled(
                format!("  Uptime: {}h {}m {}s    Press Ctrl+C to exit", hours, minutes, seconds),
                Style::default().fg(Color::DarkGray),
            ),
        ]));
        frame.render_widget(footer, area);
    }
}
```

- [ ] **Step 2: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 3: Commit**

```bash
git add src/crypto/live/tui.rs
git commit -m "feat: add TuiRenderer with full dashboard layout"
```

---

## Task 4: Create LiveDashboard Orchestrator

**Files:**
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/crypto/live/mod.rs`

- [ ] **Step 1: Create mod.rs with LiveDashboard**

```rust
pub mod paper_trading;
pub mod tui;

use paper_trading::{PaperTradingConfig, PaperTradingEngine};
use tui::TuiRenderer;
use crate::crypto::binance_ws::{BinanceWsClient, Candle};
use crate::crypto::indicators::Timeframe;
use crate::crypto::signals::SignalEngine;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use anyhow::Result;

pub struct LiveDashboard {
    config: PaperTradingConfig,
}

impl LiveDashboard {
    pub fn new(capital: f64, max_order: f64) -> Self {
        let config = PaperTradingConfig {
            initial_capital: capital,
            max_order_usd: max_order,
            ..Default::default()
        };
        Self { config }
    }

    pub async fn run(&self) -> Result<()> {
        let ws_client = BinanceWsClient::new(1000);
        let signal_engine = SignalEngine::new();
        let mut paper_engine = PaperTradingEngine::new(self.config.clone());
        let mut tui = TuiRenderer::new();

        // Start WebSocket
        ws_client.start()?;
        let mut rx = ws_client.subscribe();

        // Candle buffer for M15 aggregation
        let mut candle_buffer: Vec<Candle> = Vec::new();
        let mut last_m15_close = 0i64;
        let start_time = Instant::now();

        // Setup terminal
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        crossterm::execute!(stdout, crossterm::terminal::EnterAlternateScreen)?;

        let backend = ratatui::backend::CrosstermBackend::new(stdout);
        let mut terminal = ratatui::Terminal::new(backend)?;

        let result = self.run_loop(
            &mut terminal,
            &mut rx,
            &signal_engine,
            &mut paper_engine,
            &mut tui,
            &mut candle_buffer,
            &mut last_m15_close,
            start_time,
        ).await;

        // Restore terminal
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(
            terminal.backend_mut(),
            crossterm::terminal::LeaveAlternateScreen
        )?;

        result
    }

    async fn run_loop(
        &self,
        terminal: &mut ratatui::Terminal<ratatui::backend::CrosstermBackend<std::io::Stdout>>,
        rx: &mut tokio::sync::broadcast::Receiver<Candle>,
        signal_engine: &SignalEngine,
        paper_engine: &mut PaperTradingEngine,
        tui: &mut TuiRenderer,
        candle_buffer: &mut Vec<Candle>,
        last_m15_close: &mut i64,
        start_time: Instant,
    ) -> Result<()> {
        let mut prev_price = 0.0;

        loop {
            // Check for key events (Ctrl+C to exit)
            if crossterm::event::poll(Duration::from_millis(100))? {
                if let crossterm::event::Event::Key(key) = crossterm::event::read()? {
                    if key.code == crossterm::event::KeyCode::Char('c')
                        && key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                    {
                        break;
                    }
                }
            }

            // Try to receive candle data (non-blocking)
            match rx.try_recv() {
                Ok(candle) => {
                    tui.update_price(candle.close, prev_price);
                    prev_price = candle.close;

                    // Add to buffer
                    candle_buffer.push(candle.clone());

                    // Keep last 100 candles
                    if candle_buffer.len() > 100 {
                        candle_buffer.remove(0);
                    }

                    // Check if M15 candle closed (every 15 minutes)
                    let m15_ts = candle.timestamp / (15 * 60 * 1000);
                    if m15_ts > *last_m15_close && candle_buffer.len() >= 15 {
                        *last_m15_close = m15_ts;

                        // Build candle map for signal engine
                        let mut candle_map: HashMap<Timeframe, Vec<Candle>> = HashMap::new();
                        candle_map.insert(Timeframe::M15, candle_buffer.clone());

                        // Generate signal
                        let signals = signal_engine.generate_signals(&candle_map);

                        if let Some(signal) = signals.first() {
                            tui.update_signal(
                                Some(format!("{} {} (conf: {:.2})", signal.timeframe.as_str(), signal.direction, signal.confidence)),
                                candle.timestamp,
                            );

                            // Execute trade
                            paper_engine.execute_signal(
                                signal.direction.clone(),
                                signal.timeframe,
                                candle.close,
                                candle.timestamp,
                            );
                        } else {
                            tui.update_signal(None, candle.timestamp);
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::TryRecvError::Empty) => {
                    // No data yet, continue
                }
                Err(_) => {
                    // Channel closed
                    break;
                }
            }

            // Check for trade exits
            let now = chrono::Utc::now().timestamp_millis();
            if let Some(_closed) = paper_engine.check_exit(prev_price as i64, now) {
                // Trade closed, stats will be updated on next render
            }

            // Update uptime
            tui.update_uptime(start_time.elapsed().as_secs());

            // Render
            let stats = paper_engine.stats();
            let open_trade = paper_engine.open_trade.clone();
            let trades = paper_engine.trades.clone();

            terminal.draw(|frame| {
                tui.render(frame, &stats, &open_trade, &trades);
            })?;

            // Small sleep to prevent CPU spinning
            tokio::time::sleep(Duration::from_millis(100)).await;
        }

        Ok(())
    }
}
```

- [ ] **Step 2: Update src/crypto/mod.rs to include live module**

Add to `/home/kucingsakti/polymarket/polymarket-bot/src/crypto/mod.rs`:

```rust
pub mod live;
```

- [ ] **Step 3: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 4: Commit**

```bash
git add src/crypto/live/mod.rs src/crypto/mod.rs
git commit -m "feat: add LiveDashboard orchestrator with WebSocket and signal integration"
```

---

## Task 5: Add CLI Command

**Files:**
- Modify: `/home/kucingsakti/polymarket/polymarket-bot/src/cli.rs`
- Modify: `/home/kucingsakti/polymarket/polymarket-bot/src/main.rs`

- [ ] **Step 1: Add Live command to CLI**

Add to `Commands` enum in `cli.rs`:

```rust
    /// Live trading dashboard (paper trading)
    Live {
        /// Initial virtual capital in USD
        #[arg(long, default_value = "2.0")]
        capital: f64,

        /// Maximum order size in USD
        #[arg(long, default_value = "0.50")]
        max_order: f64,
    },
```

- [ ] **Step 2: Add Live handler to main.rs**

Add to `main.rs` match block:

```rust
        Commands::Live { capital, max_order } => {
            tracing::info!("Starting live trading dashboard...");
            tracing::info!("Capital: ${:.2}, Max Order: ${:.2}", capital, max_order);

            let dashboard = polymarket_bot::crypto::live::LiveDashboard::new(capital, max_order);

            if let Err(e) = dashboard.run().await {
                tracing::error!("Dashboard error: {}", e);
            }
        }
```

- [ ] **Step 3: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add 'live' CLI command for trading dashboard"
```

---

## Task 6: Build and Test

**Files:**
- Verify: `/home/kucingsakti/polymarket/polymarket-bot/target/release/polymarket-bot`

- [ ] **Step 1: Build release binary**

Run: `cargo build --release 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 2: Test run dashboard (10 second test)**

Run: `timeout 10 cargo run --release -- live --capital 2.0` or kill after 10 seconds
Expected: Dashboard renders, shows BTC price, updates in real-time

- [ ] **Step 3: Final commit**

```bash
git add -A
git commit -m "feat: complete live trading dashboard implementation"
```

---

## Summary

| Task | Description | Files Changed |
|------|-------------|---------------|
| 1 | Add dependencies | Cargo.toml |
| 2 | PaperTradingEngine | src/crypto/live/paper_trading.rs |
| 3 | TuiRenderer | src/crypto/live/tui.rs |
| 4 | LiveDashboard | src/crypto/live/mod.rs |
| 5 | CLI command | src/cli.rs, src/main.rs |
| 6 | Build and test | target/release/polymarket-bot |

**Total estimated time:** 45-60 minutes
