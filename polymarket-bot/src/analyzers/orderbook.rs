use crate::api::types::OrderBookLevel;
use crate::config::OrderBookConfig;

#[derive(Debug, Clone)]
pub struct OrderBookSnapshot {
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
}

#[derive(Debug, Clone)]
pub struct OrderBookMetrics {
    pub spread_pct: f64,
    pub bid_depth: f64,
    pub ask_depth: f64,
    pub order_book_imbalance: f64,
    pub liquidity_score: f64,
    pub is_tradeable: bool,
}

#[derive(Debug, Clone)]
pub struct OrderBookAnalyzer {
    config: OrderBookConfig,
}

impl OrderBookAnalyzer {
    pub fn new() -> Self {
        Self {
            config: OrderBookConfig::default(),
        }
    }

    pub fn with_config(config: OrderBookConfig) -> Self {
        Self { config }
    }

    pub fn analyze(&self, snapshot: &OrderBookSnapshot) -> OrderBookMetrics {
        let bid_depth: f64 = snapshot.bids.iter().map(|l| l.size).sum();
        let ask_depth: f64 = snapshot.asks.iter().map(|l| l.size).sum();
        let total_depth = bid_depth + ask_depth;

        let best_bid = snapshot.bids.first().map(|l| l.price).unwrap_or(0.0);
        let best_ask = snapshot.asks.first().map(|l| l.price).unwrap_or(1.0);
        let spread = best_ask - best_bid;
        let mid_price = (best_bid + best_ask) / 2.0;
        let spread_pct = if mid_price > 0.0 {
            spread / mid_price
        } else {
            0.0
        };

        let order_book_imbalance = if total_depth > 0.0 {
            (bid_depth - ask_depth) / total_depth
        } else {
            0.0
        };

        let liquidity_score = if total_depth > 0.0 {
            (total_depth / 10000.0).min(1.0)
        } else {
            0.0
        };

        let is_tradeable =
            spread_pct <= self.config.max_spread_pct && total_depth >= self.config.min_depth;

        OrderBookMetrics {
            spread_pct,
            bid_depth,
            ask_depth,
            order_book_imbalance,
            liquidity_score,
            is_tradeable,
        }
    }
}

impl Default for OrderBookAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}
