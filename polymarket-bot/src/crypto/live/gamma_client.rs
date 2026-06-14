use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct GammaMarket {
    pub id: String,
    pub question: String,
    #[serde(rename = "outcomePrices")]
    pub outcome_prices: String,
    #[serde(rename = "clobTokenIds")]
    pub clob_token_ids: Option<String>,
    pub volume: serde_json::Value,
    pub liquidity: serde_json::Value,
    pub active: bool,
    pub closed: bool,
    #[serde(rename = "endDate")]
    pub end_date: String,
    #[serde(rename = "enableOrderBook", default)]
    pub enable_order_book: bool,
    #[serde(default)]
    pub tags: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GammaEvent {
    pub slug: String,
    pub title: String,
    pub description: Option<String>,
    #[serde(rename = "startDate")]
    pub start_date: Option<String>,
    #[serde(rename = "endDate")]
    pub end_date: Option<String>,
    pub active: Option<bool>,
    pub closed: Option<bool>,
    pub volume: Option<serde_json::Value>,
    #[serde(rename = "volume24hr")]
    pub volume_24hr: Option<serde_json::Value>,
    pub liquidity: Option<serde_json::Value>,
    #[serde(rename = "enableOrderBook")]
    pub enable_order_book: Option<bool>,
    pub markets: Option<Vec<GammaMarket>>,
    pub tags: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClobOrderBook {
    pub market: String,
    pub asset_id: String,
    pub bids: Vec<ClobOrderLevel>,
    pub asks: Vec<ClobOrderLevel>,
    pub hash: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClobOrderLevel {
    pub price: String,
    pub size: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClobPrice {
    pub price: String,
    pub side: String,
}

impl GammaMarket {
    pub fn yes_price(&self) -> f64 {
        let prices: Vec<String> = serde_json::from_str(&self.outcome_prices).unwrap_or_default();
        prices.first().and_then(|p| p.parse().ok()).unwrap_or(0.5)
    }

    pub fn no_price(&self) -> f64 {
        let prices: Vec<String> = serde_json::from_str(&self.outcome_prices).unwrap_or_default();
        prices.get(1).and_then(|p| p.parse().ok()).unwrap_or(0.5)
    }

    pub fn volume_usd(&self) -> f64 {
        match &self.volume {
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
            serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn liquidity_usd(&self) -> f64 {
        match &self.liquidity {
            serde_json::Value::Number(n) => n.as_f64().unwrap_or(0.0),
            serde_json::Value::String(s) => s.parse().unwrap_or(0.0),
            _ => 0.0,
        }
    }

    pub fn is_crypto_related(&self) -> bool {
        let q = self.question.to_lowercase();
        let crypto_keywords = [
            "bitcoin",
            "btc",
            "ethereum",
            "eth",
            "solana",
            "sol",
            "xrp",
            "doge",
            "crypto",
            "coinbase",
            "binance",
            "stablecoin",
            "blockchain",
            "etf",
            "microstrategy",
            "treasury",
            "usdt",
            "usdc",
            "defi",
            "nft",
        ];
        crypto_keywords.iter().any(|kw| q.contains(kw))
    }

    pub fn has_orderbook(&self) -> bool {
        self.enable_order_book && self.clob_token_ids.is_some()
    }

    pub fn get_tags(&self) -> Vec<String> {
        match &self.tags {
            Some(serde_json::Value::Array(arr)) => arr
                .iter()
                .filter_map(|t| t.as_str().map(|s| s.to_string()))
                .collect(),
            _ => vec![],
        }
    }

    pub fn get_token_ids(&self) -> Vec<String> {
        self.clob_token_ids
            .as_ref()
            .and_then(|ids| serde_json::from_str(ids).ok())
            .unwrap_or_default()
    }
}

pub struct GammaClient {
    base_url: String,
    client: reqwest::Client,
}

impl GammaClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn fetch_markets(&self, limit: u32) -> Result<Vec<GammaMarket>> {
        let url = format!("{}/api/markets?limit={}", self.base_url, limit);
        let resp = self.client.get(&url).send().await?;
        let markets: Vec<GammaMarket> = resp.json().await?;
        Ok(markets)
    }

    pub async fn fetch_events(&self, limit: u32) -> Result<Vec<serde_json::Value>> {
        let url = format!("{}/api/events?limit={}", self.base_url, limit);
        let resp = self.client.get(&url).send().await?;
        let events: Vec<serde_json::Value> = resp.json().await?;
        Ok(events)
    }

    pub async fn fetch_event_by_slug(&self, slug: &str) -> Result<Option<GammaEvent>> {
        let url = format!("{}/api/events/slug/{}", self.base_url, slug);
        match self.client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let event: GammaEvent = resp.json().await?;
                    Ok(Some(event))
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }

    pub async fn search_markets(&self, query: &str) -> Result<Vec<GammaMarket>> {
        let url = format!("{}/api/markets?limit=500", self.base_url);
        let resp = self.client.get(&url).send().await?;
        let all_markets: Vec<GammaMarket> = resp.json().await?;
        let query_lower = query.to_lowercase();
        let filtered: Vec<GammaMarket> = all_markets
            .into_iter()
            .filter(|m| m.question.to_lowercase().contains(&query_lower))
            .collect();
        Ok(filtered)
    }

    pub async fn discover_crypto_markets(&self) -> Result<Vec<GammaMarket>> {
        let all_markets = self.fetch_markets(500).await?;

        let crypto_markets: Vec<GammaMarket> = all_markets
            .into_iter()
            .filter(|m| m.active && !m.closed && m.is_crypto_related() && m.has_orderbook())
            .collect();

        let mut sorted = crypto_markets;
        sorted.sort_by(|a, b| {
            b.volume_usd()
                .partial_cmp(&a.volume_usd())
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(sorted)
    }
}

pub struct ClobClient {
    base_url: String,
    client: reqwest::Client,
}

impl ClobClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
        }
    }

    pub async fn fetch_orderbook(&self, token_id: &str) -> Result<ClobOrderBook> {
        let url = format!("{}/api/book?token_id={}", self.base_url, token_id);
        let resp = self.client.get(&url).send().await?;
        let orderbook: ClobOrderBook = resp.json().await?;
        Ok(orderbook)
    }

    pub async fn fetch_price(&self, token_id: &str, side: &str) -> Result<Option<f64>> {
        let url = format!(
            "{}/api/price?token_id={}&side={}",
            self.base_url, token_id, side
        );
        match self.client.get(&url).send().await {
            Ok(resp) => {
                if resp.status().is_success() {
                    let price: ClobPrice = resp.json().await?;
                    Ok(price.price.parse().ok())
                } else {
                    Ok(None)
                }
            }
            Err(_) => Ok(None),
        }
    }
}

// Helper functions for generating BTC Up/Down slugs
pub fn generate_updown_slug(asset: &str, interval: &str, start_ts: i64) -> String {
    format!("{}-updown-{}-{}", asset, interval, start_ts)
}

pub fn get_current_interval_start(interval_minutes: u32) -> i64 {
    let now = chrono::Utc::now().timestamp();
    let interval_seconds = interval_minutes as i64 * 60;
    now - (now % interval_seconds)
}

pub fn get_next_interval_start(interval_minutes: u32) -> i64 {
    let current_start = get_current_interval_start(interval_minutes);
    let interval_seconds = interval_minutes as i64 * 60;
    current_start + interval_seconds
}

pub fn get_remaining_seconds(end_ts: i64) -> i64 {
    let now = chrono::Utc::now().timestamp();
    (end_ts - now).max(0)
}
