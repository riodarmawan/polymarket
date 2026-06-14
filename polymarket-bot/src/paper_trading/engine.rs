use crate::engine::decision::Decision;
use crate::storage::database::Database;
use crate::storage::types::StoredPosition;
use chrono::Utc;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct PaperTradingEngine {
    pub capital: f64,
    pub initial_capital: f64,
}

impl PaperTradingEngine {
    pub fn new(initial_capital: f64) -> Self {
        Self {
            capital: initial_capital,
            initial_capital,
        }
    }

    pub async fn execute_trade(
        &mut self,
        decision: &Decision,
        db: &Database,
    ) -> Option<StoredPosition> {
        match decision {
            Decision::Buy {
                market_id,
                side,
                price,
                size_usd,
                ..
            } => {
                if *size_usd > self.capital {
                    return None;
                }

                let position = StoredPosition {
                    id: Uuid::new_v4().to_string(),
                    market_id: market_id.clone(),
                    side: side.clone(),
                    entry_price: *price,
                    current_price: *price,
                    size_usd: *size_usd,
                    status: "open".to_string(),
                    opened_at: Utc::now(),
                    closed_at: None,
                };

                if let Err(e) = db.save_position(&position).await {
                    eprintln!("Failed to save position: {}", e);
                    return None;
                }

                self.capital -= size_usd;
                Some(position)
            }
            _ => None,
        }
    }

    pub fn get_portfolio_summary(&self, open_positions: &[StoredPosition]) -> PortfolioSummary {
        let total_invested: f64 = open_positions.iter().map(|p| p.size_usd).sum();
        let current_value: f64 = open_positions
            .iter()
            .map(|p| {
                let shares = p.size_usd / p.entry_price;
                shares * p.current_price
            })
            .sum();

        let unrealized_pnl = current_value - total_invested;
        let total_value = self.capital + current_value;

        PortfolioSummary {
            initial_capital: self.initial_capital,
            current_capital: self.capital,
            total_invested,
            current_value,
            unrealized_pnl,
            total_value,
            total_return: (total_value - self.initial_capital) / self.initial_capital,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PortfolioSummary {
    pub initial_capital: f64,
    pub current_capital: f64,
    pub total_invested: f64,
    pub current_value: f64,
    pub unrealized_pnl: f64,
    pub total_value: f64,
    pub total_return: f64,
}
