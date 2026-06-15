use crate::web::state::{Settings, StatsInfo, TradeInfo, UpDownMarket};
use anyhow::{Context, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::{Row, Sqlite, Transaction};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

const MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        r#"
    CREATE TABLE IF NOT EXISTS dashboard_snapshots (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        payload_json TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS audit_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        event_type TEXT NOT NULL,
        runtime_mode TEXT NOT NULL,
        strategy_version TEXT NOT NULL,
        detail_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_audit_events_created_at
        ON audit_events(created_at);
    "#,
    ),
    (
        2,
        r#"
    CREATE TABLE IF NOT EXISTS execution_intents (
        client_key TEXT PRIMARY KEY,
        market_slug TEXT NOT NULL,
        timeframe TEXT NOT NULL,
        runtime_mode TEXT NOT NULL,
        strategy_version TEXT NOT NULL,
        detail_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_execution_intents_created_at
        ON execution_intents(created_at);
    "#,
    ),
    (
        3,
        r#"
    CREATE TABLE IF NOT EXISTS paper_trades (
        market_slug TEXT PRIMARY KEY,
        timeframe TEXT NOT NULL,
        direction TEXT NOT NULL,
        entry_price REAL NOT NULL,
        exit_price REAL,
        shares REAL NOT NULL,
        size_usd REAL NOT NULL,
        fee_usd REAL NOT NULL,
        price_to_beat REAL NOT NULL,
        end_ts INTEGER NOT NULL,
        confidence REAL NOT NULL,
        edge REAL NOT NULL,
        pnl REAL,
        status TEXT NOT NULL,
        opened_at_ms INTEGER NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS capital_ledger (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        event_key TEXT NOT NULL UNIQUE,
        market_slug TEXT NOT NULL,
        timeframe TEXT NOT NULL,
        entry_type TEXT NOT NULL,
        amount_usd REAL NOT NULL,
        balance_usd REAL NOT NULL,
        created_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS daily_risk_state (
        date_utc TEXT NOT NULL,
        timeframe TEXT NOT NULL,
        orders_count INTEGER NOT NULL,
        realized_pnl_usd REAL NOT NULL,
        consecutive_losses INTEGER NOT NULL,
        open_positions INTEGER NOT NULL,
        current_capital_usd REAL NOT NULL,
        peak_capital_usd REAL NOT NULL,
        max_drawdown REAL NOT NULL,
        updated_at TEXT NOT NULL,
        PRIMARY KEY(date_utc, timeframe)
    );
    "#,
    ),
    (
        4,
        r#"
    CREATE TABLE IF NOT EXISTS market_windows (
        slug TEXT PRIMARY KEY,
        asset TEXT NOT NULL,
        timeframe TEXT NOT NULL,
        start_ts INTEGER NOT NULL,
        end_ts INTEGER NOT NULL,
        up_token_id TEXT,
        down_token_id TEXT,
        latest_status TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS market_snapshots (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        market_slug TEXT NOT NULL,
        up_best_ask REAL,
        up_best_bid REAL,
        down_best_ask REAL,
        down_best_bid REAL,
        spread REAL NOT NULL,
        price_to_beat REAL NOT NULL,
        current_price REAL NOT NULL,
        status TEXT NOT NULL,
        captured_at TEXT NOT NULL,
        FOREIGN KEY(market_slug) REFERENCES market_windows(slug)
    );

    CREATE INDEX IF NOT EXISTS idx_market_snapshots_slug_captured
        ON market_snapshots(market_slug, captured_at);
    "#,
    ),
    (
        5,
        r#"
    CREATE TABLE IF NOT EXISTS market_data_quality (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        market_slug TEXT NOT NULL,
        data_status TEXT NOT NULL,
        detail TEXT NOT NULL,
        token_mapping_valid INTEGER NOT NULL,
        captured_at_ms INTEGER NOT NULL,
        recorded_at TEXT NOT NULL,
        FOREIGN KEY(market_slug) REFERENCES market_windows(slug)
    );

    CREATE INDEX IF NOT EXISTS idx_market_data_quality_slug_recorded
        ON market_data_quality(market_slug, recorded_at);
    "#,
    ),
    (
        6,
        r#"
    CREATE TABLE IF NOT EXISTS market_execution_metadata (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        market_slug TEXT NOT NULL,
        tick_size REAL,
        min_order_size REAL,
        fee_rate_bps INTEGER,
        negative_risk INTEGER,
        up_executable_depth_usd REAL NOT NULL,
        down_executable_depth_usd REAL NOT NULL,
        up_expected_fill_price REAL,
        down_expected_fill_price REAL,
        clock_drift_ms INTEGER,
        recorded_at TEXT NOT NULL,
        FOREIGN KEY(market_slug) REFERENCES market_windows(slug)
    );

    CREATE INDEX IF NOT EXISTS idx_market_execution_metadata_slug_recorded
        ON market_execution_metadata(market_slug, recorded_at);
    "#,
    ),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardSnapshot {
    pub settings: Settings,
    pub trades: Vec<TradeInfo>,
    pub stats_15m: StatsInfo,
    pub stats_5m: StatsInfo,
}

#[derive(Clone)]
pub struct DashboardStore {
    path: PathBuf,
    pool: SqlitePool,
}

impl DashboardStore {
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create database directory {}", parent.display()))?;
        }

        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path.display()))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        let store = Self {
            path: path.to_path_buf(),
            pool,
        };
        store.migrate().await?;
        Ok(store)
    }

    async fn migrate(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        for (version, migration) in MIGRATIONS {
            let applied =
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM schema_migrations WHERE version = ?")
                    .bind(version)
                    .fetch_one(&self.pool)
                    .await?;
            if applied > 0 {
                continue;
            }

            let mut transaction = self.pool.begin().await?;
            for statement in migration.split(';').map(str::trim).filter(|sql| !sql.is_empty()) {
                sqlx::query(statement).execute(&mut *transaction).await?;
            }
            sqlx::query("INSERT INTO schema_migrations (version, applied_at) VALUES (?, ?)")
                .bind(version)
                .bind(Utc::now().to_rfc3339())
                .execute(&mut *transaction)
                .await?;
            transaction.commit().await?;
        }
        Ok(())
    }

    pub async fn load_snapshot(&self) -> Result<Option<DashboardSnapshot>> {
        let row = sqlx::query("SELECT payload_json FROM dashboard_snapshots WHERE id = 1")
            .fetch_optional(&self.pool)
            .await?;
        row.map(|row| {
            serde_json::from_str(row.get::<String, _>("payload_json").as_str())
                .context("invalid dashboard snapshot JSON")
        })
        .transpose()
    }

    pub async fn save_snapshot(
        &self,
        snapshot: &DashboardSnapshot,
        event_type: &str,
        runtime_mode: &str,
        strategy_version: &str,
        detail: serde_json::Value,
    ) -> Result<()> {
        let payload = serde_json::to_string(snapshot)?;
        let detail = serde_json::to_string(&detail)?;
        let timestamp = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        let previous = sqlx::query("SELECT payload_json FROM dashboard_snapshots WHERE id = 1")
            .fetch_optional(&mut *transaction)
            .await?
            .map(|row| {
                serde_json::from_str::<DashboardSnapshot>(
                    row.get::<String, _>("payload_json").as_str(),
                )
            })
            .transpose()
            .context("invalid previous dashboard snapshot JSON")?;

        sqlx::query(
            r#"
            INSERT INTO dashboard_snapshots (id, payload_json, updated_at)
            VALUES (1, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                payload_json = excluded.payload_json,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(payload)
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?;

        sync_normalized_state(&mut transaction, snapshot, previous.as_ref(), &timestamp).await?;

        sqlx::query(
            r#"
            INSERT INTO audit_events
                (event_type, runtime_mode, strategy_version, detail_json, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(event_type)
        .bind(runtime_mode)
        .bind(strategy_version)
        .bind(detail)
        .bind(timestamp)
        .execute(&mut *transaction)
        .await?;

        transaction.commit().await?;
        Ok(())
    }

    pub async fn audit_event(
        &self,
        event_type: &str,
        runtime_mode: &str,
        strategy_version: &str,
        detail: serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO audit_events
                (event_type, runtime_mode, strategy_version, detail_json, created_at)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(event_type)
        .bind(runtime_mode)
        .bind(strategy_version)
        .bind(serde_json::to_string(&detail)?)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn reserve_execution_intent(
        &self,
        client_key: &str,
        market_slug: &str,
        timeframe: &str,
        runtime_mode: &str,
        strategy_version: &str,
        detail: serde_json::Value,
    ) -> Result<bool> {
        let result = sqlx::query(
            r#"
            INSERT INTO execution_intents
                (client_key, market_slug, timeframe, runtime_mode, strategy_version, detail_json, created_at)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(client_key) DO NOTHING
            "#,
        )
        .bind(client_key)
        .bind(market_slug)
        .bind(timeframe)
        .bind(runtime_mode)
        .bind(strategy_version)
        .bind(serde_json::to_string(&detail)?)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn record_market_scan(&self, markets: &[UpDownMarket]) -> Result<()> {
        let timestamp = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        for market in markets {
            sqlx::query(
                r#"
                INSERT INTO market_windows (
                    slug, asset, timeframe, start_ts, end_ts, up_token_id,
                    down_token_id, latest_status, updated_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(slug) DO UPDATE SET
                    up_token_id = excluded.up_token_id,
                    down_token_id = excluded.down_token_id,
                    latest_status = excluded.latest_status,
                    updated_at = excluded.updated_at
                "#,
            )
            .bind(&market.slug)
            .bind(&market.asset)
            .bind(&market.interval)
            .bind(market.start_ts)
            .bind(market.end_ts)
            .bind(&market.up_token_id)
            .bind(&market.down_token_id)
            .bind(&market.status)
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await?;

            sqlx::query(
                r#"
                INSERT INTO market_execution_metadata (
                    market_slug, tick_size, min_order_size, fee_rate_bps, negative_risk,
                    up_executable_depth_usd, down_executable_depth_usd,
                    up_expected_fill_price, down_expected_fill_price, clock_drift_ms, recorded_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&market.slug)
            .bind(market.tick_size)
            .bind(market.min_order_size)
            .bind(market.fee_rate_bps.map(|value| value as i64))
            .bind(market.negative_risk)
            .bind(market.up_executable_depth_usd)
            .bind(market.down_executable_depth_usd)
            .bind(market.up_expected_fill_price)
            .bind(market.down_expected_fill_price)
            .bind(market.clock_drift_ms)
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await?;

            sqlx::query(
                r#"
                INSERT INTO market_data_quality (
                    market_slug, data_status, detail, token_mapping_valid, captured_at_ms, recorded_at
                ) VALUES (?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&market.slug)
            .bind(market.data_status.as_str())
            .bind(&market.data_detail)
            .bind(market.token_mapping_valid)
            .bind(market.captured_at_ms)
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await?;

            sqlx::query(
                r#"
                INSERT INTO market_snapshots (
                    market_slug, up_best_ask, up_best_bid, down_best_ask, down_best_bid,
                    spread, price_to_beat, current_price, status, captured_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&market.slug)
            .bind(market.up_best_ask)
            .bind(market.up_best_bid)
            .bind(market.down_best_ask)
            .bind(market.down_best_bid)
            .bind(market.spread)
            .bind(market.price_to_beat)
            .bind(market.current_price)
            .bind(&market.status)
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn migration_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM schema_migrations")
            .fetch_one(&self.pool)
            .await?)
    }

    pub async fn audit_event_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar("SELECT COUNT(*) FROM audit_events")
            .fetch_one(&self.pool)
            .await?)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub async fn backup_to(&self, destination: &Path) -> Result<()> {
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create backup directory {}", parent.display()))?;
        }
        sqlx::query("VACUUM INTO ?")
            .bind(destination.display().to_string())
            .execute(&self.pool)
            .await
            .with_context(|| format!("failed to back up database to {}", destination.display()))?;
        Ok(())
    }
}

async fn sync_normalized_state(
    transaction: &mut Transaction<'_, Sqlite>,
    snapshot: &DashboardSnapshot,
    previous: Option<&DashboardSnapshot>,
    timestamp: &str,
) -> Result<()> {
    let previous_trades: HashMap<&str, &TradeInfo> = previous
        .map(|snapshot| {
            snapshot
                .trades
                .iter()
                .map(|trade| (trade.market_slug.as_str(), trade))
                .collect()
        })
        .unwrap_or_default();

    for trade in &snapshot.trades {
        sqlx::query(
            r#"
            INSERT INTO paper_trades (
                market_slug, timeframe, direction, entry_price, exit_price, shares,
                size_usd, fee_usd, price_to_beat, end_ts, confidence, edge, pnl,
                status, opened_at_ms, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(market_slug) DO UPDATE SET
                exit_price = excluded.exit_price,
                pnl = excluded.pnl,
                status = excluded.status,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&trade.market_slug)
        .bind(&trade.timeframe)
        .bind(&trade.direction)
        .bind(trade.entry_price)
        .bind(trade.exit_price)
        .bind(trade.shares)
        .bind(trade.size_usd)
        .bind(trade.fee_usd)
        .bind(trade.price_to_beat)
        .bind(trade.end_ts)
        .bind(trade.confidence)
        .bind(trade.edge)
        .bind(trade.pnl)
        .bind(&trade.status)
        .bind(trade.timestamp)
        .bind(timestamp)
        .execute(&mut **transaction)
        .await?;

        let mut ledger_entries = Vec::new();
        match previous_trades.get(trade.market_slug.as_str()) {
            None => {
                ledger_entries.push((
                format!("{}:opened", trade.market_slug),
                "trade_reserved",
                -(trade.size_usd + trade.fee_usd),
                ));
                if trade.status == "settled" {
                    ledger_entries.push((
                        format!("{}:settled", trade.market_slug),
                        "settlement",
                        trade.pnl.unwrap_or(0.0) + trade.size_usd + trade.fee_usd,
                    ));
                }
            }
            Some(previous) if previous.status == "open" && trade.status == "settled" => {
                ledger_entries.push((
                format!("{}:settled", trade.market_slug),
                "settlement",
                trade.pnl.unwrap_or(0.0) + trade.size_usd + trade.fee_usd,
                ));
            }
            _ => {}
        }
        for (event_key, entry_type, amount) in ledger_entries {
            let balance = if trade.timeframe == "5m" {
                snapshot.stats_5m.current_capital
            } else {
                snapshot.stats_15m.current_capital
            };
            sqlx::query(
                r#"
                INSERT INTO capital_ledger (
                    event_key, market_slug, timeframe, entry_type, amount_usd, balance_usd, created_at
                ) VALUES (?, ?, ?, ?, ?, ?, ?)
                ON CONFLICT(event_key) DO NOTHING
                "#,
            )
            .bind(event_key)
            .bind(&trade.market_slug)
            .bind(&trade.timeframe)
            .bind(entry_type)
            .bind(amount)
            .bind(balance)
            .bind(timestamp)
            .execute(&mut **transaction)
            .await?;
        }
    }

    let now = Utc::now();
    let date_utc = now.date_naive().to_string();
    let day_start_ms = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("UTC midnight must be valid")
        .and_utc()
        .timestamp_millis();
    for (timeframe, stats) in [
        ("15m", &snapshot.stats_15m),
        ("5m", &snapshot.stats_5m),
    ] {
        let timeframe_trades: Vec<&TradeInfo> = snapshot
            .trades
            .iter()
            .filter(|trade| trade.timeframe == timeframe)
            .collect();
        let daily_trades: Vec<&TradeInfo> = timeframe_trades
            .iter()
            .copied()
            .filter(|trade| trade.timestamp >= day_start_ms)
            .collect();
        let consecutive_losses = daily_trades
            .iter()
            .rev()
            .take_while(|trade| trade.status == "settled" && trade.pnl.unwrap_or(0.0) < 0.0)
            .count();
        let open_positions = timeframe_trades
            .iter()
            .filter(|trade| trade.status == "open")
            .count();
        let daily_realized_pnl: f64 = daily_trades
            .iter()
            .filter(|trade| trade.status == "settled")
            .filter_map(|trade| trade.pnl)
            .sum();
        sqlx::query(
            r#"
            INSERT INTO daily_risk_state (
                date_utc, timeframe, orders_count, realized_pnl_usd, consecutive_losses,
                open_positions, current_capital_usd, peak_capital_usd, max_drawdown, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(date_utc, timeframe) DO UPDATE SET
                orders_count = excluded.orders_count,
                realized_pnl_usd = excluded.realized_pnl_usd,
                consecutive_losses = excluded.consecutive_losses,
                open_positions = excluded.open_positions,
                current_capital_usd = excluded.current_capital_usd,
                peak_capital_usd = excluded.peak_capital_usd,
                max_drawdown = excluded.max_drawdown,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&date_utc)
        .bind(timeframe)
        .bind(daily_trades.len() as i64)
        .bind(daily_realized_pnl)
        .bind(consecutive_losses as i64)
        .bind(open_positions as i64)
        .bind(stats.current_capital)
        .bind(stats.peak_capital)
        .bind(stats.max_drawdown)
        .bind(timestamp)
        .execute(&mut **transaction)
        .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn snapshot() -> DashboardSnapshot {
        DashboardSnapshot {
            settings: Settings::default(),
            trades: vec![TradeInfo {
                timestamp: 1,
                market_slug: "btc-updown-5m-test".to_string(),
                timeframe: "5m".to_string(),
                direction: "Up".to_string(),
                entry_price: 0.50,
                exit_price: None,
                shares: 0.20,
                size_usd: 0.10,
                fee_usd: 0.002,
                price_to_beat: 100.0,
                end_ts: 2,
                confidence: 0.70,
                edge: 0.20,
                pnl: None,
                status: "open".to_string(),
            }],
            stats_15m: StatsInfo::default(),
            stats_5m: StatsInfo::default(),
        }
    }

    #[tokio::test]
    async fn restores_snapshot_and_audit_after_reopen() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("dashboard.db");
        let store = DashboardStore::open(&path).await.unwrap();
        store
            .save_snapshot(
                &snapshot(),
                "paper_trade_opened",
                "paper",
                "test-strategy",
                serde_json::json!({"market_slug": "btc-updown-5m-test"}),
            )
            .await
            .unwrap();
        store
            .record_market_scan(&[UpDownMarket {
                asset: "btc".to_string(),
                slug: "btc-updown-5m-test".to_string(),
                interval: "5m".to_string(),
                start_ts: 1,
                end_ts: 301,
                remaining_seconds: 300,
                up_token_id: Some("up-token".to_string()),
                down_token_id: Some("down-token".to_string()),
                up_best_ask: Some(0.51),
                up_best_bid: Some(0.49),
                down_best_ask: Some(0.52),
                down_best_bid: Some(0.48),
                spread: 0.03,
                status: "live".to_string(),
                price_to_beat: 100.0,
                current_price: 101.0,
                captured_at_ms: Utc::now().timestamp_millis(),
                data_status: crate::web::state::DataStatus::Ready,
                data_detail: "test".to_string(),
                token_mapping_valid: true,
                tick_size: Some(0.01),
                min_order_size: Some(5.0),
                fee_rate_bps: Some(0),
                negative_risk: Some(false),
                up_executable_depth_usd: 100.0,
                down_executable_depth_usd: 100.0,
                up_expected_fill_price: Some(0.51),
                down_expected_fill_price: Some(0.52),
                clock_drift_ms: Some(0),
            }])
            .await
            .unwrap();
        let mut settled = snapshot();
        settled.trades[0].status = "settled".to_string();
        settled.trades[0].exit_price = Some(1.0);
        settled.trades[0].pnl = Some(0.098);
        settled.stats_5m.current_capital = 2.098;
        settled.stats_5m.total_pnl = 0.098;
        store
            .save_snapshot(
                &settled,
                "paper_trades_settled",
                "paper",
                "test-strategy",
                serde_json::json!({"market_slug": "btc-updown-5m-test"}),
            )
            .await
            .unwrap();
        drop(store);

        let reopened = DashboardStore::open(&path).await.unwrap();
        let restored = reopened.load_snapshot().await.unwrap().unwrap();
        assert_eq!(restored.trades.len(), 1);
        assert_eq!(restored.trades[0].status, "settled");
        assert_eq!(reopened.migration_count().await.unwrap(), 6);
        assert_eq!(reopened.audit_event_count().await.unwrap(), 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM paper_trades")
                .fetch_one(&reopened.pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM capital_ledger")
                .fetch_one(&reopened.pool)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM daily_risk_state")
                .fetch_one(&reopened.pool)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM market_windows")
                .fetch_one(&reopened.pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM market_snapshots")
                .fetch_one(&reopened.pool)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM market_data_quality")
                .fetch_one(&reopened.pool)
                .await
                .unwrap(),
            1
        );
        assert!(
            reopened
                .reserve_execution_intent(
                    "btc-updown-5m-test",
                    "btc-updown-5m-test",
                    "5m",
                    "paper",
                    "test-strategy",
                    serde_json::json!({})
                )
                .await
                .unwrap()
        );
        assert!(
            !reopened
                .reserve_execution_intent(
                    "btc-updown-5m-test",
                    "btc-updown-5m-test",
                    "5m",
                    "paper",
                    "test-strategy",
                    serde_json::json!({})
                )
                .await
                .unwrap()
        );

        let backup_path = temp.path().join("backup.db");
        reopened.backup_to(&backup_path).await.unwrap();
        let backup = DashboardStore::open(&backup_path).await.unwrap();
        assert_eq!(backup.load_snapshot().await.unwrap().unwrap().trades.len(), 1);
    }
}
