use crate::crypto::live::gamma_client::BuyQuote;
use crate::engine::risk::Direction;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryLayer {
    Early,
    Confirmation,
    LateMakerOnly,
}

impl EntryLayer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Early => "early",
            Self::Confirmation => "confirmation",
            Self::LateMakerOnly => "late_maker_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingDecision {
    pub layer: Option<EntryLayer>,
    pub entry_start_secs: i64,
    pub entry_end_secs: i64,
    pub taker_allowed: bool,
    pub maker_allowed: bool,
    pub reason_code: &'static str,
}

pub fn timing_for(timeframe: &str, elapsed_secs: i64) -> TimingDecision {
    match timeframe {
        "5m" if elapsed_secs < 20 => waiting(20, 150),
        "5m" if elapsed_secs <= 90 => active(EntryLayer::Early, 20, 150, true, true),
        "5m" if elapsed_secs <= 150 => active(EntryLayer::Confirmation, 20, 150, true, true),
        "5m" => expired(20, 150),
        "15m" if elapsed_secs < 20 => waiting(20, 570),
        "15m" if elapsed_secs <= 180 => active(EntryLayer::Early, 20, 570, true, true),
        "15m" if elapsed_secs <= 420 => active(EntryLayer::Confirmation, 20, 570, true, true),
        "15m" if elapsed_secs <= 570 => active(EntryLayer::LateMakerOnly, 20, 570, false, true),
        "15m" => expired(20, 570),
        _ => expired(0, 0),
    }
}

fn waiting(start: i64, end: i64) -> TimingDecision {
    TimingDecision {
        layer: None,
        entry_start_secs: start,
        entry_end_secs: end,
        taker_allowed: false,
        maker_allowed: false,
        reason_code: "entry_not_open",
    }
}

fn active(
    layer: EntryLayer,
    start: i64,
    end: i64,
    taker_allowed: bool,
    maker_allowed: bool,
) -> TimingDecision {
    TimingDecision {
        layer: Some(layer),
        entry_start_secs: start,
        entry_end_secs: end,
        taker_allowed,
        maker_allowed,
        reason_code: "entry_open",
    }
}

fn expired(start: i64, end: i64) -> TimingDecision {
    TimingDecision {
        layer: None,
        entry_start_secs: start,
        entry_end_secs: end,
        taker_allowed: false,
        maker_allowed: false,
        reason_code: "entry_deadline",
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProbabilityInput {
    pub current_price: f64,
    pub price_to_beat: f64,
    pub drift_per_second: f64,
    pub realized_vol_per_sqrt_second: f64,
    pub tau_seconds: f64,
    pub momentum: f64,
    pub book_imbalance: f64,
    pub spread: f64,
    pub latency_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProbabilityEstimate {
    pub model_probability_up: f64,
    pub adjusted_probability_up: f64,
}

pub fn estimate_probability(input: ProbabilityInput) -> ProbabilityEstimate {
    let tau = input.tau_seconds.max(1.0);
    let vol = input.realized_vol_per_sqrt_second.max(0.000_000_1);
    let z = (input.current_price - input.price_to_beat + input.drift_per_second * tau)
        / (vol * tau.sqrt());
    let model = normal_cdf(z);
    let adjustment = input.momentum.clamp(-0.03, 0.03)
        + (0.035 * input.book_imbalance.clamp(-1.0, 1.0))
        - input.spread.max(0.0).min(0.20) * 0.25
        - ((input.latency_ms.max(0) as f64) / 1_000.0 * 0.002).min(0.04);
    ProbabilityEstimate {
        model_probability_up: model,
        adjusted_probability_up: (model + adjustment).clamp(0.01, 0.99),
    }
}

fn normal_cdf(z: f64) -> f64 {
    0.5 * (1.0 + erf(z / std::f64::consts::SQRT_2))
}

fn erf(x: f64) -> f64 {
    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let x = x.abs();
    let t = 1.0 / (1.0 + 0.327_591_1 * x);
    let y = 1.0
        - (((((1.061_405_429 * t - 1.453_152_027) * t) + 1.421_413_741) * t - 0.284_496_736) * t
            + 0.254_829_592)
            * t
            * (-x * x).exp();
    sign * y
}

#[derive(Debug, Clone, PartialEq)]
pub enum QuoteDecision {
    Taker {
        price: f64,
        shares: f64,
        depth_usd: f64,
        edge: f64,
    },
    Maker {
        bid_price: f64,
        time_in_force_ms: u64,
        edge: f64,
        reason_code: &'static str,
    },
    Reject {
        reason_code: &'static str,
    },
}

#[derive(Debug, Clone)]
pub struct QuoteInput {
    pub direction: Direction,
    pub probability_up: f64,
    pub up_quote: Option<BuyQuote>,
    pub down_quote: Option<BuyQuote>,
    pub up_best_bid: Option<f64>,
    pub down_best_bid: Option<f64>,
    pub min_edge: f64,
    pub requested_usd: f64,
    pub tick_size: f64,
    pub timing: TimingDecision,
    pub maker_time_in_force_ms: u64,
}

pub fn executable_quote(input: QuoteInput) -> QuoteDecision {
    let probability = match input.direction {
        Direction::Up => input.probability_up,
        Direction::Down => 1.0 - input.probability_up,
    };
    let target_quote = match input.direction {
        Direction::Up => input.up_quote,
        Direction::Down => input.down_quote,
    };

    if input.timing.reason_code == "entry_deadline" {
        return QuoteDecision::Reject {
            reason_code: "entry_deadline",
        };
    }

    if input.timing.taker_allowed {
        if let Some(quote) = target_quote {
            let edge = probability - quote.average_price;
            if edge >= input.min_edge && quote.available_depth_usd >= input.requested_usd {
                return QuoteDecision::Taker {
                    price: quote.average_price,
                    shares: quote.shares,
                    depth_usd: quote.available_depth_usd,
                    edge,
                };
            }
        } else if !input.timing.maker_allowed {
            return QuoteDecision::Reject {
                reason_code: "no_executable_ask",
            };
        }
    }

    if !input.timing.maker_allowed {
        return QuoteDecision::Reject {
            reason_code: "no_executable_ask",
        };
    }

    let target_bid = match input.direction {
        Direction::Up => input.up_best_bid,
        Direction::Down => input.down_best_bid,
    };
    let base_bid = target_bid.unwrap_or(0.01).max(0.01);
    let bid_price =
        (base_bid + input.tick_size.max(0.01)).min((probability - input.min_edge).max(0.01));
    let edge = probability - bid_price;
    if edge >= input.min_edge {
        QuoteDecision::Maker {
            bid_price,
            time_in_force_ms: input.maker_time_in_force_ms,
            edge,
            reason_code: "maker_fallback",
        }
    } else {
        QuoteDecision::Reject {
            reason_code: if target_quote.is_none() {
                "no_executable_ask"
            } else {
                "edge_below_threshold"
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(price: f64, depth: f64) -> BuyQuote {
        BuyQuote {
            average_price: price,
            shares: depth / price,
            available_depth_usd: depth,
        }
    }

    #[test]
    fn no_new_entry_after_deadline() {
        assert_eq!(timing_for("5m", 151).reason_code, "entry_deadline");
        assert_eq!(timing_for("15m", 571).reason_code, "entry_deadline");
    }

    #[test]
    fn taker_buy_only_uses_target_token_asks() {
        let decision = executable_quote(QuoteInput {
            direction: Direction::Down,
            probability_up: 0.20,
            up_quote: Some(quote(0.10, 10.0)),
            down_quote: Some(quote(0.45, 10.0)),
            up_best_bid: Some(0.09),
            down_best_bid: Some(0.44),
            min_edge: 0.10,
            requested_usd: 1.0,
            tick_size: 0.01,
            timing: timing_for("5m", 50),
            maker_time_in_force_ms: 1_500,
        });
        assert!(
            matches!(decision, QuoteDecision::Taker { price, .. } if (price - 0.45).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn one_sided_down_book_rejects_down_taker() {
        let decision = executable_quote(QuoteInput {
            direction: Direction::Down,
            probability_up: 0.20,
            up_quote: Some(quote(0.10, 10.0)),
            down_quote: None,
            up_best_bid: Some(0.09),
            down_best_bid: None,
            min_edge: 0.10,
            requested_usd: 1.0,
            tick_size: 0.01,
            timing: TimingDecision {
                maker_allowed: false,
                ..timing_for("5m", 50)
            },
            maker_time_in_force_ms: 1_500,
        });
        assert_eq!(
            decision,
            QuoteDecision::Reject {
                reason_code: "no_executable_ask"
            }
        );
    }

    #[test]
    fn maker_fallback_may_place_bid_when_edge_and_time_allow() {
        let decision = executable_quote(QuoteInput {
            direction: Direction::Up,
            probability_up: 0.80,
            up_quote: None,
            down_quote: Some(quote(0.60, 10.0)),
            up_best_bid: Some(0.55),
            down_best_bid: Some(0.59),
            min_edge: 0.10,
            requested_usd: 1.0,
            tick_size: 0.01,
            timing: timing_for("15m", 450),
            maker_time_in_force_ms: 1_500,
        });
        assert!(matches!(
            decision,
            QuoteDecision::Maker {
                reason_code: "maker_fallback",
                ..
            }
        ));
    }
}
