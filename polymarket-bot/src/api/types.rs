use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Market {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub question: String,
    #[serde(rename = "outcomePrices", default)]
    pub outcome_prices: String,
    #[serde(default)]
    pub volume: serde_json::Value,
    #[serde(rename = "endDate", default)]
    pub end_date: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub slug: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub closed: bool,
    #[serde(default)]
    pub liquidity: serde_json::Value,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(rename = "conditionId", default)]
    pub condition_id: String,
    #[serde(rename = "enableOrderBook", default)]
    pub enable_order_book: bool,
    #[serde(default)]
    pub fee: Option<serde_json::Value>,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub image: String,
    #[serde(rename = "openInterest", default)]
    pub open_interest: f64,
    #[serde(default)]
    pub outcomes: String,
    #[serde(rename = "startDate", default)]
    pub start_date: String,
    #[serde(default)]
    pub tags: Option<serde_json::Value>,
    #[serde(default)]
    pub tokens: Option<serde_json::Value>,
    #[serde(rename = "clobTokenIds", default)]
    pub clob_token_ids: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Event {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub markets: Vec<Market>,
}

#[derive(Debug, Clone)]
pub struct OrderBook {
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
}

#[derive(Debug, Clone)]
pub struct OrderBookLevel {
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Trade {
    pub id: String,
    pub market: String,
    pub side: String,
    pub price: String,
    pub size: String,
    #[serde(rename = "timestamp")]
    pub timestamp: String,
}

impl Market {
    pub fn yes_price(&self) -> f64 {
        let prices: Vec<String> = serde_json::from_str(&self.outcome_prices).unwrap_or_default();
        prices.first().and_then(|p| p.parse().ok()).unwrap_or(0.5)
    }

    pub fn no_price(&self) -> f64 {
        let prices: Vec<String> = serde_json::from_str(&self.outcome_prices).unwrap_or_default();
        prices.get(1).and_then(|p| p.parse().ok()).unwrap_or(0.5)
    }

    pub fn volume_24h(&self) -> f64 {
        match &self.volume {
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
            serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn yes_token_id(&self) -> Option<String> {
        let ids: Vec<String> = serde_json::from_str(self.clob_token_ids.as_deref()?).ok()?;
        ids.into_iter().next()
    }
}
