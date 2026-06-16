use crate::crypto::indicators::Timeframe;
use crate::crypto::signals::Direction;

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
        if self.open_trade.is_some() {
            return None;
        }

        let size = self.calculate_position_size();

        if size < 0.10 || self.capital < size {
            return None;
        }

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

    pub fn check_exit(&mut self, current_price: f64, timestamp: i64) -> Option<Trade> {
        let trade = self.open_trade.as_ref()?;

        let price_change = match trade.direction {
            Direction::Up => (current_price - trade.entry_price) / trade.entry_price,
            Direction::Down => (trade.entry_price - current_price) / trade.entry_price,
        };

        let minutes_elapsed = (timestamp - trade.timestamp) / 60000;

        let should_exit = price_change >= self.config.take_profit_pct
            || price_change <= -self.config.stop_loss_pct
            || minutes_elapsed >= self.config.timeout_minutes as i64;

        if should_exit {
            let mut closed_trade = self.open_trade.take().unwrap();
            closed_trade.exit_price = Some(current_price);
            closed_trade.status = if minutes_elapsed >= self.config.timeout_minutes as i64 {
                TradeStatus::Timeout
            } else {
                TradeStatus::Closed
            };

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
        let completed: Vec<&Trade> = self
            .trades
            .iter()
            .filter(|t| t.status != TradeStatus::Open)
            .collect();

        let total = completed.len();
        let wins = completed
            .iter()
            .filter(|t| t.pnl.map_or(false, |p| p > 0.0))
            .count();
        let losses = total - wins;

        let total_pnl: f64 = completed.iter().filter_map(|t| t.pnl).sum();

        let avg_win = if wins > 0 {
            completed
                .iter()
                .filter_map(|t| t.pnl)
                .filter(|p| *p > 0.0)
                .sum::<f64>()
                / wins as f64
        } else {
            0.0
        };

        let avg_loss = if losses > 0 {
            completed
                .iter()
                .filter_map(|t| t.pnl)
                .filter(|p| *p < 0.0)
                .sum::<f64>()
                / losses as f64
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
            win_rate: if total > 0 {
                wins as f64 / total as f64
            } else {
                0.0
            },
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
