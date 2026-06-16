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
    pub risk_fraction: f64,
    pub min_order_shares: f64,
    pub fee_pct: f64,
    pub timeframes: Vec<Timeframe>,
    pub min_entry_price: f64,
    pub max_entry_price: f64,
    pub min_edge: f64,
    pub entry_minute: usize,
    pub execution_delay_secs: u32,
    pub source_interval_minutes: u32,
    pub target_start_ts: Option<i64>,
    pub target_end_ts: Option<i64>,
}

impl Default for CryptoBacktestConfig {
    fn default() -> Self {
        Self {
            initial_capital: 7.50,
            min_order_usd: 0.50,
            max_order_usd: 4.00,
            risk_fraction: 0.50,
            min_order_shares: 5.0,
            fee_pct: 0.10,
            timeframes: vec![Timeframe::M15],
            min_entry_price: 0.15,
            max_entry_price: 0.60,
            min_edge: 0.10,
            entry_minute: 3,
            execution_delay_secs: 30,
            source_interval_minutes: 1,
            target_start_ts: None,
            target_end_ts: None,
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
struct CandidateTrade {
    timestamp: i64,
    timeframe: Timeframe,
    direction: Direction,
    entry_price: f64,
    exit_price: f64,
    market_price: f64,
    won: bool,
    confidence: f64,
    edge: f64,
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
    pub diagnostics: ModelDiagnostics,
    pub trades: Vec<CryptoTrade>,
}

#[derive(Debug, Clone, Default)]
pub struct ModelDiagnostics {
    pub raw_signals: usize,
    pub raw_correct: usize,
    pub raw_accuracy: f64,
    pub average_confidence: f64,
    pub calibration_gap: f64,
    pub brier_score: f64,
    pub first_half_signals: usize,
    pub first_half_accuracy: f64,
    pub second_half_signals: usize,
    pub second_half_accuracy: f64,
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
        let mut candles = self.load_one_minute_candles(days).await?;
        let now_ms = chrono::Utc::now().timestamp_millis();
        candles.retain(|candle| candle.timestamp + 60_000 <= now_ms);
        if candles.len() < 120 {
            anyhow::bail!("not enough 1-minute candles for a realistic backtest");
        }

        let mut capital = config.initial_capital;
        let mut peak_capital = capital;
        let mut max_drawdown: f64 = 0.0;
        let mut candidates = Vec::new();
        let mut trades = Vec::new();
        let mut diagnostics = ModelDiagnostics::default();
        let mut confidence_sum = 0.0;
        let mut brier_sum = 0.0;
        let mut first_half_correct = 0usize;
        let mut second_half_correct = 0usize;
        let midpoint_ts = match (config.target_start_ts, config.target_end_ts) {
            (Some(start), Some(end)) => start + (end - start) / 2,
            _ => candles[candles.len() / 2].timestamp,
        };

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
                if config
                    .target_start_ts
                    .is_some_and(|start| window[0].timestamp < start)
                    || config
                        .target_end_ts
                        .is_some_and(|end| window[0].timestamp >= end)
                {
                    continue;
                }

                let global_start = first_aligned + window_index * window_minutes;
                let entry_index = global_start + entry_minute - 1;
                let delay_minutes = ((config.execution_delay_secs as usize) + 59) / 60;
                let execution_index = entry_index + delay_minutes;
                if entry_index < 60 || execution_index >= candles.len() {
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

                let execution_elapsed_secs =
                    (entry_minute as u32 * 60) + config.execution_delay_secs;
                let (entry_start_secs, entry_end_secs, min_entry_price, max_entry_price, min_edge) =
                    if *timeframe == Timeframe::M5 {
                        (45, 180, 0.05, 0.62, 0.08)
                    } else {
                        (190, 600, config.min_entry_price, config.max_entry_price, config.min_edge)
                    };
                if execution_elapsed_secs < entry_start_secs || execution_elapsed_secs > entry_end_secs
                {
                    continue;
                }

                let entry_btc_price = candles[execution_index].close;
                let final_btc_price = window[window_minutes - 1].close;
                let window_went_up = final_btc_price >= window_open;
                let signal_won = match signal.direction {
                    Direction::Up => window_went_up,
                    Direction::Down => !window_went_up,
                };
                diagnostics.raw_signals += 1;
                diagnostics.raw_correct += usize::from(signal_won);
                confidence_sum += signal.confidence;
                brier_sum += (signal.confidence - if signal_won { 1.0 } else { 0.0 }).powi(2);
                if window[entry_minute - 1].timestamp < midpoint_ts {
                    diagnostics.first_half_signals += 1;
                    first_half_correct += usize::from(signal_won);
                } else {
                    diagnostics.second_half_signals += 1;
                    second_half_correct += usize::from(signal_won);
                }

                let execution_history_start = execution_index.saturating_sub(60);
                let execution_prices: Vec<f64> = candles[execution_history_start..=execution_index]
                    .iter()
                    .map(|candle| candle.close)
                    .collect();
                let minutes_remaining = window_minutes.saturating_sub(entry_minute + delay_minutes);
                let ask = estimate_historical_ask(
                    &signal.direction,
                    window_open,
                    entry_btc_price,
                    &execution_prices,
                    minutes_remaining,
                );
                let edge = signal.confidence - ask;
                if ask < min_entry_price || ask > max_entry_price {
                    continue;
                }
                if edge < min_edge + config.fee_pct {
                    continue;
                }

                candidates.push(CandidateTrade {
                    timestamp: candles[execution_index].timestamp,
                    timeframe: *timeframe,
                    direction: signal.direction,
                    entry_price: entry_btc_price,
                    exit_price: final_btc_price,
                    market_price: ask,
                    won: signal_won,
                    confidence: signal.confidence,
                    edge,
                });
            }
        }

        candidates.sort_by_key(|candidate| candidate.timestamp);
        let mut consecutive_m5_losses = 0usize;
        let mut m5_pause_until = 0i64;
        for candidate in candidates {
            if candidate.timeframe == Timeframe::M5 && candidate.timestamp < m5_pause_until {
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
            let size_usd = (capital * config.risk_fraction * drawdown_scale)
                .max(config.min_order_usd)
                .min(config.max_order_usd)
                .min(capital);
            if size_usd < config.min_order_usd || capital < size_usd {
                continue;
            }

            let fee = size_usd * config.fee_pct;
            let shares = size_usd / candidate.market_price;
            if shares < config.min_order_shares {
                continue;
            }
            let pnl = if candidate.won {
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
                timestamp: candidate.timestamp,
                timeframe: candidate.timeframe,
                direction: candidate.direction,
                entry_price: candidate.entry_price,
                exit_price: candidate.exit_price,
                market_price: candidate.market_price,
                size_usd,
                pnl,
                won: candidate.won,
                confidence: candidate.confidence,
                edge: candidate.edge,
            });

            if candidate.timeframe == Timeframe::M5 {
                if candidate.won {
                    consecutive_m5_losses = 0;
                } else {
                    consecutive_m5_losses += 1;
                    if consecutive_m5_losses >= 3 {
                        m5_pause_until = candidate.timestamp + 90 * 60 * 1000;
                        consecutive_m5_losses = 0;
                    }
                }
            }
        }

        if diagnostics.raw_signals > 0 {
            diagnostics.raw_accuracy =
                diagnostics.raw_correct as f64 / diagnostics.raw_signals as f64;
            diagnostics.average_confidence = confidence_sum / diagnostics.raw_signals as f64;
            diagnostics.calibration_gap = diagnostics.average_confidence - diagnostics.raw_accuracy;
            diagnostics.brier_score = brier_sum / diagnostics.raw_signals as f64;
        }
        if diagnostics.first_half_signals > 0 {
            diagnostics.first_half_accuracy =
                first_half_correct as f64 / diagnostics.first_half_signals as f64;
        }
        if diagnostics.second_half_signals > 0 {
            diagnostics.second_half_accuracy =
                second_half_correct as f64 / diagnostics.second_half_signals as f64;
        }

        Ok(build_result(
            config.initial_capital,
            capital,
            max_drawdown,
            diagnostics,
            trades,
        ))
    }

    async fn load_one_minute_candles(&self, days: u32) -> Result<Vec<Candle>, anyhow::Error> {
        if days == 30 && Path::new(CACHE_PATH).exists() {
            let data = std::fs::read_to_string(CACHE_PATH)?;
            let candles: Vec<Candle> = serde_json::from_str(&data)?;
            let fresh_cutoff = chrono::Utc::now().timestamp_millis() - 10 * 60 * 1000;
            let cache_is_fresh = candles
                .last()
                .map(|candle| candle.timestamp >= fresh_cutoff)
                .unwrap_or(false);
            if candles.len() >= 40_000 && cache_is_fresh {
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
    diagnostics: ModelDiagnostics,
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
        } else if gross_profit > 0.0 {
            f64::INFINITY
        } else {
            0.0
        },
        max_drawdown,
        diagnostics,
        trades,
    }
}

impl Default for CryptoBacktestEngine {
    fn default() -> Self {
        Self::new()
    }
}
