# Backtest Engine Fix — Remove Look-Ahead Bias

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the backtest engine to produce realistic results by removing look-ahead bias, adding slippage/fees, and implementing proper mark-to-market equity tracking.

**Architecture:** 
- New `BacktestEngine` that iterates over **time steps** (not markets)
- Each time step: check all markets, evaluate signals, execute trades
- Mark-to-market equity updated every step
- Slippage model based on order size vs orderbook depth
- Fee deduction on every trade

**Tech Stack:** Rust

---

## Root Cause Analysis

| Issue | Current Behavior | Correct Behavior |
|-------|-----------------|------------------|
| Win rate 100% | Uses `yes_price` from resolved markets | Only use data available at entry time |
| Entry at 0.001 | Buys winners at historical low prices | Use ask price at entry time |
| Drawdown 0% | Equity only calculated at trade close | Mark-to-market every step |
| Sharpe 0.00 | Equity curve not time-series | Proper time-series equity |
| No slippage | Assumes perfect fill at last price | Price impact based on size vs depth |
| No fees | No trading costs | Deduct ~2% taker fee |

---

## File Structure

```
polymarket-bot/src/
├── backtesting/
│   ├── mod.rs           # Update exports
│   ├── engine.rs        # REWRITE: time-based backtest
│   ├── types.rs         # NEW: BacktestConfig, Position, EquityPoint
│   ├── slippage.rs      # NEW: slippage model
│   ├── report.rs        # Keep (update if needed)
│   └── ui.rs            # Keep (update if needed)
```

---

## Task 1: Create Backtest Types

**Files:**
- Create: `polymarket-bot/src/backtesting/types.rs`

**Context:** Define all types needed for the realistic backtest engine.

- [ ] **Step 1: Create types.rs**

```rust
use serde::{Deserialize, Serialize};

/// Configuration for backtest
#[derive(Debug, Clone)]
pub struct BacktestConfig {
    /// Initial capital in USD
    pub initial_capital: f64,
    /// Taker fee percentage (e.g., 0.02 = 2%)
    pub taker_fee_pct: f64,
    /// Minimum order size in USD
    pub min_order_usd: f64,
    /// Maximum slippage tolerance (e.g., 0.05 = 5%)
    pub max_slippage_pct: f64,
    /// Model confidence threshold (q_model must be > market_price + this)
    pub min_edge_pct: f64,
    /// Fraction of Kelly to use (0.0 to 1.0)
    pub kelly_fraction: f64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_capital: 2.0,
            taker_fee_pct: 0.02,
            min_order_usd: 5.0,
            max_slippage_pct: 0.05,
            min_edge_pct: 0.02,
            kelly_fraction: 0.25,
        }
    }
}

/// A single price observation at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceObservation {
    /// Timestamp (Unix epoch seconds)
    pub timestamp: u64,
    /// Market/token ID
    pub market_id: String,
    /// Best ask price (for buying)
    pub ask_price: f64,
    /// Best bid price (for selling)
    pub bid_price: f64,
    /// Ask depth (total size available at ask)
    pub ask_depth: f64,
    /// Bid depth (total size available at bid)
    pub bid_depth: f64,
    /// Spread (ask - bid)
    pub spread: f64,
    /// Mid price (ask + bid) / 2
    pub mid_price: f64,
}

/// An open position
#[derive(Debug, Clone)]
pub struct Position {
    /// Unique position ID
    pub id: String,
    /// Market/token ID
    pub market_id: String,
    /// Side: "YES" or "NO"
    pub side: String,
    /// Entry price (what we paid)
    pub entry_price: f64,
    /// Current market price (mark-to-market)
    pub current_price: f64,
    /// Size in USD
    pub size_usd: f64,
    /// Number of shares/tokens bought
    pub shares: f64,
    /// Entry timestamp
    pub entry_timestamp: u64,
    /// Unrealized P&L
    pub unrealized_pnl: f64,
}

impl Position {
    /// Update mark-to-market price
    pub fn update_price(&mut self, current_price: f64) {
        self.current_price = current_price;
        self.unrealized_pnl = (current_price - self.entry_price) * self.shares;
    }

    /// Calculate realized P&L if we sell at given price
    pub fn realize_pnl(&self, sell_price: f64, fee_pct: f64) -> f64 {
        let gross = (sell_price - self.entry_price) * self.shares;
        let fee = self.size_usd * fee_pct;
        gross - fee
    }
}

/// A trade record
#[derive(Debug, Clone, Serialize)]
pub struct TradeRecord {
    /// Step number
    pub step: usize,
    /// Timestamp
    pub timestamp: u64,
    /// Action: "BUY", "SELL", "HOLD"
    pub action: String,
    /// Market ID
    pub market_id: String,
    /// Side: "YES" or "NO"
    pub side: String,
    /// Entry/exit price
    pub price: f64,
    /// Size in USD
    pub size_usd: f64,
    /// Fee paid
    pub fee_usd: f64,
    /// Shares bought/sold
    pub shares: f64,
    /// Reason for trade
    pub reason: String,
    /// P&L (for sell trades)
    pub pnl: f64,
}

/// Equity point for the equity curve
#[derive(Debug, Clone, Serialize)]
pub struct EquityPoint {
    /// Step number
    pub step: usize,
    /// Timestamp
    pub timestamp: u64,
    /// Cash balance
    pub cash: f64,
    /// Unrealized P&L from open positions
    pub unrealized_pnl: f64,
    /// Total equity (cash + unrealized)
    pub total_equity: f64,
    /// Number of open positions
    pub open_positions: usize,
}

/// Result of a backtest run
#[derive(Debug, Clone, Serialize)]
pub struct BacktestResult {
    pub initial_capital: f64,
    pub final_capital: f64,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub total_fees: f64,
    pub max_drawdown: f64,
    pub max_drawdown_pct: f64,
    pub sharpe_ratio: f64,
    pub trades: Vec<TradeRecord>,
    pub equity_curve: Vec<EquityPoint>,
}
```

- [ ] **Step 2: Update mod.rs**

Add to `polymarket-bot/src/backtesting/mod.rs`:

```rust
pub mod engine;
pub mod report;
pub mod slippage;
pub mod types;
pub mod ui;
```

- [ ] **Step 3: Test compilation**

```bash
cd polymarket-bot && cargo check
```

- [ ] **Step 4: Commit**

```bash
git add src/backtesting/types.rs src/backtesting/mod.rs
git commit -m "feat: add backtest types for realistic simulation"
```

---

## Task 2: Create Slippage Model

**Files:**
- Create: `polymarket-bot/src/backtesting/slippage.rs`

**Context:** Model realistic price impact based on order size vs available liquidity.

- [ ] **Step 1: Create slippage.rs**

```rust
/// Calculate the average execution price considering slippage
/// 
/// # Arguments
/// * `base_price` - The best ask/bid price
/// * `depth` - Total liquidity available at/near base price
/// * `order_size_usd` - How much we want to buy/sell
/// * `tick_size` - Minimum price increment (e.g., 0.01)
/// 
/// # Returns
/// Average execution price (worse than base_price due to slippage)
pub fn calculate_slippage(
    base_price: f64,
    depth: f64,
    order_size_usd: f64,
    tick_size: f64,
) -> f64 {
    if depth <= 0.0 || order_size_usd <= 0.0 {
        return base_price;
    }

    // Number of shares we want to buy
    let shares = order_size_usd / base_price;
    
    // If our order is smaller than available depth, minimal slippage
    if shares <= depth {
        // Simple model: slippage proportional to order/depth ratio
        let slippage_ratio = shares / depth;
        let slippage = slippage_ratio * base_price * 0.1; // 10% of price per 100% depth
        let execution_price = base_price + slippage;
        round_to_tick(execution_price, tick_size)
    } else {
        // Order exceeds depth - significant slippage
        // Assume linear price impact beyond available depth
        let excess_shares = shares - depth;
        let slippage_from_depth = base_price * 0.1; // 10% for filling all depth
        let slippage_from_excess = (excess_shares / depth) * base_price * 0.2;
        let execution_price = base_price + slippage_from_depth + slippage_from_excess;
        round_to_tick(execution_price.min(base_price * 1.5), tick_size) // Cap at 150% of base
    }
}

/// Round price to nearest tick size
pub fn round_to_tick(price: f64, tick_size: f64) -> f64 {
    if tick_size <= 0.0 {
        return price;
    }
    (price / tick_size).round() * tick_size
}

/// Calculate effective spread considering order size
pub fn effective_spread(
    bid: f64,
    ask: f64,
    order_size_usd: f64,
    bid_depth: f64,
    ask_depth: f64,
) -> f64 {
    let mid = (bid + ask) / 2.0;
    let exec_price = calculate_slippage(ask, ask_depth, order_size_usd, 0.01);
    (exec_price - mid) / mid
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_slippage_small_order() {
        let price = calculate_slippage(0.5, 1000.0, 10.0, 0.01);
        assert!(price >= 0.5 && price < 0.55);
    }

    #[test]
    fn test_slippage_large_order() {
        let price = calculate_slippage(0.5, 100.0, 200.0, 0.01);
        assert!(price > 0.5); // Should be worse than base price
    }

    #[test]
    fn test_tick_rounding() {
        let price = round_to_tick(0.5123, 0.01);
        assert!((price - 0.51).abs() < 0.001);
    }
}
```

- [ ] **Step 2: Test compilation**

```bash
cd polymarket-bot && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/backtesting/slippage.rs src/backtesting/mod.rs
git commit -m "feat: add slippage model for realistic execution"
```

---

## Task 3: Rewrite Backtest Engine

**Files:**
- Modify: `polymarket-bot/src/backtesting/engine.rs`

**Context:** Complete rewrite to iterate over time steps, not markets. Each step: evaluate all markets, decide trades, update equity.

- [ ] **Step 1: Rewrite engine.rs**

```rust
use crate::backtesting::slippage;
use crate::backtesting::types::*;

/// Evaluate whether to buy a market based on simple momentum/mean-reversion
/// Returns (should_buy, side, confidence)
fn evaluate_market(
    history: &[PriceObservation],
    current_idx: usize,
    config: &BacktestConfig,
) -> (bool, String, f64) {
    if current_idx < 10 {
        return (false, "YES".to_string(), 0.0);
    }

    // Get recent prices for this market
    let market_id = &history[current_idx].market_id;
    let recent: Vec<&PriceObservation> = history[..=current_idx]
        .iter()
        .filter(|o| o.market_id == market_id)
        .rev()
        .take(10)
        .collect();

    if recent.len() < 5 {
        return (false, "YES".to_string(), 0.0);
    }

    let current = recent[0];
    let prices: Vec<f64> = recent.iter().map(|o| o.mid_price).collect();
    
    // Simple strategy: mean reversion
    // If current price is below mean, expect it to rise
    let mean = prices.iter().sum::<f64>() / prices.len() as f64;
    let std = (prices.iter().map(|p| (p - mean).powi(2).sum::<f64>() / prices.len() as f64)).sqrt();
    
    if std < 0.01 {
        return (false, "YES".to_string(), 0.0); // Too stable
    }

    let z_score = (current.mid_price - mean) / std;
    
    // Buy YES if price is below mean (expect rise)
    // Buy NO if price is above mean (expect fall)
    if z_score < -1.0 {
        let confidence = (z_score.abs() / 3.0).min(1.0);
        if current.mid_price < 0.95 { // Don't buy if already very high
            return (true, "YES".to_string(), confidence);
        }
    } else if z_score > 1.0 {
        let confidence = (z_score.abs() / 3.0).min(1.0);
        let no_price = 1.0 - current.mid_price;
        if no_price < 0.95 {
            return (true, "NO".to_string(), confidence);
        }
    }

    (false, "YES".to_string(), 0.0)
}

pub fn run_backtest(
    observations: &[PriceObservation],
    config: &BacktestConfig,
) -> BacktestResult {
    let mut cash = config.initial_capital;
    let mut positions: Vec<Position> = Vec::new();
    let mut trades: Vec<TradeRecord> = Vec::new();
    let mut equity_curve: Vec<EquityPoint> = Vec::new();
    let mut total_fees = 0.0;
    let mut peak_equity = config.initial_capital;
    let mut max_drawdown = 0.0;
    let mut max_drawdown_pct = 0.0;

    // Group observations by timestamp
    let mut timestamps: Vec<u64> = observations.iter().map(|o| o.timestamp).collect();
    timestamps.sort();
    timestamps.dedup();

    for (step, &ts) in timestamps.iter().enumerate() {
        // Get all observations at this timestamp
        let current_obs: Vec<&PriceObservation> = observations
            .iter()
            .filter(|o| o.timestamp == ts)
            .collect();

        // 1. Update mark-to-market for all positions
        for pos in &mut positions {
            if let Some(obs) = current_obs.iter().find(|o| o.market_id == pos.market_id) {
                let new_price = match pos.side.as_str() {
                    "YES" => obs.mid_price,
                    "NO" => 1.0 - obs.mid_price,
                    _ => obs.mid_price,
                };
                pos.update_price(new_price);
            }
        }

        // 2. Check for exit conditions (stop loss, take profit)
        let mut positions_to_remove = Vec::new();
        for (i, pos) in positions.iter().enumerate() {
            let pnl_pct = (pos.current_price - pos.entry_price) / pos.entry_price;
            
            // Stop loss at -20%
            if pnl_pct < -0.20 {
                let sell_price = pos.current_price;
                let fee = pos.size_usd * config.taker_fee_pct;
                let pnl = pos.realize_pnl(sell_price, config.taker_fee_pct);
                cash += pos.size_usd + pnl;
                total_fees += fee;
                
                trades.push(TradeRecord {
                    step,
                    timestamp: ts,
                    action: "SELL".to_string(),
                    market_id: pos.market_id.clone(),
                    side: pos.side.clone(),
                    price: sell_price,
                    size_usd: pos.size_usd,
                    fee_usd: fee,
                    shares: pos.shares,
                    reason: format!("Stop loss: {:.1}%", pnl_pct * 100.0),
                    pnl,
                });
                positions_to_remove.push(i);
            }
            // Take profit at +30%
            else if pnl_pct > 0.30 {
                let sell_price = pos.current_price;
                let fee = pos.size_usd * config.taker_fee_pct;
                let pnl = pos.realize_pnl(sell_price, config.taker_fee_pct);
                cash += pos.size_usd + pnl;
                total_fees += fee;
                
                trades.push(TradeRecord {
                    step,
                    timestamp: ts,
                    action: "SELL".to_string(),
                    market_id: pos.market_id.clone(),
                    side: pos.side.clone(),
                    price: sell_price,
                    size_usd: pos.size_usd,
                    fee_usd: fee,
                    shares: pos.shares,
                    reason: format!("Take profit: {:.1}%", pnl_pct * 100.0),
                    pnl,
                });
                positions_to_remove.push(i);
            }
        }

        // Remove closed positions (in reverse order)
        for i in positions_to_remove.into_iter().rev() {
            positions.remove(i);
        }

        // 3. Evaluate new entry opportunities
        for obs in &current_obs {
            // Skip if we already have a position in this market
            if positions.iter().any(|p| p.market_id == obs.market_id) {
                continue;
            }

            let (should_buy, side, confidence) = evaluate_market(observations, step, config);
            
            if !should_buy {
                continue;
            }

            // Check confidence threshold
            if confidence < config.min_edge_pct {
                continue;
            }

            // Calculate position size using simplified Kelly
            let market_price = match side.as_str() {
                "YES" => obs.mid_price,
                "NO" => 1.0 - obs.mid_price,
                _ => obs.mid_price,
            };
            
            let odds = 1.0 / market_price;
            let edge = confidence * 0.1; // Simplified edge from confidence
            let kelly_bet = (edge * odds - (1.0 - confidence)) / odds;
            let adjusted_bet = (kelly_bet * config.kelly_fraction).max(0.0);
            
            let size_usd = (adjusted_bet * cash)
                .max(config.min_order_usd)
                .min(cash * 0.25) // Max 25% per trade
                .min(cash - 1.0); // Keep at least $1 cash

            if size_usd < config.min_order_usd || size_usd > cash {
                continue;
            }

            // Check liquidity
            let depth = match side.as_str() {
                "YES" => obs.ask_depth,
                "NO" => obs.bid_depth,
                _ => obs.ask_depth,
            };

            if size_usd / market_price > depth * 0.5 {
                continue; // Order too large for available liquidity
            }

            // Calculate slippage
            let exec_price = slippage::calculate_slippage(
                market_price,
                depth,
                size_usd,
                0.01, // tick size
            );

            let fee = size_usd * config.taker_fee_pct;
            let shares = size_usd / exec_price;
            
            cash -= (size_usd + fee);
            total_fees += fee;

            let pos = Position {
                id: format!("{}_{}", obs.market_id, step),
                market_id: obs.market_id.clone(),
                side: side.clone(),
                entry_price: exec_price,
                current_price: exec_price,
                size_usd,
                shares,
                entry_timestamp: ts,
                unrealized_pnl: 0.0,
            };
            positions.push(pos);

            trades.push(TradeRecord {
                step,
                timestamp: ts,
                action: "BUY".to_string(),
                market_id: obs.market_id.clone(),
                side,
                price: exec_price,
                size_usd,
                fee_usd: fee,
                shares,
                reason: format!("Confidence: {:.1}%, Edge: {:.1}%", confidence * 100.0, edge * 100.0),
                pnl: 0.0,
            });
        }

        // 4. Calculate equity
        let unrealized_pnl: f64 = positions.iter().map(|p| p.unrealized_pnl).sum();
        let total_equity = cash + unrealized_pnl + positions.iter().map(|p| p.size_usd).sum::<f64>();

        equity_curve.push(EquityPoint {
            step,
            timestamp: ts,
            cash,
            unrealized_pnl,
            total_equity,
            open_positions: positions.len(),
        });

        // Update drawdown
        if total_equity > peak_equity {
            peak_equity = total_equity;
        }
        let drawdown = peak_equity - total_equity;
        let drawdown_pct = if peak_equity > 0.0 {
            drawdown / peak_equity
        } else {
            0.0
        };
        if drawdown > max_drawdown {
            max_drawdown = drawdown;
        }
        if drawdown_pct > max_drawdown_pct {
            max_drawdown_pct = drawdown_pct;
        }
    }

    // Close any remaining positions at last known price
    let final_ts = timestamps.last().unwrap_or(&0);
    for pos in positions {
        let pnl = pos.unrealized_pnl;
        cash += pos.size_usd + pnl;
        
        trades.push(TradeRecord {
            step: equity_curve.len(),
            timestamp: *final_ts,
            action: "SELL".to_string(),
            market_id: pos.market_id,
            side: pos.side,
            price: pos.current_price,
            size_usd: pos.size_usd,
            fee_usd: 0.0,
            shares: pos.shares,
            reason: "Backtest end - close position".to_string(),
            pnl,
        });
    }

    // Calculate final stats
    let winning_trades = trades.iter().filter(|t| t.action == "SELL" && t.pnl > 0.0).count();
    let losing_trades = trades.iter().filter(|t| t.action == "SELL" && t.pnl <= 0.0).count();

    // Calculate Sharpe ratio (simplified)
    let returns: Vec<f64> = equity_curve
        .windows(2)
        .map(|w| {
            let r = (w[1].total_equity - w[0].total_equity) / w[0].total_equity;
            r
        })
        .collect();
    
    let avg_return = if !returns.is_empty() {
        returns.iter().sum::<f64>() / returns.len() as f64
    } else {
        0.0
    };
    
    let std_return = if returns.len() > 1 {
        let variance = returns.iter().map(|r| (r - avg_return).powi(2).sum::<f64>() / returns.len() as f64);
        variance.sqrt()
    } else {
        0.0
    };
    
    let sharpe = if std_return > 0.0 {
        avg_return / std_return * (365.0_f64).sqrt() // Annualized
    } else {
        0.0
    };

    BacktestResult {
        initial_capital: config.initial_capital,
        final_capital: cash,
        total_trades: trades.len(),
        winning_trades,
        losing_trades,
        total_fees,
        max_drawdown,
        max_drawdown_pct,
        sharpe_ratio: sharpe,
        trades,
        equity_curve,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backtest_basic() {
        let config = BacktestConfig::default();
        let observations = vec![
            PriceObservation {
                timestamp: 1000,
                market_id: "m1".to_string(),
                ask_price: 0.5,
                bid_price: 0.49,
                ask_depth: 1000.0,
                bid_depth: 1000.0,
                spread: 0.01,
                mid_price: 0.495,
            },
        ];
        let result = run_backtest(&observations, &config);
        assert_eq!(result.initial_capital, config.initial_capital);
    }
}
```

- [ ] **Step 2: Test compilation**

```bash
cd polymarket-bot && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/backtesting/engine.rs
git commit -m "feat: rewrite backtest engine with time-based simulation"
```

---

## Task 4: Update TUI for New Result Type

**Files:**
- Modify: `polymarket-bot/src/backtesting/ui.rs`

**Context:** The TUI needs to be updated to work with the new `BacktestResult` type which now has `equity_curve: Vec<EquityPoint>` instead of `Vec<(usize, f64)>`.

- [ ] **Step 1: Update ui.rs**

Update the `render_equity_curve` function to use the new `EquityPoint` type:

```rust
fn render_equity_curve(f: &mut Frame, result: &BacktestResult, area: Rect) {
    let height = area.height as usize;
    let width = area.width as usize;

    if result.equity_curve.is_empty() || height < 3 || width < 3 {
        return;
    }

    let values: Vec<f64> = result.equity_curve.iter().map(|p| p.total_equity).collect();
    // ... rest of function stays the same
}
```

Also update the summary to show fees:

```rust
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
                format!("${:.2} -> ${:.2}", result.initial_capital, result.final_capital),
                if ret >= 0.0 { Style::default().fg(Color::Green) } else { Style::default().fg(Color::Red) },
            ),
            Span::raw("  "),
            Span::styled("Return: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:+.2}%", ret),
                if ret >= 0.0 { Style::default().fg(Color::Green) } else { Style::default().fg(Color::Red) },
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
            Span::styled("Drawdown: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:.2}%", result.max_drawdown_pct * 100.0),
                Style::default().fg(Color::Red),
            ),
            Span::raw("  "),
            Span::styled("Sharpe: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("{:.2}", result.sharpe_ratio),
                Style::default().fg(Color::Cyan),
            ),
            Span::raw("  "),
            Span::styled("Fees: ", Style::default().fg(Color::White)),
            Span::styled(
                format!("${:.2}", result.total_fees),
                Style::default().fg(Color::Yellow),
            ),
        ]),
    ];

    let summary = Paragraph::new(lines)
        .block(Block::default().borders(Borders::ALL).title("Summary"));
    f.render_widget(summary, area);
}
```

- [ ] **Step 2: Test compilation**

```bash
cd polymarket-bot && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/backtesting/ui.rs
git commit -m "feat: update TUI for new backtest result types"
```

---

## Task 5: Update Backtest Command

**Files:**
- Modify: `polymarket-bot/src/main.rs`

**Context:** The backtest command needs to convert stored markets into `PriceObservation` format and use the new engine.

- [ ] **Step 1: Update main.rs backtest command**

Replace the `Commands::Backtest` arm:

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

    // Convert stored markets to price observations
    // For now, create synthetic observations from current prices
    // In production, this would use historical price data from CLOB API
    let mut observations = Vec::new();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    
    for market in &markets {
        // Create multiple observations over time (synthetic)
        // In production, fetch real historical data from CLOB API
        for i in 0..24 {
            let ts = now - (24 - i) * 3600; // Last 24 hours, hourly
            let priceVariation = (i as f64 * 0.01).sin() * 0.05;
            let base_price = market.yes_price + priceVariation;
            
            observations.push(backtesting::types::PriceObservation {
                timestamp: ts,
                market_id: market.id.clone(),
                ask_price: base_price + 0.01,
                bid_price: base_price - 0.01,
                ask_depth: 500.0,
                bid_depth: 500.0,
                spread: 0.02,
                mid_price: base_price,
            });
        }
    }

    // Sort observations by timestamp
    observations.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    let bt_config = backtesting::types::BacktestConfig {
        initial_capital: config.general.initial_capital,
        ..Default::default()
    };

    let result = backtesting::engine::run_backtest(&observations, &bt_config);

    // Run TUI
    backtesting::ui::run_backtest_ui(&result)?;

    // Print summary after TUI exits
    backtesting::report::print_report(&result);
}
```

- [ ] **Step 2: Test compilation**

```bash
cd polymarket-bot && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat: update backtest command for new engine"
```

---

## Task 6: Update Report for New Result Type

**Files:**
- Modify: `polymarket-bot/src/backtesting/report.rs`

**Context:** Update the report to show the new fields (fees, drawdown %, etc.)

- [ ] **Step 1: Update report.rs**

```rust
use crate::backtesting::types::BacktestResult;

pub fn print_report(result: &BacktestResult) {
    let ret = (result.final_capital - result.initial_capital) / result.initial_capital * 100.0;
    let win_rate = if result.total_trades > 0 {
        result.winning_trades as f64 / result.total_trades as f64 * 100.0
    } else {
        0.0
    };

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    BACKTEST REPORT                          ║");
    println!("╠══════════════════════════════════════════════════════════════╣");
    println!(
        "║ Initial Capital:  ${:>10.2}                              ║",
        result.initial_capital
    );
    println!(
        "║ Final Capital:    ${:>10.2}                              ║",
        result.final_capital
    );
    println!(
        "║ Total Return:     {:>10.2}%                              ║",
        ret
    );
    println!(
        "║ Total Trades:     {:>10}                              ║",
        result.total_trades
    );
    println!(
        "║ Winning Trades:   {:>10} ({:.0}%)                       ║",
        result.winning_trades, win_rate
    );
    println!(
        "║ Losing Trades:    {:>10}                              ║",
        result.losing_trades
    );
    println!(
        "║ Total Fees:       ${:>10.2}                              ║",
        result.total_fees
    );
    println!(
        "║ Max Drawdown:     ${:>10.2} ({:.2}%)                    ║",
        result.max_drawdown, result.max_drawdown_pct * 100.0
    );
    println!(
        "║ Sharpe Ratio:     {:>10.2}                              ║",
        result.sharpe_ratio
    );
    println!("╚══════════════════════════════════════════════════════════════╝");
}
```

- [ ] **Step 2: Test compilation**

```bash
cd polymarket-bot && cargo check
```

- [ ] **Step 3: Commit**

```bash
git add src/backtesting/report.rs
git commit -m "feat: update report for new backtest result types"
```

---

## Task 7: End-to-End Test

**Files:** None (test only)

- [ ] **Step 1: Build**

```bash
cd polymarket-bot && cargo build --release
```

- [ ] **Step 2: Collect data**

```bash
./target/release/polymarket-bot collect
```

- [ ] **Step 3: Run backtest**

```bash
./target/release/polymarket-bot backtest --period 30d
```

Expected: Realistic results with:
- Win rate < 100%
- Drawdown > 0%
- Fees > $0
- Sharpe ratio that makes sense

- [ ] **Step 4: Commit if fixes needed**

---

## Summary

| Task | Description | Est. Time |
|------|-------------|-----------|
| 1 | Create backtest types | 10 min |
| 2 | Create slippage model | 10 min |
| 3 | Rewrite backtest engine | 30 min |
| 4 | Update TUI | 10 min |
| 5 | Update backtest command | 10 min |
| 6 | Update report | 5 min |
| 7 | End-to-end test | 10 min |
| **Total** | | **~85 min** |
