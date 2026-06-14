use crate::api::types::{OrderBook, OrderBookLevel, Trade};
use crate::error::BotError;
use reqwest::Client;

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PricePoint {
    pub t: u64,
    pub p: f64,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PriceHistory {
    pub history: Vec<PricePoint>,
}

#[derive(Debug, Clone)]
pub struct ClobClient {
    pub base_url: String,
    client: Client,
}

impl ClobClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: Client::new(),
        }
    }

    pub async fn get_order_book(&self, token_id: &str) -> Result<OrderBook, BotError> {
        let url = format!("{}/book?token_id={}", self.base_url, token_id);
        let response = self.client.get(&url).send().await?;
        let data: serde_json::Value = response.json().await?;

        let bids = data["bids"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|item| OrderBookLevel {
                        price: item["price"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        size: item["size"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();

        let asks = data["asks"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .map(|item| OrderBookLevel {
                        price: item["price"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                        size: item["size"].as_str().unwrap_or("0").parse().unwrap_or(0.0),
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(OrderBook { bids, asks })
    }

    pub async fn get_price(&self, token_id: &str) -> Result<f64, BotError> {
        let url = format!("{}/price?token_id={}", self.base_url, token_id);
        let response = self.client.get(&url).send().await?;
        let data: serde_json::Value = response.json().await?;

        data["price"]
            .as_str()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| BotError::ParseError("Invalid price format".to_string()))
    }

    pub async fn get_midpoint(&self, token_id: &str) -> Result<f64, BotError> {
        let url = format!("{}/midpoint?token_id={}", self.base_url, token_id);
        let response = self.client.get(&url).send().await?;
        let data: serde_json::Value = response.json().await?;

        data["mid"]
            .as_str()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| BotError::ParseError("Invalid midpoint format".to_string()))
    }

    pub async fn get_spread(&self, token_id: &str) -> Result<(f64, f64), BotError> {
        let url = format!("{}/spread?token_id={}", self.base_url, token_id);
        let response = self.client.get(&url).send().await?;
        let data: serde_json::Value = response.json().await?;

        let bid = data["bid"]
            .as_str()
            .and_then(|p| p.parse().ok())
            .unwrap_or(0.0);
        let ask = data["ask"]
            .as_str()
            .and_then(|p| p.parse().ok())
            .unwrap_or(0.0);

        Ok((bid, ask))
    }

    pub async fn get_trades(&self, token_id: &str, limit: usize) -> Result<Vec<Trade>, BotError> {
        let url = format!(
            "{}/trades?token_id={}&limit={}",
            self.base_url, token_id, limit
        );
        let response = self.client.get(&url).send().await?;
        let trades: Vec<Trade> = response.json().await?;
        Ok(trades)
    }

    pub async fn fetch_price_history(
        &self,
        token_id: &str,
        interval: &str,
        fidelity: u32,
    ) -> Result<PriceHistory, crate::error::BotError> {
        let url = format!(
            "{}/api/prices-history?market={}&interval={}&fidelity={}",
            self.base_url, token_id, interval, fidelity
        );
        let response = self.client.get(&url).send().await?;
        let history: PriceHistory = response.json().await?;
        Ok(history)
    }
}
