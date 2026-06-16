use crate::config::CryptoConfig;
use crate::crypto::binance_ws::BinanceWsClient;
use crate::crypto::indicators::{IndicatorEngine, Timeframe};
use crate::crypto::market_matcher::MarketMatcher;
use crate::crypto::signals::SignalEngine;
use anyhow::Result;
use std::collections::HashMap;

pub struct CryptoEngine {
    #[allow(dead_code)]
    config: CryptoConfig,
    ws_client: BinanceWsClient,
    #[allow(dead_code)]
    indicator_engine: IndicatorEngine,
    signal_engine: SignalEngine,
    #[allow(dead_code)]
    market_matcher: MarketMatcher,
}

impl CryptoEngine {
    pub fn new(config: CryptoConfig, gamma_base_url: &str) -> Self {
        let ws_client = BinanceWsClient::new(1000);
        let indicator_engine = IndicatorEngine::new();
        let signal_engine = SignalEngine::new();
        let market_matcher = MarketMatcher::new(gamma_base_url);

        Self {
            config,
            ws_client,
            indicator_engine,
            signal_engine,
            market_matcher,
        }
    }

    pub async fn run(&self) -> Result<()> {
        // Start WebSocket connection
        self.ws_client.start()?;
        let mut rx = self.ws_client.subscribe();

        // Buffer candles per timeframe
        let mut candle_buffers: HashMap<Timeframe, Vec<_>> = HashMap::new();

        println!("Crypto engine started. Listening for BTC price...");

        while let Ok(candle) = rx.recv().await {
            // Add candle to buffer (using M5 as placeholder, should aggregate 1m to 5m)
            let tf = Timeframe::M5;
            candle_buffers.entry(tf).or_default().push(candle.clone());

            // Keep last 100 candles per buffer
            if let Some(buf) = candle_buffers.get_mut(&tf) {
                if buf.len() > 100 {
                    buf.remove(0);
                }
            }

            // Generate signals every 5 minutes (when timestamp is divisible by 300)
            if candle.timestamp % 300 == 0 {
                let signals = self.signal_engine.generate_signals(&candle_buffers);

                for signal in signals {
                    println!(
                        "Signal: {:?} {} confidence {:.2}",
                        signal.timeframe, signal.direction, signal.confidence
                    );

                    // TODO: Find matching market and execute trade
                }
            }
        }

        Ok(())
    }
}
