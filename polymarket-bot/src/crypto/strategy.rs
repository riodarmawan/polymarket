use crate::crypto::signals::Direction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyParameters {
    pub fifteen_minute_strength_threshold: f64,
    pub fifteen_minute_fast_opening_z_threshold: f64,
    pub five_minute_window_strength_threshold: f64,
    pub five_minute_opening_z_threshold: f64,
    pub minimum_volatility: f64,
    pub early_confidence_base: f64,
    pub early_confidence_strength_cap: f64,
    pub early_confidence_strength_scale: f64,
    pub early_confidence_cap: f64,
    pub continuation_confidence_base: f64,
    pub continuation_confidence_excess_z_cap: f64,
    pub continuation_confidence_scale: f64,
    pub continuation_confidence_cap: f64,
    pub historical_default_volatility: f64,
    pub historical_minimum_volatility: f64,
    pub historical_probability_base: f64,
    pub historical_probability_displacement_scale: f64,
    pub historical_probability_z_cap: f64,
    pub historical_probability_floor: f64,
    pub historical_probability_cap: f64,
    pub historical_ask_premium: f64,
    pub historical_ask_floor: f64,
    pub historical_ask_cap: f64,
}

impl Default for StrategyParameters {
    fn default() -> Self {
        Self {
            fifteen_minute_strength_threshold: 1.50,
            fifteen_minute_fast_opening_z_threshold: 2.00,
            five_minute_window_strength_threshold: 0.55,
            five_minute_opening_z_threshold: 1.50,
            minimum_volatility: 0.000_05,
            early_confidence_base: 0.52,
            early_confidence_strength_cap: 2.0,
            early_confidence_strength_scale: 0.08,
            early_confidence_cap: 0.68,
            continuation_confidence_base: 0.72,
            continuation_confidence_excess_z_cap: 2.0,
            continuation_confidence_scale: 0.035,
            continuation_confidence_cap: 0.79,
            historical_default_volatility: 0.000_5,
            historical_minimum_volatility: 0.000_1,
            historical_probability_base: 0.5,
            historical_probability_displacement_scale: 0.18,
            historical_probability_z_cap: 2.0,
            historical_probability_floor: 0.12,
            historical_probability_cap: 0.88,
            historical_ask_premium: 0.015,
            historical_ask_floor: 0.05,
            historical_ask_cap: 0.95,
        }
    }
}

/// Predict a 15-minute market from the first 1-2 completed minutes.
///
/// This is the latency-sensitive path used before the older minute-3
/// confirmation model. It requires a statistically large opening displacement
/// and alignment with the short pre-window trend.
pub fn predict_fifteen_minute_latency_breakout(
    prices: &[f64],
    window_open: f64,
) -> Option<EarlyWindowSignal> {
    predict_fifteen_minute_latency_breakout_with_params(
        prices,
        window_open,
        &StrategyParameters::default(),
    )
}

pub fn predict_fifteen_minute_latency_breakout_with_params(
    prices: &[f64],
    window_open: f64,
    params: &StrategyParameters,
) -> Option<EarlyWindowSignal> {
    if prices.len() < 31 || window_open <= 0.0 {
        return None;
    }

    let n = prices.len();
    let current = prices[n - 1];
    let displacement = (current - window_open) / window_open;
    if displacement.abs() < 0.000_05 {
        return None;
    }

    let prior = &prices[n - 31..n - 1];
    let returns: Vec<f64> = prior
        .windows(2)
        .map(|window| (window[1] - window[0]) / window[0])
        .collect();
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let volatility = variance.sqrt().max(params.minimum_volatility);
    let opening_z = displacement / volatility;
    let prior_short = (prior[29] - prior[24]) / prior[24];

    if opening_z.abs() < params.fifteen_minute_fast_opening_z_threshold
        || prior_short.signum() != displacement.signum()
    {
        return None;
    }

    let confidence = (params.continuation_confidence_base
        + (opening_z.abs() - params.fifteen_minute_fast_opening_z_threshold)
            .min(params.continuation_confidence_excess_z_cap)
            * params.continuation_confidence_scale)
        .min(params.continuation_confidence_cap);
    let direction = if displacement > 0.0 {
        Direction::Up
    } else {
        Direction::Down
    };

    Some(EarlyWindowSignal {
        direction,
        confidence,
        reason: format!(
            "15m latency breakout opening_z={:.2} prior_short={:.3}%",
            opening_z,
            prior_short * 100.0
        ),
    })
}

#[derive(Debug, Clone)]
pub struct EarlyWindowSignal {
    pub direction: Direction,
    pub confidence: f64,
    pub reason: String,
}

/// Predict the 15-minute window direction from prices available at entry time.
///
/// The model deliberately stays small: short momentum must agree with the
/// broader move and must be meaningful relative to recent volatility.
pub fn predict_early_window(prices: &[f64]) -> Option<EarlyWindowSignal> {
    predict_window(prices, 15)
}

/// Predict a settlement window using lookbacks appropriate for its duration.
pub fn predict_window(prices: &[f64], window_minutes: usize) -> Option<EarlyWindowSignal> {
    predict_window_with_params(prices, window_minutes, &StrategyParameters::default())
}

pub fn predict_window_with_params(
    prices: &[f64],
    window_minutes: usize,
    params: &StrategyParameters,
) -> Option<EarlyWindowSignal> {
    let (short_lookback, medium_lookback, strength_divisor) = if window_minutes <= 5 {
        (2, 5, 3.0)
    } else {
        (3, 11, 4.0)
    };
    let required = medium_lookback + 1;
    if prices.len() < required {
        return None;
    }

    let n = prices.len();
    let current = prices[n - 1];
    let short_momentum =
        (current - prices[n - 1 - short_lookback]) / prices[n - 1 - short_lookback];
    let medium_momentum =
        (current - prices[n - 1 - medium_lookback]) / prices[n - 1 - medium_lookback];

    if short_momentum.signum() != medium_momentum.signum() {
        return None;
    }

    let returns: Vec<f64> = prices[n - required..]
        .windows(2)
        .map(|window| (window[1] - window[0]) / window[0])
        .collect();
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let volatility = variance.sqrt().max(params.minimum_volatility);

    let strength = (short_momentum.abs() + medium_momentum.abs()) / (volatility * strength_divisor);
    let minimum_strength = if window_minutes <= 5 {
        params.five_minute_window_strength_threshold
    } else {
        params.fifteen_minute_strength_threshold
    };
    if strength < minimum_strength {
        return None;
    }

    let confidence = (params.early_confidence_base
        + strength.min(params.early_confidence_strength_cap)
            * params.early_confidence_strength_scale)
        .min(params.early_confidence_cap);
    let direction = if medium_momentum > 0.0 {
        Direction::Up
    } else {
        Direction::Down
    };

    Some(EarlyWindowSignal {
        direction,
        confidence,
        reason: format!(
            "early momentum short={:.3}% medium={:.3}% vol={:.3}%",
            short_momentum * 100.0,
            medium_momentum * 100.0,
            volatility * 100.0
        ),
    })
}

/// Find statistically meaningful continuation after the first minute of a
/// 5-minute market. This is intentionally separate from the 15-minute model.
pub fn predict_five_minute_continuation(
    prices: &[f64],
    window_open: f64,
) -> Option<EarlyWindowSignal> {
    predict_five_minute_continuation_with_params(
        prices,
        window_open,
        &StrategyParameters::default(),
    )
}

pub fn predict_five_minute_continuation_with_params(
    prices: &[f64],
    window_open: f64,
    params: &StrategyParameters,
) -> Option<EarlyWindowSignal> {
    if prices.len() < 31 || window_open <= 0.0 {
        return None;
    }

    let n = prices.len();
    let current = prices[n - 1];
    let displacement = (current - window_open) / window_open;
    if displacement.abs() < 0.000_05 {
        return None;
    }

    let prior = &prices[n - 31..n - 1];
    let returns: Vec<f64> = prior
        .windows(2)
        .map(|window| (window[1] - window[0]) / window[0])
        .collect();
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let volatility = variance.sqrt().max(params.minimum_volatility);
    let opening_z = displacement / volatility;

    let prior_short = (prior[29] - prior[24]) / prior[24];
    if opening_z.abs() < params.five_minute_opening_z_threshold
        || prior_short.signum() != displacement.signum()
    {
        return None;
    }

    let confidence = (params.continuation_confidence_base
        + (opening_z.abs() - params.five_minute_opening_z_threshold)
            .min(params.continuation_confidence_excess_z_cap)
            * params.continuation_confidence_scale)
        .min(params.continuation_confidence_cap);
    let direction = if displacement > 0.0 {
        Direction::Up
    } else {
        Direction::Down
    };

    Some(EarlyWindowSignal {
        direction,
        confidence,
        reason: format!(
            "5m continuation opening_z={:.2} prior_short={:.3}%",
            opening_z,
            prior_short * 100.0
        ),
    })
}

pub fn diagnose_five_minute_continuation(prices: &[f64], window_open: f64) -> String {
    diagnose_five_minute_continuation_with_params(
        prices,
        window_open,
        &StrategyParameters::default(),
    )
}

pub fn diagnose_five_minute_continuation_with_params(
    prices: &[f64],
    window_open: f64,
    params: &StrategyParameters,
) -> String {
    if prices.len() < 31 {
        return format!("Need 31 one-minute prices; available {}", prices.len());
    }
    if window_open <= 0.0 {
        return "Invalid 5m window open price".to_string();
    }

    let n = prices.len();
    let current = prices[n - 1];
    let displacement = (current - window_open) / window_open;
    let prior = &prices[n - 31..n - 1];
    let returns: Vec<f64> = prior
        .windows(2)
        .map(|window| (window[1] - window[0]) / window[0])
        .collect();
    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / returns.len() as f64;
    let volatility = variance.sqrt().max(params.minimum_volatility);
    let opening_z = displacement / volatility;
    let prior_short = (prior[29] - prior[24]) / prior[24];
    let aligned = prior_short.signum() == displacement.signum();

    if opening_z.abs() < params.five_minute_opening_z_threshold {
        format!(
            "WAIT: opening move {:.2}z below {:.2}z; prior trend {:.3}%",
            opening_z.abs(),
            params.five_minute_opening_z_threshold,
            prior_short * 100.0
        )
    } else if !aligned {
        format!(
            "WAIT: opening move {:.2}z strong, but prior trend {:.3}% is opposite",
            opening_z.abs(),
            prior_short * 100.0
        )
    } else {
        format!(
            "Qualified setup: opening move {:.2}z and prior trend {:.3}% aligned",
            opening_z.abs(),
            prior_short * 100.0
        )
    }
}

/// Approximate the ask price available early in a historical market.
///
/// Historical BTC candles do not contain Polymarket orderbooks. This estimate
/// is intentionally conservative and is reported as simulated odds.
pub fn estimate_historical_ask(
    direction: &Direction,
    window_open: f64,
    current_price: f64,
    recent_prices: &[f64],
    minutes_remaining: usize,
) -> f64 {
    estimate_historical_ask_with_params(
        direction,
        window_open,
        current_price,
        recent_prices,
        minutes_remaining,
        &StrategyParameters::default(),
    )
}

pub fn estimate_historical_ask_with_params(
    direction: &Direction,
    window_open: f64,
    current_price: f64,
    recent_prices: &[f64],
    minutes_remaining: usize,
    params: &StrategyParameters,
) -> f64 {
    let returns: Vec<f64> = recent_prices
        .windows(2)
        .map(|window| (window[1] - window[0]) / window[0])
        .collect();
    let volatility = if returns.is_empty() {
        params.historical_default_volatility
    } else {
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / returns.len() as f64;
        variance.sqrt().max(params.historical_minimum_volatility)
    };

    let displacement = (current_price - window_open) / window_open;
    let scale = volatility * (minutes_remaining.max(1) as f64).sqrt();
    let up_probability = (params.historical_probability_base
        + params.historical_probability_displacement_scale
            * (displacement / scale).clamp(
                -params.historical_probability_z_cap,
                params.historical_probability_z_cap,
            ))
    .clamp(
        params.historical_probability_floor,
        params.historical_probability_cap,
    );
    let side_probability = match direction {
        Direction::Up => up_probability,
        Direction::Down => 1.0 - up_probability,
    };

    (side_probability + params.historical_ask_premium)
        .clamp(params.historical_ask_floor, params.historical_ask_cap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predicts_consistent_upward_momentum() {
        let prices: Vec<f64> = (0..20).map(|index| 100.0 + index as f64 * 0.2).collect();
        let signal = predict_early_window(&prices).expect("expected signal");
        assert_eq!(signal.direction, Direction::Up);
        assert!(signal.confidence > 0.5);
    }

    #[test]
    fn skips_conflicting_momentum() {
        let prices = vec![
            100.0, 100.2, 100.4, 100.6, 100.8, 101.0, 101.2, 101.4, 101.6, 101.8, 102.0, 101.0,
        ];
        assert!(predict_early_window(&prices).is_none());
    }

    #[test]
    fn predicts_five_minute_window_from_short_context() {
        let prices = vec![100.0, 100.1, 100.2, 100.3, 100.4, 100.6];
        let signal = predict_window(&prices, 5).expect("expected 5m signal");
        assert_eq!(signal.direction, Direction::Up);
    }

    #[test]
    fn predicts_five_minute_continuation_in_strong_regime() {
        let mut prices: Vec<f64> = (0..30).map(|index| 100.0 + index as f64 * 0.02).collect();
        let window_open = *prices.last().expect("price");
        prices.push(window_open + 0.35);
        let signal =
            predict_five_minute_continuation(&prices, window_open).expect("expected continuation");
        assert_eq!(signal.direction, Direction::Up);
    }

    #[test]
    fn default_parameters_match_public_prediction_wrappers() {
        let params = StrategyParameters::default();
        let mut prices: Vec<f64> = (0..30).map(|index| 100.0 + index as f64 * 0.02).collect();
        let window_open = *prices.last().expect("price");
        prices.push(window_open + 0.35);

        let wrapped = predict_five_minute_continuation(&prices, window_open).unwrap();
        let explicit =
            predict_five_minute_continuation_with_params(&prices, window_open, &params).unwrap();
        assert_eq!(wrapped.direction, explicit.direction);
        assert_eq!(wrapped.confidence, explicit.confidence);
        assert_eq!(
            diagnose_five_minute_continuation(&prices, window_open),
            diagnose_five_minute_continuation_with_params(&prices, window_open, &params)
        );

        let ask = estimate_historical_ask(&Direction::Up, 100.0, 100.2, &prices, 4);
        let explicit_ask =
            estimate_historical_ask_with_params(&Direction::Up, 100.0, 100.2, &prices, 4, &params);
        assert_eq!(ask, explicit_ask);
    }

    #[test]
    fn strategy_manifest_exposes_current_thresholds() {
        let params = StrategyParameters::default();
        assert_eq!(params.fifteen_minute_strength_threshold, 1.50);
        assert_eq!(params.fifteen_minute_fast_opening_z_threshold, 2.00);
        assert_eq!(params.five_minute_opening_z_threshold, 1.50);
        assert_eq!(params.continuation_confidence_cap, 0.79);
        assert_eq!(params.historical_ask_premium, 0.015);
    }

    #[test]
    fn predicts_fast_fifteen_minute_breakout() {
        let mut prices: Vec<f64> = (0..30).map(|index| 100.0 + index as f64 * 0.02).collect();
        let window_open = *prices.last().expect("price");
        prices.push(window_open + 0.28);
        let signal = predict_fifteen_minute_latency_breakout(&prices, window_open)
            .expect("expected fast breakout");
        assert_eq!(signal.direction, Direction::Up);
        assert!(signal.confidence >= 0.70);
    }

    #[test]
    fn diagnoses_weak_five_minute_opening_move() {
        let mut prices: Vec<f64> = (0..30).map(|index| 100.0 + index as f64 * 0.02).collect();
        let window_open = *prices.last().expect("price");
        prices.push(window_open + 0.001);
        assert!(diagnose_five_minute_continuation(&prices, window_open).contains("below 1.50z"));
    }
}
