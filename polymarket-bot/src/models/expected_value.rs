use crate::config::EVConfig;

#[derive(Debug, Clone)]
pub struct EVResult {
    pub ev_gross: f64,
    pub cost: f64,
    pub ev_net: f64,
    pub edge: f64,
}

#[derive(Debug, Clone)]
pub struct EVCalculator {
    config: EVConfig,
}

impl EVCalculator {
    pub fn new() -> Self {
        Self {
            config: EVConfig::default(),
        }
    }

    pub fn with_config(config: EVConfig) -> Self {
        Self { config }
    }

    pub fn calculate(&self, q_model: f64, market_price: f64, cost_per_trade: f64) -> EVResult {
        let edge = q_model - market_price;
        let payout_if_yes = 1.0 / market_price;
        let ev_gross = q_model * payout_if_yes - 1.0;
        let cost = cost_per_trade;
        let ev_net = ev_gross - cost;

        EVResult {
            ev_gross,
            cost,
            ev_net,
            edge,
        }
    }

    pub fn has_positive_edge(&self, q_model: f64, market_price: f64) -> bool {
        let edge = q_model - market_price;
        edge > self.config.min_edge_pct
    }

    pub fn has_positive_ev(&self, q_model: f64, market_price: f64) -> bool {
        let result = self.calculate(q_model, market_price, self.config.cost_per_trade_pct);
        result.ev_net > self.config.min_ev_threshold
    }
}

impl Default for EVCalculator {
    fn default() -> Self {
        Self::new()
    }
}
