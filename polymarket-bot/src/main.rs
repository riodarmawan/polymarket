pub mod analyzers;
pub mod api;
pub mod backtesting;
pub mod cli;
pub mod collector;
pub mod config;
pub mod crypto;
pub mod dashboard;
pub mod engine;
pub mod error;
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
    tracing_subscriber::fmt().with_env_filter(filter).init();

    let cli = Cli::parse();
    let config = Config::load()?;

    match cli.command {
        Commands::Collect { daemon, interval } => {
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let collector =
                collector::data_collector::DataCollector::new(
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
            let db_path = std::path::Path::new(&config.general.data_dir).join("polymarket.db");
            let db = storage::database::Database::new(&db_path).await?;
            let markets = db.get_markets().await?;

            let period_days: u32 = period.trim_end_matches('d').parse().unwrap_or(30);
            tracing::info!(
                "Backtesting {} markets with period: {} days, strategy: {}",
                markets.len(),
                period_days,
                strategy
            );

            // Fetch real price history from CLOB for each market with token ID
            // Use gamma_base_url (proxy at localhost:3000) since bot can't reach CLOB directly
            let clob_client = api::clob::ClobClient::new(&config.api.gamma_base_url);
            let mut observations = Vec::new();
            let mut markets_with_data = 0;

            for market in &markets {
                let token_id = match &market.yes_token_id {
                    Some(t) => t.clone(),
                    None => {
                        tracing::debug!("Skipping {} - no token ID", market.id);
                        continue;
                    }
                };

                let interval = match period_days {
                    1 => "1d",
                    7 => "1w",
                    30 => "1m",
                    90 => "3m",
                    _ => "1m",
                };

                match clob_client.fetch_price_history(&token_id, interval, 60).await {
                    Ok(history) => {
                        let points = history.history;
                        if points.is_empty() {
                            tracing::debug!("No history for {}", market.id);
                            continue;
                        }
                        tracing::info!("Market {}: {} price points", market.question.chars().take(40).collect::<String>(), points.len());

                        for p in &points {
                            let spread = 0.02;
                            observations.push(backtesting::types::PriceObservation {
                                timestamp: p.t,
                                market_id: market.id.clone(),
                                ask_price: p.p + spread / 2.0,
                                bid_price: p.p - spread / 2.0,
                                ask_depth: 500.0,
                                bid_depth: 500.0,
                                spread,
                                mid_price: p.p,
                            });
                        }
                        markets_with_data += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to fetch history for {}: {}", market.id, e);
                    }
                }
            }

            tracing::info!("Fetched price history for {} markets ({} total observations)", markets_with_data, observations.len());

            if observations.is_empty() {
                tracing::error!("No price data available. Run 'collect' first and ensure proxy is running.");
                return Ok(());
            }

            observations.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

            let bt_config = backtesting::types::BacktestConfig {
                initial_capital: config.general.initial_capital,
                ..Default::default()
            };

            let result = backtesting::engine::run_backtest(&observations, &bt_config);

            // Try TUI, fallback to report if terminal not interactive
            match backtesting::ui::run_backtest_ui(&result) {
                Ok(_) => {},
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
        Commands::Crypto { paper, timeframes: _ } => {
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
        Commands::CryptoBacktest { period, capital, timeframes, source_interval } => {
            tracing::info!("Running crypto backtest...");
            tracing::info!("Period: {} days, Capital: ${:.2}", period, capital);
            tracing::info!("Timeframes: {}", timeframes);
            tracing::info!("Source interval: {}m", source_interval);
            
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
                min_order_usd: 0.10,
                max_order_usd: 0.50,
                fee_pct: 0.02,
                timeframes,
                min_entry_price: 0.15,
                max_entry_price: 0.60,
                min_edge: 0.10,
                entry_minute: 3,
                source_interval_minutes: source_interval,
            };
            
            let engine = polymarket_bot::crypto::backtest::CryptoBacktestEngine::new();
            
            match engine.run_backtest(&bt_config, period).await {
                Ok(result) => {
                    println!("\n╔══════════════════════════════════════════════════════════════╗");
                    println!("║              CRYPTO BACKTEST REPORT                         ║");
                    println!("╠══════════════════════════════════════════════════════════════╣");
                    println!("║ Initial Capital:  ${:>8.2}                              ║", result.initial_capital);
                    println!("║ Final Capital:    ${:>8.2}                              ║", result.final_capital);
                    println!("║ Total PnL:        ${:>8.2} ({:.1}%)                    ║", result.total_pnl, (result.total_pnl / result.initial_capital) * 100.0);
                    println!("╠══════════════════════════════════════════════════════════════╣");
                    println!("║ Total Trades:     {:>8}                              ║", result.total_trades);
                    println!("║ Winning Trades:   {:>8} ({:.1}%)                    ║", result.winning_trades, result.win_rate * 100.0);
                    println!("║ Losing Trades:    {:>8}                              ║", result.losing_trades);
                    println!("╠══════════════════════════════════════════════════════════════╣");
                    println!("║ Avg Win:          ${:>8.2}                              ║", result.avg_win);
                    println!("║ Avg Loss:         ${:>8.2}                              ║", result.avg_loss);
                    println!("║ Profit Factor:    {:>8.2}                              ║", result.profit_factor);
                    println!("║ Max Drawdown:     {:>7.1}%                             ║", result.max_drawdown * 100.0);
                    println!("╚══════════════════════════════════════════════════════════════╝");
                    
                    // Show last 10 trades
                    if !result.trades.is_empty() {
                        println!("\nLast 10 trades:");
                        let start = result.trades.len().saturating_sub(10);
                        for t in &result.trades[start..] {
                            let emoji = if t.won { "✓" } else { "✗" };
                            println!(
                                "  {} {:?} {} BTC {:.2} -> {:.2} | Ask: {:.2} Edge: {:.1}% | PnL: ${:.2}",
                                emoji, t.timeframe, t.direction, t.entry_price, t.exit_price, t.market_price, t.edge * 100.0, t.pnl
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
            crate::web::run_web_server(port).await?;
        }
        Commands::ProductionCheck => {
            production::run_preflight().await?;
        }
    }

    Ok(())
}
