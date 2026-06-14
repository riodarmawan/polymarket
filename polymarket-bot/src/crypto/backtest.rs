use crate::crypto::binance_ws::{BinanceRestClient, Candle};
use crate::crypto::indicators::Timeframe;
use crate::crypto::signals::Direction;
use crate::crypto::strategy::{
    estimate_historical_ask, predict_five_minute_continuation, predict_window,
};
use std::path::Path;

const CACHE_PATH: &str = "/home/kucingsakti/polymarket/.playwright-mcp/btc_1m_30d.json";

#[derive(Debug, Clone)]
pub struct CryptoBacktestConfig {
    pub initial_capital: f64,
    pub min_order_usd: f64,
    pub max_order_usd: f64,
    pub fee_pct: f64,
    pub timeframes: Vec<Timeframe>,
    pub min_entry_price: f64,
    pub max_entry_price: f64,
    pub min_edge: f64,
    pub entry_minute: usize,
    pub source_interval_minutes: u32,
}

impl Default for CryptoBacktestConfig {
    fn default() -> Self {
        Self {
            initial_capital: 2.0,
            min_order_usd: 0.10,
            max_order_usd: 0.50,
            fee_pct: 0.02,
            timeframes: vec![Timeframe::M15],
            min_entry_price: 0.15,
            max_entry_price: 0.60,
            min_edge: 0.10,
            entry_minute: 3,
            source_interval_minutes: 1,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CryptoTrade {
    pub timestamp: i64,
    pub timeframe: Timeframe,
    pub direction: Direction,
    pub entry_price: f64,
    pub exit_price: f64,
    pub market_price: f64,
    pub size_usd: f64,
    pub pnl: f64,
    pub won: bool,
    pub confidence: f64,
    pub edge: f64,
}

#[derive(Debug, Clone)]
pub struct CryptoBacktestResult {
    pub initial_capital: f64,
    pub final_capital: f64,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
    pub trades: Vec<CryptoTrade>,
}

pub struct CryptoBacktestEngine {
    rest_client: BinanceRestClient,
}

impl CryptoBacktestEngine {
    pub fn new() -> Self {
        Self {
            rest_client: BinanceRestClient::new(),
        }
    }

    pub async fn run_backtest(
        &self,
        config: &CryptoBacktestConfig,
        days: u32,
    ) -> Result<CryptoBacktestResult, anyhow::Error> {
        let candles = self.load_one_minute_candles(days).await?;
        if candles.len() < 120 {
            anyhow::bail!("not enough 1-minute candles for a realistic backtest");
        }

        let mut capital = config.initial_capital;
        let mut peak_capital = capital;
        let mut max_drawdown: f64 = 0.0;
        let mut trades = Vec::new();
        let mut consecutive_m5_losses = 0usize;
        let mut m5_pause_until = 0i64;

        for timeframe in &config.timeframes {
            let window_minutes = timeframe.candle_count();
            let entry_minute = if *timeframe == Timeframe::M5 {
                1
            } else {
                config.entry_minute.min(window_minutes.saturating_sub(1))
            };
            if entry_minute == 0 || window_minutes < 2 {
                continue;
            }

            tracing::info!(
                "Running {} settlement backtest with entry after minute {}",
                timeframe.as_str(),
                entry_minute
            );

            let window_ms = window_minutes as i64 * 60_000;
            let first_aligned = candles
                .iter()
                .position(|candle| candle.timestamp % window_ms == 0)
                .unwrap_or(0);
            let aligned = &candles[first_aligned..];

            for (window_index, window) in aligned.chunks(window_minutes).enumerate() {
                if window.len() < window_minutes {
                    continue;
                }

                let global_start = first_aligned + window_index * window_minutes;
                let entry_index = global_start + entry_minute - 1;
                if entry_index < 60 || entry_index >= candles.len() {
                    continue;
                }
                if *timeframe == Timeframe::M5 && window[entry_minute - 1].timestamp < m5_pause_until
                {
                    continue;
                }

                let history_start = entry_index.saturating_sub(60);
                let available_prices: Vec<f64> = candles[history_start..=entry_index]
                    .iter()
                    .map(|candle| candle.close)
                    .collect();
                let window_open = window[0].open;
                let signal = match if *timeframe == Timeframe::M5 {
                    predict_five_minute_continuation(&available_prices, window_open)
                } else {
                    predict_window(&available_prices, window_minutes)
                } {
                    Some(signal) => signal,
                    None => continue,
                };

                let entry_btc_price = window[entry_minute - 1].close;
                let final_btc_price = window[window_minutes - 1].close;
                let recent_start = available_prices.len().saturating_sub(20);
                let ask = estimate_historical_ask(
                    &signal.direction,
                    window_open,
                    entry_btc_price,
                    &available_prices[recent_start..],
                    window_minutes - entry_minute,
                );
                let edge = signal.confidence - ask;

                let max_entry_price = if *timeframe == Timeframe::M5 {
                    0.62
                } else {
                    config.max_entry_price
                };
                let min_edge = if *timeframe == Timeframe::M5 {
                    0.08
                } else {
                    config.min_edge
                };
                let min_entry_price = if *timeframe == Timeframe::M15 {
                    0.50
                } else {
                    config.min_entry_price
                };
                if ask < min_entry_price
                    || ask > max_entry_price
                    || edge < min_edge
                {
                    continue;
                }

                let current_drawdown = if peak_capital > 0.0 {
                    (peak_capital - capital) / peak_capital
                } else {
                    0.0
                };
                if current_drawdown >= 0.25 {
                    continue;
                }

                let drawdown_scale = if current_drawdown > 0.20 {
                    0.50
                } else if current_drawdown > 0.10 {
                    0.75
                } else {
                    1.0
                };
                let risk_fraction = if *timeframe == Timeframe::M5 {
                    0.03
                } else {
                    0.05
                };
                let max_order_usd = if *timeframe == Timeframe::M5 {
                    config.max_order_usd.min(0.25)
                } else {
                    config.max_order_usd
                };
                let size_usd = (capital * risk_fraction * drawdown_scale)
                    .max(config.min_order_usd)
                    .min(max_order_usd)
                    .min(capital);
                if size_usd < config.min_order_usd || capital < size_usd {
                    continue;
                }

                let window_went_up = final_btc_price >= window_open;
                let won = match signal.direction {
                    Direction::Up => window_went_up,
                    Direction::Down => !window_went_up,
                };
                let fee = size_usd * config.fee_pct;
                let shares = size_usd / ask;
                let pnl = if won {
                    shares - size_usd - fee
                } else {
                    -size_usd - fee
                };

                capital = (capital + pnl).max(0.0);
                peak_capital = peak_capital.max(capital);
                if peak_capital > 0.0 {
                    max_drawdown = max_drawdown.max((peak_capital - capital) / peak_capital);
                }

                trades.push(CryptoTrade {
                    timestamp: window[entry_minute - 1].timestamp,
                    timeframe: *timeframe,
                    direction: signal.direction,
                    entry_price: entry_btc_price,
                    exit_price: final_btc_price,
                    market_price: ask,
                    size_usd,
                    pnl,
                    won,
                    confidence: signal.confidence,
                    edge,
                });

                if *timeframe == Timeframe::M5 {
                    if won {
                        consecutive_m5_losses = 0;
                    } else {
                        consecutive_m5_losses += 1;
                        if consecutive_m5_losses >= 3 {
                            m5_pause_until =
                                window[entry_minute - 1].timestamp + 90 * 60 * 1000;
                            consecutive_m5_losses = 0;
                        }
                    }
                }
            }
        }

        Ok(build_result(config.initial_capital, capital, max_drawdown, trades))
    }

    async fn load_one_minute_candles(&self, days: u32) -> Result<Vec<Candle>, anyhow::Error> {
        if days == 30 && Path::new(CACHE_PATH).exists() {
            let data = std::fs::read_to_string(CACHE_PATH)?;
            let candles: Vec<Candle> = serde_json::from_str(&data)?;
            if candles.len() >= 40_000 {
                tracing::info!("Loaded {} cached 1m candles", candles.len());
                return Ok(candles);
            }
        }

        let end_time = chrono::Utc::now().timestamp_millis();
        let start_time = end_time - days as i64 * 24 * 60 * 60 * 1000;
        let candles = self
            .rest_client
            .fetch_candles_range("BTCUSDT", "1m", start_time, end_time)
            .await?;

        if days == 30 && candles.len() >= 40_000 {
            std::fs::write(CACHE_PATH, serde_json::to_string(&candles)?)?;
        }
        Ok(candles)
    }
}

fn build_result(
    initial_capital: f64,
    final_capital: f64,
    max_drawdown: f64,
    trades: Vec<CryptoTrade>,
) -> CryptoBacktestResult {
    let winning_trades = trades.iter().filter(|trade| trade.won).count();
    let losing_trades = trades.len() - winning_trades;
    let gross_profit: f64 = trades
        .iter()
        .filter(|trade| trade.pnl > 0.0)
        .map(|trade| trade.pnl)
        .sum();
    let gross_loss: f64 = trades
        .iter()
        .filter(|trade| trade.pnl < 0.0)
        .map(|trade| trade.pnl.abs())
        .sum();

    CryptoBacktestResult {
        initial_capital,
        final_capital,
        total_trades: trades.len(),
        winning_trades,
        losing_trades,
        win_rate: if trades.is_empty() {
            0.0
        } else {
            winning_trades as f64 / trades.len() as f64
        },
        total_pnl: final_capital - initial_capital,
        avg_win: if winning_trades == 0 {
            0.0
        } else {
            gross_profit / winning_trades as f64
        },
        avg_loss: if losing_trades == 0 {
            0.0
        } else {
            gross_loss / losing_trades as f64
        },
        profit_factor: if gross_loss > 0.0 {
            gross_profit / gross_loss
        } else {
            0.0
        },
        max_drawdown,
        trades,
    }
}

impl Default for CryptoBacktestEngine {
    fn default() -> Self {
        Self::new()
    }
}
