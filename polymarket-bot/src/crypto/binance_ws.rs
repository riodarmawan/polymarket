use tokio::sync::broadcast;
use tungstenite::connect;
use serde::{Deserialize, Serialize};
use anyhow::Result;

#[derive(Debug, Clone, Deserialize)]
pub struct KlineEvent {
    #[serde(rename = "e")]
    pub event_type: String,
    #[serde(rename = "E")]
    pub event_time: i64,
    #[serde(rename = "s")]
    pub symbol: String,
    pub k: Kline,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Kline {
    #[serde(rename = "t")]
    pub start_time: i64,
    #[serde(rename = "T")]
    pub close_time: i64,
    #[serde(rename = "o")]
    pub open: String,
    #[serde(rename = "c")]
    pub close: String,
    #[serde(rename = "h")]
    pub high: String,
    #[serde(rename = "l")]
    pub low: String,
    #[serde(rename = "v")]
    pub volume: String,
    #[serde(rename = "n")]
    pub number_of_trades: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BinanceCandleResponse {
    #[serde(rename = "openTime")]
    pub open_time: i64,
    #[serde(rename = "open")]
    pub open: String,
    #[serde(rename = "high")]
    pub high: String,
    #[serde(rename = "low")]
    pub low: String,
    #[serde(rename = "close")]
    pub close: String,
    #[serde(rename = "volume")]
    pub volume: String,
    #[serde(rename = "closeTime")]
    pub close_time: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Candle {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl From<Kline> for Candle {
    fn from(k: Kline) -> Self {
        Self {
            timestamp: k.start_time,
            open: k.open.parse().unwrap_or(0.0),
            high: k.high.parse().unwrap_or(0.0),
            low: k.low.parse().unwrap_or(0.0),
            close: k.close.parse().unwrap_or(0.0),
            volume: k.volume.parse().unwrap_or(0.0),
        }
    }
}

impl From<BinanceCandleResponse> for Candle {
    fn from(c: BinanceCandleResponse) -> Self {
        Self {
            timestamp: c.open_time,
            open: c.open.parse().unwrap_or(0.0),
            high: c.high.parse().unwrap_or(0.0),
            low: c.low.parse().unwrap_or(0.0),
            close: c.close.parse().unwrap_or(0.0),
            volume: c.volume.parse().unwrap_or(0.0),
        }
    }
}

pub struct BinanceWsClient {
    tx: broadcast::Sender<Candle>,
}

impl BinanceWsClient {
    pub fn new(buffer_size: usize) -> Self {
        let (tx, _) = broadcast::channel(buffer_size);
        Self { tx }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Candle> {
        self.tx.subscribe()
    }

    pub fn start(&self) -> Result<()> {
        let tx = self.tx.clone();
        
        std::thread::spawn(move || -> Result<()> {
            let url = "wss://stream.binance.com:9443/ws/btcusdt@kline_1m";
            let (mut socket, _) = connect(url)?;
            
            loop {
                let msg = socket.read_message()?;
                if let tungstenite::Message::Text(text) = msg {
                    if let Ok(event) = serde_json::from_str::<KlineEvent>(&text) {
                        let candle = event.k.into();
                        let _ = tx.send(candle);
                    }
                }
            }
        });
        
        Ok(())
    }
}

pub struct BinanceRestClient {
    base_url: String,
}

impl BinanceRestClient {
    pub fn new() -> Self {
        Self {
            base_url: "https://api.binance.com".to_string(),
        }
    }
    
    pub async fn fetch_candles(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> Result<Vec<Candle>> {
        let url = format!(
            "{}/api/v3/klines?symbol={}&interval={}&limit={}",
            self.base_url, symbol, interval, limit
        );
        
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;
        let candles: Vec<BinanceCandleResponse> = response.json().await?;
        
        Ok(candles.into_iter().map(|c| c.into()).collect())
    }
    
    pub async fn fetch_candles_since(
        &self,
        symbol: &str,
        interval: &str,
        start_time: i64,
        end_time: Option<i64>,
    ) -> Result<Vec<Candle>> {
        let mut url = format!(
            "{}/api/v3/klines?symbol={}&interval={}&startTime={}",
            self.base_url, symbol, interval, start_time
        );
        
        if let Some(end) = end_time {
            url.push_str(&format!("&endTime={}", end));
        }
        
        let client = reqwest::Client::new();
        let response = client.get(&url).send().await?;
        let candles: Vec<BinanceCandleResponse> = response.json().await?;
        
        Ok(candles.into_iter().map(|c| c.into()).collect())
    }

    pub async fn fetch_candles_range(
        &self,
        symbol: &str,
        interval: &str,
        start_time: i64,
        end_time: i64,
    ) -> Result<Vec<Candle>> {
        let client = reqwest::Client::new();
        let mut cursor = start_time;
        let mut candles = Vec::new();

        while cursor < end_time {
            let url = format!(
                "https://data-api.binance.vision/api/v3/klines?symbol={symbol}&interval={interval}&startTime={cursor}&endTime={end_time}&limit=1000"
            );
            let rows: Vec<Vec<serde_json::Value>> = client.get(&url).send().await?.json().await?;
            if rows.is_empty() {
                break;
            }

            for row in &rows {
                let parse = |index: usize| {
                    row.get(index)
                        .and_then(|value| value.as_str())
                        .and_then(|value| value.parse::<f64>().ok())
                        .unwrap_or(0.0)
                };
                candles.push(Candle {
                    timestamp: row.first().and_then(|value| value.as_i64()).unwrap_or(0),
                    open: parse(1),
                    high: parse(2),
                    low: parse(3),
                    close: parse(4),
                    volume: parse(5),
                });
            }

            let last_timestamp = rows
                .last()
                .and_then(|row| row.first())
                .and_then(|value| value.as_i64())
                .unwrap_or(cursor);
            let next_cursor = last_timestamp + 60_000;
            if next_cursor <= cursor {
                break;
            }
            cursor = next_cursor;
        }

        Ok(candles)
    }

    pub async fn fetch_recent_candles(
        &self,
        symbol: &str,
        interval: &str,
        limit: usize,
    ) -> Result<Vec<Candle>> {
        let url = format!(
            "https://data-api.binance.vision/api/v3/klines?symbol={symbol}&interval={interval}&limit={limit}"
        );
        let rows: Vec<Vec<serde_json::Value>> =
            reqwest::Client::new().get(&url).send().await?.json().await?;

        Ok(rows
            .iter()
            .map(|row| {
                let parse = |index: usize| {
                    row.get(index)
                        .and_then(|value| value.as_str())
                        .and_then(|value| value.parse::<f64>().ok())
                        .unwrap_or(0.0)
                };
                Candle {
                    timestamp: row.first().and_then(|value| value.as_i64()).unwrap_or(0),
                    open: parse(1),
                    high: parse(2),
                    low: parse(3),
                    close: parse(4),
                    volume: parse(5),
                }
            })
            .collect())
    }
}
