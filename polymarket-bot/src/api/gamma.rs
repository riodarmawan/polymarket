use crate::api::types::{Event, Market};
use crate::error::BotError;
use reqwest::Client;

#[derive(Debug, Clone)]
pub struct GammaClient {
    pub base_url: String,
    client: Client,
}

impl GammaClient {
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: Client::new(),
        }
    }

    pub async fn fetch_markets(&self, limit: usize) -> Result<Vec<Market>, BotError> {
        let url = format!("{}/api/markets?limit={}", self.base_url, limit);
        let response = self.client.get(&url).send().await?;
        let markets: Vec<Market> = response.json().await?;
        Ok(markets)
    }

    pub async fn fetch_market_by_id(&self, id: &str) -> Result<Option<Market>, BotError> {
        let url = format!("{}/api/markets/{}", self.base_url, id);
        let response = self.client.get(&url).send().await?;

        if response.status().is_success() {
            let market: Market = response.json().await?;
            Ok(Some(market))
        } else {
            Ok(None)
        }
    }

    pub async fn fetch_events(&self, limit: usize) -> Result<Vec<Event>, BotError> {
        let url = format!("{}/api/events?limit={}", self.base_url, limit);
        let response = self.client.get(&url).send().await?;
        let events: Vec<Event> = response.json().await?;
        Ok(events)
    }

    pub async fn search_markets(&self, query: &str) -> Result<Vec<Market>, BotError> {
        let url = format!("{}/api/markets?_q={}", self.base_url, query);
        let response = self.client.get(&url).send().await?;
        let markets: Vec<Market> = response.json().await?;
        Ok(markets)
    }
}
