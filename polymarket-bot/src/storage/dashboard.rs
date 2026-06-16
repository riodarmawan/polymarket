use crate::engine::risk::RiskDecision;
use crate::evaluation::ForwardOpportunity;
use crate::execution::lifecycle::{OrderRecord, OrderState, ReconciliationReport};
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
    (
        7,
        r#"
    CREATE TABLE IF NOT EXISTS risk_decisions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        market_slug TEXT NOT NULL,
        timeframe TEXT NOT NULL,
        runtime_mode TEXT NOT NULL,
        strategy_version TEXT NOT NULL,
        approved INTEGER NOT NULL,
        reason_code TEXT NOT NULL,
        detail_json TEXT NOT NULL,
        created_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_risk_decisions_slug_created
        ON risk_decisions(market_slug, created_at);
    "#,
    ),
    (
        8,
        r#"
    CREATE TABLE IF NOT EXISTS forward_opportunities (
        market_slug TEXT PRIMARY KEY,
        timeframe TEXT NOT NULL,
        direction TEXT NOT NULL,
        confidence REAL NOT NULL,
        expected_fill_price REAL,
        spread REAL NOT NULL,
        fee_rate_bps INTEGER,
        approved INTEGER NOT NULL,
        reason_code TEXT NOT NULL,
        captured_at_ms INTEGER NOT NULL,
        end_ts INTEGER NOT NULL,
        official_outcome TEXT,
        settled_at_ms INTEGER
    );

    CREATE INDEX IF NOT EXISTS idx_forward_opportunities_captured
        ON forward_opportunities(captured_at_ms);
    "#,
    ),
    (
        9,
        r#"
    CREATE TABLE IF NOT EXISTS orders (
        client_key TEXT PRIMARY KEY,
        market_slug TEXT NOT NULL,
        token_id TEXT NOT NULL,
        side TEXT NOT NULL,
        requested_price TEXT NOT NULL,
        requested_size TEXT NOT NULL,
        state TEXT NOT NULL,
        clob_order_id TEXT UNIQUE,
        filled_size TEXT NOT NULL DEFAULT '0',
        detail_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS order_transitions (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        client_key TEXT NOT NULL,
        previous_state TEXT,
        next_state TEXT NOT NULL,
        detail_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        FOREIGN KEY(client_key) REFERENCES orders(client_key)
    );

    CREATE TABLE IF NOT EXISTS fills (
        fill_key TEXT PRIMARY KEY,
        client_key TEXT NOT NULL,
        clob_order_id TEXT,
        price TEXT NOT NULL,
        size TEXT NOT NULL,
        fee_usd TEXT NOT NULL,
        transaction_hash TEXT,
        created_at TEXT NOT NULL,
        FOREIGN KEY(client_key) REFERENCES orders(client_key)
    );

    CREATE TABLE IF NOT EXISTS positions (
        token_id TEXT PRIMARY KEY,
        market_slug TEXT NOT NULL,
        size TEXT NOT NULL,
        average_price TEXT NOT NULL,
        reconciled_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS reconciliation_runs (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        status TEXT NOT NULL,
        remote_checked INTEGER NOT NULL,
        local_non_terminal_orders INTEGER NOT NULL,
        mismatch_count INTEGER NOT NULL,
        detail_json TEXT NOT NULL,
        started_at TEXT NOT NULL,
        finished_at TEXT
    );

    CREATE TABLE IF NOT EXISTS canary_authorizations (
        authorization_id TEXT PRIMARY KEY,
        client_key TEXT NOT NULL UNIQUE,
        max_order_usd REAL NOT NULL,
        issued_at_ms INTEGER NOT NULL,
        expires_at_ms INTEGER NOT NULL,
        consumed_at_ms INTEGER,
        revoked_at_ms INTEGER
    );

    CREATE INDEX IF NOT EXISTS idx_orders_state ON orders(state);
    CREATE INDEX IF NOT EXISTS idx_order_transitions_client ON order_transitions(client_key, created_at);
    "#,
    ),
    (
        10,
        r#"
    CREATE TABLE IF NOT EXISTS runtime_state (
        id INTEGER PRIMARY KEY CHECK (id = 1),
        state TEXT NOT NULL,
        reason TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE TABLE IF NOT EXISTS incidents (
        incident_key TEXT PRIMARY KEY,
        incident_type TEXT NOT NULL,
        status TEXT NOT NULL,
        detail_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        resolved_at TEXT
    );

    INSERT OR IGNORE INTO runtime_state (id, state, reason, updated_at)
    VALUES (1, 'booting', 'runtime state initialized', datetime('now'));
    "#,
    ),
    (
        11,
        r#"
    CREATE TABLE IF NOT EXISTS capital_reservations (
        client_key TEXT PRIMARY KEY,
        amount_usd REAL NOT NULL,
        status TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        FOREIGN KEY(client_key) REFERENCES orders(client_key)
    );

    CREATE INDEX IF NOT EXISTS idx_capital_reservations_status
        ON capital_reservations(status);
    "#,
    ),
    (
        12,
        r#"
    CREATE TABLE IF NOT EXISTS redemption_plans (
        plan_key TEXT PRIMARY KEY,
        condition_id TEXT NOT NULL,
        token_id TEXT NOT NULL,
        market_slug TEXT NOT NULL,
        size TEXT NOT NULL,
        status TEXT NOT NULL,
        detail_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    );

    CREATE INDEX IF NOT EXISTS idx_redemption_plans_status
        ON redemption_plans(status);
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
    pub async fn verify_integrity(path: &Path) -> Result<()> {
        if !path.is_file() {
            anyhow::bail!("database does not exist: {}", path.display());
        }
        let options = SqliteConnectOptions::from_str(&format!("sqlite:{}", path.display()))?
            .read_only(true)
            .create_if_missing(false);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        let result: String = sqlx::query_scalar("PRAGMA integrity_check")
            .fetch_one(&pool)
            .await?;
        pool.close().await;
        if result != "ok" {
            anyhow::bail!("database integrity check failed: {result}");
        }
        Ok(())
    }

    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create database directory {}", parent.display())
            })?;
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
            let applied = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM schema_migrations WHERE version = ?",
            )
            .bind(version)
            .fetch_one(&self.pool)
            .await?;
            if applied > 0 {
                continue;
            }

            let mut transaction = self.pool.begin().await?;
            for statement in migration
                .split(';')
                .map(str::trim)
                .filter(|sql| !sql.is_empty())
            {
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

    pub async fn load_execution_intent(
        &self,
        client_key: &str,
    ) -> Result<Option<crate::engine::risk::ExecutionIntent>> {
        let detail = sqlx::query_scalar::<_, String>(
            "SELECT detail_json FROM execution_intents WHERE client_key = ?",
        )
        .bind(client_key)
        .fetch_optional(&self.pool)
        .await?;
        detail
            .map(|json| serde_json::from_str(&json).context("invalid execution intent JSON"))
            .transpose()
    }

    pub async fn record_risk_decision(
        &self,
        market_slug: &str,
        timeframe: &str,
        runtime_mode: &str,
        strategy_version: &str,
        decision: &RiskDecision,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO risk_decisions (
                market_slug, timeframe, runtime_mode, strategy_version,
                approved, reason_code, detail_json, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(market_slug)
        .bind(timeframe)
        .bind(runtime_mode)
        .bind(strategy_version)
        .bind(decision.approved)
        .bind(&decision.reason_code)
        .bind(serde_json::to_string(decision)?)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_forward_opportunity(&self, opportunity: &ForwardOpportunity) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO forward_opportunities (
                market_slug, timeframe, direction, confidence, expected_fill_price,
                spread, fee_rate_bps, approved, reason_code, captured_at_ms
                , end_ts
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(market_slug) DO UPDATE SET
                expected_fill_price = excluded.expected_fill_price,
                spread = excluded.spread,
                fee_rate_bps = excluded.fee_rate_bps,
                approved = MAX(forward_opportunities.approved, excluded.approved),
                reason_code = CASE
                    WHEN excluded.approved = 1 THEN excluded.reason_code
                    ELSE forward_opportunities.reason_code
                END
            "#,
        )
        .bind(&opportunity.market_slug)
        .bind(&opportunity.timeframe)
        .bind(&opportunity.direction)
        .bind(opportunity.confidence)
        .bind(opportunity.expected_fill_price)
        .bind(opportunity.spread)
        .bind(opportunity.fee_rate_bps.map(|value| value as i64))
        .bind(opportunity.approved)
        .bind(&opportunity.reason_code)
        .bind(opportunity.captured_at_ms)
        .bind(opportunity.end_ts)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_official_outcomes(&self, outcomes: &HashMap<String, String>) -> Result<()> {
        let settled_at_ms = Utc::now().timestamp_millis();
        let mut transaction = self.pool.begin().await?;
        for (slug, outcome) in outcomes {
            sqlx::query(
                "UPDATE forward_opportunities SET official_outcome = ?, settled_at_ms = ? WHERE market_slug = ?",
            )
            .bind(outcome)
            .bind(settled_at_ms)
            .bind(slug)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn load_forward_opportunities(&self) -> Result<Vec<ForwardOpportunity>> {
        let rows = sqlx::query(
            r#"
            SELECT market_slug, timeframe, direction, confidence, expected_fill_price,
                   spread, fee_rate_bps, approved, reason_code, captured_at_ms, end_ts, official_outcome
            FROM forward_opportunities
            ORDER BY captured_at_ms
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| ForwardOpportunity {
                market_slug: row.get("market_slug"),
                timeframe: row.get("timeframe"),
                direction: row.get("direction"),
                confidence: row.get("confidence"),
                expected_fill_price: row.get("expected_fill_price"),
                spread: row.get("spread"),
                fee_rate_bps: row
                    .get::<Option<i64>, _>("fee_rate_bps")
                    .map(|value| value as u64),
                approved: row.get("approved"),
                reason_code: row.get("reason_code"),
                captured_at_ms: row.get("captured_at_ms"),
                end_ts: row.get("end_ts"),
                official_outcome: row.get("official_outcome"),
            })
            .collect())
    }

    pub async fn load_unsettled_opportunity_slugs(&self, now_ts: i64) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT market_slug FROM forward_opportunities WHERE official_outcome IS NULL AND end_ts <= ? ORDER BY end_ts LIMIT 100",
        )
        .bind(now_ts)
        .fetch_all(&self.pool)
        .await?)
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

    pub async fn create_order(
        &self,
        order: &OrderRecord,
        detail: serde_json::Value,
    ) -> Result<bool> {
        let timestamp = Utc::now().to_rfc3339();
        let result = sqlx::query(
            r#"
            INSERT INTO orders (
                client_key, market_slug, token_id, side, requested_price,
                requested_size, state, clob_order_id, filled_size, detail_json,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(client_key) DO NOTHING
            "#,
        )
        .bind(&order.client_key)
        .bind(&order.market_slug)
        .bind(&order.token_id)
        .bind(&order.side)
        .bind(&order.requested_price)
        .bind(&order.requested_size)
        .bind(order.state.to_string())
        .bind(&order.clob_order_id)
        .bind(&order.filled_size)
        .bind(serde_json::to_string(&detail)?)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn create_order_and_reserve(
        &self,
        order: &OrderRecord,
        amount_usd: f64,
        detail: serde_json::Value,
    ) -> Result<bool> {
        let timestamp = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        let inserted = sqlx::query(
            r#"
            INSERT INTO orders (
                client_key, market_slug, token_id, side, requested_price,
                requested_size, state, clob_order_id, filled_size, detail_json,
                created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(client_key) DO NOTHING
            "#,
        )
        .bind(&order.client_key)
        .bind(&order.market_slug)
        .bind(&order.token_id)
        .bind(&order.side)
        .bind(&order.requested_price)
        .bind(&order.requested_size)
        .bind(order.state.to_string())
        .bind(&order.clob_order_id)
        .bind(&order.filled_size)
        .bind(serde_json::to_string(&detail)?)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
            == 1;
        if inserted {
            sqlx::query(
                r#"
                INSERT INTO capital_reservations
                    (client_key, amount_usd, status, created_at, updated_at)
                VALUES (?, ?, 'held', ?, ?)
                "#,
            )
            .bind(&order.client_key)
            .bind(amount_usd)
            .bind(&timestamp)
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(inserted)
    }

    pub async fn update_capital_reservation(&self, client_key: &str, status: &str) -> Result<()> {
        if !["held", "filled", "released"].contains(&status) {
            anyhow::bail!("invalid capital reservation status");
        }
        sqlx::query(
            "UPDATE capital_reservations SET status = ?, updated_at = ? WHERE client_key = ?",
        )
        .bind(status)
        .bind(Utc::now().to_rfc3339())
        .bind(client_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn held_capital_usd(&self) -> Result<f64> {
        Ok(sqlx::query_scalar(
            "SELECT COALESCE(SUM(amount_usd), 0.0) FROM capital_reservations WHERE status = 'held'",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn local_positions(&self) -> Result<Vec<(String, String)>> {
        Ok(
            sqlx::query("SELECT token_id, size FROM positions ORDER BY token_id")
                .fetch_all(&self.pool)
                .await?
                .into_iter()
                .map(|row| (row.get("token_id"), row.get("size")))
                .collect(),
        )
    }

    pub async fn record_redemption_plan(
        &self,
        condition_id: &str,
        token_id: &str,
        market_slug: &str,
        size: &str,
        detail: serde_json::Value,
    ) -> Result<String> {
        let plan_key = format!("{condition_id}:{token_id}");
        let timestamp = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO redemption_plans (
                plan_key, condition_id, token_id, market_slug, size, status,
                detail_json, created_at, updated_at
            ) VALUES (?, ?, ?, ?, ?, 'planned', ?, ?, ?)
            ON CONFLICT(plan_key) DO UPDATE SET
                market_slug = excluded.market_slug,
                size = excluded.size,
                detail_json = excluded.detail_json,
                updated_at = excluded.updated_at
            WHERE redemption_plans.status = 'planned'
            "#,
        )
        .bind(&plan_key)
        .bind(condition_id)
        .bind(token_id)
        .bind(market_slug)
        .bind(size)
        .bind(serde_json::to_string(&detail)?)
        .bind(&timestamp)
        .bind(&timestamp)
        .execute(&self.pool)
        .await?;
        Ok(plan_key)
    }

    pub async fn load_redemption_plans(&self) -> Result<Vec<serde_json::Value>> {
        Ok(sqlx::query(
            r#"
            SELECT plan_key, condition_id, token_id, market_slug, size, status,
                   detail_json, created_at, updated_at
            FROM redemption_plans ORDER BY created_at
            "#,
        )
        .fetch_all(&self.pool)
        .await?
        .into_iter()
        .map(|row| {
            serde_json::json!({
                "plan_key": row.get::<String, _>("plan_key"),
                "condition_id": row.get::<String, _>("condition_id"),
                "token_id": row.get::<String, _>("token_id"),
                "market_slug": row.get::<String, _>("market_slug"),
                "size": row.get::<String, _>("size"),
                "status": row.get::<String, _>("status"),
                "detail": serde_json::from_str::<serde_json::Value>(
                    &row.get::<String, _>("detail_json")
                ).unwrap_or(serde_json::Value::Null),
                "created_at": row.get::<String, _>("created_at"),
                "updated_at": row.get::<String, _>("updated_at")
            })
        })
        .collect())
    }

    pub async fn record_live_fill(
        &self,
        fill_key: &str,
        clob_order_id: &str,
        token_id: &str,
        price: &str,
        size: &str,
        transaction_hash: Option<&str>,
    ) -> Result<bool> {
        let price_value = price.parse::<f64>().context("invalid fill price")?;
        let size_value = size.parse::<f64>().context("invalid fill size")?;
        if !price_value.is_finite()
            || price_value <= 0.0
            || !size_value.is_finite()
            || size_value <= 0.0
        {
            anyhow::bail!("fill price and size must be finite and positive");
        }
        let timestamp = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        let order =
            sqlx::query("SELECT client_key, market_slug, side FROM orders WHERE clob_order_id = ?")
                .bind(clob_order_id)
                .fetch_optional(&mut *transaction)
                .await?;
        let Some(order) = order else {
            return Ok(false);
        };
        let client_key: String = order.get("client_key");
        let market_slug: String = order.get("market_slug");
        let side: String = order.get("side");
        let inserted = sqlx::query(
            r#"
            INSERT INTO fills (
                fill_key, client_key, clob_order_id, price, size, fee_usd,
                transaction_hash, created_at
            ) VALUES (?, ?, ?, ?, ?, '0', ?, ?)
            ON CONFLICT(fill_key) DO NOTHING
            "#,
        )
        .bind(fill_key)
        .bind(&client_key)
        .bind(clob_order_id)
        .bind(price)
        .bind(size)
        .bind(transaction_hash)
        .bind(&timestamp)
        .execute(&mut *transaction)
        .await?
        .rows_affected()
            == 1;
        if inserted {
            if side != "BUY" {
                anyhow::bail!("live position accounting only supports BUY orders");
            }
            let existing =
                sqlx::query("SELECT size, average_price FROM positions WHERE token_id = ?")
                    .bind(token_id)
                    .fetch_optional(&mut *transaction)
                    .await?;
            let (new_size, new_average) = if let Some(existing) = existing {
                let old_size = existing
                    .get::<String, _>("size")
                    .parse::<f64>()
                    .context("invalid stored position size")?;
                let old_average = existing
                    .get::<String, _>("average_price")
                    .parse::<f64>()
                    .context("invalid stored average price")?;
                let new_size = old_size + size_value;
                (
                    new_size,
                    ((old_size * old_average) + (size_value * price_value)) / new_size,
                )
            } else {
                (size_value, price_value)
            };
            sqlx::query(
                r#"
                INSERT INTO positions (token_id, market_slug, size, average_price, reconciled_at)
                VALUES (?, ?, ?, ?, ?)
                ON CONFLICT(token_id) DO UPDATE SET
                    market_slug = excluded.market_slug,
                    size = excluded.size,
                    average_price = excluded.average_price,
                    reconciled_at = excluded.reconciled_at
                "#,
            )
            .bind(token_id)
            .bind(market_slug)
            .bind(new_size.to_string())
            .bind(new_average.to_string())
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(true)
    }

    pub async fn transition_order(
        &self,
        client_key: &str,
        next: OrderState,
        clob_order_id: Option<&str>,
        filled_size: &str,
        detail: serde_json::Value,
    ) -> Result<()> {
        let timestamp = Utc::now().to_rfc3339();
        let mut transaction = self.pool.begin().await?;
        let current: String = sqlx::query_scalar("SELECT state FROM orders WHERE client_key = ?")
            .bind(client_key)
            .fetch_one(&mut *transaction)
            .await
            .with_context(|| format!("unknown order client key {client_key}"))?;
        let current: OrderState = current.parse()?;
        current.validate_transition(next)?;

        sqlx::query(
            r#"
            UPDATE orders SET
                state = ?,
                clob_order_id = COALESCE(?, clob_order_id),
                filled_size = ?,
                detail_json = ?,
                updated_at = ?
            WHERE client_key = ?
            "#,
        )
        .bind(next.to_string())
        .bind(clob_order_id)
        .bind(filled_size)
        .bind(serde_json::to_string(&detail)?)
        .bind(&timestamp)
        .bind(client_key)
        .execute(&mut *transaction)
        .await?;

        let reservation_status = match next {
            OrderState::Filled => "filled",
            OrderState::Cancelled | OrderState::Rejected => "released",
            _ => "held",
        };
        sqlx::query(
            "UPDATE capital_reservations SET status = ?, updated_at = ? WHERE client_key = ?",
        )
        .bind(reservation_status)
        .bind(&timestamp)
        .bind(client_key)
        .execute(&mut *transaction)
        .await?;

        if current != next {
            sqlx::query(
                r#"
                INSERT INTO order_transitions
                    (client_key, previous_state, next_state, detail_json, created_at)
                VALUES (?, ?, ?, ?, ?)
                "#,
            )
            .bind(client_key)
            .bind(current.to_string())
            .bind(next.to_string())
            .bind(serde_json::to_string(&detail)?)
            .bind(&timestamp)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn transition_order_by_remote_id(
        &self,
        clob_order_id: &str,
        next: OrderState,
        filled_size: &str,
        detail: serde_json::Value,
    ) -> Result<bool> {
        let client_key = sqlx::query_scalar::<_, String>(
            "SELECT client_key FROM orders WHERE clob_order_id = ?",
        )
        .bind(clob_order_id)
        .fetch_optional(&self.pool)
        .await?;
        let Some(client_key) = client_key else {
            return Ok(false);
        };
        self.transition_order(&client_key, next, Some(clob_order_id), filled_size, detail)
            .await?;
        Ok(true)
    }

    pub async fn local_non_terminal_order_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar(
            "SELECT COUNT(*) FROM orders WHERE state NOT IN ('filled', 'cancelled', 'rejected')",
        )
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn order_state(&self, client_key: &str) -> Result<Option<OrderState>> {
        sqlx::query_scalar::<_, String>("SELECT state FROM orders WHERE client_key = ?")
            .bind(client_key)
            .fetch_optional(&self.pool)
            .await?
            .map(|state| state.parse())
            .transpose()
    }

    pub async fn local_non_terminal_remote_ids(&self) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            r#"
            SELECT clob_order_id FROM orders
            WHERE state NOT IN ('filled', 'cancelled', 'rejected')
              AND clob_order_id IS NOT NULL
            ORDER BY clob_order_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?)
    }

    pub async fn local_non_terminal_without_remote_id_count(&self) -> Result<i64> {
        Ok(sqlx::query_scalar(
            r#"
            SELECT COUNT(*) FROM orders
            WHERE state NOT IN ('filled', 'cancelled', 'rejected')
              AND clob_order_id IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await?)
    }

    pub async fn apply_remote_order_evidence(
        &self,
        remote_open_ids: &std::collections::HashSet<String>,
        remote_trade_ids: &std::collections::HashSet<String>,
    ) -> Result<usize> {
        let rows = sqlx::query(
            r#"
            SELECT client_key, clob_order_id FROM orders
            WHERE state NOT IN ('filled', 'cancelled', 'rejected')
              AND clob_order_id IS NOT NULL
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        let mut changed = 0;
        for row in rows {
            let client_key: String = row.get("client_key");
            let order_id: String = row.get("clob_order_id");
            let next = if remote_trade_ids.contains(&order_id) {
                Some(OrderState::Filled)
            } else if remote_open_ids.contains(&order_id) {
                Some(OrderState::Submitted)
            } else {
                None
            };
            if let Some(next) = next {
                self.transition_order(
                    &client_key,
                    next,
                    Some(&order_id),
                    "0",
                    serde_json::json!({"source": "authenticated_reconciliation"}),
                )
                .await?;
                changed += 1;
            }
        }
        Ok(changed)
    }

    pub async fn mark_remote_orders_cancel_pending(&self) -> Result<usize> {
        let client_keys: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT client_key FROM orders
            WHERE state NOT IN ('filled', 'cancelled', 'rejected')
              AND clob_order_id IS NOT NULL
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        for client_key in &client_keys {
            self.transition_order(
                client_key,
                OrderState::CancelPending,
                None,
                "0",
                serde_json::json!({"source": "emergency_cancel_all"}),
            )
            .await?;
        }
        Ok(client_keys.len())
    }

    pub async fn mark_remote_orders_cancelled(&self, order_ids: &[String]) -> Result<usize> {
        let mut changed = 0;
        for order_id in order_ids {
            if let Some(client_key) = sqlx::query_scalar::<_, String>(
                "SELECT client_key FROM orders WHERE clob_order_id = ?",
            )
            .bind(order_id)
            .fetch_optional(&self.pool)
            .await?
            {
                self.transition_order(
                    &client_key,
                    OrderState::Cancelled,
                    Some(order_id),
                    "0",
                    serde_json::json!({"source": "emergency_cancel_all_response"}),
                )
                .await?;
                changed += 1;
            }
        }
        Ok(changed)
    }

    pub async fn set_runtime_state(&self, state: &str, reason: &str) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO runtime_state (id, state, reason, updated_at)
            VALUES (1, ?, ?, ?)
            ON CONFLICT(id) DO UPDATE SET
                state = excluded.state,
                reason = excluded.reason,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(state)
        .bind(reason)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn runtime_state(&self) -> Result<(String, String)> {
        let row = sqlx::query("SELECT state, reason FROM runtime_state WHERE id = 1")
            .fetch_one(&self.pool)
            .await?;
        Ok((row.get("state"), row.get("reason")))
    }

    pub async fn open_incident(
        &self,
        incident_key: &str,
        incident_type: &str,
        detail: serde_json::Value,
    ) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO incidents (
                incident_key, incident_type, status, detail_json, created_at
            ) VALUES (?, ?, 'open', ?, ?)
            ON CONFLICT(incident_key) DO UPDATE SET
                status = 'open',
                detail_json = excluded.detail_json,
                resolved_at = NULL
            "#,
        )
        .bind(incident_key)
        .bind(incident_type)
        .bind(serde_json::to_string(&detail)?)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn open_incident_count(&self) -> Result<i64> {
        Ok(
            sqlx::query_scalar("SELECT COUNT(*) FROM incidents WHERE status = 'open'")
                .fetch_one(&self.pool)
                .await?,
        )
    }

    pub async fn resolve_incident(&self, incident_key: &str) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE incidents SET status = 'resolved', resolved_at = ?
            WHERE incident_key = ? AND status = 'open'
            "#,
        )
        .bind(Utc::now().to_rfc3339())
        .bind(incident_key)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    pub async fn load_open_incidents(&self) -> Result<Vec<serde_json::Value>> {
        let rows = sqlx::query(
            r#"
            SELECT incident_key, incident_type, detail_json, created_at
            FROM incidents WHERE status = 'open' ORDER BY created_at
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| {
                serde_json::json!({
                    "incident_key": row.get::<String, _>("incident_key"),
                    "incident_type": row.get::<String, _>("incident_type"),
                    "detail": serde_json::from_str::<serde_json::Value>(
                        &row.get::<String, _>("detail_json")
                    ).unwrap_or(serde_json::Value::Null),
                    "created_at": row.get::<String, _>("created_at")
                })
            })
            .collect())
    }

    pub async fn record_reconciliation(
        &self,
        remote_checked: bool,
        mismatch_count: i64,
        detail: serde_json::Value,
    ) -> Result<ReconciliationReport> {
        let local_non_terminal_orders = self.local_non_terminal_order_count().await?;
        let ready = remote_checked && mismatch_count == 0;
        let reason = if ready {
            "remote and local state agree".to_string()
        } else if !remote_checked {
            "remote CLOB state was not checked".to_string()
        } else if mismatch_count > 0 {
            format!("{mismatch_count} remote/local mismatch(es)")
        } else {
            "reconciliation blocked for an unknown reason".to_string()
        };
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO reconciliation_runs (
                status, remote_checked, local_non_terminal_orders, mismatch_count,
                detail_json, started_at, finished_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(if ready { "ready" } else { "blocked" })
        .bind(remote_checked)
        .bind(local_non_terminal_orders)
        .bind(mismatch_count)
        .bind(serde_json::to_string(&detail)?)
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await?;
        Ok(ReconciliationReport {
            ready,
            remote_checked,
            local_non_terminal_orders,
            mismatch_count,
            reason,
        })
    }

    pub async fn latest_reconciliation_ready(&self) -> Result<bool> {
        let reconciliation_ready = sqlx::query_scalar::<_, String>(
            r#"
            SELECT status FROM reconciliation_runs
            WHERE datetime(finished_at) >= datetime('now', '-5 minutes')
            ORDER BY id DESC LIMIT 1
            "#,
        )
        .fetch_optional(&self.pool)
        .await?
        .is_some_and(|status| status == "ready");
        let runtime_ready = self
            .runtime_state()
            .await
            .map(|(state, _)| state == "ready")
            .unwrap_or(false);
        Ok(reconciliation_ready && runtime_ready && self.open_incident_count().await? == 0)
    }

    pub async fn issue_canary_authorization(
        &self,
        client_key: &str,
        max_order_usd: f64,
        expires_at_ms: i64,
    ) -> Result<String> {
        let now_ms = Utc::now().timestamp_millis();
        let authorization_id = format!("canary-{now_ms}-{client_key}");
        sqlx::query(
            r#"
            INSERT INTO canary_authorizations (
                authorization_id, client_key, max_order_usd, issued_at_ms, expires_at_ms
            ) VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(&authorization_id)
        .bind(client_key)
        .bind(max_order_usd)
        .bind(now_ms)
        .bind(expires_at_ms)
        .execute(&self.pool)
        .await?;
        Ok(authorization_id)
    }

    pub async fn consume_canary_authorization(
        &self,
        authorization_id: &str,
        client_key: &str,
        order_usd: f64,
    ) -> Result<bool> {
        let now_ms = Utc::now().timestamp_millis();
        let result = sqlx::query(
            r#"
            UPDATE canary_authorizations SET consumed_at_ms = ?
            WHERE authorization_id = ?
              AND client_key = ?
              AND max_order_usd >= ?
              AND expires_at_ms >= ?
              AND consumed_at_ms IS NULL
              AND revoked_at_ms IS NULL
            "#,
        )
        .bind(now_ms)
        .bind(authorization_id)
        .bind(client_key)
        .bind(order_usd)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
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
            std::fs::create_dir_all(parent).with_context(|| {
                format!("failed to create backup directory {}", parent.display())
            })?;
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
    for (timeframe, stats) in [("15m", &snapshot.stats_15m), ("5m", &snapshot.stats_5m)] {
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
        store
            .record_risk_decision(
                "btc-updown-5m-test",
                "5m",
                "paper",
                "test-strategy",
                &RiskDecision {
                    approved: false,
                    reason_code: "spread".to_string(),
                    checks: Vec::new(),
                    intent: None,
                },
            )
            .await
            .unwrap();
        store
            .record_forward_opportunity(&ForwardOpportunity {
                market_slug: "btc-updown-5m-test".to_string(),
                timeframe: "5m".to_string(),
                direction: "Up".to_string(),
                confidence: 0.70,
                expected_fill_price: Some(0.50),
                spread: 0.02,
                fee_rate_bps: Some(200),
                approved: true,
                reason_code: "approved".to_string(),
                captured_at_ms: 1,
                end_ts: 2,
                official_outcome: None,
            })
            .await
            .unwrap();
        store
            .record_official_outcomes(&HashMap::from([(
                "btc-updown-5m-test".to_string(),
                "Up".to_string(),
            )]))
            .await
            .unwrap();
        drop(store);

        let reopened = DashboardStore::open(&path).await.unwrap();
        let restored = reopened.load_snapshot().await.unwrap().unwrap();
        assert_eq!(restored.trades.len(), 1);
        assert_eq!(restored.trades[0].status, "settled");
        assert_eq!(reopened.migration_count().await.unwrap(), 12);
        assert_eq!(reopened.audit_event_count().await.unwrap(), 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM risk_decisions")
                .fetch_one(&reopened.pool)
                .await
                .unwrap(),
            1
        );
        let opportunities = reopened.load_forward_opportunities().await.unwrap();
        assert_eq!(opportunities.len(), 1);
        assert_eq!(opportunities[0].official_outcome.as_deref(), Some("Up"));
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
        assert!(reopened
            .reserve_execution_intent(
                "btc-updown-5m-test",
                "btc-updown-5m-test",
                "5m",
                "paper",
                "test-strategy",
                serde_json::json!({})
            )
            .await
            .unwrap());
        assert!(!reopened
            .reserve_execution_intent(
                "btc-updown-5m-test",
                "btc-updown-5m-test",
                "5m",
                "paper",
                "test-strategy",
                serde_json::json!({})
            )
            .await
            .unwrap());

        let backup_path = temp.path().join("backup.db");
        reopened.backup_to(&backup_path).await.unwrap();
        let backup = DashboardStore::open(&backup_path).await.unwrap();
        assert_eq!(
            backup.load_snapshot().await.unwrap().unwrap().trades.len(),
            1
        );
    }

    #[tokio::test]
    async fn persists_order_lifecycle_and_consumes_canary_once() {
        let temp = TempDir::new().unwrap();
        let store = DashboardStore::open(&temp.path().join("lifecycle.db"))
            .await
            .unwrap();
        let order = OrderRecord {
            client_key: "client-1".to_string(),
            market_slug: "btc-updown-5m-test".to_string(),
            token_id: "123".to_string(),
            side: "BUY".to_string(),
            requested_price: "0.50".to_string(),
            requested_size: "0.20".to_string(),
            state: OrderState::IntentPersisted,
            clob_order_id: None,
            filled_size: "0".to_string(),
            updated_at: Utc::now().to_rfc3339(),
        };
        assert!(store
            .create_order(&order, serde_json::json!({}))
            .await
            .unwrap());
        assert!(!store
            .create_order(&order, serde_json::json!({}))
            .await
            .unwrap());
        store
            .transition_order(
                "client-1",
                OrderState::Signed,
                None,
                "0",
                serde_json::json!({}),
            )
            .await
            .unwrap();
        store
            .transition_order(
                "client-1",
                OrderState::Rejected,
                None,
                "0",
                serde_json::json!({}),
            )
            .await
            .unwrap();
        assert_eq!(store.local_non_terminal_order_count().await.unwrap(), 0);

        let authorization = store
            .issue_canary_authorization(
                "client-canary",
                0.10,
                Utc::now().timestamp_millis() + 60_000,
            )
            .await
            .unwrap();
        assert!(store
            .consume_canary_authorization(&authorization, "client-canary", 0.10)
            .await
            .unwrap());
        assert!(!store
            .consume_canary_authorization(&authorization, "client-canary", 0.10)
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn reserves_and_releases_capital_with_order_lifecycle() {
        let temp = TempDir::new().unwrap();
        let store = DashboardStore::open(&temp.path().join("capital.db"))
            .await
            .unwrap();
        let order = OrderRecord {
            client_key: "capital-1".to_string(),
            market_slug: "btc-updown-5m-test".to_string(),
            token_id: "123".to_string(),
            side: "BUY".to_string(),
            requested_price: "0.50".to_string(),
            requested_size: "0.20".to_string(),
            state: OrderState::IntentPersisted,
            clob_order_id: None,
            filled_size: "0".to_string(),
            updated_at: Utc::now().to_rfc3339(),
        };

        assert!(store
            .create_order_and_reserve(&order, 0.10, serde_json::json!({}))
            .await
            .unwrap());
        assert!(!store
            .create_order_and_reserve(&order, 0.10, serde_json::json!({}))
            .await
            .unwrap());
        assert!((store.held_capital_usd().await.unwrap() - 0.10).abs() < f64::EPSILON);

        store
            .transition_order(
                "capital-1",
                OrderState::Rejected,
                None,
                "0",
                serde_json::json!({"reason": "fok_not_filled"}),
            )
            .await
            .unwrap();
        assert_eq!(store.held_capital_usd().await.unwrap(), 0.0);
    }

    #[tokio::test]
    async fn live_fill_is_idempotent_and_updates_position_average() {
        let temp = TempDir::new().unwrap();
        let store = DashboardStore::open(&temp.path().join("fills.db"))
            .await
            .unwrap();
        let order = OrderRecord {
            client_key: "fill-client".to_string(),
            market_slug: "btc-updown-5m-test".to_string(),
            token_id: "123".to_string(),
            side: "BUY".to_string(),
            requested_price: "0.60".to_string(),
            requested_size: "2".to_string(),
            state: OrderState::IntentPersisted,
            clob_order_id: None,
            filled_size: "0".to_string(),
            updated_at: Utc::now().to_rfc3339(),
        };
        store
            .create_order_and_reserve(&order, 0.10, serde_json::json!({}))
            .await
            .unwrap();
        store
            .transition_order(
                "fill-client",
                OrderState::Signed,
                None,
                "0",
                serde_json::json!({}),
            )
            .await
            .unwrap();
        store
            .transition_order(
                "fill-client",
                OrderState::Submitted,
                Some("remote-fill"),
                "0",
                serde_json::json!({}),
            )
            .await
            .unwrap();

        assert!(store
            .record_live_fill(
                "trade-1:remote-fill",
                "remote-fill",
                "123",
                "0.40",
                "1",
                None
            )
            .await
            .unwrap());
        assert!(store
            .record_live_fill(
                "trade-1:remote-fill",
                "remote-fill",
                "123",
                "0.40",
                "1",
                None
            )
            .await
            .unwrap());
        assert!(store
            .record_live_fill(
                "trade-2:remote-fill",
                "remote-fill",
                "123",
                "0.60",
                "1",
                None
            )
            .await
            .unwrap());

        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM fills")
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            2
        );
        let positions = store.local_positions().await.unwrap();
        assert_eq!(positions.len(), 1);
        assert_eq!(positions[0].0, "123");
        assert!((positions[0].1.parse::<f64>().unwrap() - 2.0).abs() < f64::EPSILON);
        let average: String =
            sqlx::query_scalar("SELECT average_price FROM positions WHERE token_id = '123'")
                .fetch_one(&store.pool)
                .await
                .unwrap();
        assert!((average.parse::<f64>().unwrap() - 0.5).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn redemption_plan_is_durable_and_idempotent() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("redemption.db");
        let store = DashboardStore::open(&path).await.unwrap();
        let key = store
            .record_redemption_plan(
                "condition-1",
                "token-1",
                "btc-updown-5m-test",
                "1.5",
                serde_json::json!({"redeemable": true}),
            )
            .await
            .unwrap();
        assert_eq!(key, "condition-1:token-1");
        store
            .record_redemption_plan(
                "condition-1",
                "token-1",
                "btc-updown-5m-test",
                "2.0",
                serde_json::json!({"redeemable": true}),
            )
            .await
            .unwrap();
        drop(store);

        let reopened = DashboardStore::open(&path).await.unwrap();
        let plans = reopened.load_redemption_plans().await.unwrap();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0]["size"], "2.0");
        assert_eq!(plans[0]["status"], "planned");
    }

    #[tokio::test]
    async fn verifies_backup_integrity_without_mutating_file() {
        let temp = TempDir::new().unwrap();
        let source = temp.path().join("source.db");
        let backup = temp.path().join("backup.db");
        let store = DashboardStore::open(&source).await.unwrap();
        store.backup_to(&backup).await.unwrap();

        let before = std::fs::metadata(&backup).unwrap().modified().unwrap();
        DashboardStore::verify_integrity(&backup).await.unwrap();
        let after = std::fs::metadata(&backup).unwrap().modified().unwrap();
        assert_eq!(before, after);
    }

    #[tokio::test]
    async fn partial_fill_keeps_capital_reserved_until_terminal_state() {
        let temp = TempDir::new().unwrap();
        let store = DashboardStore::open(&temp.path().join("partial.db"))
            .await
            .unwrap();
        let order = OrderRecord {
            client_key: "partial-client".to_string(),
            market_slug: "btc-updown-5m-test".to_string(),
            token_id: "123".to_string(),
            side: "BUY".to_string(),
            requested_price: "0.50".to_string(),
            requested_size: "1".to_string(),
            state: OrderState::IntentPersisted,
            clob_order_id: None,
            filled_size: "0".to_string(),
            updated_at: Utc::now().to_rfc3339(),
        };
        store
            .create_order_and_reserve(&order, 0.10, serde_json::json!({}))
            .await
            .unwrap();
        store
            .transition_order(
                "partial-client",
                OrderState::Signed,
                None,
                "0",
                serde_json::json!({}),
            )
            .await
            .unwrap();
        store
            .transition_order(
                "partial-client",
                OrderState::PartiallyFilled,
                Some("remote-partial"),
                "0.5",
                serde_json::json!({}),
            )
            .await
            .unwrap();
        assert!((store.held_capital_usd().await.unwrap() - 0.10).abs() < f64::EPSILON);
        store
            .transition_order(
                "partial-client",
                OrderState::Cancelled,
                None,
                "0.5",
                serde_json::json!({}),
            )
            .await
            .unwrap();
        assert_eq!(store.held_capital_usd().await.unwrap(), 0.0);
    }
}
