use crate::build_info;
use anyhow::{bail, Result};
use serde::Deserialize;
use std::fmt;
use std::path::{Path, PathBuf};

pub const STRATEGY_VERSION: &str = "btc-updown-v2";

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    #[serde(default)]
    pub runtime: RuntimeConfig,
    #[serde(default)]
    pub risk: RiskConfig,
    #[serde(default)]
    pub execution: ExecutionConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub dashboard: DashboardConfig,
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub api: ApiConfig,
    #[serde(default)]
    pub orderbook: OrderBookConfig,
    #[serde(default)]
    pub probability: ProbabilityConfig,
    #[serde(default)]
    pub expected_value: EVConfig,
    #[serde(default)]
    pub position_sizing: PositionSizingConfig,
    #[serde(default)]
    pub exit: ExitConfig,
    #[serde(default)]
    pub collector: CollectorConfig,
    #[serde(default)]
    pub backtesting: BacktestingConfig,
    #[serde(default)]
    pub paper_trading: PaperTradingConfig,
    #[serde(default)]
    pub trading: TradingConfig,
    #[serde(default)]
    pub crypto: CryptoConfig,
    #[serde(skip)]
    pub source_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeMode {
    Paper,
    Shadow,
    DrySigned,
    Canary,
    Live,
}

impl fmt::Display for RuntimeMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Paper => "paper",
            Self::Shadow => "shadow",
            Self::DrySigned => "dry_signed",
            Self::Canary => "canary",
            Self::Live => "live",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEnvironment {
    Development,
    Production,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RuntimeConfig {
    #[serde(default = "default_runtime_environment")]
    pub environment: RuntimeEnvironment,
    #[serde(default = "default_runtime_mode")]
    pub mode: RuntimeMode,
    #[serde(default = "default_strategy_version")]
    pub strategy_version: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RiskConfig {
    #[serde(default = "default_true")]
    pub trading_enabled: bool,
    #[serde(default = "default_true")]
    pub enable_5m: bool,
    #[serde(default = "default_true")]
    pub enable_15m: bool,
    #[serde(default = "default_production_max_positions")]
    pub max_open_positions: usize,
    #[serde(default = "default_production_max_order_usd")]
    pub max_order_usd: f64,
    #[serde(default = "default_max_daily_orders")]
    pub max_daily_orders: usize,
    #[serde(default = "default_max_daily_realized_loss_usd")]
    pub max_daily_realized_loss_usd: f64,
    #[serde(default = "default_max_drawdown")]
    pub max_drawdown: f64,
    #[serde(default = "default_max_consecutive_losses")]
    pub max_consecutive_losses: usize,
    #[serde(default = "default_max_spread_pct")]
    pub max_spread: f64,
    #[serde(default = "default_max_data_age_ms")]
    pub max_data_age_ms: u64,
    #[serde(default)]
    pub min_balance_reserve_usd: f64,
    #[serde(default = "default_max_fee_rate_bps")]
    pub max_fee_rate_bps: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExecutionConfig {
    #[serde(default = "default_order_type")]
    pub order_type: String,
    #[serde(default = "default_heartbeat_interval_secs")]
    pub heartbeat_interval_secs: u64,
    #[serde(default = "default_true")]
    pub reconcile_before_ready: bool,
    #[serde(default = "default_true")]
    pub cancel_on_shutdown: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageConfig {
    #[serde(default = "default_database_path")]
    pub database_path: String,
    #[serde(default = "default_backup_directory")]
    pub backup_directory: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DashboardConfig {
    #[serde(default = "default_dashboard_bind")]
    pub bind: String,
    #[serde(default)]
    pub allow_live_mode_changes: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct GeneralConfig {
    #[serde(default = "default_initial_capital")]
    pub initial_capital: f64,
    #[serde(default = "default_max_positions")]
    pub max_positions: usize,
    #[serde(default = "default_log_level")]
    pub log_level: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ApiConfig {
    #[serde(default = "default_gamma_base_url")]
    pub gamma_base_url: String,
    #[serde(default = "default_clob_base_url")]
    pub clob_base_url: String,
    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OrderBookConfig {
    #[serde(default = "default_max_spread_pct")]
    pub max_spread_pct: f64,
    #[serde(default = "default_min_depth")]
    pub min_depth: f64,
    #[serde(default = "default_min_volume_24h")]
    pub min_volume_24h: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ProbabilityConfig {
    #[serde(default = "default_prior_weight")]
    pub prior_weight: f64,
    #[serde(default = "default_news_weight")]
    pub news_weight: f64,
    #[serde(default = "default_polling_weight")]
    pub polling_weight: f64,
    #[serde(default = "default_expert_weight")]
    pub expert_weight: f64,
    #[serde(default = "default_historical_weight")]
    pub historical_weight: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EVConfig {
    #[serde(default = "default_cost_per_trade_pct")]
    pub cost_per_trade_pct: f64,
    #[serde(default = "default_min_ev_threshold")]
    pub min_ev_threshold: f64,
    #[serde(default = "default_min_edge_pct")]
    pub min_edge_pct: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PositionSizingConfig {
    #[serde(default = "default_kelly_fraction")]
    pub kelly_fraction: f64,
    #[serde(default = "default_max_position_pct")]
    pub max_position_pct: f64,
    #[serde(default = "default_min_position_usd")]
    pub min_position_usd: f64,
    #[serde(default = "default_max_position_usd")]
    pub max_position_usd: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExitConfig {
    #[serde(default = "default_take_profit_pct")]
    pub take_profit_pct: f64,
    #[serde(default = "default_stop_loss_pct")]
    pub stop_loss_pct: f64,
    #[serde(default = "default_trailing_stop_pct")]
    pub trailing_stop_pct: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct CollectorConfig {
    #[serde(default = "default_interval_secs")]
    pub interval_secs: u64,
    #[serde(default = "default_max_markets")]
    pub max_markets: usize,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BacktestingConfig {
    #[serde(default = "default_period_days")]
    pub default_period_days: u32,
    #[serde(default = "default_initial_capital")]
    pub initial_capital: f64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct PaperTradingConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_dry_run")]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TradingConfig {
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_max_hours_to_resolution")]
    pub max_hours_to_resolution: f64,
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,
    #[serde(default = "default_sort_by")]
    pub sort_by: String,
}

fn default_initial_capital() -> f64 {
    1000.0
}
fn default_runtime_environment() -> RuntimeEnvironment {
    RuntimeEnvironment::Development
}
fn default_runtime_mode() -> RuntimeMode {
    RuntimeMode::Paper
}
fn default_strategy_version() -> String {
    STRATEGY_VERSION.to_string()
}
fn default_production_max_positions() -> usize {
    1
}
fn default_production_max_order_usd() -> f64 {
    0.10
}
fn default_max_daily_orders() -> usize {
    3
}
fn default_max_daily_realized_loss_usd() -> f64 {
    0.30
}
fn default_max_drawdown() -> f64 {
    0.20
}
fn default_max_consecutive_losses() -> usize {
    3
}
fn default_max_data_age_ms() -> u64 {
    15_000
}
fn default_max_fee_rate_bps() -> u64 {
    500
}
fn default_order_type() -> String {
    "FOK".to_string()
}
fn default_heartbeat_interval_secs() -> u64 {
    5
}
fn default_true() -> bool {
    true
}
fn default_database_path() -> String {
    "data/trading.db".to_string()
}
fn default_backup_directory() -> String {
    "data/backups".to_string()
}
fn default_dashboard_bind() -> String {
    "127.0.0.1:3001".to_string()
}
fn default_max_positions() -> usize {
    10
}
fn default_log_level() -> String {
    "info".to_string()
}
fn default_data_dir() -> String {
    "data".to_string()
}
fn default_gamma_base_url() -> String {
    "https://gamma-api.polymarket.com".to_string()
}
fn default_clob_base_url() -> String {
    "https://clob.polymarket.com".to_string()
}
fn default_timeout_secs() -> u64 {
    30
}
fn default_max_spread_pct() -> f64 {
    0.04
}
fn default_min_depth() -> f64 {
    1000.0
}
fn default_min_volume_24h() -> f64 {
    10000.0
}
fn default_prior_weight() -> f64 {
    0.3
}
fn default_news_weight() -> f64 {
    0.25
}
fn default_polling_weight() -> f64 {
    0.15
}
fn default_expert_weight() -> f64 {
    0.20
}
fn default_historical_weight() -> f64 {
    0.10
}
fn default_cost_per_trade_pct() -> f64 {
    0.02
}
fn default_min_ev_threshold() -> f64 {
    0.05
}
fn default_min_edge_pct() -> f64 {
    0.02
}
fn default_kelly_fraction() -> f64 {
    0.125
}
fn default_max_position_pct() -> f64 {
    0.05
}
fn default_min_position_usd() -> f64 {
    0.10
}
fn default_max_position_usd() -> f64 {
    0.10
}
fn default_take_profit_pct() -> f64 {
    0.30
}
fn default_stop_loss_pct() -> f64 {
    0.20
}
fn default_trailing_stop_pct() -> f64 {
    0.10
}
fn default_interval_secs() -> u64 {
    300
}
fn default_max_markets() -> usize {
    50
}
fn default_period_days() -> u32 {
    30
}
fn default_enabled() -> bool {
    true
}
fn default_dry_run() -> bool {
    true
}
fn default_strategy() -> String {
    "last_minute".to_string()
}
fn default_max_hours_to_resolution() -> f64 {
    1.0
}
fn default_min_confidence() -> f64 {
    0.7
}
fn default_sort_by() -> String {
    "deadline".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            runtime: RuntimeConfig::default(),
            risk: RiskConfig::default(),
            execution: ExecutionConfig::default(),
            storage: StorageConfig::default(),
            dashboard: DashboardConfig::default(),
            general: GeneralConfig::default(),
            api: ApiConfig::default(),
            orderbook: OrderBookConfig::default(),
            probability: ProbabilityConfig::default(),
            expected_value: EVConfig::default(),
            position_sizing: PositionSizingConfig::default(),
            exit: ExitConfig::default(),
            collector: CollectorConfig::default(),
            backtesting: BacktestingConfig::default(),
            paper_trading: PaperTradingConfig::default(),
            trading: TradingConfig::default(),
            crypto: CryptoConfig::default(),
            source_path: None,
        }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            environment: default_runtime_environment(),
            mode: default_runtime_mode(),
            strategy_version: default_strategy_version(),
        }
    }
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            trading_enabled: true,
            enable_5m: true,
            enable_15m: true,
            max_open_positions: default_production_max_positions(),
            max_order_usd: default_production_max_order_usd(),
            max_daily_orders: default_max_daily_orders(),
            max_daily_realized_loss_usd: default_max_daily_realized_loss_usd(),
            max_drawdown: default_max_drawdown(),
            max_consecutive_losses: default_max_consecutive_losses(),
            max_spread: default_max_spread_pct(),
            max_data_age_ms: default_max_data_age_ms(),
            min_balance_reserve_usd: 0.0,
            max_fee_rate_bps: default_max_fee_rate_bps(),
        }
    }
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            order_type: default_order_type(),
            heartbeat_interval_secs: default_heartbeat_interval_secs(),
            reconcile_before_ready: true,
            cancel_on_shutdown: true,
        }
    }
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            database_path: default_database_path(),
            backup_directory: default_backup_directory(),
        }
    }
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            bind: default_dashboard_bind(),
            allow_live_mode_changes: false,
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            initial_capital: 1000.0,
            max_positions: 10,
            log_level: "info".to_string(),
            data_dir: "data".to_string(),
        }
    }
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            gamma_base_url: "https://gamma-api.polymarket.com".to_string(),
            clob_base_url: "https://clob.polymarket.com".to_string(),
            timeout_secs: 30,
        }
    }
}

impl Default for OrderBookConfig {
    fn default() -> Self {
        Self {
            max_spread_pct: 0.04,
            min_depth: 1000.0,
            min_volume_24h: 10000.0,
        }
    }
}

impl Default for ProbabilityConfig {
    fn default() -> Self {
        Self {
            prior_weight: 0.3,
            news_weight: 0.25,
            polling_weight: 0.15,
            expert_weight: 0.20,
            historical_weight: 0.10,
        }
    }
}

impl Default for EVConfig {
    fn default() -> Self {
        Self {
            cost_per_trade_pct: 0.02,
            min_ev_threshold: 0.05,
            min_edge_pct: 0.02,
        }
    }
}

impl Default for PositionSizingConfig {
    fn default() -> Self {
        Self {
            kelly_fraction: 0.125,
            max_position_pct: 0.05,
            min_position_usd: 0.10,
            max_position_usd: 0.10,
        }
    }
}

impl Default for ExitConfig {
    fn default() -> Self {
        Self {
            take_profit_pct: 0.30,
            stop_loss_pct: 0.20,
            trailing_stop_pct: 0.10,
        }
    }
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300,
            max_markets: 50,
        }
    }
}

impl Default for BacktestingConfig {
    fn default() -> Self {
        Self {
            default_period_days: 30,
            initial_capital: 1000.0,
        }
    }
}

impl Default for PaperTradingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dry_run: true,
        }
    }
}

impl Default for TradingConfig {
    fn default() -> Self {
        Self {
            strategy: "last_minute".to_string(),
            max_hours_to_resolution: 1.0,
            min_confidence: 0.7,
            sort_by: "deadline".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct CryptoConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_initial_capital")]
    pub initial_capital: f64,
    #[serde(default = "default_min_position_usd")]
    pub min_order_usd: f64,
    #[serde(default = "default_max_trades_per_hour")]
    pub max_trades_per_hour: usize,
    #[serde(default = "default_min_confidence")]
    pub min_confidence: f64,
    #[serde(default = "default_timeframes")]
    pub timeframes: Vec<String>,
}

fn default_max_trades_per_hour() -> usize {
    1
}

fn default_timeframes() -> Vec<String> {
    vec!["5m".into(), "15m".into(), "1h".into()]
}

impl Default for CryptoConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            initial_capital: 2.0,
            min_order_usd: 0.50,
            max_trades_per_hour: 1,
            min_confidence: 0.6,
            timeframes: vec!["5m".into(), "15m".into(), "1h".into()],
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        dotenvy::dotenv().ok();

        // Production must select its config explicitly. Development keeps the
        // existing executable-relative fallback.
        let explicit_config = std::env::var("POLYMARKET_CONFIG").ok();
        let config_path = if let Some(explicit_path) = &explicit_config {
            Path::new(explicit_path).to_path_buf()
        } else if let Ok(current_dir) = std::env::current_dir() {
            let current_dir_config = current_dir.join("config/default.toml");
            if current_dir_config.exists() {
                current_dir_config
            } else {
                executable_default_config()
            }
        } else {
            executable_default_config()
        };

        let config = if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            toml::from_str(&content)?
        } else if explicit_config.is_some() {
            anyhow::bail!(
                "POLYMARKET_CONFIG points to missing file: {}",
                config_path.display()
            );
        } else {
            Self::default()
        };

        // Apply env var overrides
        let mut config: Self = config;
        config.source_path = config_path.exists().then_some(config_path);
        if let Ok(val) = std::env::var("POLY_LOG_LEVEL") {
            config.general.log_level = val;
        }
        if let Ok(val) = std::env::var("POLY_DATA_DIR") {
            config.general.data_dir = val;
        }

        config.validate(explicit_config.is_some())?;
        Ok(config)
    }

    pub fn validate(&self, explicit_config: bool) -> Result<()> {
        if matches!(self.runtime.mode, RuntimeMode::Canary | RuntimeMode::Live) {
            if let Some(reason) = build_info::live_provenance_rejection(
                &build_info::BuildInfo::current(),
                self.runtime.environment,
                build_info::dirty_dev_override_enabled(),
            ) {
                bail!("canary/live runtime is blocked: {reason}");
            }
        }
        if self.runtime.mode != RuntimeMode::Paper {
            bail!(
                "runtime mode '{}' is blocked: this build only implements paper execution",
                self.runtime.mode
            );
        }
        if self.runtime.strategy_version != STRATEGY_VERSION {
            bail!(
                "unknown strategy version '{}'; expected '{}'",
                self.runtime.strategy_version,
                STRATEGY_VERSION
            );
        }
        if !self.paper_trading.enabled || !self.paper_trading.dry_run {
            bail!("paper execution and dry_run must remain enabled in this build");
        }
        if self.risk.max_open_positions == 0
            || self.risk.max_order_usd <= 0.0
            || self.risk.max_daily_orders == 0
            || self.risk.max_daily_realized_loss_usd <= 0.0
            || self.risk.max_drawdown <= 0.0
            || self.risk.max_consecutive_losses == 0
            || self.risk.max_spread <= 0.0
            || self.risk.max_data_age_ms == 0
            || self.risk.max_fee_rate_bps == 0
        {
            bail!("risk limits must all be greater than zero");
        }
        if !self.risk.min_balance_reserve_usd.is_finite()
            || self.risk.min_balance_reserve_usd < 0.0
            || self.risk.min_balance_reserve_usd >= self.general.initial_capital
        {
            bail!("risk.min_balance_reserve_usd must be non-negative and below initial capital");
        }
        if self.risk.max_order_usd > self.general.initial_capital {
            bail!("risk.max_order_usd cannot exceed general.initial_capital");
        }
        if self.position_sizing.min_position_usd > self.risk.max_order_usd {
            bail!("position_sizing.min_position_usd cannot exceed risk.max_order_usd");
        }
        if self.position_sizing.max_position_pct <= 0.0
            || self.position_sizing.max_position_pct > 1.0
            || self.expected_value.cost_per_trade_pct < 0.0
        {
            bail!("position sizing and fee configuration is invalid");
        }
        if self.storage.database_path.trim().is_empty()
            || self.storage.backup_directory.trim().is_empty()
        {
            bail!("storage database and backup paths must not be empty");
        }
        if self.dashboard.allow_live_mode_changes {
            bail!("dashboard.allow_live_mode_changes must remain false");
        }

        if self.runtime.environment == RuntimeEnvironment::Production {
            if !explicit_config {
                bail!("production requires an explicit POLYMARKET_CONFIG path");
            }
            if self.dashboard.bind != "127.0.0.1:3001" {
                bail!("production dashboard must bind to 127.0.0.1:3001");
            }
            if self.risk.max_open_positions > 1
                || self.risk.max_order_usd > 4.00
                || self.risk.max_daily_orders > 3
                || self.risk.max_daily_realized_loss_usd > 0.30
                || self.risk.max_drawdown > 0.20
                || self.risk.max_consecutive_losses > 3
            {
                bail!("production risk limits exceed the approved $7.50 operator limits");
            }
            if !self.execution.reconcile_before_ready || !self.execution.cancel_on_shutdown {
                bail!("production requires reconciliation and cancel-on-shutdown");
            }
            if self.execution.heartbeat_interval_secs == 0 {
                bail!("production heartbeat interval must be greater than zero");
            }
            if self.execution.order_type != "FOK" {
                bail!("production execution.order_type must be FOK");
            }
        }

        Ok(())
    }

    pub fn source_label(&self) -> String {
        self.source_path
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "built-in defaults".to_string())
    }

    pub fn print_startup_summary(&self) {
        let build = build_info::BuildInfo::current();
        tracing::info!(
            runtime_mode = %self.runtime.mode,
            environment = ?self.runtime.environment,
            config = %self.source_label(),
            database = %self.storage.database_path,
            build_version = build.package_version,
            git_sha = build.git_short_sha(),
            git_dirty = build.git_dirty,
            build_timestamp = build.build_timestamp,
            strategy_version = %self.runtime.strategy_version,
            "Startup configuration"
        );
    }
}

fn executable_default_config() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .map(|parent| parent.join("config/default.toml"))
        })
        .unwrap_or_else(|| Path::new("config/default.toml").to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_paper_only_and_valid() {
        let config = Config::default();
        assert_eq!(config.runtime.mode, RuntimeMode::Paper);
        assert!(config.paper_trading.dry_run);
        config.validate(false).unwrap();
    }

    #[test]
    fn rejects_any_non_paper_mode() {
        for mode in [
            RuntimeMode::Shadow,
            RuntimeMode::DrySigned,
            RuntimeMode::Canary,
            RuntimeMode::Live,
        ] {
            let mut config = Config::default();
            config.runtime.mode = mode;
            assert!(config.validate(false).is_err());
        }
    }

    #[test]
    fn production_requires_explicit_config_and_operator_limits() {
        let mut config = Config::default();
        config.runtime.environment = RuntimeEnvironment::Production;
        assert!(config.validate(false).is_err());

        config.risk.max_order_usd = 4.01;
        assert!(config.validate(true).is_err());

        config.risk.max_order_usd = 4.00;
        config.general.initial_capital = 7.50;
        config.position_sizing.max_position_pct = 0.50;
        config.position_sizing.max_position_usd = 4.00;
        assert!(config.validate(true).is_ok());
    }

    #[test]
    fn rejects_dashboard_live_mode_changes() {
        let mut config = Config::default();
        config.dashboard.allow_live_mode_changes = true;
        assert!(config.validate(false).is_err());
    }
}
