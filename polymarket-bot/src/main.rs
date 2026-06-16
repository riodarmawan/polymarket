pub mod analyzers;
pub mod api;
pub mod backtesting;
pub mod build_info;
pub mod cli;
pub mod collector;
pub mod config;
pub mod crypto;
pub mod dashboard;
pub mod engine;
pub mod error;
pub mod evaluation;
pub mod execution;
pub mod models;
pub mod paper_trading;
pub mod production;
pub mod storage;
pub mod web;

use clap::Parser;
use cli::{Cli, Commands, ConfigAction};
use config::Config;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    if std::env::var("POLYMARKET_JSON_LOGS").as_deref() == Ok("true") {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt().with_env_filter(filter).init();
    }

    let cli = Cli::parse();
    let config = Config::load()?;
    let json_stdout = matches!(
        &cli.command,
        Commands::ProductionReadiness { json: true, .. }
            | Commands::StrategyManifest
            | Commands::ForwardReport
            | Commands::CanaryReview { .. }
            | Commands::MonitorForward { .. }
            | Commands::OperationalStatus
            | Commands::VerifyDatabase { .. }
    );
    if !json_stdout {
        config.print_startup_summary();
    }

    match cli.command {
        Commands::Collect { daemon, interval } => {
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let collector = collector::data_collector::DataCollector::new(
                config.api.gamma_base_url.clone(),
                config.trading.max_hours_to_resolution,
            );

            if daemon {
                loop {
                    match collector
                        .collect_markets(&db, config.collector.max_markets)
                        .await
                    {
                        Ok(count) => tracing::info!("Collected {} markets", count),
                        Err(e) => tracing::error!("Collection failed: {}", e),
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(interval)).await;
                }
            } else {
                let count = collector
                    .collect_markets(&db, config.collector.max_markets)
                    .await?;
                tracing::info!("Collected {} markets", count);
            }
        }
        Commands::Trade { daemon, dry_run } => {
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let decision_engine = engine::decision::DecisionEngine::new();
            let mut paper_engine =
                paper_trading::engine::PaperTradingEngine::new(config.general.initial_capital);

            tracing::info!("Paper trading started (dry_run: {})", dry_run);

            if daemon {
                loop {
                    let markets = db.get_markets().await?;
                    for market in &markets {
                        let signals = vec![];
                        let decision = decision_engine.evaluate(
                            &market.id,
                            &market.question,
                            market.yes_price,
                            market.yes_price,
                            signals,
                            config.general.initial_capital,
                        );

                        if !dry_run {
                            paper_engine.execute_trade(&decision, &db).await;
                        }

                        tracing::info!("Decision for {}: {:?}", market.question, decision);
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                }
            }
        }
        Commands::Backtest { period, strategy } => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;

            let windows = store.list_market_windows().await?;
            let snapshots = store.list_market_snapshots().await?;

            tracing::info!(
                "Found {} market windows, {} snapshots in trading.db",
                windows.len(),
                snapshots.len()
            );

            if windows.is_empty() || snapshots.is_empty() {
                tracing::error!(
                    "No market data available. Run the trading bot first to populate trading.db."
                );
                return Ok(());
            }

            let period_days: u32 = period.trim_end_matches('d').parse().unwrap_or(30);
            let cutoff_ts = if period_days < u32::MAX {
                let now_ts = chrono::Utc::now().timestamp();
                now_ts - (period_days as i64 * 86400)
            } else {
                0i64
            };

            let window_map: std::collections::HashMap<String, &storage::dashboard::MarketWindow> =
                windows.iter().map(|w| (w.slug.clone(), w)).collect();

            let mut observations = Vec::new();
            let mut slugs_with_data = std::collections::HashSet::new();

            for snap in &snapshots {
                let ts = chrono::DateTime::parse_from_rfc3339(&snap.captured_at)
                    .map(|dt| dt.timestamp() as u64)
                    .unwrap_or_else(|_| {
                        snap.captured_at
                            .parse::<f64>()
                            .map(|f| f as u64)
                            .unwrap_or(0)
                    });

                if ts == 0 {
                    continue;
                }

                let unix_ts = ts as i64;
                if unix_ts < cutoff_ts {
                    continue;
                }

                let up_ask = snap.up_best_ask.unwrap_or(0.0);
                let up_bid = snap.up_best_bid.unwrap_or(0.0);

                if up_ask <= 0.0 || up_bid <= 0.0 {
                    continue;
                }

                let mid_price = (up_ask + up_bid) / 2.0;
                let spread = snap.spread.max(up_ask - up_bid);

                observations.push(backtesting::types::PriceObservation {
                    timestamp: ts,
                    market_id: snap.market_slug.clone(),
                    ask_price: up_ask,
                    bid_price: up_bid,
                    ask_depth: 500.0,
                    bid_depth: 500.0,
                    spread,
                    mid_price,
                });

                slugs_with_data.insert(snap.market_slug.clone());
            }

            tracing::info!(
                "Using {} observations across {} markets from trading.db (period: {}d, strategy: {})",
                observations.len(),
                slugs_with_data.len(),
                period_days,
                strategy
            );

            for (slug, w) in &window_map {
                let count = observations.iter().filter(|o| o.market_id == *slug).count();
                if count > 0 {
                    tracing::info!(
                        "  {} ({}/{}): {} observations",
                        slug,
                        w.asset,
                        w.timeframe,
                        count
                    );
                }
            }

            if observations.is_empty() {
                tracing::error!(
                    "No observations in the selected period. Try a longer period like --period 90d."
                );
                return Ok(());
            }

            observations.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

            let bt_config = backtesting::types::BacktestConfig {
                initial_capital: config.general.initial_capital,
                ..Default::default()
            };

            let result = backtesting::engine::run_backtest(&observations, &bt_config);

            match backtesting::ui::run_backtest_ui(&result) {
                Ok(_) => {}
                Err(e) => {
                    tracing::warn!("TUI not available ({}), using text report", e);
                }
            }
            backtesting::report::print_report(&result);
        }
        Commands::Dashboard { refresh } => {
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let dash = dashboard::terminal::Dashboard::new()?;

            loop {
                dash.render(&db, config.general.initial_capital).await?;
                tokio::time::sleep(std::time::Duration::from_secs(refresh)).await;
            }
        }
        Commands::Portfolio { detail: _ } => {
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let positions = db.get_open_positions().await?;

            println!("\nPortfolio ({} open positions):", positions.len());
            for pos in &positions {
                println!(
                    "  {} {} @ ${:.3} (size: ${:.2})",
                    pos.side, pos.market_id, pos.entry_price, pos.size_usd
                );
            }
        }
        Commands::Config { action } => match action {
            ConfigAction::Init => {
                std::fs::write(".env", include_str!("../.env.example"))?;
                tracing::info!("Created .env file from template");
            }
            ConfigAction::Show => {
                println!("{:#?}", config);
            }
        },
        Commands::Crypto {
            paper,
            timeframes: _,
        } => {
            tracing::info!("Starting crypto trading bot...");
            tracing::info!("Paper mode: {}", paper);

            let crypto_config = polymarket_bot::config::CryptoConfig {
                enabled: true,
                initial_capital: config.general.initial_capital,
                min_order_usd: 0.50,
                max_trades_per_hour: 1,
                min_confidence: 0.6,
                timeframes: vec!["5m".into(), "15m".into(), "1h".into()],
            };

            let engine = polymarket_bot::crypto::CryptoEngine::new(
                crypto_config,
                &config.api.gamma_base_url,
            );

            if let Err(e) = engine.run().await {
                tracing::error!("Crypto engine error: {}", e);
            }
        }
        Commands::CryptoBacktest {
            period,
            date,
            capital,
            timeframes,
            source_interval,
        } => {
            tracing::info!("Running crypto backtest...");
            tracing::info!("Period: {} days, Capital: ${:.2}", period, capital);
            tracing::info!("Timeframes: {}", timeframes);
            tracing::info!("Source interval: {}m", source_interval);

            let target_range = date
                .as_deref()
                .map(|value| -> anyhow::Result<(i64, i64)> {
                    let date = chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d")?;
                    let local_start = date
                        .and_hms_opt(0, 0, 0)
                        .ok_or_else(|| anyhow::anyhow!("invalid WIB calendar date"))?;
                    let start_ts = local_start.and_utc().timestamp_millis() - 7 * 60 * 60 * 1000;
                    Ok((start_ts, start_ts + 24 * 60 * 60 * 1000))
                })
                .transpose()?;

            if let Some((start_ts, end_ts)) = target_range {
                tracing::info!(
                    "Restricting evaluated windows to {} WIB (UTC {} to {})",
                    date.as_deref().unwrap_or_default(),
                    chrono::DateTime::from_timestamp_millis(start_ts)
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default(),
                    chrono::DateTime::from_timestamp_millis(end_ts)
                        .map(|value| value.to_rfc3339())
                        .unwrap_or_default()
                );
            }

            let timeframes: Vec<polymarket_bot::crypto::indicators::Timeframe> = timeframes
                .split(',')
                .filter_map(|tf| match tf.trim() {
                    "5m" => Some(polymarket_bot::crypto::indicators::Timeframe::M5),
                    "15m" => Some(polymarket_bot::crypto::indicators::Timeframe::M15),
                    "1h" => Some(polymarket_bot::crypto::indicators::Timeframe::H1),
                    "4h" => Some(polymarket_bot::crypto::indicators::Timeframe::H4),
                    "1d" => Some(polymarket_bot::crypto::indicators::Timeframe::D1),
                    _ => None,
                })
                .collect();

            let bt_config = polymarket_bot::crypto::backtest::CryptoBacktestConfig {
                initial_capital: capital,
                min_order_usd: 0.50,
                max_order_usd: 0.50,
                fee_pct: 0.02,
                timeframes,
                min_entry_price: 0.15,
                max_entry_price: 0.60,
                min_edge: 0.10,
                entry_minute: 3,
                source_interval_minutes: source_interval,
                target_start_ts: target_range.map(|range| range.0),
                target_end_ts: target_range.map(|range| range.1),
            };

            let engine = polymarket_bot::crypto::backtest::CryptoBacktestEngine::new();

            match engine.run_backtest(&bt_config, period).await {
                Ok(result) => {
                    println!("\n╔══════════════════════════════════════════════════════════════╗");
                    println!("║              CRYPTO BACKTEST REPORT                         ║");
                    println!("╠══════════════════════════════════════════════════════════════╣");
                    println!(
                        "║ Initial Capital:  ${:>8.2}                              ║",
                        result.initial_capital
                    );
                    println!(
                        "║ Final Capital:    ${:>8.2}                              ║",
                        result.final_capital
                    );
                    println!(
                        "║ Total PnL:        ${:>8.2} ({:.1}%)                    ║",
                        result.total_pnl,
                        (result.total_pnl / result.initial_capital) * 100.0
                    );
                    println!("╠══════════════════════════════════════════════════════════════╣");
                    println!(
                        "║ Total Trades:     {:>8}                              ║",
                        result.total_trades
                    );
                    println!(
                        "║ Winning Trades:   {:>8} ({:.1}%)                    ║",
                        result.winning_trades,
                        result.win_rate * 100.0
                    );
                    println!(
                        "║ Losing Trades:    {:>8}                              ║",
                        result.losing_trades
                    );
                    println!("╠══════════════════════════════════════════════════════════════╣");
                    println!(
                        "║ Avg Win:          ${:>8.2}                              ║",
                        result.avg_win
                    );
                    println!(
                        "║ Avg Loss:         ${:>8.2}                              ║",
                        result.avg_loss
                    );
                    println!(
                        "║ Profit Factor:    {:>8.2}                              ║",
                        result.profit_factor
                    );
                    println!(
                        "║ Max Drawdown:     {:>7.1}%                             ║",
                        result.max_drawdown * 100.0
                    );
                    println!("╚══════════════════════════════════════════════════════════════╝");
                    println!("\nModel diagnostics before synthetic-odds filtering:");
                    println!(
                        "  Raw signals: {} | accuracy: {:.1}% | avg confidence: {:.1}% | calibration gap: {:+.1}%",
                        result.diagnostics.raw_signals,
                        result.diagnostics.raw_accuracy * 100.0,
                        result.diagnostics.average_confidence * 100.0,
                        result.diagnostics.calibration_gap * 100.0
                    );
                    println!(
                        "  First half accuracy: {:.1}% (n={}) | second half: {:.1}% (n={}) | Brier: {:.3}",
                        result.diagnostics.first_half_accuracy * 100.0,
                        result.diagnostics.first_half_signals,
                        result.diagnostics.second_half_accuracy * 100.0,
                        result.diagnostics.second_half_signals,
                        result.diagnostics.brier_score
                    );
                    println!(
                        "  Conservative max-ask stress trades: {} | accuracy: {:.1}%",
                        result.total_trades,
                        result.win_rate * 100.0
                    );
                    println!(
                        "  WARNING: PnL assumes every signal fills at the configured maximum ask and resolves from Binance."
                    );

                    // Show last 10 trades
                    if !result.trades.is_empty() {
                        println!("\nLast 10 trades:");
                        let start = result.trades.len().saturating_sub(10);
                        for t in &result.trades[start..] {
                            let emoji = if t.won { "✓" } else { "✗" };
                            println!(
                                "  {} {:?} {} BTC {:.2} -> {:.2} | Ask: {:.2} Model margin: {:.1}% | PnL: ${:.2}",
                                emoji,
                                t.timeframe,
                                t.direction,
                                t.entry_price,
                                t.exit_price,
                                t.market_price,
                                t.edge * 100.0,
                                t.pnl
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("Backtest failed: {}", e);
                }
            }
        }
        Commands::Live { capital, max_order } => {
            tracing::info!("Starting live trading dashboard...");
            tracing::info!("Capital: ${:.2}, Max Order: ${:.2}", capital, max_order);

            let dashboard = polymarket_bot::crypto::live::LiveDashboard::new(capital, max_order);

            if let Err(e) = dashboard.run().await {
                tracing::error!("Dashboard error: {}", e);
            }
        }
        Commands::Web { port } => {
            tracing::info!("Starting web trading dashboard on port {}...", port);
            crate::web::run_web_server(port, &config).await?;
        }
        Commands::ProductionCheck => {
            production::run_preflight(&config).await?;
        }
        Commands::ProductionReadiness {
            json,
            require_canary_ready,
        } => {
            production::print_readiness(&config, json, require_canary_ready).await?;
        }
        Commands::StrategyManifest => {
            let build = build_info::BuildInfo::current();
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "strategy_version": config.runtime.strategy_version,
                    "build": {
                        "package_version": build.package_version,
                        "git_sha": build.git_sha,
                        "git_dirty": build.git_dirty,
                        "build_timestamp": build.build_timestamp,
                    },
                    "parameters": crypto::strategy::StrategyParameters::default(),
                }))?
            );
        }
        Commands::ForwardReport => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            let opportunities = store.load_forward_opportunities().await?;
            let trades = store
                .load_snapshot()
                .await?
                .map(|snapshot| snapshot.trades)
                .unwrap_or_default();
            println!(
                "{}",
                serde_json::to_string_pretty(&evaluation::build_report(&opportunities, &trades))?
            );
        }
        Commands::MonitorForward {
            interval_secs,
            max_iterations,
        } => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            let mut iterations = 0_u64;
            loop {
                let opportunities = store.load_forward_opportunities().await?;
                let trades = store
                    .load_snapshot()
                    .await?
                    .map(|snapshot| snapshot.trades)
                    .unwrap_or_default();
                let report = evaluation::build_report(&opportunities, &trades);
                let decision = evaluation::monitor_decision(&report);
                let summary = serde_json::json!({
                    "generated_at_ms": report.generated_at_ms,
                    "status": decision.status,
                    "should_halt_runtime": decision.should_halt_runtime,
                    "reasons": decision.reasons,
                    "settled_trades": report.settled_trades,
                    "executable_trade_accuracy": report.executable_trade_accuracy,
                    "profit_factor": report.profit_factor,
                    "max_drawdown_usd": report.max_drawdown_usd,
                    "settlement_mismatches": report.settlement_mismatches,
                    "promotion_ready": report.promotion_ready,
                    "promotion_reasons": report.promotion_reasons,
                });
                store
                    .audit_event(
                        "forward_monitor_report",
                        &config.runtime.mode.to_string(),
                        &config.runtime.strategy_version,
                        serde_json::json!({
                            "summary": summary,
                            "decision": decision,
                            "report": report,
                        }),
                    )
                    .await?;

                if decision.should_halt_runtime {
                    store
                        .set_runtime_state("halted", "forward-test monitor breach")
                        .await?;
                    store
                        .open_incident(
                            "forward-monitor-halt",
                            "forward_monitor_halt",
                            serde_json::json!({ "summary": summary }),
                        )
                        .await?;
                }

                println!("{}", serde_json::to_string_pretty(&summary)?);

                iterations += 1;
                if max_iterations > 0 && iterations >= max_iterations {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(interval_secs)).await;
            }
        }
        Commands::DrySign {
            token_id,
            price,
            size,
        } => {
            let report =
                execution::dry_signed::create_from_environment(&token_id, &price, &size).await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::Reconcile => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            store
                .set_runtime_state("preflight", "authenticated account preflight")
                .await?;
            store
                .set_runtime_state("reconciling", "querying authenticated CLOB state")
                .await?;
            let remote = match execution::dry_signed::reconcile_from_environment().await {
                Ok(remote) => remote,
                Err(error) => {
                    store
                        .set_runtime_state("halted", "authenticated reconciliation failed")
                        .await?;
                    store
                        .open_incident(
                            "reconciliation-request-failed",
                            "reconciliation_failure",
                            serde_json::json!({"message": error.to_string()}),
                        )
                        .await?;
                    return Err(error);
                }
            };
            store
                .resolve_incident("reconciliation-request-failed")
                .await?;
            let remote_open: std::collections::HashSet<_> =
                remote.open_order_ids.iter().cloned().collect();
            let remote_trades: std::collections::HashSet<_> =
                remote.trade_order_ids.iter().cloned().collect();
            store
                .apply_remote_order_evidence(&remote_open, &remote_trades)
                .await?;
            let local_ids = store.local_non_terminal_remote_ids().await?;
            let unresolved_without_remote_id =
                store.local_non_terminal_without_remote_id_count().await?;
            let local_positions = store.local_positions().await?;
            let remote_positions: Vec<_> = remote
                .positions
                .iter()
                .map(|position| (position.token_id.clone(), position.size.clone()))
                .collect();
            let assessment = execution::recovery::assess(
                &local_ids,
                unresolved_without_remote_id,
                &remote.open_order_ids,
                &remote.trade_order_ids,
                &local_positions,
                &remote_positions,
            );
            let report = store
                .record_reconciliation(
                    true,
                    assessment.mismatch_count,
                    serde_json::json!({"remote": remote, "assessment": assessment}),
                )
                .await?;
            if report.ready {
                store
                    .set_runtime_state("ready", "authenticated reconciliation passed")
                    .await?;
            } else {
                store.set_runtime_state("halted", &report.reason).await?;
                store
                    .open_incident(
                        &format!("reconciliation-mismatch-{}", chrono::Utc::now().timestamp()),
                        "reconciliation_mismatch",
                        serde_json::to_value(&report)?,
                    )
                    .await?;
            }
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !report.ready {
                anyhow::bail!("reconciliation blocked execution: {}", report.reason);
            }
        }
        Commands::AuthorizeCanary {
            client_key,
            max_usd,
            expires_minutes,
            confirm,
        } => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            let opportunities = store.load_forward_opportunities().await?;
            let trades = store
                .load_snapshot()
                .await?
                .map(|snapshot| snapshot.trades)
                .unwrap_or_default();
            let forward = evaluation::build_report(&opportunities, &trades);
            let reconciliation_ready = store.latest_reconciliation_ready().await?;
            let live_switch_enabled = std::env::var("POLYMARKET_LIVE_TRADING_ENABLED")
                .map(|value| value == "I_UNDERSTAND_LIVE_TRADING")
                .unwrap_or(false);
            execution::lifecycle::validate_canary_authorization(
                &confirm,
                max_usd,
                config.risk.max_order_usd,
                forward.promotion_ready,
                reconciliation_ready,
                live_switch_enabled,
            )?;
            let intent = store
                .load_execution_intent(&client_key)
                .await?
                .ok_or_else(|| anyhow::anyhow!("unknown durable execution intent"))?;
            if (intent.requested_usd.as_f64() - max_usd).abs() > 0.000_001 {
                anyhow::bail!(
                    "authorization amount must exactly match intent amount ${:.6}",
                    intent.requested_usd.as_f64()
                );
            }
            let expires_at_ms =
                chrono::Utc::now().timestamp_millis() + expires_minutes.clamp(1, 30) * 60_000;
            let authorization_id = store
                .issue_canary_authorization(&client_key, max_usd, expires_at_ms)
                .await?;
            println!(
                "{}",
                serde_json::json!({
                    "authorized": true,
                    "authorization_id": authorization_id,
                    "client_key": &client_key,
                    "max_usd": max_usd,
                    "expires_at_ms": expires_at_ms,
                    "submitted": false
                })
            );
        }
        Commands::CanaryReview { client_key } => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            let readiness = production::build_readiness_report(&config).await;
            let intent = store
                .load_execution_intent(&client_key)
                .await?
                .ok_or_else(|| anyhow::anyhow!("unknown durable execution intent"))?;
            let intent_amount_usd = intent.requested_usd.as_f64();
            let intent_checks = serde_json::json!({
                "client_key_matches": intent.client_order_key == client_key,
                "within_configured_max_order_usd": intent_amount_usd <= config.risk.max_order_usd,
                "strategy_version_matches": intent.strategy_version == config.runtime.strategy_version,
                "risk_checks_all_passed": intent.risk_checks.iter().all(|check| check.passed),
                "order_type": config.execution.order_type,
                "fok_only": config.execution.order_type == "FOK",
            });
            let intent_ready = intent.client_order_key == client_key
                && intent_amount_usd <= config.risk.max_order_usd
                && intent.strategy_version == config.runtime.strategy_version
                && intent.risk_checks.iter().all(|check| check.passed)
                && config.execution.order_type == "FOK";
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "review_ready": readiness.canary_ready && intent_ready,
                    "submitted": false,
                    "authorization_issued": false,
                    "client_key": client_key,
                    "intent": intent,
                    "intent_checks": intent_checks,
                    "readiness": readiness,
                    "strategy_manifest": {
                        "strategy_version": config.runtime.strategy_version,
                        "parameters": crypto::strategy::StrategyParameters::default(),
                    },
                    "operator_next_steps": {
                        "authorize_command": format!(
                            "authorize-canary --client-key {} --max-usd {:.6} --confirm {}",
                            &client_key,
                            intent_amount_usd,
                            execution::lifecycle::CANARY_CONFIRMATION
                        ),
                        "submit_command": format!(
                            "submit-canary --authorization-id <authorization_id> --client-key {} --confirm {}",
                            &client_key,
                            execution::live::SUBMIT_CANARY_CONFIRMATION
                        ),
                        "note": "Do not run authorization or submission until review_ready is true and the exact intent is manually approved."
                    }
                }))?
            );
        }
        Commands::SubmitCanary {
            authorization_id,
            client_key,
            confirm,
        } => {
            if confirm != execution::live::SUBMIT_CANARY_CONFIRMATION {
                anyhow::bail!("exact submit-canary confirmation phrase is required");
            }
            production::run_preflight(&config).await?;
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            if !store.latest_reconciliation_ready().await? {
                anyhow::bail!("fresh successful reconciliation is required");
            }
            let intent = store
                .load_execution_intent(&client_key)
                .await?
                .ok_or_else(|| anyhow::anyhow!("unknown durable execution intent"))?;
            if intent.requested_usd.as_f64() > config.risk.max_order_usd {
                anyhow::bail!("intent exceeds configured maximum order amount");
            }
            let executor = execution::live::SdkOrderExecutor::new(
                config.execution.heartbeat_interval_secs,
                config.risk.max_fee_rate_bps,
                config.risk.min_balance_reserve_usd,
            );
            let result = execution::live::execute_authorized_canary(
                &store,
                &executor,
                &authorization_id,
                &intent,
            )
            .await;
            store
                .set_runtime_state("halted", "post-canary reconciliation required")
                .await?;
            match result {
                Ok(outcome) => println!("{}", serde_json::to_string_pretty(&outcome)?),
                Err(error) => {
                    store
                        .open_incident(
                            &format!("ambiguous-canary-{}", client_key),
                            "ambiguous_order_submission",
                            serde_json::json!({"client_key": client_key}),
                        )
                        .await?;
                    return Err(error);
                }
            }
        }
        Commands::CancelAllLive { confirm } => {
            use execution::live::OrderExecutor;
            if confirm != execution::live::CANCEL_ALL_CONFIRMATION {
                anyhow::bail!("exact cancel-all confirmation phrase is required");
            }
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            store
                .set_runtime_state("halted", "emergency cancel-all initiated")
                .await?;
            store.mark_remote_orders_cancel_pending().await?;
            let executor = execution::live::SdkOrderExecutor::new(
                config.execution.heartbeat_interval_secs,
                config.risk.max_fee_rate_bps,
                config.risk.min_balance_reserve_usd,
            );
            let outcome = executor.cancel_all().await?;
            store
                .mark_remote_orders_cancelled(&outcome.canceled)
                .await?;
            if outcome.not_canceled > 0 {
                store
                    .open_incident(
                        &format!("cancel-all-incomplete-{}", chrono::Utc::now().timestamp()),
                        "cancel_all_incomplete",
                        serde_json::to_value(&outcome)?,
                    )
                    .await?;
            }
            println!("{}", serde_json::to_string_pretty(&outcome)?);
        }
        Commands::Incidents => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&store.load_open_incidents().await?)?
            );
        }
        Commands::ResolveIncident {
            incident_key,
            confirm,
        } => {
            if confirm != "I_INVESTIGATED_AND_RESOLVE_THIS_INCIDENT" {
                anyhow::bail!("exact incident-resolution confirmation phrase is required");
            }
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            if !store.resolve_incident(&incident_key).await? {
                anyhow::bail!("incident does not exist or is already resolved");
            }
            store
                .set_runtime_state(
                    "halted",
                    "incident resolved; fresh reconciliation is still required",
                )
                .await?;
            println!("Incident resolved. Run authenticated reconciliation before execution.");
        }
        Commands::MonitorUserStream { max_events } => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            let events = execution::user_stream::monitor(&store, max_events).await?;
            println!("Processed {events} authenticated user event(s)");
        }
        Commands::PlanRedemptions => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            let remote = execution::dry_signed::reconcile_from_environment().await?;
            let local_ids = store.local_non_terminal_remote_ids().await?;
            let local_without_remote_id =
                store.local_non_terminal_without_remote_id_count().await?;
            let local_positions = store.local_positions().await?;
            let remote_positions: Vec<_> = remote
                .positions
                .iter()
                .map(|position| (position.token_id.clone(), position.size.clone()))
                .collect();
            let assessment = execution::recovery::assess(
                &local_ids,
                local_without_remote_id,
                &remote.open_order_ids,
                &remote.trade_order_ids,
                &local_positions,
                &remote_positions,
            );
            if assessment.state != execution::recovery::StartupState::Ready {
                store
                    .set_runtime_state("halted", "redemption planning found remote/local mismatch")
                    .await?;
                store
                    .open_incident(
                        "redemption-planning-mismatch",
                        "redemption_reconciliation_mismatch",
                        serde_json::to_value(&assessment)?,
                    )
                    .await?;
                anyhow::bail!("redemption planning blocked by remote/local mismatch");
            }
            let redeemable: Vec<_> = remote
                .positions
                .iter()
                .filter(|position| position.redeemable)
                .collect();
            for position in &redeemable {
                store
                    .record_redemption_plan(
                        &position.condition_id,
                        &position.token_id,
                        &position.market_slug,
                        &position.size,
                        serde_json::to_value(position)?,
                    )
                    .await?;
            }
            if !redeemable.is_empty() {
                store
                    .set_runtime_state(
                        "halted",
                        "redeemable positions require operator-reviewed Relayer execution",
                    )
                    .await?;
                store
                    .open_incident(
                        "redeemable-positions-pending",
                        "redemption_required",
                        serde_json::json!({"count": redeemable.len()}),
                    )
                    .await?;
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "remote_checked": true,
                    "redeemable_positions": redeemable.len(),
                    "submitted": false,
                    "relayer_execution_implemented": false,
                    "reason": "POLY_1271 deposit-wallet redemption must use an operator-reviewed Relayer V2 batch",
                    "plans": store.load_redemption_plans().await?
                }))?
            );
        }
        Commands::OperationalStatus => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            let opportunities = store.load_forward_opportunities().await?;
            let trades = store
                .load_snapshot()
                .await?
                .map(|snapshot| snapshot.trades)
                .unwrap_or_default();
            let forward = evaluation::build_report(&opportunities, &trades);
            let live_switch_enabled = std::env::var("POLYMARKET_LIVE_TRADING_ENABLED")
                .map(|value| value == "I_UNDERSTAND_LIVE_TRADING")
                .unwrap_or(false);
            let reconciliation_ready = store.latest_reconciliation_ready().await?;
            let canary_control_gates_ready = reconciliation_ready && live_switch_enabled;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "runtime_mode": config.runtime.mode.to_string(),
                    "live_submission_implemented": true,
                    "live_switch_enabled": live_switch_enabled,
                    "forward_promotion_ready": forward.promotion_ready,
                    "forward_promotion_required_for_canary": false,
                    "forward_promotion_reasons": forward.promotion_reasons,
                    "reconciliation_ready": reconciliation_ready,
                    "local_non_terminal_orders": store.local_non_terminal_order_count().await?,
                    "open_incidents": store.open_incident_count().await?,
                    "startup_state": store.runtime_state().await?.0,
                    "canary_control_gates_ready": canary_control_gates_ready,
                    "canary_submission_ready": false,
                    "canary_submission_ready_reason":
                        "manual canary submission is retained for reviewed drills; auto-live uses explicit local env confirmations plus hard production gates; forward promotion metrics are observational"
                }))?
            );
        }
        Commands::Backup { destination } => {
            let store = storage::dashboard::DashboardStore::open(std::path::Path::new(
                &config.storage.database_path,
            ))
            .await?;
            let destination = destination
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| {
                    std::path::Path::new(&config.storage.backup_directory).join(format!(
                        "trading-{}.db",
                        chrono::Utc::now().format("%Y%m%dT%H%M%SZ")
                    ))
                });
            store.backup_to(&destination).await?;
            println!("Backup created: {}", destination.display());
        }
        Commands::VerifyDatabase { path } => {
            storage::dashboard::DashboardStore::verify_integrity(std::path::Path::new(&path))
                .await?;
            println!(
                "{}",
                serde_json::json!({
                    "ok": true,
                    "path": path,
                    "integrity_check": "ok"
                })
            );
        }
    }

    Ok(())
}
