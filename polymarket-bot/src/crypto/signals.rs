use crate::crypto::binance_ws::Candle;
use crate::crypto::indicators::Timeframe;
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

impl std::fmt::Display for Direction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Direction::Up => write!(f, "Up"),
            Direction::Down => write!(f, "Down"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Regime {
    Trending,
    Ranging,
    Choppy,
}

pub struct SignalEngine {
    min_confidence: f64,
}

impl SignalEngine {
    pub fn new() -> Self {
        Self {
            min_confidence: 0.4,
        }
    }

    pub fn generate_signals(&self, candles: &HashMap<Timeframe, Vec<Candle>>) -> Vec<Signal> {
        let mut signals = Vec::new();

        // Try 15m and 1h timeframes
        for target in [Timeframe::M15, Timeframe::H1] {
            if let Some(signal) = self.generate_regime_adaptive_signal(target, candles) {
                signals.push(signal);
            }
        }

        signals
    }

    fn detect_regime(&self, candles: &[Candle]) -> Regime {
        if candles.len() < 30 {
            return Regime::Choppy;
        }

        let n = candles.len();

        // Calculate recent momentum (last 5 candles)
        let recent_momentum = (candles[n - 1].close - candles[n - 6].close) / candles[n - 6].close;

        // Calculate volatility (last 20 candles)
        let returns: Vec<f64> = candles[n - 20..]
            .windows(2)
            .map(|w| (w[1].close - w[0].close) / w[0].close)
            .collect();
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        let volatility = variance.sqrt();

        // Classify regime
        let momentum_ratio = recent_momentum.abs() / volatility;

        if momentum_ratio > 2.0 {
            Regime::Trending
        } else if volatility > 0.005 {
            Regime::Choppy
        } else {
            Regime::Ranging
        }
    }

    fn generate_regime_adaptive_signal(
        &self,
        target: Timeframe,
        candles: &HashMap<Timeframe, Vec<Candle>>,
    ) -> Option<Signal> {
        let tf_candles = candles.get(&target)?;
        if tf_candles.len() < 30 {
            return None;
        }

        let regime = self.detect_regime(tf_candles);

        match regime {
            Regime::Trending => self.generate_momentum_signal(target, tf_candles),
            Regime::Ranging => self.generate_mean_reversion_signal(target, tf_candles),
            Regime::Choppy => None, // Don't trade in choppy markets
        }
    }

    fn generate_momentum_signal(&self, target: Timeframe, candles: &[Candle]) -> Option<Signal> {
        let n = candles.len();

        // Multi-period momentum
        let lookbacks = [2, 3, 4];
        let mut momentum_scores: Vec<f64> = Vec::new();

        for &lookback in &lookbacks {
            if n < lookback + 1 {
                continue;
            }
            let current_price = candles[n - 1].close;
            let past_price = candles[n - lookback - 1].close;
            let momentum = (current_price - past_price) / past_price;
            momentum_scores.push(momentum);
        }

        if momentum_scores.is_empty() {
            return None;
        }

        // All momentum must agree on direction
        let all_up = momentum_scores.iter().all(|&m| m > 0.0);
        let all_down = momentum_scores.iter().all(|&m| m < 0.0);

        if !all_up && !all_down {
            return None;
        }

        let avg_momentum = momentum_scores.iter().sum::<f64>() / momentum_scores.len() as f64;

        // Volatility for confidence calculation
        let returns: Vec<f64> = candles
            .windows(2)
            .map(|w| (w[1].close - w[0].close) / w[0].close)
            .collect();
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance =
            returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
        let volatility = variance.sqrt();

        // Need momentum > 1.5x volatility (lower threshold for trending regime)
        if avg_momentum.abs() < volatility * 1.5 {
            return None;
        }

        let confidence = (avg_momentum.abs() / (volatility * 3.0)).min(0.95);

        let direction = if all_up {
            Direction::Up
        } else {
            Direction::Down
        };

        Some(Signal {
            timeframe: target,
            direction,
            confidence,
            reason: format!("TF:{} Mom:{:.4} Trending", target.as_str(), avg_momentum),
        })
    }

    fn generate_mean_reversion_signal(
        &self,
        target: Timeframe,
        candles: &[Candle],
    ) -> Option<Signal> {
        let n = candles.len();

        // Calculate RSI-like indicator
        let period = 14;
        if n < period + 1 {
            return None;
        }

        let mut gains = Vec::new();
        let mut losses = Vec::new();

        for i in (n - period)..n {
            let change = candles[i].close - candles[i - 1].close;
            if change > 0.0 {
                gains.push(change);
                losses.push(0.0);
            } else {
                gains.push(0.0);
                losses.push(change.abs());
            }
        }

        let avg_gain = gains.iter().sum::<f64>() / period as f64;
        let avg_loss = losses.iter().sum::<f64>() / period as f64;

        let rsi = if avg_loss == 0.0 {
            100.0
        } else {
            100.0 - (100.0 / (1.0 + avg_gain / avg_loss))
        };

        // Mean reversion: RSI extremes
        let (direction, confidence) = if rsi < 30.0 {
            // Oversold - expect bounce up
            (Direction::Up, 0.7 + (30.0 - rsi) / 100.0)
        } else if rsi > 70.0 {
            // Overbought - expect drop down
            (Direction::Down, 0.7 + (rsi - 70.0) / 100.0)
        } else if rsi < 40.0 {
            (Direction::Up, 0.5)
        } else if rsi > 60.0 {
            (Direction::Down, 0.5)
        } else {
            return None;
        };

        // Verify price is actually deviating from recent mean
        let lookback = 10;
        if n < lookback + 1 {
            return None;
        }

        let recent_mean: f64 = candles[n - lookback..n]
            .iter()
            .map(|c| c.close)
            .sum::<f64>()
            / lookback as f64;
        let current_price = candles[n - 1].close;
        let deviation = (current_price - recent_mean) / recent_mean;

        // Need meaningful deviation (>0.3% from mean)
        if deviation.abs() < 0.003 {
            return None;
        }

        // Direction must match RSI signal
        let matches = match (&direction, deviation > 0.0) {
            (Direction::Down, true) => true, // Overbought + price above mean
            (Direction::Up, false) => true,  // Oversold + price below mean
            _ => false,
        };

        if !matches {
            return None;
        }

        Some(Signal {
            timeframe: target,
            direction,
            confidence: confidence.min(0.95),
            reason: format!(
                "TF:{} RSI:{:.1} Dev:{:.4} Ranging",
                target.as_str(),
                rsi,
                deviation
            ),
        })
    }
}
