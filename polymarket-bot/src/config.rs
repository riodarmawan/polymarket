use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
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
    10.0
}
fn default_max_position_usd() -> f64 {
    100.0
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
            min_position_usd: 10.0,
            max_position_usd: 100.0,
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
    pub fn load() -> anyhow::Result<Self> {
        dotenvy::dotenv().ok();

        // Production must select its config explicitly. Development keeps the
        // existing executable-relative fallback.
        let explicit_config = std::env::var("POLYMARKET_CONFIG").ok();
        let config_path = if let Some(explicit_path) = &explicit_config {
            Path::new(explicit_path).to_path_buf()
        } else if let Ok(exe_path) = std::env::current_exe() {
            let exe_dir = exe_path.parent().unwrap_or_else(|| Path::new("."));
            exe_dir.join("config/default.toml")
        } else {
            Path::new("config/default.toml").to_path_buf()
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
        let mut config = config;
        if let Ok(val) = std::env::var("POLY_LOG_LEVEL") {
            config.general.log_level = val;
        }
        if let Ok(val) = std::env::var("POLY_DATA_DIR") {
            config.general.data_dir = val;
        }

        Ok(config)
    }
}
