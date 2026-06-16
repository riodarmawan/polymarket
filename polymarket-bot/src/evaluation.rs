use crate::web::state::TradeInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardOpportunity {
    pub market_slug: String,
    pub timeframe: String,
    pub direction: String,
    pub confidence: f64,
    pub expected_fill_price: Option<f64>,
    pub spread: f64,
    pub fee_rate_bps: Option<u64>,
    pub approved: bool,
    pub reason_code: String,
    pub captured_at_ms: i64,
    pub end_ts: i64,
    pub official_outcome: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SegmentMetrics {
    pub opportunities: usize,
    pub approved: usize,
    pub settled: usize,
    pub wins: usize,
    pub pnl_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardReport {
    pub generated_at_ms: i64,
    pub opportunities: usize,
    pub approved: usize,
    pub settled_trades: usize,
    pub raw_direction_accuracy: f64,
    pub executable_trade_accuracy: f64,
    pub fill_ratio: f64,
    pub total_pnl_usd: f64,
    pub profit_factor: f64,
    pub max_drawdown_usd: f64,
    pub brier_score: Option<f64>,
    pub settlement_mismatches: usize,
    pub settlement_mismatch_rate: f64,
    pub promotion_ready: bool,
    pub promotion_reasons: Vec<String>,
    pub by_timeframe: BTreeMap<String, SegmentMetrics>,
    pub by_date_utc: BTreeMap<String, SegmentMetrics>,
    pub by_hour_utc: BTreeMap<String, SegmentMetrics>,
    pub by_ask_band: BTreeMap<String, SegmentMetrics>,
    pub by_spread_band: BTreeMap<String, SegmentMetrics>,
    pub by_regime: BTreeMap<String, SegmentMetrics>,
    pub rejection_reasons: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForwardMonitorStatus {
    Collecting,
    PromotionReady,
    Halted,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForwardMonitorDecision {
    pub status: ForwardMonitorStatus,
    pub should_halt_runtime: bool,
    pub should_open_incident: bool,
    pub reasons: Vec<String>,
}

pub fn monitor_decision(report: &ForwardReport) -> ForwardMonitorDecision {
    let mut halt_reasons = Vec::new();
    if report.settlement_mismatches > 0 {
        halt_reasons.push(format!(
            "{} settlement mismatch(es) detected",
            report.settlement_mismatches
        ));
    }
    if report.max_drawdown_usd > 0.40 {
        halt_reasons.push(format!(
            "drawdown ${:.2} exceeds the $0.40 limit for $2 capital",
            report.max_drawdown_usd
        ));
    }

    if !halt_reasons.is_empty() {
        return ForwardMonitorDecision {
            status: ForwardMonitorStatus::Halted,
            should_halt_runtime: true,
            should_open_incident: true,
            reasons: halt_reasons,
        };
    }

    if report.promotion_ready {
        ForwardMonitorDecision {
            status: ForwardMonitorStatus::PromotionReady,
            should_halt_runtime: false,
            should_open_incident: false,
            reasons: vec!["all forward-test promotion gates passed".to_string()],
        }
    } else {
        ForwardMonitorDecision {
            status: ForwardMonitorStatus::Collecting,
            should_halt_runtime: false,
            should_open_incident: false,
            reasons: report.promotion_reasons.clone(),
        }
    }
}

pub fn build_report(opportunities: &[ForwardOpportunity], trades: &[TradeInfo]) -> ForwardReport {
    let settled_trades: Vec<&TradeInfo> = trades
        .iter()
        .filter(|trade| {
            trade.status == "settled"
                && trade.pnl.is_some()
                && opportunities
                    .iter()
                    .any(|item| item.market_slug == trade.market_slug)
        })
        .collect();
    let matched_trades = trades
        .iter()
        .filter(|trade| {
            opportunities
                .iter()
                .any(|item| item.market_slug == trade.market_slug)
        })
        .count();
    let approved = opportunities.iter().filter(|item| item.approved).count();
    let settled_opportunities: Vec<&ForwardOpportunity> = opportunities
        .iter()
        .filter(|item| item.official_outcome.is_some())
        .collect();
    let raw_wins = settled_opportunities
        .iter()
        .filter(|item| item.official_outcome.as_deref() == Some(item.direction.as_str()))
        .count();
    let trade_wins = settled_trades
        .iter()
        .filter(|trade| trade.pnl.unwrap_or(0.0) > 0.0)
        .count();
    let gross_profit: f64 = settled_trades
        .iter()
        .filter_map(|trade| trade.pnl)
        .filter(|pnl| *pnl > 0.0)
        .sum();
    let gross_loss: f64 = settled_trades
        .iter()
        .filter_map(|trade| trade.pnl)
        .filter(|pnl| *pnl < 0.0)
        .map(f64::abs)
        .sum();
    let total_pnl_usd: f64 = settled_trades.iter().filter_map(|trade| trade.pnl).sum();
    let mut equity: f64 = 0.0;
    let mut peak: f64 = 0.0;
    let mut max_drawdown_usd: f64 = 0.0;
    for trade in &settled_trades {
        equity += trade.pnl.unwrap_or(0.0);
        peak = peak.max(equity);
        max_drawdown_usd = max_drawdown_usd.max(peak - equity);
    }
    let brier_score = if settled_opportunities.is_empty() {
        None
    } else {
        Some(
            settled_opportunities
                .iter()
                .map(|item| {
                    let outcome =
                        if item.official_outcome.as_deref() == Some(item.direction.as_str()) {
                            1.0
                        } else {
                            0.0
                        };
                    (item.confidence - outcome).powi(2)
                })
                .sum::<f64>()
                / settled_opportunities.len() as f64,
        )
    };
    let settlement_mismatches = settled_trades
        .iter()
        .filter(|trade| {
            opportunities
                .iter()
                .find(|item| item.market_slug == trade.market_slug)
                .and_then(|item| item.official_outcome.as_deref())
                .is_some_and(|outcome| {
                    (trade.exit_price == Some(1.0)) != (trade.direction == outcome)
                })
        })
        .count();

    let mut by_timeframe = BTreeMap::new();
    let mut by_date_utc = BTreeMap::new();
    let mut by_hour_utc = BTreeMap::new();
    let mut by_ask_band = BTreeMap::new();
    let mut by_spread_band = BTreeMap::new();
    let mut by_regime = BTreeMap::new();
    let mut rejection_reasons = BTreeMap::new();
    for item in opportunities {
        add_segment(&mut by_timeframe, item.timeframe.clone(), item, trades);
        let captured = chrono::DateTime::from_timestamp_millis(item.captured_at_ms);
        let date = captured
            .map(|date| date.format("%Y-%m-%d").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        add_segment(&mut by_date_utc, date, item, trades);
        let hour = captured
            .map(|date| date.format("%H").to_string())
            .unwrap_or_else(|| "unknown".to_string());
        add_segment(&mut by_hour_utc, hour, item, trades);
        add_segment(
            &mut by_ask_band,
            ask_band(item.expected_fill_price),
            item,
            trades,
        );
        add_segment(&mut by_spread_band, spread_band(item.spread), item, trades);
        add_segment(
            &mut by_regime,
            format!(
                "{}_{}",
                item.timeframe,
                if item.confidence >= 0.75 {
                    "high_confidence"
                } else {
                    "standard"
                }
            ),
            item,
            trades,
        );
        if !item.approved {
            *rejection_reasons
                .entry(item.reason_code.clone())
                .or_insert(0) += 1;
        }
    }

    let profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else if gross_profit > 0.0 {
        999.0
    } else {
        0.0
    };
    let executable_trade_accuracy = ratio(trade_wins, settled_trades.len());
    let mut promotion_reasons = Vec::new();
    if settled_trades.len() < 200 {
        promotion_reasons.push(format!(
            "need 200 settled trades; have {}",
            settled_trades.len()
        ));
    }
    if executable_trade_accuracy < 0.68 {
        promotion_reasons.push(format!(
            "win rate {:.1}% is below 68%",
            executable_trade_accuracy * 100.0
        ));
    }
    if profit_factor < 1.40 {
        promotion_reasons.push(format!("profit factor {:.2} is below 1.40", profit_factor));
    }
    if max_drawdown_usd > 0.40 {
        promotion_reasons.push(format!(
            "drawdown ${:.2} exceeds the $0.40 limit for $2 capital",
            max_drawdown_usd
        ));
    }
    if settlement_mismatches > 0 {
        promotion_reasons.push(format!("{settlement_mismatches} settlement mismatch(es)"));
    }

    ForwardReport {
        generated_at_ms: chrono::Utc::now().timestamp_millis(),
        opportunities: opportunities.len(),
        approved,
        settled_trades: settled_trades.len(),
        raw_direction_accuracy: ratio(raw_wins, settled_opportunities.len()),
        executable_trade_accuracy,
        fill_ratio: ratio(matched_trades, approved),
        total_pnl_usd,
        profit_factor,
        max_drawdown_usd,
        brier_score,
        settlement_mismatches,
        settlement_mismatch_rate: ratio(settlement_mismatches, settled_trades.len()),
        promotion_ready: promotion_reasons.is_empty(),
        promotion_reasons,
        by_timeframe,
        by_date_utc,
        by_hour_utc,
        by_ask_band,
        by_spread_band,
        by_regime,
        rejection_reasons,
    }
}

fn add_segment(
    groups: &mut BTreeMap<String, SegmentMetrics>,
    key: String,
    item: &ForwardOpportunity,
    trades: &[TradeInfo],
) {
    let segment = groups.entry(key).or_default();
    segment.opportunities += 1;
    segment.approved += usize::from(item.approved);
    if let Some(trade) = trades.iter().find(|trade| {
        trade.market_slug == item.market_slug && trade.status == "settled" && trade.pnl.is_some()
    }) {
        segment.settled += 1;
        segment.wins += usize::from(trade.pnl.unwrap_or(0.0) > 0.0);
        segment.pnl_usd += trade.pnl.unwrap_or(0.0);
    }
}

fn ask_band(ask: Option<f64>) -> String {
    match ask {
        Some(value) if value < 0.30 => "<0.30",
        Some(value) if value < 0.50 => "0.30-0.49",
        Some(value) if value < 0.65 => "0.50-0.64",
        Some(_) => ">=0.65",
        None => "unavailable",
    }
    .to_string()
}

fn spread_band(spread: f64) -> String {
    if spread <= 0.01 {
        "<=1%"
    } else if spread <= 0.02 {
        "1-2%"
    } else if spread <= 0.04 {
        "2-4%"
    } else {
        ">4%"
    }
    .to_string()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_uses_official_outcomes_and_executable_trades_separately() {
        let opportunities = vec![
            ForwardOpportunity {
                market_slug: "a".to_string(),
                timeframe: "5m".to_string(),
                direction: "Up".to_string(),
                confidence: 0.75,
                expected_fill_price: Some(0.50),
                spread: 0.01,
                fee_rate_bps: Some(200),
                approved: true,
                reason_code: "approved".to_string(),
                captured_at_ms: 0,
                end_ts: 1,
                official_outcome: Some("Up".to_string()),
            },
            ForwardOpportunity {
                market_slug: "b".to_string(),
                timeframe: "15m".to_string(),
                direction: "Down".to_string(),
                confidence: 0.70,
                expected_fill_price: Some(0.55),
                spread: 0.05,
                fee_rate_bps: Some(200),
                approved: false,
                reason_code: "spread".to_string(),
                captured_at_ms: 0,
                end_ts: 1,
                official_outcome: Some("Up".to_string()),
            },
        ];
        let trades = vec![TradeInfo {
            timestamp: 0,
            market_slug: "a".to_string(),
            timeframe: "5m".to_string(),
            direction: "Up".to_string(),
            entry_price: 0.50,
            exit_price: Some(1.0),
            shares: 0.20,
            size_usd: 0.10,
            fee_usd: 0.0,
            price_to_beat: 1.0,
            end_ts: 1,
            confidence: 0.75,
            edge: 0.25,
            pnl: Some(0.10),
            status: "settled".to_string(),
        }];

        let report = build_report(&opportunities, &trades);
        assert_eq!(report.raw_direction_accuracy, 0.5);
        assert_eq!(report.executable_trade_accuracy, 1.0);
        assert_eq!(report.fill_ratio, 1.0);
        assert_eq!(report.rejection_reasons.get("spread"), Some(&1));
    }

    #[test]
    fn monitor_collects_until_promotion_gates_pass() {
        let report = ForwardReport {
            generated_at_ms: 0,
            opportunities: 20,
            approved: 10,
            settled_trades: 10,
            raw_direction_accuracy: 0.7,
            executable_trade_accuracy: 0.7,
            fill_ratio: 1.0,
            total_pnl_usd: 0.2,
            profit_factor: 1.5,
            max_drawdown_usd: 0.2,
            brier_score: Some(0.2),
            settlement_mismatches: 0,
            settlement_mismatch_rate: 0.0,
            promotion_ready: false,
            promotion_reasons: vec!["need 200 settled trades; have 10".to_string()],
            by_timeframe: BTreeMap::new(),
            by_date_utc: BTreeMap::new(),
            by_hour_utc: BTreeMap::new(),
            by_ask_band: BTreeMap::new(),
            by_spread_band: BTreeMap::new(),
            by_regime: BTreeMap::new(),
            rejection_reasons: BTreeMap::new(),
        };

        let decision = monitor_decision(&report);
        assert_eq!(decision.status, ForwardMonitorStatus::Collecting);
        assert!(!decision.should_halt_runtime);
        assert_eq!(decision.reasons, report.promotion_reasons);
    }

    #[test]
    fn monitor_halts_on_settlement_mismatch_or_drawdown_breach() {
        let report = ForwardReport {
            generated_at_ms: 0,
            opportunities: 250,
            approved: 220,
            settled_trades: 210,
            raw_direction_accuracy: 0.7,
            executable_trade_accuracy: 0.7,
            fill_ratio: 1.0,
            total_pnl_usd: -0.5,
            profit_factor: 1.5,
            max_drawdown_usd: 0.41,
            brier_score: Some(0.2),
            settlement_mismatches: 1,
            settlement_mismatch_rate: 0.01,
            promotion_ready: false,
            promotion_reasons: vec![],
            by_timeframe: BTreeMap::new(),
            by_date_utc: BTreeMap::new(),
            by_hour_utc: BTreeMap::new(),
            by_ask_band: BTreeMap::new(),
            by_spread_band: BTreeMap::new(),
            by_regime: BTreeMap::new(),
            rejection_reasons: BTreeMap::new(),
        };

        let decision = monitor_decision(&report);
        assert_eq!(decision.status, ForwardMonitorStatus::Halted);
        assert!(decision.should_halt_runtime);
        assert!(decision.should_open_incident);
        assert_eq!(decision.reasons.len(), 2);
    }

    #[test]
    fn monitor_marks_promotion_ready_only_after_all_gates_pass() {
        let report = ForwardReport {
            generated_at_ms: 0,
            opportunities: 260,
            approved: 230,
            settled_trades: 220,
            raw_direction_accuracy: 0.72,
            executable_trade_accuracy: 0.69,
            fill_ratio: 0.9,
            total_pnl_usd: 1.2,
            profit_factor: 1.6,
            max_drawdown_usd: 0.2,
            brier_score: Some(0.18),
            settlement_mismatches: 0,
            settlement_mismatch_rate: 0.0,
            promotion_ready: true,
            promotion_reasons: vec![],
            by_timeframe: BTreeMap::new(),
            by_date_utc: BTreeMap::new(),
            by_hour_utc: BTreeMap::new(),
            by_ask_band: BTreeMap::new(),
            by_spread_band: BTreeMap::new(),
            by_regime: BTreeMap::new(),
            rejection_reasons: BTreeMap::new(),
        };

        let decision = monitor_decision(&report);
        assert_eq!(decision.status, ForwardMonitorStatus::PromotionReady);
        assert!(!decision.should_halt_runtime);
    }
}
