use anyhow::Result;
use serde::Deserialize;
use std::time::Duration;

async fn send_with_retry(builder: reqwest::RequestBuilder) -> Result<reqwest::Response> {
    let retry = builder.try_clone();
    match builder.send().await {
        Ok(response)
            if response.status().is_server_error()
                || response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS =>
        {
            tokio::time::sleep(Duration::from_millis(250)).await;
            Ok(retry
                .ok_or_else(|| anyhow::anyhow!("request cannot be retried"))?
                .send()
                .await?
                .error_for_status()?)
        }
        Ok(response) => Ok(response.error_for_status()?),
        Err(error) if error.is_timeout() || error.is_connect() => {
            tokio::time::sleep(Duration::from_millis(250)).await;
            Ok(retry
                .ok_or_else(|| anyhow::anyhow!("request cannot be retried"))?
                .send()
                .await?
                .error_for_status()?)
        }
        Err(error) => Err(error.into()),
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GammaMarket {
    pub id: String,
    pub question: String,
    #[serde(rename = "outcomePrices")]
    pub outcome_prices: String,
    #[serde(rename = "clobTokenIds")]
    pub clob_token_ids: Option<String>,
    #[serde(default)]
    pub outcomes: String,
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
    #[serde(default)]
    pub timestamp: String,
    #[serde(default)]
    pub min_order_size: String,
    #[serde(default)]
    pub tick_size: String,
    #[serde(default)]
    pub neg_risk: bool,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuyQuote {
    pub average_price: f64,
    pub shares: f64,
    pub available_depth_usd: f64,
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

    pub fn mapped_up_down_tokens(&self) -> Option<(String, String)> {
        let outcomes: Vec<String> = serde_json::from_str(&self.outcomes).ok()?;
        let token_ids = self.get_token_ids();
        if outcomes.len() != token_ids.len() || outcomes.len() != 2 {
            return None;
        }

        let mut up = None;
        let mut down = None;
        for (outcome, token_id) in outcomes.into_iter().zip(token_ids) {
            match outcome.to_ascii_lowercase().as_str() {
                "up" | "yes" => up = Some(token_id),
                "down" | "no" => down = Some(token_id),
                _ => return None,
            }
        }
        Some((up?, down?))
    }

    pub fn winning_direction(&self) -> Option<String> {
        let prices: Vec<String> = serde_json::from_str(&self.outcome_prices).ok()?;
        let up = prices.first()?.parse::<f64>().ok()?;
        let down = prices.get(1)?.parse::<f64>().ok()?;

        if up >= 0.99 && down <= 0.01 {
            Some("Up".to_string())
        } else if down >= 0.99 && up <= 0.01 {
            Some("Down".to_string())
        } else {
            None
        }
    }
}

impl ClobOrderBook {
    pub fn validated_top_of_book(
        &self,
        expected_token: &str,
    ) -> Option<(Option<f64>, Option<f64>)> {
        if self.asset_id != expected_token {
            return None;
        }
        let parse = |level: &ClobOrderLevel| -> Option<(f64, f64)> {
            let price = level.price.parse::<f64>().ok()?;
            let size = level.size.parse::<f64>().ok()?;
            ((0.0..=1.0).contains(&price) && size > 0.0).then_some((price, size))
        };
        let best_bid = self
            .bids
            .iter()
            .filter_map(parse)
            .map(|(price, _)| price)
            .reduce(f64::max);
        let best_ask = self
            .asks
            .iter()
            .filter_map(parse)
            .map(|(price, _)| price)
            .reduce(f64::min);

        match (best_bid, best_ask) {
            (None, None) => None,
            (Some(bid), Some(ask)) if bid > ask => None,
            prices => Some(prices),
        }
    }

    pub fn validated_best_bid_ask(&self, expected_token: &str) -> Option<(f64, f64)> {
        let (best_bid, best_ask) = self.validated_top_of_book(expected_token)?;
        Some((best_bid?, best_ask?))
    }

    pub fn tick_size(&self) -> Option<f64> {
        self.tick_size
            .parse::<f64>()
            .ok()
            .filter(|value| *value > 0.0)
    }

    pub fn min_order_size(&self) -> Option<f64> {
        self.min_order_size
            .parse::<f64>()
            .ok()
            .filter(|value| *value > 0.0)
    }

    pub fn timestamp_ms(&self) -> Option<i64> {
        let value = self.timestamp.parse::<i64>().ok()?;
        Some(if value < 10_000_000_000 {
            value * 1_000
        } else {
            value
        })
    }

    pub fn quote_buy_usd(&self, expected_token: &str, requested_usd: f64) -> Option<BuyQuote> {
        if self.asset_id != expected_token || !requested_usd.is_finite() || requested_usd <= 0.0 {
            return None;
        }
        let mut asks: Vec<(f64, f64)> = self
            .asks
            .iter()
            .filter_map(|level| {
                let price = level.price.parse::<f64>().ok()?;
                let shares = level.size.parse::<f64>().ok()?;
                ((0.0..=1.0).contains(&price) && price > 0.0 && shares > 0.0)
                    .then_some((price, shares))
            })
            .collect();
        asks.sort_by(|a, b| a.0.total_cmp(&b.0));

        let available_depth_usd = asks.iter().map(|(price, shares)| price * shares).sum();
        let mut remaining_usd = requested_usd;
        let mut filled_shares = 0.0;
        let mut spent_usd = 0.0;
        for (price, available_shares) in asks {
            let spend = remaining_usd.min(price * available_shares);
            filled_shares += spend / price;
            spent_usd += spend;
            remaining_usd -= spend;
            if remaining_usd <= 0.000_000_1 {
                break;
            }
        }
        if remaining_usd > 0.000_000_1 || filled_shares <= 0.0 {
            return None;
        }
        Some(BuyQuote {
            average_price: spent_usd / filled_shares,
            shares: filled_shares,
            available_depth_usd,
        })
    }
}

#[derive(Clone)]
pub struct GammaClient {
    base_url: String,
    client: reqwest::Client,
}

impl GammaClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("valid Gamma HTTP client"),
        }
    }

    fn endpoint(&self, path: &str) -> String {
        if self.base_url.contains("localhost") || self.base_url.contains("127.0.0.1") {
            format!("{}/api{}", self.base_url.trim_end_matches('/'), path)
        } else {
            format!("{}{}", self.base_url.trim_end_matches('/'), path)
        }
    }

    pub async fn fetch_markets(&self, limit: u32) -> Result<Vec<GammaMarket>> {
        let url = self.endpoint(&format!("/markets?limit={}", limit));
        let resp = send_with_retry(self.client.get(&url)).await?;
        let markets: Vec<GammaMarket> = resp.json().await?;
        Ok(markets)
    }

    pub async fn fetch_events(&self, limit: u32) -> Result<Vec<serde_json::Value>> {
        let url = self.endpoint(&format!("/events?limit={}", limit));
        let resp = send_with_retry(self.client.get(&url)).await?;
        let events: Vec<serde_json::Value> = resp.json().await?;
        Ok(events)
    }

    pub async fn fetch_event_by_slug(&self, slug: &str) -> Result<Option<GammaEvent>> {
        let url = self.endpoint(&format!("/events/slug/{}", slug));
        let resp = match send_with_retry(self.client.get(&url)).await {
            Ok(response) => response,
            Err(error)
                if error
                    .downcast_ref::<reqwest::Error>()
                    .and_then(|error| error.status())
                    == Some(reqwest::StatusCode::NOT_FOUND) =>
            {
                return Ok(None);
            }
            Err(error) => return Err(error),
        };

        let event: GammaEvent = resp.json().await?;
        Ok(Some(event))
    }

    pub async fn search_markets(&self, query: &str) -> Result<Vec<GammaMarket>> {
        let url = self.endpoint("/markets?limit=500");
        let resp = send_with_retry(self.client.get(&url)).await?;
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

#[derive(Clone)]
pub struct ClobClient {
    base_url: String,
    client: reqwest::Client,
}

impl ClobClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("valid CLOB HTTP client"),
        }
    }

    fn endpoint(&self, path: &str) -> String {
        if self.base_url.contains("localhost") || self.base_url.contains("127.0.0.1") {
            format!("{}/api{}", self.base_url.trim_end_matches('/'), path)
        } else {
            format!("{}{}", self.base_url.trim_end_matches('/'), path)
        }
    }

    pub async fn fetch_orderbook(&self, token_id: &str) -> Result<ClobOrderBook> {
        let url = self.endpoint(&format!("/book?token_id={}", token_id));
        let resp = send_with_retry(self.client.get(&url)).await?;
        let orderbook: ClobOrderBook = resp.json().await?;
        Ok(orderbook)
    }

    pub async fn fetch_price(&self, token_id: &str, side: &str) -> Result<Option<f64>> {
        let url = self.endpoint(&format!("/price?token_id={}&side={}", token_id, side));
        match send_with_retry(self.client.get(&url)).await {
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

    pub async fn fetch_fee_rate_bps(&self, token_id: &str) -> Result<u64> {
        let url = self.endpoint(&format!("/fee-rate?token_id={}", token_id));
        let response: serde_json::Value =
            send_with_retry(self.client.get(&url)).await?.json().await?;
        response
            .get("base_fee")
            .or_else(|| response.get("fee_rate_bps"))
            .and_then(|value| value.as_u64())
            .ok_or_else(|| anyhow::anyhow!("CLOB fee-rate payload missing base_fee"))
    }

    pub async fn fetch_server_time_ms(&self) -> Result<i64> {
        let url = self.endpoint("/time");
        let value: serde_json::Value = send_with_retry(self.client.get(&url)).await?.json().await?;
        let timestamp = value
            .as_i64()
            .or_else(|| value.as_str().and_then(|value| value.parse::<i64>().ok()))
            .ok_or_else(|| anyhow::anyhow!("CLOB time payload is not an integer"))?;
        Ok(if timestamp < 10_000_000_000 {
            timestamp * 1_000
        } else {
            timestamp
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn market(outcome_prices: &str) -> GammaMarket {
        GammaMarket {
            id: "test".to_string(),
            question: "BTC Up or Down".to_string(),
            outcome_prices: outcome_prices.to_string(),
            clob_token_ids: None,
            outcomes: String::new(),
            volume: serde_json::Value::Null,
            liquidity: serde_json::Value::Null,
            active: false,
            closed: true,
            end_date: String::new(),
            enable_order_book: false,
            tags: None,
        }
    }

    #[test]
    fn reads_official_winning_direction() {
        assert_eq!(
            market("[\"1\", \"0\"]").winning_direction().as_deref(),
            Some("Up")
        );
        assert_eq!(
            market("[\"0\", \"1\"]").winning_direction().as_deref(),
            Some("Down")
        );
        assert_eq!(market("[\"0.5\", \"0.5\"]").winning_direction(), None);
    }

    #[test]
    fn maps_tokens_by_outcome_name_not_array_position() {
        let mut market = market("[\"0.5\", \"0.5\"]");
        market.outcomes = "[\"Down\", \"Up\"]".to_string();
        market.clob_token_ids = Some("[\"down-token\", \"up-token\"]".to_string());

        assert_eq!(
            market.mapped_up_down_tokens(),
            Some(("up-token".to_string(), "down-token".to_string()))
        );
    }

    #[test]
    fn validates_orderbook_and_finds_true_best_prices() {
        let book = ClobOrderBook {
            market: "test".to_string(),
            asset_id: "up-token".to_string(),
            bids: vec![
                ClobOrderLevel {
                    price: "0.40".to_string(),
                    size: "1".to_string(),
                },
                ClobOrderLevel {
                    price: "0.49".to_string(),
                    size: "1".to_string(),
                },
            ],
            asks: vec![
                ClobOrderLevel {
                    price: "0.60".to_string(),
                    size: "1".to_string(),
                },
                ClobOrderLevel {
                    price: "0.51".to_string(),
                    size: "1".to_string(),
                },
            ],
            hash: "hash".to_string(),
            timestamp: "1781433900000".to_string(),
            min_order_size: "5".to_string(),
            tick_size: "0.01".to_string(),
            neg_risk: false,
        };

        assert_eq!(book.validated_best_bid_ask("up-token"), Some((0.49, 0.51)));
        assert_eq!(book.validated_best_bid_ask("wrong-token"), None);
        assert_eq!(book.tick_size(), Some(0.01));
        assert_eq!(book.min_order_size(), Some(5.0));
        assert_eq!(book.timestamp_ms(), Some(1_781_433_900_000));
        let quote = book.quote_buy_usd("up-token", 0.51).unwrap();
        assert!((quote.average_price - 0.51).abs() < f64::EPSILON);
        assert!((quote.shares - 1.0).abs() < f64::EPSILON);
        assert!((quote.available_depth_usd - 1.11).abs() < 0.000_001);
        assert_eq!(book.quote_buy_usd("up-token", 2.0), None);

        let mut one_sided = book.clone();
        one_sided.asks.clear();
        assert_eq!(
            one_sided.validated_top_of_book("up-token"),
            Some((Some(0.49), None))
        );
        assert_eq!(one_sided.validated_best_bid_ask("up-token"), None);
    }
}
