use crate::config::PositionSizingConfig;

#[derive(Debug, Clone)]
pub struct PositionSizer {
    config: PositionSizingConfig,
}

impl PositionSizer {
    pub fn new() -> Self {
        Self {
            config: PositionSizingConfig::default(),
        }
    }

    pub fn with_config(config: PositionSizingConfig) -> Self {
        Self { config }
    }

    pub fn calculate_size(&self, q_model: f64, market_price: f64, capital: f64) -> f64 {
        let edge = q_model - market_price;
        if edge <= 0.0 {
            return 0.0;
        }

        let odds = 1.0 / market_price;
        let kelly_bet = (edge * odds - (1.0 - q_model)) / odds;
        let adjusted_bet = kelly_bet * self.config.kelly_fraction;
        let position_usd = (adjusted_bet * capital).max(0.0);

        position_usd
            .max(self.config.min_position_usd)
            .min(self.config.max_position_usd)
            .min(capital * self.config.max_position_pct)
    }

    pub fn should_trade(&self, q_model: f64, market_price: f64, capital: f64) -> bool {
        let size = self.calculate_size(q_model, market_price, capital);
        size >= self.config.min_position_usd
    }
}

impl Default for PositionSizer {
    fn default() -> Self {
        Self::new()
    }
}
