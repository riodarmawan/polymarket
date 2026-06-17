use crate::config::Config;
use crate::crypto::binance_ws::Candle;
use crate::engine::risk::{ExecutionIntent, RiskDecision};
use crate::evaluation::{build_report, ForwardOpportunity, ForwardReport};
use crate::storage::dashboard::{DashboardSnapshot, DashboardStore};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Settings {
    pub capital: f64,
    pub max_order: f64,
    pub timeframe: String,
    pub auto_trade: bool,
    pub min_edge: f64,
    pub max_entry_price: f64,
    pub risk_fraction: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capital: 7.50,
            max_order: 4.00,
            timeframe: "15m".to_string(),
            auto_trade: true,
            min_edge: 0.04,
            max_entry_price: 0.72,
            risk_fraction: 0.10,
        }
    }
}

impl Settings {
    pub fn from_config(config: &Config) -> Self {
        Self {
            capital: config.general.initial_capital,
            max_order: config.risk.max_order_usd,
            timeframe: "15m".to_string(),
            auto_trade: true,
            min_edge: config.expected_value.min_edge_pct,
            max_entry_price: 0.72,
            risk_fraction: config.position_sizing.max_position_pct,
        }
    }

    pub fn respects(&self, runtime: &RuntimeInfo) -> bool {
        self.capital.is_finite()
            && self.capital > 0.0
            && self.capital <= runtime.configured_initial_capital
            && self.max_order.is_finite()
            && self.max_order >= runtime.configured_min_order_usd
            && self.max_order <= runtime.configured_max_order_usd
            && self.risk_fraction.is_finite()
            && self.risk_fraction > 0.0
            && self.risk_fraction <= runtime.configured_max_risk_fraction
            && self.min_edge.is_finite()
            && self.min_edge >= runtime.configured_min_edge
            && self.max_entry_price.is_finite()
            && (0.0..=1.0).contains(&self.max_entry_price)
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeInfo {
    pub mode: String,
    pub environment: String,
    pub strategy_version: String,
    pub build_version: String,
    pub configured_max_order_usd: f64,
    pub configured_min_order_usd: f64,
    pub configured_initial_capital: f64,
    pub configured_max_open_positions: usize,
    pub configured_max_daily_orders: usize,
    pub configured_max_spread: f64,
    pub configured_max_daily_loss_usd: f64,
    pub configured_max_drawdown: f64,
    pub configured_max_consecutive_losses: usize,
    pub configured_max_risk_fraction: f64,
    pub configured_min_edge: f64,
    pub configured_fee_pct: f64,
    pub configured_max_data_age_ms: u64,
    pub configured_trading_enabled: bool,
    pub configured_enable_5m: bool,
    pub configured_enable_15m: bool,
    pub configured_min_balance_reserve_usd: f64,
    pub configured_max_fee_rate_bps: u64,
    pub gamma_base_url: String,
    pub clob_base_url: String,
}

#[derive(Debug, Clone)]
pub struct PriceData {
    pub price: f64,
    pub change_pct: f64,
    pub timestamp: i64,
    pub source: String,
}

impl Default for PriceData {
    fn default() -> Self {
        Self {
            price: 0.0,
            change_pct: 0.0,
            timestamp: 0,
            source: "unknown".to_string(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TradeInfo {
    pub timestamp: i64,
    pub market_slug: String,
    pub timeframe: String,
    pub direction: String,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub shares: f64,
    pub size_usd: f64,
    pub fee_usd: f64,
    pub price_to_beat: f64,
    pub end_ts: i64,
    pub confidence: f64,
    pub edge: f64,
    pub pnl: Option<f64>,
    pub status: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StatsInfo {
    pub total_trades: usize,
    pub wins: usize,
    pub losses: usize,
    pub win_rate: f64,
    pub total_pnl: f64,
    pub avg_win: f64,
    pub avg_loss: f64,
    pub profit_factor: f64,
    pub max_drawdown: f64,
    pub current_capital: f64,
    pub peak_capital: f64,
}

impl Default for StatsInfo {
    fn default() -> Self {
        Self {
            total_trades: 0,
            wins: 0,
            losses: 0,
            win_rate: 0.0,
            total_pnl: 0.0,
            avg_win: 0.0,
            avg_loss: 0.0,
            profit_factor: 0.0,
            max_drawdown: 0.0,
            current_capital: 2.0,
            peak_capital: 2.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MarketInfo {
    pub id: String,
    pub question: String,
    pub yes_price: f64,
    pub no_price: f64,
    pub volume: f64,
    pub liquidity: f64,
    pub enable_order_book: bool,
    pub clob_token_ids: Option<String>,
    pub end_date: String,
    pub tags: Vec<String>,
    pub minutes_left: i64,
}

#[derive(Debug, Clone)]
pub struct UpDownMarket {
    pub asset: String,
    pub slug: String,
    pub interval: String,
    pub start_ts: i64,
    pub end_ts: i64,
    pub remaining_seconds: i64,
    pub up_token_id: Option<String>,
    pub down_token_id: Option<String>,
    pub up_best_ask: Option<f64>,
    pub up_best_bid: Option<f64>,
    pub down_best_ask: Option<f64>,
    pub down_best_bid: Option<f64>,
    pub spread: f64,
    pub status: String,
    pub price_to_beat: f64,
    pub current_price: f64,
    pub captured_at_ms: i64,
    pub data_status: DataStatus,
    pub data_detail: String,
    pub token_mapping_valid: bool,
    pub tick_size: Option<f64>,
    pub min_order_size: Option<f64>,
    pub fee_rate_bps: Option<u64>,
    pub negative_risk: Option<bool>,
    pub up_executable_depth_usd: f64,
    pub down_executable_depth_usd: f64,
    pub up_expected_fill_price: Option<f64>,
    pub down_expected_fill_price: Option<f64>,
    pub clock_drift_ms: Option<i64>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DataStatus {
    Ready,
    NotFound,
    Timeout,
    RateLimited,
    InvalidPayload,
    Stale,
    Unavailable,
    Incomplete,
    OneSided,
}

impl DataStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::NotFound => "not_found",
            Self::Timeout => "timeout",
            Self::RateLimited => "rate_limited",
            Self::InvalidPayload => "invalid_payload",
            Self::Stale => "stale",
            Self::Unavailable => "unavailable",
            Self::Incomplete => "incomplete",
            Self::OneSided => "one_sided",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignalInfo {
    pub direction: String,
    pub confidence: f64,
    pub timeframe: String,
    pub reason: String,
    pub timestamp: i64,
    pub window_start_ts: i64,
}

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub store: DashboardStore,
    pub runtime: RuntimeInfo,
    pub price: Arc<RwLock<PriceData>>,
    pub settings: Arc<RwLock<Settings>>,
    pub trades: Arc<RwLock<Vec<TradeInfo>>>,
    pub stats: Arc<RwLock<StatsInfo>>,
    pub stats_5m: Arc<RwLock<StatsInfo>>,
    pub markets: Arc<RwLock<Vec<MarketInfo>>>,
    pub updown_markets: Arc<RwLock<Vec<UpDownMarket>>>,
    pub last_scan_at: Arc<RwLock<i64>>,
    pub candle_buffer: Arc<RwLock<Vec<Candle>>>,
    pub last_signal: Arc<RwLock<Option<SignalInfo>>>,
    pub last_signal_5m: Arc<RwLock<Option<SignalInfo>>>,
    pub last_signal_time: Arc<RwLock<i64>>,
    pub last_signal_5m_time: Arc<RwLock<i64>>,
    pub execution_note: Arc<RwLock<String>>,
    pub execution_note_5m: Arc<RwLock<String>>,
}

impl AppState {
    pub async fn new(config: &Config) -> Result<Self> {
        let store = DashboardStore::open(Path::new(&config.storage.database_path)).await?;
        let snapshot = store.load_snapshot().await?;
        let restored = snapshot.is_some();
        let snapshot = snapshot.unwrap_or_else(|| DashboardSnapshot {
            settings: Settings::from_config(config),
            trades: Vec::new(),
            stats_15m: StatsInfo::with_capital(config.general.initial_capital),
            stats_5m: StatsInfo::with_capital(config.general.initial_capital),
        });
        let runtime = RuntimeInfo::from_config(config);
        let mut settings = snapshot.settings;
        if settings.max_order < runtime.configured_min_order_usd {
            settings.max_order = runtime.configured_max_order_usd;
        }
        if settings.max_order > runtime.configured_max_order_usd {
            settings.max_order = runtime.configured_max_order_usd;
        }
        if settings.risk_fraction > runtime.configured_max_risk_fraction {
            settings.risk_fraction = runtime.configured_max_risk_fraction;
        }
        if settings.min_edge > runtime.configured_min_edge {
            settings.min_edge = runtime.configured_min_edge;
        }
        if !settings.respects(&runtime) {
            anyhow::bail!("restored dashboard settings exceed the active configuration limits");
        }
        let state = Self {
            config: config.clone(),
            store,
            runtime,
            price: Arc::new(RwLock::new(PriceData::default())),
            settings: Arc::new(RwLock::new(settings)),
            trades: Arc::new(RwLock::new(snapshot.trades)),
            stats: Arc::new(RwLock::new(snapshot.stats_15m)),
            stats_5m: Arc::new(RwLock::new(snapshot.stats_5m)),
            markets: Arc::new(RwLock::new(Vec::new())),
            updown_markets: Arc::new(RwLock::new(Vec::new())),
            last_scan_at: Arc::new(RwLock::new(0)),
            candle_buffer: Arc::new(RwLock::new(Vec::new())),
            last_signal: Arc::new(RwLock::new(None)),
            last_signal_5m: Arc::new(RwLock::new(None)),
            last_signal_time: Arc::new(RwLock::new(0)),
            last_signal_5m_time: Arc::new(RwLock::new(0)),
            execution_note: Arc::new(RwLock::new("Waiting for a valid signal".to_string())),
            execution_note_5m: Arc::new(RwLock::new("Waiting for a valid 5m signal".to_string())),
        };
        state
            .store
            .audit_event(
                "dashboard_started",
                &state.runtime.mode,
                &state.runtime.strategy_version,
                json!({
                    "restored": restored,
                    "database_path": state.store.path().display().to_string()
                }),
            )
            .await?;
        let (runtime_state, _) = state.store.runtime_state().await?;
        if runtime_state == "booting" {
            state
                .store
                .set_runtime_state("ready", "dashboard initialized in paper mode")
                .await?;
        }
        Ok(state)
    }

    pub async fn persist(&self, event_type: &str, detail: serde_json::Value) -> Result<()> {
        let snapshot = DashboardSnapshot {
            settings: self.settings.read().await.clone(),
            trades: self.trades.read().await.clone(),
            stats_15m: self.stats.read().await.clone(),
            stats_5m: self.stats_5m.read().await.clone(),
        };
        self.store
            .save_snapshot(
                &snapshot,
                event_type,
                &self.runtime.mode,
                &self.runtime.strategy_version,
                detail,
            )
            .await
    }

    pub async fn audit(&self, event_type: &str, detail: serde_json::Value) -> Result<()> {
        self.store
            .audit_event(
                event_type,
                &self.runtime.mode,
                &self.runtime.strategy_version,
                detail,
            )
            .await
    }

    pub async fn reserve_execution_intent(&self, intent: &ExecutionIntent) -> Result<bool> {
        self.store
            .reserve_execution_intent(
                &intent.client_order_key,
                &intent.market_slug,
                &intent.timeframe,
                &self.runtime.mode,
                &self.runtime.strategy_version,
                serde_json::to_value(intent)?,
            )
            .await
    }

    pub async fn record_risk_decision(
        &self,
        market_slug: &str,
        timeframe: &str,
        decision: &RiskDecision,
    ) -> Result<()> {
        self.store
            .record_risk_decision(
                market_slug,
                timeframe,
                &self.runtime.mode,
                &self.runtime.strategy_version,
                decision,
            )
            .await
    }

    pub async fn record_risk_decision_with_context(
        &self,
        market_slug: &str,
        timeframe: &str,
        decision: &RiskDecision,
        context: serde_json::Value,
    ) -> Result<()> {
        self.store
            .record_risk_decision_with_context(
                market_slug,
                timeframe,
                &self.runtime.mode,
                &self.runtime.strategy_version,
                decision,
                context,
            )
            .await
    }

    pub async fn record_forward_opportunity(
        &self,
        market: &UpDownMarket,
        signal: &SignalInfo,
        decision: &RiskDecision,
    ) -> Result<()> {
        let direction_up = signal.direction == "Up";
        self.store
            .record_forward_opportunity(&ForwardOpportunity {
                market_slug: market.slug.clone(),
                timeframe: market.interval.clone(),
                direction: signal.direction.clone(),
                confidence: signal.confidence,
                expected_fill_price: if direction_up {
                    market.up_expected_fill_price
                } else {
                    market.down_expected_fill_price
                },
                spread: market.spread,
                fee_rate_bps: market.fee_rate_bps,
                approved: decision.approved,
                reason_code: decision.reason_code.clone(),
                captured_at_ms: market.captured_at_ms,
                end_ts: market.end_ts,
                official_outcome: None,
            })
            .await
    }

    pub async fn forward_report(&self) -> Result<ForwardReport> {
        let opportunities = self.store.load_forward_opportunities().await?;
        let trades = self.trades.read().await.clone();
        Ok(build_report(&opportunities, &trades))
    }

    pub async fn execution_audit(&self, limit: i64) -> Result<Vec<serde_json::Value>> {
        self.store.load_execution_audit(limit).await
    }

    pub async fn halt_after_persistence_failure(&self, context: &str, error: &anyhow::Error) {
        self.settings.write().await.auto_trade = false;
        tracing::error!(
            "Persistence failure halted auto-trade during {}: {}",
            context,
            error
        );
    }
}

impl RuntimeInfo {
    pub(crate) fn from_config(config: &Config) -> Self {
        Self {
            mode: config.runtime.mode.to_string(),
            environment: format!("{:?}", config.runtime.environment).to_lowercase(),
            strategy_version: config.runtime.strategy_version.clone(),
            build_version: env!("CARGO_PKG_VERSION").to_string(),
            configured_max_order_usd: config.risk.max_order_usd,
            configured_min_order_usd: config.position_sizing.min_position_usd,
            configured_initial_capital: config.general.initial_capital,
            configured_max_open_positions: config.risk.max_open_positions,
            configured_max_daily_orders: config.risk.max_daily_orders,
            configured_max_spread: config.risk.max_spread,
            configured_max_daily_loss_usd: config.risk.max_daily_realized_loss_usd,
            configured_max_drawdown: config.risk.max_drawdown,
            configured_max_consecutive_losses: config.risk.max_consecutive_losses,
            configured_max_risk_fraction: config.position_sizing.max_position_pct,
            configured_min_edge: config.expected_value.min_edge_pct,
            configured_fee_pct: config.expected_value.cost_per_trade_pct,
            configured_max_data_age_ms: config.risk.max_data_age_ms,
            configured_trading_enabled: config.risk.trading_enabled,
            configured_enable_5m: config.risk.enable_5m,
            configured_enable_15m: config.risk.enable_15m,
            configured_min_balance_reserve_usd: config.risk.min_balance_reserve_usd,
            configured_max_fee_rate_bps: config.risk.max_fee_rate_bps,
            gamma_base_url: config.api.gamma_base_url.clone(),
            clob_base_url: config.api.clob_base_url.clone(),
        }
    }
}

impl StatsInfo {
    fn with_capital(capital: f64) -> Self {
        Self {
            current_capital: capital,
            peak_capital: capital,
            ..Self::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn rejects_settings_above_configured_order_ceiling() {
        let config = Config::default();
        let runtime = RuntimeInfo::from_config(&config);
        let mut settings = Settings::from_config(&config);
        assert!(settings.respects(&runtime));

        settings.max_order = runtime.configured_max_order_usd + 0.01;
        assert!(!settings.respects(&runtime));
    }

    #[tokio::test]
    async fn restores_dashboard_state_after_restart() {
        let temp = TempDir::new().unwrap();
        let mut config = Config::default();
        config.storage.database_path = temp.path().join("state.db").display().to_string();

        let state = AppState::new(&config).await.unwrap();
        state.trades.write().await.push(TradeInfo {
            timestamp: 1,
            market_slug: "btc-updown-restart-test".to_string(),
            timeframe: "15m".to_string(),
            direction: "Down".to_string(),
            entry_price: 0.40,
            exit_price: None,
            shares: 0.25,
            size_usd: 0.10,
            fee_usd: 0.002,
            price_to_beat: 100.0,
            end_ts: 2,
            confidence: 0.70,
            edge: 0.30,
            pnl: None,
            status: "open".to_string(),
        });
        state.trades.write().await.push(TradeInfo {
            timestamp: 0,
            market_slug: "btc-updown-settled-restart-test".to_string(),
            timeframe: "5m".to_string(),
            direction: "Up".to_string(),
            entry_price: 0.50,
            exit_price: Some(1.0),
            shares: 0.20,
            size_usd: 0.10,
            fee_usd: 0.002,
            price_to_beat: 100.0,
            end_ts: 1,
            confidence: 0.70,
            edge: 0.20,
            pnl: Some(0.098),
            status: "settled".to_string(),
        });
        state.stats.write().await.current_capital = 1.898;
        state.stats_5m.write().await.total_pnl = 0.098;
        state
            .persist(
                "restart_test",
                serde_json::json!({"step": "before_restart"}),
            )
            .await
            .unwrap();
        drop(state);

        let restored = AppState::new(&config).await.unwrap();
        assert_eq!(restored.trades.read().await.len(), 2);
        assert_eq!(restored.stats.read().await.current_capital, 1.898);
        assert_eq!(restored.trades.read().await[0].status, "open");
        assert_eq!(restored.trades.read().await[1].status, "settled");
        assert_eq!(restored.stats_5m.read().await.total_pnl, 0.098);
    }
}
