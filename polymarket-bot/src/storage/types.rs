use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredMarket {
    pub id: String,
    pub question: String,
    pub yes_price: f64,
    pub no_price: f64,
    pub volume: f64,
    pub end_date: String,
    pub created_at: DateTime<Utc>,
    pub yes_token_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPosition {
    pub id: String,
    pub market_id: String,
    pub side: String,
    pub entry_price: f64,
    pub current_price: f64,
    pub size_usd: f64,
    pub status: String,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDecision {
    pub id: i64,
    pub market_id: String,
    pub decision: String,
    pub q_model: f64,
    pub market_price: f64,
    pub ev_net: f64,
    pub size_usd: f64,
    pub timestamp: DateTime<Utc>,
}
