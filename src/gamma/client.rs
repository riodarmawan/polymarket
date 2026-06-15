use crate::gamma::types::*;
use reqwest::Client;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tracing::instrument;

const GAMMA_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(104, 18, 34, 205));
const CLOB_IP: IpAddr = IpAddr::V4(Ipv4Addr::new(104, 18, 34, 205));

#[derive(Debug, Clone)]
pub struct GammaClient {
    http: Client,
}

impl GammaClient {
    pub fn new() -> Self {
        let addr = SocketAddr::new(GAMMA_IP, 443);
        let http = Client::builder()
            .resolve_to_addrs("gamma-api.polymarket.com", &[addr])
            .build()
            .expect("failed to build reqwest client");
        Self { http }
    }

    fn url(path: &str) -> String {
        format!("https://gamma-api.polymarket.com{path}")
    }

    // ── Events ──────────────────────────────────────────────

    #[instrument(skip(self))]
    pub async fn list_events(&self, limit: u32, offset: u32) -> color_eyre::Result<Vec<Event>> {
        let url = Self::url(&format!("/events?limit={limit}&offset={offset}"));
        tracing::debug!(%url, "fetching events");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        let events = serde_json::from_value(body)?;
        Ok(events)
    }

    #[instrument(skip(self))]
    pub async fn get_event(&self, id_or_slug: &str) -> color_eyre::Result<Event> {
        let url = Self::url(&format!("/events/{id_or_slug}"));
        tracing::debug!(%url, "fetching event");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    #[instrument(skip(self))]
    pub async fn get_event_by_slug(&self, slug: &str) -> color_eyre::Result<Event> {
        let url = Self::url(&format!("/events/slug/{slug}"));
        tracing::debug!(%url, "fetching event by slug");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    // ── Markets ─────────────────────────────────────────────

    #[instrument(skip(self))]
    pub async fn list_markets(
        &self,
        limit: u32,
        offset: u32,
        tag: Option<&str>,
        closed: Option<bool>,
    ) -> color_eyre::Result<Vec<Market>> {
        let mut url = Self::url(&format!("/markets?limit={limit}&offset={offset}"));
        if let Some(t) = tag {
            url.push_str(&format!("&tag={t}"));
        }
        if let Some(c) = closed {
            url.push_str(&format!("&closed={c}"));
        }
        tracing::debug!(%url, "fetching markets");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        let markets = serde_json::from_value(body)?;
        Ok(markets)
    }

    #[instrument(skip(self))]
    pub async fn get_market(&self, id_or_slug: &str) -> color_eyre::Result<Market> {
        let url = Self::url(&format!("/markets/{id_or_slug}"));
        tracing::debug!(%url, "fetching market");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    // ── Search ──────────────────────────────────────────────

    #[instrument(skip(self))]
    pub async fn search(&self, query: &str) -> color_eyre::Result<SearchResult> {
        let url = Self::url(&format!("/public-search?query={query}"));
        tracing::debug!(%url, "searching");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    // ── Tags ────────────────────────────────────────────────

    #[instrument(skip(self))]
    pub async fn list_tags(&self) -> color_eyre::Result<Vec<Tag>> {
        let url = Self::url("/tags");
        tracing::debug!(%url, "fetching tags");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }
}

impl Default for GammaClient {
    fn default() -> Self {
        Self::new()
    }
}

// ── CLOB Client ───────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ClobClient {
    http: Client,
}

impl ClobClient {
    pub fn new() -> Self {
        let addr = SocketAddr::new(CLOB_IP, 443);
        let http = Client::builder()
            .resolve_to_addrs("clob.polymarket.com", &[addr])
            .build()
            .expect("failed to build CLOB reqwest client");
        Self { http }
    }

    #[instrument(skip(self))]
    pub async fn fetch_price_history(
        &self,
        market: &str,
        interval: &str,
        fidelity: u32,
    ) -> color_eyre::Result<serde_json::Value> {
        let url = format!(
            "https://clob.polymarket.com/prices-history?market={market}&interval={interval}&fidelity={fidelity}"
        );
        tracing::debug!(%url, "fetching price history from CLOB");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body)
    }

    #[instrument(skip(self))]
    pub async fn fetch_orderbook(&self, token_id: &str) -> color_eyre::Result<serde_json::Value> {
        let url = format!("https://clob.polymarket.com/book?token_id={token_id}");
        tracing::debug!(%url, "fetching orderbook from CLOB");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    #[instrument(skip(self))]
    pub async fn fetch_price(
        &self,
        token_id: &str,
        side: &str,
    ) -> color_eyre::Result<serde_json::Value> {
        let url = format!("https://clob.polymarket.com/price?token_id={token_id}&side={side}");
        tracing::debug!(%url, "fetching price from CLOB");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    #[instrument(skip(self))]
    pub async fn fetch_fee_rate(&self, token_id: &str) -> color_eyre::Result<serde_json::Value> {
        let url = format!("https://clob.polymarket.com/fee-rate?token_id={token_id}");
        tracing::debug!(%url, "fetching fee rate from CLOB");
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }

    #[instrument(skip(self))]
    pub async fn fetch_server_time(&self) -> color_eyre::Result<serde_json::Value> {
        let url = "https://clob.polymarket.com/time";
        tracing::debug!(%url, "fetching server time from CLOB");
        let resp = self.http.get(url).send().await?.error_for_status()?;
        Ok(resp.json().await?)
    }
}

impl Default for ClobClient {
    fn default() -> Self {
        Self::new()
    }
}
