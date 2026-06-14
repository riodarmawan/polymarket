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
    /// Model confidence threshold
    pub min_edge_pct: f64,
    /// Fraction of Kelly to use (0.0 to 1.0)
    pub kelly_fraction: f64,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_capital: 2.0,
            taker_fee_pct: 0.02,
            min_order_usd: 0.50,
            max_slippage_pct: 0.05,
            min_edge_pct: 0.0,
            kelly_fraction: 0.125,
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
    pub id: String,
    pub market_id: String,
    pub side: String,
    pub entry_price: f64,
    pub current_price: f64,
    pub size_usd: f64,
    pub shares: f64,
    pub entry_timestamp: u64,
    pub unrealized_pnl: f64,
}

impl Position {
    pub fn update_price(&mut self, current_price: f64) {
        self.current_price = current_price;
        self.unrealized_pnl = (current_price - self.entry_price) * self.shares;
    }

    pub fn realize_pnl(&self, sell_price: f64, fee_pct: f64) -> f64 {
        let gross = (sell_price - self.entry_price) * self.shares;
        let fee = self.size_usd * fee_pct;
        gross - fee
    }
}

/// A trade record
#[derive(Debug, Clone, Serialize)]
pub struct TradeRecord {
    pub step: usize,
    pub timestamp: u64,
    pub action: String,
    pub market_id: String,
    pub side: String,
    pub price: f64,
    pub size_usd: f64,
    pub fee_usd: f64,
    pub shares: f64,
    pub reason: String,
    pub pnl: f64,
}

/// Equity point for the equity curve
#[derive(Debug, Clone, Serialize)]
pub struct EquityPoint {
    pub step: usize,
    pub timestamp: u64,
    pub cash: f64,
    pub unrealized_pnl: f64,
    pub total_equity: f64,
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