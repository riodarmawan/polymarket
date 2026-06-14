use crate::analyzers::orderbook::OrderBookAnalyzer;
use crate::models::expected_value::EVCalculator;
use crate::models::position_sizing::PositionSizer;
use crate::models::probability::{ProbabilityModel, Signal};

#[derive(Debug, Clone, PartialEq)]
pub enum Decision {
    Buy {
        market_id: String,
        side: String,
        price: f64,
        size_usd: f64,
        reason: String,
    },
    Sell {
        market_id: String,
        side: String,
        price: f64,
        size_usd: f64,
        reason: String,
    },
    Hold {
        market_id: String,
        reason: String,
    },
    Skip {
        reason: String,
    },
}

#[derive(Debug, Clone)]
pub struct DecisionEngine {
    probability_model: ProbabilityModel,
    ev_calculator: EVCalculator,
    position_sizer: PositionSizer,
    orderbook_analyzer: OrderBookAnalyzer,
}

impl DecisionEngine {
    pub fn new() -> Self {
        Self {
            probability_model: ProbabilityModel::new(),
            ev_calculator: EVCalculator::new(),
            position_sizer: PositionSizer::new(),
            orderbook_analyzer: OrderBookAnalyzer::new(),
        }
    }

    pub fn evaluate(
        &self,
        market_id: &str,
        market_question: &str,
        market_price: f64,
        q_model: f64,
        signals: Vec<Signal>,
        capital: f64,
    ) -> Decision {
        let _prob = self.probability_model.calculate(market_price, &signals);

        if !self.ev_calculator.has_positive_edge(q_model, market_price) {
            return Decision::Skip {
                reason: "No positive edge".to_string(),
            };
        }

        if !self.ev_calculator.has_positive_ev(q_model, market_price) {
            return Decision::Skip {
                reason: "EV too low".to_string(),
            };
        }

        if !self
            .position_sizer
            .should_trade(q_model, market_price, capital)
        {
            return Decision::Skip {
                reason: "Position too small".to_string(),
            };
        }

        let size_usd = self
            .position_sizer
            .calculate_size(q_model, market_price, capital);

        Decision::Buy {
            market_id: market_id.to_string(),
            side: "YES".to_string(),
            price: market_price,
            size_usd,
            reason: format!(
                "Edge: {:.2}%, EV: positive, Size: ${:.2}",
                (q_model - market_price) * 100.0,
                size_usd
            ),
        }
    }
}

impl Default for DecisionEngine {
    fn default() -> Self {
        Self::new()
    }
}
