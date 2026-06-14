use chrono::Utc;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use sqlx::Row;
use std::path::PathBuf;

use super::types::{StoredDecision, StoredMarket, StoredPosition};

pub struct Database {
    pub path: PathBuf,
    pool: SqlitePool,
}

impl Database {
    pub async fn new(path: &std::path::Path) -> Result<Self, sqlx::Error> {
        let path = path.to_path_buf();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&format!("sqlite:{}?mode=rwc", path.display()))
            .await?;

        let db = Self { path, pool };
        db.create_tables().await?;
        Ok(db)
    }

    async fn create_tables(&self) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS markets (
                id TEXT PRIMARY KEY,
                question TEXT NOT NULL,
                yes_price REAL NOT NULL,
                no_price REAL NOT NULL,
                volume REAL NOT NULL,
                end_date TEXT NOT NULL,
                created_at TEXT NOT NULL,
                yes_token_id TEXT
            );

            CREATE TABLE IF NOT EXISTS positions (
                id TEXT PRIMARY KEY,
                market_id TEXT NOT NULL,
                side TEXT NOT NULL,
                entry_price REAL NOT NULL,
                current_price REAL NOT NULL,
                size_usd REAL NOT NULL,
                status TEXT NOT NULL,
                opened_at TEXT NOT NULL,
                closed_at TEXT,
                FOREIGN KEY (market_id) REFERENCES markets(id)
            );

            CREATE TABLE IF NOT EXISTS decisions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                market_id TEXT NOT NULL,
                decision TEXT NOT NULL,
                q_model REAL NOT NULL,
                market_price REAL NOT NULL,
                ev_net REAL NOT NULL,
                size_usd REAL NOT NULL,
                timestamp TEXT NOT NULL,
                FOREIGN KEY (market_id) REFERENCES markets(id)
            );
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Migration: add yes_token_id if missing
        let _ = sqlx::query("ALTER TABLE markets ADD COLUMN yes_token_id TEXT")
            .execute(&self.pool)
            .await;

        Ok(())
    }

    pub async fn save_market(&self, market: &StoredMarket) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO markets (id, question, yes_price, no_price, volume, end_date, created_at, yes_token_id)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&market.id)
        .bind(&market.question)
        .bind(market.yes_price)
        .bind(market.no_price)
        .bind(market.volume)
        .bind(&market.end_date)
        .bind(market.created_at.to_rfc3339())
        .bind(&market.yes_token_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_markets(&self) -> Result<Vec<StoredMarket>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM markets")
            .fetch_all(&self.pool)
            .await?;

        let markets = rows
            .iter()
            .map(|row| {
                Ok(StoredMarket {
                    id: row.get("id"),
                    question: row.get("question"),
                    yes_price: row.get("yes_price"),
                    no_price: row.get("no_price"),
                    volume: row.get("volume"),
                    end_date: row.get("end_date"),
                    created_at: chrono::DateTime::parse_from_rfc3339(
                        row.get::<String, _>("created_at").as_str(),
                    )
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                    yes_token_id: row.get("yes_token_id"),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(markets)
    }

    pub async fn save_position(&self, position: &StoredPosition) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT OR REPLACE INTO positions (id, market_id, side, entry_price, current_price, size_usd, status, opened_at, closed_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&position.id)
        .bind(&position.market_id)
        .bind(&position.side)
        .bind(position.entry_price)
        .bind(position.current_price)
        .bind(position.size_usd)
        .bind(&position.status)
        .bind(position.opened_at.to_rfc3339())
        .bind(position.closed_at.map(|dt| dt.to_rfc3339()))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_open_positions(&self) -> Result<Vec<StoredPosition>, sqlx::Error> {
        let rows = sqlx::query("SELECT * FROM positions WHERE status = 'open'")
            .fetch_all(&self.pool)
            .await?;

        let positions = rows
            .iter()
            .map(|row| {
                Ok(StoredPosition {
                    id: row.get("id"),
                    market_id: row.get("market_id"),
                    side: row.get("side"),
                    entry_price: row.get("entry_price"),
                    current_price: row.get("current_price"),
                    size_usd: row.get("size_usd"),
                    status: row.get("status"),
                    opened_at: chrono::DateTime::parse_from_rfc3339(
                        row.get::<String, _>("opened_at").as_str(),
                    )
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                    closed_at: row.get::<Option<String>, _>("closed_at").and_then(|s| {
                        chrono::DateTime::parse_from_rfc3339(&s)
                            .map(|dt| dt.with_timezone(&Utc))
                            .ok()
                    }),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(positions)
    }

    pub async fn save_decision(
        &self,
        market_id: &str,
        decision: &str,
        q_model: f64,
        market_price: f64,
        ev_net: f64,
        size_usd: f64,
    ) -> Result<i64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            INSERT INTO decisions (market_id, decision, q_model, market_price, ev_net, size_usd, timestamp)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(market_id)
        .bind(decision)
        .bind(q_model)
        .bind(market_price)
        .bind(ev_net)
        .bind(size_usd)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    pub async fn get_decisions(&self, market_id: &str) -> Result<Vec<StoredDecision>, sqlx::Error> {
        let rows =
            sqlx::query("SELECT * FROM decisions WHERE market_id = ? ORDER BY timestamp DESC")
                .bind(market_id)
                .fetch_all(&self.pool)
                .await?;

        let decisions = rows
            .iter()
            .map(|row| {
                Ok(StoredDecision {
                    id: row.get("id"),
                    market_id: row.get("market_id"),
                    decision: row.get("decision"),
                    q_model: row.get("q_model"),
                    market_price: row.get("market_price"),
                    ev_net: row.get("ev_net"),
                    size_usd: row.get("size_usd"),
                    timestamp: chrono::DateTime::parse_from_rfc3339(
                        row.get::<String, _>("timestamp").as_str(),
                    )
                    .map(|dt| dt.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                })
            })
            .collect::<Result<Vec<_>, sqlx::Error>>()?;

        Ok(decisions)
    }
}

impl std::fmt::Debug for Database {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Database")
            .field("path", &self.path)
            .finish()
    }
}
