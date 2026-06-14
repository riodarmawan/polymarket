use crate::web::state::{AppState, PriceData};
use std::time::Duration;
use serde::Deserialize;

// Binance data-api response
#[derive(Deserialize)]
struct BinancePriceResponse {
    price: String,
}

// CoinGecko response
#[derive(Deserialize)]
struct CoinGeckoResponse {
    bitcoin: Option<CoinGeckoPrice>,
}

#[derive(Deserialize)]
struct CoinGeckoPrice {
    usd: Option<f64>,
}

// Coinbase response
#[derive(Deserialize)]
struct CoinbaseResponse {
    data: Option<CoinbaseData>,
}

#[derive(Deserialize)]
struct CoinbaseData {
    amount: Option<String>,
}

async fn try_binance_data_api() -> Option<f64> {
    let url = "https://data-api.binance.vision/api/v3/ticker/price?symbol=BTCUSDT";
    let client = reqwest::Client::new();
    
    match client.get(url).timeout(Duration::from_secs(5)).send().await {
        Ok(resp) => {
            if let Ok(data) = resp.json::<BinancePriceResponse>().await {
                data.price.parse().ok()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

async fn try_binance_main_api() -> Option<f64> {
    let url = "https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT";
    let client = reqwest::Client::new();
    
    match client.get(url).timeout(Duration::from_secs(5)).send().await {
        Ok(resp) => {
            if let Ok(data) = resp.json::<BinancePriceResponse>().await {
                data.price.parse().ok()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

async fn try_coingecko() -> Option<f64> {
    let url = "https://api.coingecko.com/api/v3/simple/price?ids=bitcoin&vs_currencies=usd";
    let client = reqwest::Client::new();
    
    match client.get(url).timeout(Duration::from_secs(5)).send().await {
        Ok(resp) => {
            if let Ok(data) = resp.json::<CoinGeckoResponse>().await {
                data.bitcoin?.usd
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

async fn try_coinbase() -> Option<f64> {
    let url = "https://api.coinbase.com/v2/prices/BTC-USD/spot";
    let client = reqwest::Client::new();
    
    match client.get(url).timeout(Duration::from_secs(5)).send().await {
        Ok(resp) => {
            if let Ok(data) = resp.json::<CoinbaseResponse>().await {
                data.data?.amount?.parse().ok()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

async fn try_kraken() -> Option<f64> {
    let url = "https://api.kraken.com/0/public/Ticker?pair=XBTUSD";
    let client = reqwest::Client::new();
    
    match client.get(url).timeout(Duration::from_secs(5)).send().await {
        Ok(resp) => {
            let data: serde_json::Value = resp.json().await.ok()?;
            let result = data.get("result")?;
            let ticker = result.get("XXBTZUSD")?;
            let close = ticker.get("c")?;
            let price_str = close.get(0)?.as_str()?;
            price_str.parse().ok()
        }
        Err(_) => None,
    }
}

fn generate_mock_price(last_price: &mut f64) -> f64 {
    let change = (rand::random::<f64>() - 0.5) * 100.0;
    *last_price += change;
    *last_price = last_price.max(75000.0).min(85000.0);
    *last_price
}

pub async fn fetch_btc_price() -> Option<f64> {
    // Priority 1: Binance data-api (market data only)
    if let Some(price) = try_binance_data_api().await {
        tracing::debug!("BTC price from Binance data-api: {}", price);
        return Some(price);
    }
    
    // Priority 2: Binance main API
    if let Some(price) = try_binance_main_api().await {
        tracing::debug!("BTC price from Binance main: {}", price);
        return Some(price);
    }
    
    // Priority 3: CoinGecko
    if let Some(price) = try_coingecko().await {
        tracing::debug!("BTC price from CoinGecko: {}", price);
        return Some(price);
    }
    
    // Priority 4: Coinbase
    if let Some(price) = try_coinbase().await {
        tracing::debug!("BTC price from Coinbase: {}", price);
        return Some(price);
    }
    
    // Priority 5: Kraken
    if let Some(price) = try_kraken().await {
        tracing::debug!("BTC price from Kraken: {}", price);
        return Some(price);
    }
    
    // Priority 6: Mock fallback
    tracing::warn!("All price sources failed, using mock data");
    None
}

pub async fn run_price_proxy(state: AppState) {
    let mut last_price = 80000.0f64;
    let mut mock_price = 80000.0f64;
    
    loop {
        // Try to fetch real price from multiple sources
        if let Some(price) = fetch_btc_price().await {
            let change = if last_price > 0.0 {
                ((price - last_price) / last_price) * 100.0
            } else {
                0.0
            };
            
            let mut price_data = state.price.write().await;
            *price_data = PriceData {
                price,
                change_pct: change,
                timestamp: chrono::Utc::now().timestamp_millis(),
                source: "live".to_string(),
            };
            last_price = price;
            mock_price = price; // Sync mock with real price
        } else {
            // Fallback to mock data
            let price = generate_mock_price(&mut mock_price);
            let change = if last_price > 0.0 {
                ((price - last_price) / last_price) * 100.0
            } else {
                0.0
            };
            
            let mut price_data = state.price.write().await;
            *price_data = PriceData {
                price,
                change_pct: change,
                timestamp: chrono::Utc::now().timestamp_millis(),
                source: "mock".to_string(),
            };
            last_price = price;
        }
        
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
