use crate::crypto::binance_ws::Candle;
use crate::crypto::strategy::{
    diagnose_five_minute_continuation, predict_early_window,
    predict_fifteen_minute_latency_breakout, predict_five_minute_continuation,
};
use crate::web::state::SignalInfo;

#[derive(Debug, Clone)]
pub struct StrategyEvaluation {
    pub signal: SignalInfo,
    pub window_open: Option<f64>,
}

pub fn evaluate(
    timeframe: &str,
    candles: &[Candle],
    now_ms: i64,
    window_start_ts: i64,
) -> StrategyEvaluation {
    let elapsed = now_ms / 1_000 - window_start_ts;
    let window_index = candles
        .iter()
        .position(|candle| candle.timestamp == window_start_ts * 1_000);

    match timeframe {
        "15m" => evaluate_15m(candles, window_index, elapsed, now_ms, window_start_ts),
        "5m" => evaluate_5m(candles, window_index, elapsed, now_ms, window_start_ts),
        _ => waiting(
            timeframe,
            "Unsupported strategy timeframe",
            now_ms,
            window_start_ts,
            None,
        ),
    }
}

fn evaluate_15m(
    candles: &[Candle],
    window_index: Option<usize>,
    elapsed: i64,
    now_ms: i64,
    window_start_ts: i64,
) -> StrategyEvaluation {
    let Some(index) = window_index else {
        return waiting(
            "15m",
            "Active window candle unavailable",
            now_ms,
            window_start_ts,
            None,
        );
    };
    let window_open = Some(candles[index].open);
    if elapsed > 600 {
        return waiting(
            "15m",
            &format!("Entry window expired; elapsed {elapsed}s"),
            now_ms,
            window_start_ts,
            window_open,
        );
    }
    if elapsed < 60 || index >= candles.len() {
        return waiting(
            "15m",
            &format!("Waiting for first minute close; elapsed {elapsed}s"),
            now_ms,
            window_start_ts,
            window_open,
        );
    }

    let completed_minutes = (elapsed / 60).clamp(1, 14) as usize;
    let latest_completed_index = index + completed_minutes - 1;
    if latest_completed_index < candles.len() {
        let history_start = latest_completed_index.saturating_sub(60);
        let prices: Vec<f64> = candles[history_start..=latest_completed_index]
            .iter()
            .map(|candle| candle.close)
            .collect();
        if elapsed < 180 {
            return match predict_fifteen_minute_latency_breakout(&prices, candles[index].open) {
                Some(signal) => StrategyEvaluation {
                    signal: SignalInfo {
                        direction: signal.direction.to_string(),
                        confidence: signal.confidence,
                        timeframe: "15m".to_string(),
                        reason: format!(
                            "fast minute-{} model | {}",
                            completed_minutes, signal.reason
                        ),
                        timestamp: now_ms,
                        window_start_ts,
                    },
                    window_open,
                },
                None => waiting(
                    "15m",
                    "Fast model found no latency-safe breakout",
                    now_ms,
                    window_start_ts,
                    window_open,
                ),
            };
        }
    }

    if index + 2 >= candles.len() {
        return waiting(
            "15m",
            &format!("Waiting for minute-3 close; elapsed {elapsed}s"),
            now_ms,
            window_start_ts,
            window_open,
        );
    }

    let entry_index = index + 2;
    let history_start = entry_index.saturating_sub(60);
    let prices: Vec<f64> = candles[history_start..=entry_index]
        .iter()
        .map(|candle| candle.close)
        .collect();
    match predict_early_window(&prices) {
        Some(signal) => StrategyEvaluation {
            signal: SignalInfo {
                direction: signal.direction.to_string(),
                confidence: signal.confidence,
                timeframe: "15m".to_string(),
                reason: format!("fixed minute-3 model | {}", signal.reason),
                timestamp: now_ms,
                window_start_ts,
            },
            window_open,
        },
        None => waiting(
            "15m",
            "Minute-3 model found no aligned momentum setup",
            now_ms,
            window_start_ts,
            window_open,
        ),
    }
}

fn evaluate_5m(
    candles: &[Candle],
    window_index: Option<usize>,
    elapsed: i64,
    now_ms: i64,
    window_start_ts: i64,
) -> StrategyEvaluation {
    let Some(index) = window_index else {
        return waiting(
            "5m",
            "Active window candle unavailable",
            now_ms,
            window_start_ts,
            None,
        );
    };
    let window_open = Some(candles[index].open);
    if elapsed < 60 || index == 0 {
        return waiting(
            "5m",
            &format!("Waiting for first minute close; elapsed {elapsed}s"),
            now_ms,
            window_start_ts,
            window_open,
        );
    }

    let prices: Vec<f64> = candles[..=index]
        .iter()
        .map(|candle| candle.close)
        .collect();
    match predict_five_minute_continuation(&prices, candles[index].open) {
        Some(signal) => StrategyEvaluation {
            signal: SignalInfo {
                direction: signal.direction.to_string(),
                confidence: signal.confidence,
                timeframe: "5m".to_string(),
                reason: signal.reason,
                timestamp: now_ms,
                window_start_ts,
            },
            window_open,
        },
        None => waiting(
            "5m",
            &diagnose_five_minute_continuation(&prices, candles[index].open),
            now_ms,
            window_start_ts,
            window_open,
        ),
    }
}

fn waiting(
    timeframe: &str,
    reason: &str,
    now_ms: i64,
    window_start_ts: i64,
    window_open: Option<f64>,
) -> StrategyEvaluation {
    StrategyEvaluation {
        signal: SignalInfo {
            direction: "WAIT".to_string(),
            confidence: 0.0,
            timeframe: timeframe.to_string(),
            reason: reason.to_string(),
            timestamp: now_ms,
            window_start_ts,
        },
        window_open,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candle(timestamp: i64, open: f64, close: f64) -> Candle {
        Candle {
            timestamp,
            open,
            high: open.max(close),
            low: open.min(close),
            close,
            volume: 1.0,
        }
    }

    #[test]
    fn service_routes_fast_fifteen_minute_model_after_first_close() {
        let start = 1_800;
        let mut candles: Vec<Candle> = (0..40)
            .map(|index| {
                candle(
                    (start - 30 + index) * 1_000,
                    100.0,
                    100.0 + index as f64 * 0.02,
                )
            })
            .collect();
        candles[30] = candle(start * 1_000, 100.58, 100.95);

        let five = evaluate("5m", &candles, (start + 60) * 1_000, start);
        let fifteen = evaluate("15m", &candles, (start + 60) * 1_000, start);

        assert_eq!(five.signal.timeframe, "5m");
        assert_eq!(fifteen.signal.timeframe, "15m");
        assert_eq!(fifteen.signal.direction, "Up");
        assert!(fifteen.signal.reason.contains("fast minute-1 model"));
    }
}
