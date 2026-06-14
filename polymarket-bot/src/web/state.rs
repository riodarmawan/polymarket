use crate::crypto::binance_ws::Candle;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Deserialize)]
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
            capital: 2.0,
            max_order: 0.50,
            timeframe: "15m".to_string(),
            auto_trade: true,
            min_edge: 0.10,
            max_entry_price: 0.60,
            risk_fraction: 0.05,
        }
    }
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
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
    pub fn new() -> Self {
        Self {
            price: Arc::new(RwLock::new(PriceData::default())),
            settings: Arc::new(RwLock::new(Settings::default())),
            trades: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(StatsInfo::default())),
            stats_5m: Arc::new(RwLock::new(StatsInfo::default())),
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
        }
    }
}
