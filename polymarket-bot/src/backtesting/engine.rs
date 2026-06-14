use crate::backtesting::slippage;
use crate::backtesting::types::*;
use std::collections::HashMap;

#[derive(Debug)]
struct Signal {
    side: String,
    confidence: f64,
    reason: String,
}

fn evaluate_market(
    market_observations: &[&PriceObservation],
) -> Option<Signal> {
    if market_observations.len() < 20 {
        return None;
    }

    let current = market_observations.last()?;
    let price = current.mid_price;

    // Skip extreme prices (already resolved or nearly resolved)
    if price > 0.95 || price < 0.05 {
        return None;
    }

    let prices: Vec<f64> = market_observations.iter().map(|o| o.mid_price).collect();
    let n = prices.len();

    // ── Signal 1: Mean Reversion (short-term, higher threshold) ──
    let short_window = 20.min(n);
    let short_prices = &prices[n - short_window..];
    let short_mean = short_prices.iter().sum::<f64>() / short_window as f64;
    let short_var = short_prices.iter().map(|p| (p - short_mean).powi(2)).sum::<f64>() / short_window as f64;
    let short_std = short_var.sqrt();

    // Only consider if there's actual volatility
    if short_std < 0.003 {
        return None;
    }

    eprintln!("  PASS std={:.4} price={:.4} n={}", short_std, price, n);

    let z_score = (price - short_mean) / short_std;

    // ── Signal 2: Momentum (medium-term, 40 periods) ──
    let long_window = 40.min(n);
    let long_mean = prices[n - long_window..].iter().sum::<f64>() / long_window as f64;
    let momentum = (price - long_mean).max(-1.0).min(1.0);

    // ── Signal 3: Trend (price direction over last 8 periods) ──
    let trend_window = 8.min(n - 1);
    let trend_start = prices[n - trend_window - 1];
    let trend = price - trend_start;

    // ── Signal 4: Price distance from 0.5 (value signal) ──
    let distance_from_mid = (price - 0.5).abs();

    // ── Signal 5: Recent price change rate (acceleration) ──
    let recent_window = 3.min(n - 1);
    let recent_start = prices[n - recent_window - 1];
    let recent_change = price - recent_start;

    // ── Combined Signal ──
    let mut yes_score: f64 = 0.0;
    let mut no_score: f64 = 0.0;
    let mut reasons = Vec::new();

    // Mean reversion: trigger on moderate deviations
    if z_score < -0.4 {
        let strength = ((z_score.abs() - 0.4) / 1.5).min(1.0);
        yes_score += strength * 0.5;
        reasons.push(format!("MR({:.1})", z_score));
    } else if z_score > 0.4 {
        let strength = ((z_score.abs() - 0.4) / 1.5).min(1.0);
        no_score += strength * 0.5;
        reasons.push(format!("MR({:.1})", z_score));
    }

    // Momentum: follow the trend
    if momentum > 0.01 {
        let strength = (momentum * 20.0).min(1.0);
        yes_score += strength * 0.35;
        reasons.push(format!("MOM({:.3})", momentum));
    } else if momentum < -0.01 {
        let strength = (momentum.abs() * 20.0).min(1.0);
        no_score += strength * 0.35;
        reasons.push(format!("MOM({:.3})", momentum));
    }

    // Trend: short-term direction
    if trend > 0.01 {
        let strength = (trend * 20.0).min(1.0);
        yes_score += strength * 0.25;
        reasons.push(format!("TR({:.3})", trend));
    } else if trend < -0.01 {
        let strength = (trend.abs() * 20.0).min(1.0);
        no_score += strength * 0.25;
        reasons.push(format!("TR({:.3})", trend));
    }

    // Value: buy when price is cheap
    if price < 0.45 && price > 0.10 {
        let val_strength = ((0.45 - price) * 2.0).min(0.35);
        yes_score += val_strength;
        reasons.push(format!("VAL({:.2})", price));
    } else if price > 0.55 && price < 0.90 {
        let val_strength = ((price - 0.55) * 2.0).min(0.35);
        no_score += val_strength;
        reasons.push(format!("VAL({:.2})", price));
    }

    // Acceleration: recent price change in same direction as momentum
    if recent_change > 0.005 && momentum > 0.005 {
        yes_score += 0.15;
        reasons.push("ACC".to_string());
    } else if recent_change < -0.005 && momentum < -0.005 {
        no_score += 0.15;
        reasons.push("ACC".to_string());
    }

    // Need at least 1 signal
    if reasons.is_empty() {
        return None;
    }

    // Lower threshold for entry
    let min_threshold = 0.15;

    if yes_score > min_threshold && yes_score > no_score {
        let confidence = (yes_score * 0.65).min(0.80);
        Some(Signal {
            side: "YES".to_string(),
            confidence,
            reason: reasons.join("+"),
        })
    } else if no_score > min_threshold && no_score > yes_score {
        let confidence = (no_score * 0.65).min(0.80);
        Some(Signal {
            side: "NO".to_string(),
            confidence,
            reason: reasons.join("+"),
        })
    } else {
        None
    }
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
    let mut debug_eval_count = 0usize;
    let mut debug_signal_count = 0usize;
    let mut max_drawdown = 0.0;
    let mut max_drawdown_pct = 0.0;

    // Sort timestamps
    let mut timestamps: Vec<u64> = observations.iter().map(|o| o.timestamp).collect();
    timestamps.sort();
    timestamps.dedup();

    // Pre-build per-market observation history (O(n) once)
    let mut market_histories: HashMap<String, Vec<&PriceObservation>> = HashMap::new();
    for obs in observations {
        market_histories
            .entry(obs.market_id.clone())
            .or_default()
            .push(obs);
    }
    // Sort each market's observations by timestamp
    for obs_vec in market_histories.values_mut() {
        obs_vec.sort_by_key(|o| o.timestamp);
    }

    // Build timestamp index for fast lookup
    let mut market_step_idx: HashMap<String, usize> = HashMap::new();
    for (market_id, _) in &market_histories {
        market_step_idx.insert(market_id.clone(), 0);
    }

    for (step, &ts) in timestamps.iter().enumerate() {
        let current_obs: Vec<&PriceObservation> = observations
            .iter()
            .filter(|o| o.timestamp == ts)
            .collect();

        // 1. Update mark-to-market
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

        // 2. Check exit conditions
        let mut positions_to_remove = Vec::new();
        for (i, pos) in positions.iter().enumerate() {
            let pnl_pct = if pos.entry_price > 0.0 {
                (pos.current_price - pos.entry_price) / pos.entry_price
            } else {
                0.0
            };

            // Exit conditions for prediction markets:
            // 1. Take profit at +15% (more realistic for prediction markets)
            // 2. Stop loss at -30% (wider SL to avoid premature exits)
            // 3. Time exit after 12 hours (6h is too short for price moves)
            let should_exit = pnl_pct < -0.30 || pnl_pct > 0.15;

            // Time-based exit: 12 hours = 43200 seconds
            let hold_duration = ts.saturating_sub(pos.entry_timestamp);
            let time_exit = hold_duration > 43200;

            if should_exit || time_exit {
                let sell_price = pos.current_price;
                let fee = pos.size_usd * config.taker_fee_pct;
                let pnl = pos.realize_pnl(sell_price, config.taker_fee_pct);
                cash += pos.size_usd + pnl;
                total_fees += fee;

                let reason = if pnl_pct < -0.30 {
                    format!("SL({:.1}%)", pnl_pct * 100.0)
                } else if pnl_pct > 0.15 {
                    format!("TP({:.1}%)", pnl_pct * 100.0)
                } else {
                    format!("TIME({:.0}h)", hold_duration as f64 / 3600.0)
                };

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
                    reason,
                    pnl,
                });
                positions_to_remove.push(i);
            }
        }

        for i in positions_to_remove.into_iter().rev() {
            positions.remove(i);
        }

        // 3. Evaluate new entries (limit to 2 new positions per step)
        let mut new_entries = 0;
        let max_new_per_step = 2;

        for obs in &current_obs {
            if new_entries >= max_new_per_step {
                break;
            }

            if positions.iter().any(|p| p.market_id == obs.market_id) {
                continue;
            }

            // Get market history up to current timestamp
            let market_obs = match market_histories.get(&obs.market_id) {
                Some(hist) => hist,
                None => continue,
            };

            // Find the slice of history up to current timestamp
            let history_slice: Vec<&PriceObservation> = market_obs
                .iter()
                .filter(|o| o.timestamp <= ts)
                .copied()
                .collect();

            if history_slice.len() < 20 {
                continue;
            }

            let signal = match evaluate_market(&history_slice) {
                Some(s) => {
                    debug_signal_count += 1;
                    if debug_signal_count <= 5 {
                        eprintln!("  SIGNAL: market={}, side={}, confidence={:.3}, reason={}", obs.market_id, s.side, s.confidence, s.reason);
                    }
                    s
                }
                None => {
                    continue;
                }
            };

            let market_price = match signal.side.as_str() {
                "YES" => obs.ask_price,
                "NO" => 1.0 - obs.bid_price,
                _ => obs.ask_price,
            };

            // Position sizing: 15% of capital per trade (more conservative)
            let base_size = cash * 0.15;
            let size_usd = base_size
                .max(config.min_order_usd)
                .min(cash * 0.20)
                .min(cash - 0.20); // Keep $0.20 reserve

            if size_usd < config.min_order_usd || size_usd > cash - 0.01 {
                continue;
            }

            let depth = match signal.side.as_str() {
                "YES" => obs.ask_depth,
                "NO" => obs.bid_depth,
                _ => obs.ask_depth,
            };

            // Skip if not enough liquidity
            if depth < size_usd / market_price {
                continue;
            }

            let exec_price = slippage::calculate_slippage(market_price, depth, size_usd, 0.01);
            let fee = size_usd * config.taker_fee_pct;
            let shares = size_usd / exec_price;

            cash -= (size_usd + fee);
            total_fees += fee;

            let pos = Position {
                id: format!("{}_{}", obs.market_id, step),
                market_id: obs.market_id.clone(),
                side: signal.side.clone(),
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
                side: signal.side,
                price: exec_price,
                size_usd,
                fee_usd: fee,
                shares,
                reason: signal.reason,
                pnl: 0.0,
            });
            new_entries += 1;
        }

        // 4. Calculate equity
        let positions_value: f64 = positions.iter().map(|p| p.size_usd + p.unrealized_pnl).sum();
        let total_equity = cash + positions_value;

        equity_curve.push(EquityPoint {
            step,
            timestamp: ts,
            cash,
            unrealized_pnl: positions.iter().map(|p| p.unrealized_pnl).sum(),
            total_equity,
            open_positions: positions.len(),
        });

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

    // Close remaining positions
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
            reason: "END".to_string(),
            pnl,
        });
    }

    let winning_trades = trades.iter().filter(|t| t.action == "SELL" && t.pnl > 0.0).count();
    let losing_trades = trades.iter().filter(|t| t.action == "SELL" && t.pnl <= 0.0).count();

    // Sharpe ratio (annualized)
    let returns: Vec<f64> = equity_curve
        .windows(2)
        .map(|w| {
            if w[0].total_equity > 0.0 {
                (w[1].total_equity - w[0].total_equity) / w[0].total_equity
            } else {
                0.0
            }
        })
        .collect();

    let avg_return = if !returns.is_empty() {
        returns.iter().sum::<f64>() / returns.len() as f64
    } else {
        0.0
    };

    let variance = if returns.len() > 1 {
        returns.iter().map(|r| (r - avg_return).powi(2)).sum::<f64>() / returns.len() as f64
    } else {
        0.0
    };
    let std_return = variance.sqrt();

    // Annualize: observations are ~30min apart, so ~48 per day, ~17520 per year
    let steps_per_year: f64 = 17520.0;
    let sharpe = if std_return > 0.0 {
        (avg_return / std_return) * steps_per_year.sqrt()
    } else {
        0.0
    };

    // Profit factor
    let gross_profit: f64 = trades.iter().filter(|t| t.pnl > 0.0).map(|t| t.pnl).sum();
    let gross_loss: f64 = trades.iter().filter(|t| t.pnl < 0.0).map(|t| t.pnl.abs()).sum();
    let profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else if gross_profit > 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    eprintln!("DEBUG: evaluated {} markets, signals found {}, trades {}", debug_eval_count + debug_signal_count, debug_signal_count, trades.len());

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
