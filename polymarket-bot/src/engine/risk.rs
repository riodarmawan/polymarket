use serde::{Deserialize, Serialize};

const SCALE: f64 = 1_000_000.0;
const CIRCUIT_BREAKER_COOLDOWN_MS: i64 = 90 * 60 * 1_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fixed(i64);

impl Fixed {
    pub fn from_f64(value: f64) -> Option<Self> {
        if value.is_finite() && value >= 0.0 && value <= i64::MAX as f64 / SCALE {
            Some(Self((value * SCALE).round() as i64))
        } else {
            None
        }
    }

    pub fn as_f64(self) -> f64 {
        self.0 as f64 / SCALE
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Up,
    Down,
}

impl Direction {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "Up" => Some(Self::Up),
            "Down" => Some(Self::Down),
            _ => None,
        }
    }

    pub fn as_title(&self) -> &'static str {
        match self {
            Self::Up => "Up",
            Self::Down => "Down",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskCheck {
    pub code: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionIntent {
    pub client_order_key: String,
    pub market_slug: String,
    pub token_id: String,
    pub timeframe: String,
    pub direction: Direction,
    pub strategy_version: String,
    pub signal_timestamp_ms: i64,
    pub market_snapshot_timestamp_ms: i64,
    pub requested_usd: Fixed,
    pub worst_allowed_price: Fixed,
    pub expected_fill_price: Fixed,
    pub expected_fee_usd: Fixed,
    pub model_margin: Fixed,
    pub expected_shares: Fixed,
    pub risk_checks: Vec<RiskCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskDecision {
    pub approved: bool,
    pub reason_code: String,
    pub checks: Vec<RiskCheck>,
    pub intent: Option<ExecutionIntent>,
}

#[derive(Debug, Clone)]
pub struct RiskPolicy {
    pub trading_enabled: bool,
    pub strategy_enabled: bool,
    pub max_open_positions: usize,
    pub max_daily_orders: usize,
    pub max_daily_realized_loss_usd: f64,
    pub max_drawdown: f64,
    pub max_consecutive_losses: usize,
    pub max_order_usd: f64,
    pub min_balance_reserve_usd: f64,
    pub max_spread: f64,
    pub max_fee_rate_bps: u64,
    pub max_data_age_ms: i64,
}

#[derive(Debug, Clone)]
pub struct RiskRequest {
    pub now_ms: i64,
    pub market_slug: String,
    pub token_id: Option<String>,
    pub timeframe: String,
    pub direction: Option<Direction>,
    pub strategy_version: String,
    pub signal_timestamp_ms: i64,
    pub signal_window_start_ts: i64,
    pub market_start_ts: i64,
    pub market_snapshot_timestamp_ms: i64,
    pub data_ready: bool,
    pub price_to_beat: f64,
    pub expected_fill_price: Option<f64>,
    pub min_entry_price: f64,
    pub max_entry_price: f64,
    pub confidence: f64,
    pub min_model_margin: f64,
    pub spread: f64,
    pub executable_depth_usd: f64,
    pub min_order_size_shares: Option<f64>,
    pub fee_rate_bps: Option<u64>,
    pub requested_usd: f64,
    pub fee_usd: f64,
    pub current_capital_usd: f64,
    pub open_positions: usize,
    pub daily_orders: usize,
    pub daily_realized_pnl_usd: f64,
    pub max_drawdown: f64,
    pub consecutive_losses: usize,
    pub last_loss_timestamp_ms: Option<i64>,
    pub market_already_traded: bool,
    pub entry_window_start_secs: i64,
    pub entry_window_end_secs: i64,
}

#[derive(Debug, Clone)]
pub struct RiskEngine {
    policy: RiskPolicy,
}

impl RiskEngine {
    pub fn new(policy: RiskPolicy) -> Self {
        Self { policy }
    }

    pub fn evaluate(&self, request: RiskRequest) -> RiskDecision {
        let mut checks = Vec::new();
        let direction = request.direction.clone();
        let fill = request.expected_fill_price;
        let elapsed_secs = request.now_ms / 1_000 - request.market_start_ts;
        let margin = fill.map(|price| request.confidence - price);
        let shares = fill
            .filter(|price| *price > 0.0)
            .map(|price| request.requested_usd / price);
        let circuit_breaker_active = request.consecutive_losses
            >= self.policy.max_consecutive_losses
            && request
                .last_loss_timestamp_ms
                .map(|timestamp| request.now_ms - timestamp < CIRCUIT_BREAKER_COOLDOWN_MS)
                .unwrap_or(false);

        push_check(
            &mut checks,
            "global_kill_switch",
            self.policy.trading_enabled,
            "global trading switch",
        );
        push_check(
            &mut checks,
            "strategy_kill_switch",
            self.policy.strategy_enabled,
            "timeframe trading switch",
        );
        push_check(
            &mut checks,
            "direction",
            direction.is_some(),
            "signal direction must be Up or Down",
        );
        push_check(
            &mut checks,
            "token_id",
            request.token_id.as_ref().is_some_and(|id| !id.is_empty()),
            "selected outcome token is required",
        );
        push_check(
            &mut checks,
            "market_data",
            request.data_ready,
            "market snapshot must be complete and fresh",
        );
        push_check(
            &mut checks,
            "price_to_beat",
            request.price_to_beat > 0.0,
            "price to beat must be positive",
        );
        push_check(
            &mut checks,
            "signal_window",
            request.signal_window_start_ts == request.market_start_ts,
            "signal and market windows must match",
        );
        push_check(
            &mut checks,
            "signal_freshness",
            request.now_ms - request.signal_timestamp_ms <= 30_000,
            "signal must be at most 30 seconds old",
        );
        push_check(
            &mut checks,
            "snapshot_freshness",
            request.now_ms - request.market_snapshot_timestamp_ms <= self.policy.max_data_age_ms,
            "market snapshot is within configured age",
        );
        push_check(
            &mut checks,
            "entry_window",
            (request.entry_window_start_secs..=request.entry_window_end_secs)
                .contains(&elapsed_secs),
            "elapsed time must be inside entry window",
        );
        push_check(
            &mut checks,
            "spread",
            request.spread <= self.policy.max_spread,
            "spread is within configured maximum",
        );
        push_check(
            &mut checks,
            "entry_price",
            fill.is_some_and(|price| {
                (request.min_entry_price..=request.max_entry_price).contains(&price)
            }),
            "expected fill is inside allowed price band",
        );
        push_check(
            &mut checks,
            "model_margin",
            margin.is_some_and(|value| value >= request.min_model_margin),
            "model margin meets threshold",
        );
        push_check(
            &mut checks,
            "market_unique",
            !request.market_already_traded,
            "market has not already been traded",
        );
        push_check(
            &mut checks,
            "open_positions",
            request.open_positions < self.policy.max_open_positions,
            "global open-position limit",
        );
        push_check(
            &mut checks,
            "daily_orders",
            request.daily_orders < self.policy.max_daily_orders,
            "global daily-order limit",
        );
        push_check(
            &mut checks,
            "daily_loss",
            request.daily_realized_pnl_usd > -self.policy.max_daily_realized_loss_usd,
            "global daily realized-loss limit",
        );
        push_check(
            &mut checks,
            "drawdown",
            request.max_drawdown < self.policy.max_drawdown,
            "maximum drawdown limit",
        );
        push_check(
            &mut checks,
            "consecutive_losses",
            !circuit_breaker_active,
            "strategy consecutive-loss cooldown",
        );
        push_check(
            &mut checks,
            "order_size",
            request.requested_usd > 0.0 && request.requested_usd <= self.policy.max_order_usd,
            "requested amount is within configured maximum",
        );
        push_check(
            &mut checks,
            "no_executable_depth",
            request.executable_depth_usd > 0.0,
            "target side has executable ask depth",
        );
        push_check(
            &mut checks,
            "depth",
            request.executable_depth_usd >= request.requested_usd,
            "executable depth covers requested amount",
        );
        push_check(
            &mut checks,
            "capital_below_minimum_shares",
            matches!((shares, request.min_order_size_shares), (Some(actual), Some(minimum)) if actual >= minimum),
            "order satisfies CLOB minimum share size",
        );
        push_check(
            &mut checks,
            "fee_rate",
            request
                .fee_rate_bps
                .is_some_and(|fee| fee <= self.policy.max_fee_rate_bps),
            "fee rate is present and within configured maximum",
        );
        push_check(
            &mut checks,
            "balance_reserve",
            request.current_capital_usd - request.requested_usd - request.fee_usd
                >= self.policy.min_balance_reserve_usd,
            "capital remains above minimum reserve",
        );
        push_check(
            &mut checks,
            "numeric_values",
            numeric_values_valid(&request),
            "all monetary and probability values are finite",
        );

        if let Some(failed) = checks.iter().find(|check| !check.passed) {
            return RiskDecision {
                approved: false,
                reason_code: failed.code.clone(),
                checks,
                intent: None,
            };
        }

        let direction = direction.expect("direction check passed");
        let expected_fill_price = Fixed::from_f64(fill.expect("entry price check passed"))
            .expect("numeric value check passed");
        let requested_usd =
            Fixed::from_f64(request.requested_usd).expect("numeric value check passed");
        let expected_fee_usd =
            Fixed::from_f64(request.fee_usd).expect("numeric value check passed");
        let model_margin = Fixed::from_f64(margin.expect("margin check passed"))
            .expect("numeric value check passed");
        let expected_shares = Fixed::from_f64(shares.expect("minimum shares check passed"))
            .expect("numeric value check passed");
        let worst_allowed_price =
            Fixed::from_f64(request.max_entry_price).expect("numeric value check passed");
        let client_order_key = format!(
            "{}:{}:{}:{}",
            request.strategy_version,
            request.market_slug,
            request.timeframe,
            direction.as_title().to_lowercase()
        );

        RiskDecision {
            approved: true,
            reason_code: "approved".to_string(),
            intent: Some(ExecutionIntent {
                client_order_key,
                market_slug: request.market_slug,
                token_id: request.token_id.expect("token check passed"),
                timeframe: request.timeframe,
                direction,
                strategy_version: request.strategy_version,
                signal_timestamp_ms: request.signal_timestamp_ms,
                market_snapshot_timestamp_ms: request.market_snapshot_timestamp_ms,
                requested_usd,
                worst_allowed_price,
                expected_fill_price,
                expected_fee_usd,
                model_margin,
                expected_shares,
                risk_checks: checks.clone(),
            }),
            checks,
        }
    }
}

fn push_check(checks: &mut Vec<RiskCheck>, code: &str, passed: bool, detail: &str) {
    checks.push(RiskCheck {
        code: code.to_string(),
        passed,
        detail: detail.to_string(),
    });
}

fn numeric_values_valid(request: &RiskRequest) -> bool {
    let non_negative = [
        request.price_to_beat,
        request.min_entry_price,
        request.max_entry_price,
        request.confidence,
        request.min_model_margin,
        request.spread,
        request.executable_depth_usd,
        request.requested_usd,
        request.fee_usd,
        request.current_capital_usd,
        request.max_drawdown,
    ];
    non_negative
        .iter()
        .all(|value| value.is_finite() && *value >= 0.0)
        && request
            .expected_fill_price
            .is_some_and(|value| value.is_finite() && (0.0..=1.0).contains(&value))
        && request
            .min_order_size_shares
            .is_some_and(|value| value.is_finite() && value >= 0.0)
        && request.daily_realized_pnl_usd.is_finite()
        && (0.0..=1.0).contains(&request.confidence)
        && (0.0..=1.0).contains(&request.max_entry_price)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RiskPolicy {
        RiskPolicy {
            trading_enabled: true,
            strategy_enabled: true,
            max_open_positions: 1,
            max_daily_orders: 3,
            max_daily_realized_loss_usd: 0.30,
            max_drawdown: 0.20,
            max_consecutive_losses: 3,
            max_order_usd: 0.10,
            min_balance_reserve_usd: 0.0,
            max_spread: 0.04,
            max_fee_rate_bps: 500,
            max_data_age_ms: 15_000,
        }
    }

    fn request() -> RiskRequest {
        RiskRequest {
            now_ms: 1_180_000,
            market_slug: "btc-updown-15m-test".to_string(),
            token_id: Some("up-token".to_string()),
            timeframe: "15m".to_string(),
            direction: Some(Direction::Up),
            strategy_version: "test-v1".to_string(),
            signal_timestamp_ms: 1_175_000,
            signal_window_start_ts: 1_000,
            market_start_ts: 1_000,
            market_snapshot_timestamp_ms: 1_175_000,
            data_ready: true,
            price_to_beat: 100.0,
            expected_fill_price: Some(0.50),
            min_entry_price: 0.50,
            max_entry_price: 0.60,
            confidence: 0.70,
            min_model_margin: 0.10,
            spread: 0.02,
            executable_depth_usd: 0.10,
            min_order_size_shares: Some(0.20),
            fee_rate_bps: Some(200),
            requested_usd: 0.10,
            fee_usd: 0.0,
            current_capital_usd: 2.0,
            open_positions: 0,
            daily_orders: 0,
            daily_realized_pnl_usd: 0.0,
            max_drawdown: 0.0,
            consecutive_losses: 0,
            last_loss_timestamp_ms: None,
            market_already_traded: false,
            entry_window_start_secs: 180,
            entry_window_end_secs: 210,
        }
    }

    #[test]
    fn approves_boundary_values_and_builds_deterministic_fixed_point_intent() {
        let engine = RiskEngine::new(policy());
        let first = engine.evaluate(request());
        let second = engine.evaluate(request());

        assert!(first.approved);
        assert_eq!(first.intent, second.intent);
        let intent = first.intent.unwrap();
        assert_eq!(intent.requested_usd.as_f64(), 0.10);
        assert_eq!(intent.expected_shares.as_f64(), 0.20);
        assert_eq!(
            intent.client_order_key,
            "test-v1:btc-updown-15m-test:15m:up"
        );
    }

    #[test]
    fn rejects_every_global_cap_at_its_boundary() {
        let engine = RiskEngine::new(policy());
        let mut value = request();
        value.open_positions = 1;
        assert_eq!(engine.evaluate(value).reason_code, "open_positions");

        let mut value = request();
        value.daily_orders = 3;
        assert_eq!(engine.evaluate(value).reason_code, "daily_orders");

        let mut value = request();
        value.daily_realized_pnl_usd = -0.30;
        assert_eq!(engine.evaluate(value).reason_code, "daily_loss");

        let mut value = request();
        value.max_drawdown = 0.20;
        assert_eq!(engine.evaluate(value).reason_code, "drawdown");
    }

    #[test]
    fn rejects_stale_depth_fee_minimum_and_balance_failures() {
        let engine = RiskEngine::new(policy());
        let mut value = request();
        value.market_snapshot_timestamp_ms = 1_000_000;
        assert_eq!(engine.evaluate(value).reason_code, "snapshot_freshness");

        let mut value = request();
        value.executable_depth_usd = 0.0;
        assert_eq!(engine.evaluate(value).reason_code, "no_executable_depth");

        let mut value = request();
        value.executable_depth_usd = 0.09;
        assert_eq!(engine.evaluate(value).reason_code, "depth");

        let mut value = request();
        value.min_order_size_shares = Some(0.21);
        assert_eq!(
            engine.evaluate(value).reason_code,
            "capital_below_minimum_shares"
        );

        let mut value = request();
        value.fee_rate_bps = Some(501);
        assert_eq!(engine.evaluate(value).reason_code, "fee_rate");

        let mut value = request();
        value.current_capital_usd = 0.09;
        assert_eq!(engine.evaluate(value).reason_code, "balance_reserve");
    }

    #[test]
    fn rejects_kill_switch_duplicate_market_and_active_loss_breaker() {
        let mut disabled = policy();
        disabled.trading_enabled = false;
        assert_eq!(
            RiskEngine::new(disabled).evaluate(request()).reason_code,
            "global_kill_switch"
        );

        let engine = RiskEngine::new(policy());
        let mut duplicate = request();
        duplicate.market_already_traded = true;
        assert_eq!(engine.evaluate(duplicate).reason_code, "market_unique");

        let mut losses = request();
        losses.consecutive_losses = 3;
        losses.last_loss_timestamp_ms = Some(losses.now_ms - 1_000);
        assert_eq!(engine.evaluate(losses).reason_code, "consecutive_losses");
    }
}
