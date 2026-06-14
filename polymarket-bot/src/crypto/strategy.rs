use crate::crypto::signals::Direction;

const FIVE_MINUTE_OPENING_Z_THRESHOLD: f64 = 0.90;

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
    let volatility = variance.sqrt().max(0.000_05);

    let strength =
        (short_momentum.abs() + medium_momentum.abs()) / (volatility * strength_divisor);
    if strength < 0.55 {
        return None;
    }

    let confidence = (0.52 + strength.min(2.0) * 0.08).min(0.68);
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
    let volatility = variance.sqrt().max(0.000_05);
    let opening_z = displacement / volatility;

    let prior_short = (prior[29] - prior[24]) / prior[24];
    if opening_z.abs() < FIVE_MINUTE_OPENING_Z_THRESHOLD
        || prior_short.signum() != displacement.signum()
    {
        return None;
    }

    let confidence =
        (0.72 + (opening_z.abs() - FIVE_MINUTE_OPENING_Z_THRESHOLD).min(2.0) * 0.035).min(0.79);
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
    let volatility = variance.sqrt().max(0.000_05);
    let opening_z = displacement / volatility;
    let prior_short = (prior[29] - prior[24]) / prior[24];
    let aligned = prior_short.signum() == displacement.signum();

    if opening_z.abs() < FIVE_MINUTE_OPENING_Z_THRESHOLD {
        format!(
            "WAIT: opening move {:.2}z below {:.2}z; prior trend {:.3}%",
            opening_z.abs(),
            FIVE_MINUTE_OPENING_Z_THRESHOLD,
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
    let returns: Vec<f64> = recent_prices
        .windows(2)
        .map(|window| (window[1] - window[0]) / window[0])
        .collect();
    let volatility = if returns.is_empty() {
        0.000_5
    } else {
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let variance = returns
            .iter()
            .map(|value| (value - mean).powi(2))
            .sum::<f64>()
            / returns.len() as f64;
        variance.sqrt().max(0.000_1)
    };

    let displacement = (current_price - window_open) / window_open;
    let scale = volatility * (minutes_remaining.max(1) as f64).sqrt();
    let up_probability = (0.5 + 0.18 * (displacement / scale).clamp(-2.0, 2.0))
        .clamp(0.12, 0.88);
    let side_probability = match direction {
        Direction::Up => up_probability,
        Direction::Down => 1.0 - up_probability,
    };

    (side_probability + 0.015).clamp(0.05, 0.95)
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
            100.0, 100.2, 100.4, 100.6, 100.8, 101.0, 101.2, 101.4, 101.6, 101.8, 102.0,
            101.0,
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
        let mut prices: Vec<f64> = (0..30)
            .map(|index| 100.0 + index as f64 * 0.02)
            .collect();
        let window_open = *prices.last().expect("price");
        prices.push(window_open + 0.35);
        let signal =
            predict_five_minute_continuation(&prices, window_open).expect("expected continuation");
        assert_eq!(signal.direction, Direction::Up);
    }

    #[test]
    fn diagnoses_weak_five_minute_opening_move() {
        let mut prices: Vec<f64> = (0..30)
            .map(|index| 100.0 + index as f64 * 0.02)
            .collect();
        let window_open = *prices.last().expect("price");
        prices.push(window_open + 0.001);
        assert!(diagnose_five_minute_continuation(&prices, window_open).contains("below 0.90z"));
    }
}
