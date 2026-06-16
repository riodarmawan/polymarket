use crate::api::gamma::GammaClient;
use crate::error::BotError;
use crate::storage::database::Database;
use crate::storage::types::StoredMarket;
use chrono::{DateTime, NaiveDateTime, Utc};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub struct DataCollector {
    pub gamma_base_url: String,
    pub max_hours_to_resolution: f64,
}

impl DataCollector {
    pub fn new(gamma_base_url: String, max_hours_to_resolution: f64) -> Self {
        Self {
            gamma_base_url,
            max_hours_to_resolution,
        }
    }

    pub async fn collect_markets(&self, db: &Database, limit: usize) -> Result<usize, BotError> {
        let client = GammaClient::new(&self.gamma_base_url);
        let markets = client.fetch_markets(limit).await?;

        let mut count = 0;
        let now = Utc::now();
        let max_duration = chrono::Duration::hours(self.max_hours_to_resolution as i64);

        for market in markets {
            // Only collect active, non-closed markets
            if !market.active || market.closed {
                continue;
            }

            // Filter by end date if available
            if let Ok(end_date) = DateTime::parse_from_rfc3339(&market.end_date) {
                let end_utc = end_date.with_timezone(&Utc);
                let time_to_resolution = end_utc - now;

                // Include markets that:
                // 1. Resolve within max_hours_to_resolution (upcoming)
                // 2. Are past deadline but still active (need resolution)
                if time_to_resolution > max_duration {
                    continue; // Too far in the future
                }
                // Skip if time_to_resolution is very negative (already resolved long ago)
                if time_to_resolution < -chrono::Duration::hours(24) {
                    continue;
                }
            }
            // If we can't parse the date, still include the market (might be important)

            let stored = StoredMarket {
                id: market.id.clone(),
                question: market.question.clone(),
                yes_price: market.yes_price(),
                no_price: market.no_price(),
                volume: market.volume_24h(),
                end_date: market.end_date.clone(),
                created_at: Utc::now(),
                yes_token_id: market.yes_token_id(),
            };

            if let Err(e) = db.save_market(&stored).await {
                eprintln!("Failed to save market {}: {}", market.id, e);
            } else {
                count += 1;
            }
        }

        Ok(count)
    }
}
