use crate::api::types::Market;
use crate::crypto::indicators::Timeframe;
use crate::crypto::signals::{Direction, Signal};

pub struct MarketMatcher {
    #[allow(dead_code)]
    gamma_base_url: String,
}

impl MarketMatcher {
    pub fn new(gamma_base_url: &str) -> Self {
        Self {
            gamma_base_url: gamma_base_url.to_string(),
        }
    }

    pub fn find_matching_market(
        &self,
        signal: &Signal,
        active_markets: &[Market],
    ) -> Option<Market> {
        let pattern = self.get_market_pattern(signal.timeframe);

        for market in active_markets {
            let question = market.question.to_lowercase();
            if question.contains(&pattern) && !market.closed {
                return Some(market.clone());
            }
        }

        None
    }

    fn get_market_pattern(&self, tf: Timeframe) -> String {
        match tf {
            Timeframe::M5 => "btc up or down 5m".to_string(),
            Timeframe::M15 => "btc up or down 15m".to_string(),
            Timeframe::H1 => "btc up or down 1h".to_string(),
            Timeframe::H4 => "btc up or down 4h".to_string(),
            Timeframe::D1 => "btc up or down daily".to_string(),
        }
    }

    pub fn get_token_for_direction(
        &self,
        market: &Market,
        direction: &Direction,
    ) -> Option<String> {
        // Parse clobTokenIds to get YES token
        let token_ids = market.clob_token_ids.as_ref()?;

        // Parse JSON array string like ["token_yes", "token_no"]
        let parsed: Vec<String> = serde_json::from_str(token_ids).ok()?;

        match direction {
            Direction::Up => parsed.first().cloned(),  // YES = Up
            Direction::Down => parsed.get(1).cloned(), // NO = Down
        }
    }
}
