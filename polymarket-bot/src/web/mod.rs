pub mod api;
pub mod price_proxy;
pub mod state;
pub mod ws;

use crate::config::Config;
use crate::crypto::binance_ws::{BinanceRestClient, Candle};
use crate::crypto::live::gamma_client::{
    generate_updown_slug, get_current_interval_start, get_remaining_seconds, BuyQuote, ClobClient,
    GammaClient,
};
use crate::engine::microstructure::{
    estimate_probability, executable_quote, timing_for, ProbabilityInput, QuoteDecision, QuoteInput,
};
use crate::engine::risk::{
    Direction, RiskCheck, RiskDecision, RiskEngine, RiskPolicy, RiskRequest,
};
use crate::engine::strategy_service;
use axum::Router;
use state::AppState;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

// Assets to scan for Up/Down markets
const CRYPTO_ASSETS: &[&str] = &["btc"];
const MAX_CLOCK_DRIFT_MS: i64 = 5_000;
const MAX_SIGNAL_AGE_MS: i64 = 15_000;
const MAKER_TIME_IN_FORCE_MS: u64 = 1_500;

pub async fn run_web_server(port: u16, config: &Config) -> anyhow::Result<()> {
    let state = AppState::new(config).await?;

    let state_clone = state.clone();
    let backup_directory = PathBuf::from(&config.storage.backup_directory);
    tokio::spawn(async move {
        run_periodic_backup(state_clone, backup_directory).await;
    });

    // Start price proxy
    let state_clone = state.clone();
    tokio::spawn(async move {
        price_proxy::run_price_proxy(state_clone).await;
    });

    // Start BTC Up/Down market scanner
    let state_clone = state.clone();
    let gamma_base_url = config.api.gamma_base_url.clone();
    let clob_base_url = config.api.clob_base_url.clone();
    tokio::spawn(async move {
        run_updown_scanner(state_clone, gamma_base_url, clob_base_url).await;
    });

    // Start signal generator
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_signal_generator(state_clone).await;
    });

    // Start the separate BTC 5m signal generator.
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_5m_signal_generator(state_clone).await;
    });

    // Start paper execution for BTC 15m markets.
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_paper_executor(state_clone).await;
    });

    // Start isolated paper execution for BTC 5m markets.
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_5m_paper_executor(state_clone).await;
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/price", axum::routing::get(api::get_price))
        .route("/api/markets", axum::routing::get(api::get_markets))
        .route("/api/updown", axum::routing::get(api::get_updown_markets))
        .route("/api/signals", axum::routing::get(api::get_signals))
        .route("/api/trades", axum::routing::get(api::get_trades))
        .route("/api/stats", axum::routing::get(api::get_stats))
        .route(
            "/api/forward-report",
            axum::routing::get(api::get_forward_report),
        )
        .route(
            "/api/execution-audit",
            axum::routing::get(api::get_execution_audit),
        )
        .route(
            "/api/production-readiness",
            axum::routing::get(api::get_production_readiness),
        )
        .route("/api/account", axum::routing::get(api::get_remote_account))
        .route("/api/health", axum::routing::get(api::get_health))
        .route("/api/settings", axum::routing::get(api::get_settings))
        .route("/api/settings", axum::routing::post(api::update_settings))
        .route("/ws", axum::routing::get(ws::ws_handler))
        .fallback_service(ServeDir::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/web/static"),
        ))
        .layer(cors)
        .with_state(state);

    let bind = std::env::var("POLYMARKET_DASHBOARD_BIND")
        .unwrap_or_else(|_| config.dashboard.bind.clone());
    let addr: SocketAddr = bind
        .parse()
        .unwrap_or_else(|_| SocketAddr::from(([127, 0, 0, 1], port)));
    tracing::info!("Web dashboard listening on {}", addr);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn run_periodic_backup(state: AppState, backup_directory: PathBuf) {
    loop {
        let destination = backup_directory.join(format!(
            "dashboard-{}.db",
            chrono::Utc::now().timestamp_millis()
        ));
        match state.store.backup_to(&destination).await {
            Ok(()) => {
                if let Err(error) = state
                    .audit(
                        "database_backup_created",
                        serde_json::json!({"path": destination.display().to_string()}),
                    )
                    .await
                {
                    state
                        .halt_after_persistence_failure("backup audit", &error)
                        .await;
                }
            }
            Err(error) => {
                state
                    .halt_after_persistence_failure("database backup", &error)
                    .await;
            }
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(6 * 60 * 60)).await;
    }
}

async fn run_updown_scanner(state: AppState, gamma_base_url: String, clob_base_url: String) {
    let gamma_client = GammaClient::new(&gamma_base_url);
    let clob_client = ClobClient::new(&clob_base_url);

    loop {
        let clock_drift_ms = clob_client
            .fetch_server_time_ms()
            .await
            .ok()
            .map(|server_time| chrono::Utc::now().timestamp_millis() - server_time);
        let price_data = state.price.read().await.clone();
        let btc_price = if price_data.source == "live" {
            price_data.price
        } else {
            0.0
        };
        let previous_prices: HashMap<String, f64> = state
            .updown_markets
            .read()
            .await
            .iter()
            .map(|market| (market.slug.clone(), market.price_to_beat))
            .collect();

        let mut updown_markets = Vec::new();

        let scan_targets: Vec<(&str, &str, u32)> = CRYPTO_ASSETS
            .iter()
            .map(|asset| (*asset, "15m", 15))
            .chain(std::iter::once(("btc", "5m", 5)))
            .collect();

        for (asset, interval, interval_minutes) in scan_targets {
            let current_start = get_current_interval_start(interval_minutes);
            let end_ts = current_start + interval_minutes as i64 * 60;
            let remaining = get_remaining_seconds(end_ts);
            let slug = generate_updown_slug(asset, interval, current_start);
            let next_start = current_start + interval_minutes as i64 * 60;
            let seconds_until_next = next_start - chrono::Utc::now().timestamp();
            if (0..=90).contains(&seconds_until_next) {
                let gamma = gamma_client.clone();
                let clob = clob_client.clone();
                let asset = asset.to_string();
                let interval = interval.to_string();
                tokio::spawn(async move {
                    prewarm_next_updown_market(
                        &gamma,
                        &clob,
                        &asset,
                        &interval,
                        next_start,
                        seconds_until_next,
                    )
                    .await;
                });
            }

            match gamma_client.fetch_event_by_slug(&slug).await {
                Ok(Some(event)) => {
                    let Some(market) = event.markets.as_ref().and_then(|markets| markets.first())
                    else {
                        updown_markets.push(unavailable_market(
                            asset,
                            interval,
                            &slug,
                            current_start,
                            end_ts,
                            remaining,
                            state::DataStatus::InvalidPayload,
                            "Gamma event has no market",
                        ));
                        continue;
                    };
                    let Some((up_token, down_token)) = market.mapped_up_down_tokens() else {
                        updown_markets.push(unavailable_market(
                            asset,
                            interval,
                            &slug,
                            current_start,
                            end_ts,
                            remaining,
                            state::DataStatus::InvalidPayload,
                            "Cannot map UP/DOWN outcomes to CLOB tokens",
                        ));
                        continue;
                    };

                    let (up_book, down_book, up_fee, down_fee) = tokio::join!(
                        clob_client.fetch_orderbook(&up_token),
                        clob_client.fetch_orderbook(&down_token),
                        clob_client.fetch_fee_rate_bps(&up_token),
                        clob_client.fetch_fee_rate_bps(&down_token)
                    );
                    let up_book = match up_book {
                        Ok(book) => match book.validated_top_of_book(&up_token) {
                            Some(_) => book,
                            None => {
                                updown_markets.push(unavailable_market(
                                    asset,
                                    interval,
                                    &slug,
                                    current_start,
                                    end_ts,
                                    remaining,
                                    state::DataStatus::Incomplete,
                                    "UP orderbook has no valid levels",
                                ));
                                continue;
                            }
                        },
                        Err(error) => {
                            updown_markets.push(unavailable_market(
                                asset,
                                interval,
                                &slug,
                                current_start,
                                end_ts,
                                remaining,
                                classify_market_data_error(&error),
                                "UP orderbook request failed",
                            ));
                            continue;
                        }
                    };
                    let down_book = match down_book {
                        Ok(book) => match book.validated_top_of_book(&down_token) {
                            Some(_) => book,
                            None => {
                                updown_markets.push(unavailable_market(
                                    asset,
                                    interval,
                                    &slug,
                                    current_start,
                                    end_ts,
                                    remaining,
                                    state::DataStatus::Incomplete,
                                    "DOWN orderbook has no valid levels",
                                ));
                                continue;
                            }
                        },
                        Err(error) => {
                            updown_markets.push(unavailable_market(
                                asset,
                                interval,
                                &slug,
                                current_start,
                                end_ts,
                                remaining,
                                classify_market_data_error(&error),
                                "DOWN orderbook request failed",
                            ));
                            continue;
                        }
                    };
                    let Some((up_bid, up_ask)) = up_book.validated_top_of_book(&up_token) else {
                        continue;
                    };
                    let Some((down_bid, down_ask)) = down_book.validated_top_of_book(&down_token)
                    else {
                        continue;
                    };
                    let up_quote =
                        up_book.quote_buy_usd(&up_token, state.runtime.configured_max_order_usd);
                    let down_quote = down_book
                        .quote_buy_usd(&down_token, state.runtime.configured_max_order_usd);
                    let tick_size = match (up_book.tick_size(), down_book.tick_size()) {
                        (Some(up), Some(down)) if (up - down).abs() < f64::EPSILON => Some(up),
                        _ => None,
                    };
                    let min_order_size =
                        match (up_book.min_order_size(), down_book.min_order_size()) {
                            (Some(up), Some(down)) => Some(up.max(down)),
                            _ => None,
                        };
                    let fee_rate_bps = match (up_fee, down_fee) {
                        (Ok(up), Ok(down)) => Some(up.max(down)),
                        _ => None,
                    };
                    let book_timestamp_ms = match (up_book.timestamp_ms(), down_book.timestamp_ms())
                    {
                        (Some(up), Some(down)) => up.min(down),
                        _ => chrono::Utc::now().timestamp_millis(),
                    };
                    let current_price = if asset == "btc" { btc_price } else { 0.0 };
                    let price_to_beat = previous_prices
                        .get(&slug)
                        .copied()
                        .filter(|price| *price > 0.0)
                        .unwrap_or(current_price);
                    let reference_prices_ready = current_price > 0.0 && price_to_beat > 0.0;
                    let one_sided = up_bid.is_none()
                        || up_ask.is_none()
                        || down_bid.is_none()
                        || down_ask.is_none();
                    let metadata_complete = tick_size.is_some()
                        && min_order_size.is_some()
                        && fee_rate_bps.is_some()
                        && up_quote.is_some()
                        && down_quote.is_some()
                        && reference_prices_ready
                        && !one_sided
                        && clock_drift_ms
                            .map(|drift| drift.abs() <= MAX_CLOCK_DRIFT_MS)
                            .unwrap_or(false);
                    let spread = match (up_ask, down_ask, up_bid, down_bid) {
                        (Some(up_ask), Some(down_ask), _, _) => (up_ask + down_ask - 1.0).abs(),
                        (_, Some(down_ask), Some(up_bid), _) => (down_ask - (1.0 - up_bid)).abs(),
                        (Some(up_ask), _, _, Some(down_bid)) => (up_ask - (1.0 - down_bid)).abs(),
                        _ => 0.0,
                    };

                    updown_markets.push(state::UpDownMarket {
                        asset: asset.to_string(),
                        slug: slug.clone(),
                        interval: interval.to_string(),
                        start_ts: current_start,
                        end_ts,
                        remaining_seconds: remaining,
                        up_token_id: Some(up_token),
                        down_token_id: Some(down_token),
                        up_best_ask: up_ask,
                        up_best_bid: up_bid,
                        down_best_ask: down_ask,
                        down_best_bid: down_bid,
                        spread,
                        status: if remaining > 0 {
                            "live".to_string()
                        } else {
                            "ended".to_string()
                        },
                        price_to_beat,
                        current_price,
                        captured_at_ms: book_timestamp_ms,
                        data_status: if one_sided {
                            state::DataStatus::OneSided
                        } else if metadata_complete {
                            state::DataStatus::Ready
                        } else {
                            state::DataStatus::Incomplete
                        },
                        data_detail: if one_sided {
                            format!(
                                "One-sided CLOB: UP bids={} asks={}, DOWN bids={} asks={}; execution blocked",
                                up_book.bids.len(),
                                up_book.asks.len(),
                                down_book.bids.len(),
                                down_book.asks.len()
                            )
                        } else if metadata_complete {
                            "Gamma metadata, CLOB books, fees, depth, and clock validated".to_string()
                        } else if !reference_prices_ready {
                            "Missing reference price or price-to-beat; execution blocked"
                                .to_string()
                        } else {
                            "Execution metadata, depth, fee rate, or clock check incomplete"
                                .to_string()
                        },
                        token_mapping_valid: true,
                        tick_size,
                        min_order_size,
                        fee_rate_bps,
                        negative_risk: Some(up_book.neg_risk || down_book.neg_risk),
                        up_executable_depth_usd: up_quote
                            .map(|quote| quote.available_depth_usd)
                            .unwrap_or(0.0),
                        down_executable_depth_usd: down_quote
                            .map(|quote| quote.available_depth_usd)
                            .unwrap_or(0.0),
                        up_expected_fill_price: up_quote.map(|quote| quote.average_price),
                        down_expected_fill_price: down_quote.map(|quote| quote.average_price),
                        clock_drift_ms,
                    });
                }
                Ok(None) => {
                    updown_markets.push(unavailable_market(
                        asset,
                        interval,
                        &slug,
                        current_start,
                        end_ts,
                        remaining,
                        state::DataStatus::NotFound,
                        "Gamma event not found",
                    ));
                }
                Err(e) => {
                    tracing::error!("Failed to fetch event {}: {}", slug, e);
                    updown_markets.push(unavailable_market(
                        asset,
                        interval,
                        &slug,
                        current_start,
                        end_ts,
                        remaining,
                        classify_market_data_error(&e),
                        "Gamma event request failed",
                    ));
                }
            }
        }

        if let Err(error) = state.store.record_market_scan(&updown_markets).await {
            state
                .halt_after_persistence_failure("market snapshot persistence", &error)
                .await;
        }

        // Update state
        let mut state_markets = state.updown_markets.write().await;
        *state_markets = updown_markets;

        let mut last_scan = state.last_scan_at.write().await;
        *last_scan = chrono::Utc::now().timestamp_millis();

        tracing::info!("Scanned {} Up/Down market targets", CRYPTO_ASSETS.len() + 1);

        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}

fn unavailable_market(
    asset: &str,
    interval: &str,
    slug: &str,
    start_ts: i64,
    end_ts: i64,
    remaining_seconds: i64,
    data_status: state::DataStatus,
    detail: &str,
) -> state::UpDownMarket {
    state::UpDownMarket {
        asset: asset.to_string(),
        slug: slug.to_string(),
        interval: interval.to_string(),
        start_ts,
        end_ts,
        remaining_seconds,
        up_token_id: None,
        down_token_id: None,
        up_best_ask: None,
        up_best_bid: None,
        down_best_ask: None,
        down_best_bid: None,
        spread: 0.0,
        status: data_status.as_str().to_string(),
        price_to_beat: 0.0,
        current_price: 0.0,
        captured_at_ms: chrono::Utc::now().timestamp_millis(),
        data_status,
        data_detail: detail.to_string(),
        token_mapping_valid: false,
        tick_size: None,
        min_order_size: None,
        fee_rate_bps: None,
        negative_risk: None,
        up_executable_depth_usd: 0.0,
        down_executable_depth_usd: 0.0,
        up_expected_fill_price: None,
        down_expected_fill_price: None,
        clock_drift_ms: None,
    }
}

fn classify_market_data_error(error: &anyhow::Error) -> state::DataStatus {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("timeout") || message.contains("timed out") {
        state::DataStatus::Timeout
    } else if message.contains("429") || message.contains("rate limit") {
        state::DataStatus::RateLimited
    } else if message.contains("decode") || message.contains("json") {
        state::DataStatus::InvalidPayload
    } else {
        state::DataStatus::Unavailable
    }
}

async fn prewarm_next_updown_market(
    gamma_client: &GammaClient,
    clob_client: &ClobClient,
    asset: &str,
    interval: &str,
    next_start: i64,
    seconds_until_next: i64,
) {
    let slug = generate_updown_slug(asset, interval, next_start);
    let result = tokio::time::timeout(tokio::time::Duration::from_secs(4), async {
        let Some(event) = gamma_client.fetch_event_by_slug(&slug).await? else {
            anyhow::bail!("next event not found");
        };
        let market = event
            .markets
            .as_ref()
            .and_then(|markets| markets.first())
            .ok_or_else(|| anyhow::anyhow!("next event has no market"))?;
        let (up_token, down_token) = market
            .mapped_up_down_tokens()
            .ok_or_else(|| anyhow::anyhow!("next event token mapping unavailable"))?;
        let (up_book, down_book) = tokio::join!(
            clob_client.fetch_orderbook(&up_token),
            clob_client.fetch_orderbook(&down_token)
        );
        up_book?
            .validated_top_of_book(&up_token)
            .ok_or_else(|| anyhow::anyhow!("next UP book invalid"))?;
        down_book?
            .validated_top_of_book(&down_token)
            .ok_or_else(|| anyhow::anyhow!("next DOWN book invalid"))?;
        Ok::<(), anyhow::Error>(())
    })
    .await;

    match result {
        Ok(Ok(())) => tracing::debug!(
            slug,
            seconds_until_next,
            "Prewarmed next Up/Down market metadata and CLOB books"
        ),
        Ok(Err(error)) => tracing::debug!(
            slug,
            seconds_until_next,
            "Next Up/Down prewarm skipped: {error}"
        ),
        Err(_) => tracing::debug!(slug, seconds_until_next, "Next Up/Down prewarm timed out"),
    }
}

fn candles_are_complete_and_fresh(candles: &[Candle], now_ms: i64) -> bool {
    if candles.is_empty() {
        return false;
    }

    let structurally_valid = candles.iter().all(|candle| {
        candle.timestamp > 0
            && candle.open.is_finite()
            && candle.high.is_finite()
            && candle.low.is_finite()
            && candle.close.is_finite()
            && candle.open > 0.0
            && candle.high >= candle.open.max(candle.close)
            && candle.low > 0.0
            && candle.low <= candle.open.min(candle.close)
    });
    let ordered = candles
        .windows(2)
        .all(|pair| pair[1].timestamp - pair[0].timestamp == 60_000);
    let latest_age_ms = now_ms - candles.last().map(|candle| candle.timestamp).unwrap_or(0);

    structurally_valid && ordered && (-10_000..=120_000).contains(&latest_age_ms)
}

async fn run_signal_generator(state: AppState) {
    run_strategy_service(state, "15m", 15, 90).await;
}

async fn run_5m_signal_generator(state: AppState) {
    run_strategy_service(state, "5m", 5, 40).await;
}

async fn run_strategy_service(
    state: AppState,
    timeframe: &'static str,
    minutes: u32,
    limit: usize,
) {
    let rest_client = BinanceRestClient::new();

    loop {
        let now = chrono::Utc::now();
        let current_start = get_current_interval_start(minutes);
        let candle_request = tokio::time::timeout(
            tokio::time::Duration::from_secs(8),
            rest_client.fetch_recent_candles("BTCUSDT", "1m", limit),
        )
        .await;

        let evaluation = match candle_request {
            Ok(Ok(candles)) if candles_are_complete_and_fresh(&candles, now.timestamp_millis()) => {
                strategy_service::evaluate(
                    timeframe,
                    &candles,
                    now.timestamp_millis(),
                    current_start,
                )
            }
            Ok(Ok(_)) => strategy_service_unavailable(
                timeframe,
                current_start,
                "Binance 1m candle payload is incomplete or stale",
            ),
            Ok(Err(error)) => strategy_service_unavailable(
                timeframe,
                current_start,
                &format!("Binance 1m data unavailable: {error}"),
            ),
            Err(_) => strategy_service_unavailable(
                timeframe,
                current_start,
                "Binance 1m request timed out after 8s",
            ),
        };
        if let Some(window_open) = evaluation.window_open {
            let current_slug = generate_updown_slug("btc", timeframe, current_start);
            if let Some(market) = state
                .updown_markets
                .write()
                .await
                .iter_mut()
                .find(|market| market.slug == current_slug)
            {
                market.price_to_beat = window_open;
            }
        }

        let signal_info = evaluation.signal;
        let signal_time = signal_info.timestamp;
        if signal_info.direction != "WAIT" {
            if let Err(error) = state
                .audit(
                    "signal_generated",
                    serde_json::json!({
                        "timeframe": &signal_info.timeframe,
                        "direction": &signal_info.direction,
                        "confidence": signal_info.confidence,
                        "reason": &signal_info.reason,
                        "window_start_ts": signal_info.window_start_ts
                    }),
                )
                .await
            {
                state
                    .halt_after_persistence_failure("strategy signal audit", &error)
                    .await;
            }
        }
        if timeframe == "5m" {
            *state.last_signal_5m.write().await = Some(signal_info);
            *state.last_signal_5m_time.write().await = signal_time;
        } else {
            *state.last_signal.write().await = Some(signal_info);
            *state.last_signal_time.write().await = signal_time;
        }
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}

fn strategy_service_unavailable(
    timeframe: &str,
    window_start_ts: i64,
    reason: &str,
) -> strategy_service::StrategyEvaluation {
    strategy_service::StrategyEvaluation {
        signal: state::SignalInfo {
            direction: "WAIT".to_string(),
            confidence: 0.0,
            timeframe: timeframe.to_string(),
            reason: reason.to_string(),
            timestamp: chrono::Utc::now().timestamp_millis(),
            window_start_ts,
        },
        window_open: None,
    }
}

async fn run_paper_executor(state: AppState) {
    loop {
        sync_forward_opportunity_outcomes(&state).await;
        settle_finished_trades(&state).await;
        try_open_unified_trade(&state, "15m").await;
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}

async fn sync_forward_opportunity_outcomes(state: &AppState) {
    let slugs = match state
        .store
        .load_unsettled_opportunity_slugs(chrono::Utc::now().timestamp())
        .await
    {
        Ok(slugs) => slugs,
        Err(error) => {
            state
                .halt_after_persistence_failure("forward outcome query", &error)
                .await;
            return;
        }
    };
    if slugs.is_empty() {
        return;
    }
    let outcomes = fetch_official_outcomes(slugs, &state.runtime.gamma_base_url).await;
    if let Err(error) = state.store.record_official_outcomes(&outcomes).await {
        state
            .halt_after_persistence_failure("forward outcome persistence", &error)
            .await;
    }
}

async fn run_5m_paper_executor(state: AppState) {
    loop {
        settle_finished_5m_trades(&state).await;
        try_open_unified_trade(&state, "5m").await;
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}

async fn settle_finished_trades(state: &AppState) {
    let now = chrono::Utc::now().timestamp();
    let pending_slugs: Vec<String> = state
        .trades
        .read()
        .await
        .iter()
        .filter(|trade| trade.status == "open" && trade.timeframe == "15m" && now >= trade.end_ts)
        .map(|trade| trade.market_slug.clone())
        .collect();
    let outcomes = fetch_official_outcomes(pending_slugs, &state.runtime.gamma_base_url).await;
    if outcomes.is_empty() {
        return;
    }
    if let Err(error) = state.store.record_official_outcomes(&outcomes).await {
        state
            .halt_after_persistence_failure("15m official outcomes", &error)
            .await;
        return;
    }

    let mut trades = state.trades.write().await;
    let mut stats = state.stats.write().await;
    let mut stats_5m = state.stats_5m.write().await;
    let mut changed = false;

    for trade in trades
        .iter_mut()
        .filter(|trade| trade.status == "open" && trade.timeframe == "15m")
    {
        if now < trade.end_ts {
            continue;
        }

        let Some(winning_direction) = outcomes.get(&trade.market_slug) else {
            continue;
        };
        let won = trade.direction == *winning_direction;
        let payout = if won { trade.shares } else { 0.0 };
        let pnl = payout - trade.size_usd - trade.fee_usd;

        trade.exit_price = Some(if won { 1.0 } else { 0.0 });
        trade.pnl = Some(pnl);
        trade.status = "settled".to_string();
        stats.current_capital += payout;
        changed = true;
    }

    if changed {
        refresh_stats(&mut stats, &trades, "15m");
        stats_5m.current_capital = stats.current_capital;
        stats_5m.peak_capital = stats_5m.peak_capital.max(stats.current_capital);
        refresh_stats(&mut stats_5m, &trades, "5m");
        drop(stats_5m);
        drop(stats);
        drop(trades);
        if let Err(error) = state
            .persist(
                "paper_trades_settled",
                serde_json::json!({"timeframe": "15m"}),
            )
            .await
        {
            state
                .halt_after_persistence_failure("15m settlement", &error)
                .await;
        }
    }
}

async fn settle_finished_5m_trades(state: &AppState) {
    let now = chrono::Utc::now().timestamp();
    let pending_slugs: Vec<String> = state
        .trades
        .read()
        .await
        .iter()
        .filter(|trade| trade.status == "open" && trade.timeframe == "5m" && now >= trade.end_ts)
        .map(|trade| trade.market_slug.clone())
        .collect();
    let outcomes = fetch_official_outcomes(pending_slugs, &state.runtime.gamma_base_url).await;
    if outcomes.is_empty() {
        return;
    }
    if let Err(error) = state.store.record_official_outcomes(&outcomes).await {
        state
            .halt_after_persistence_failure("5m official outcomes", &error)
            .await;
        return;
    }

    let mut trades = state.trades.write().await;
    let mut stats = state.stats.write().await;
    let mut stats_5m = state.stats_5m.write().await;
    let mut changed = false;
    for trade in trades
        .iter_mut()
        .filter(|trade| trade.status == "open" && trade.timeframe == "5m")
    {
        if now < trade.end_ts {
            continue;
        }
        let Some(winning_direction) = outcomes.get(&trade.market_slug) else {
            continue;
        };
        let won = trade.direction == *winning_direction;
        let payout = if won { trade.shares } else { 0.0 };
        trade.exit_price = Some(if won { 1.0 } else { 0.0 });
        trade.pnl = Some(payout - trade.size_usd - trade.fee_usd);
        trade.status = "settled".to_string();
        stats.current_capital += payout;
        changed = true;
    }
    if changed {
        refresh_stats(&mut stats, &trades, "15m");
        stats_5m.current_capital = stats.current_capital;
        stats_5m.peak_capital = stats_5m.peak_capital.max(stats.current_capital);
        refresh_stats(&mut stats_5m, &trades, "5m");
        drop(stats_5m);
        drop(stats);
        drop(trades);
        if let Err(error) = state
            .persist(
                "paper_trades_settled",
                serde_json::json!({"timeframe": "5m"}),
            )
            .await
        {
            state
                .halt_after_persistence_failure("5m settlement", &error)
                .await;
        }
    }
}

async fn fetch_official_outcomes(
    slugs: Vec<String>,
    gamma_base_url: &str,
) -> HashMap<String, String> {
    let gamma_client = GammaClient::new(gamma_base_url);
    let mut outcomes = HashMap::new();

    for slug in slugs {
        let Ok(Some(event)) = gamma_client.fetch_event_by_slug(&slug).await else {
            continue;
        };
        let Some(winning_direction) = event
            .markets
            .as_ref()
            .and_then(|markets| markets.first())
            .and_then(|market| market.winning_direction())
        else {
            continue;
        };
        outcomes.insert(slug, winning_direction);
    }

    outcomes
}

async fn try_open_unified_trade(state: &AppState, timeframe: &str) {
    let settings = state.settings.read().await.clone();
    if !settings.auto_trade {
        set_timeframe_note(state, timeframe, "Auto-trade is disabled").await;
        return;
    }
    let signal = if timeframe == "5m" {
        state.last_signal_5m.read().await.clone()
    } else {
        state.last_signal.read().await.clone()
    };
    let Some(signal) =
        signal.filter(|signal| signal.direction == "Up" || signal.direction == "Down")
    else {
        set_timeframe_note(
            state,
            timeframe,
            "Waiting for an actionable strategy signal",
        )
        .await;
        return;
    };
    let market = state
        .updown_markets
        .read()
        .await
        .iter()
        .find(|market| {
            market.asset == "btc" && market.interval == timeframe && market.status == "live"
        })
        .cloned();
    let Some(market) = market else {
        set_timeframe_note(state, timeframe, "Waiting for the active BTC market").await;
        return;
    };

    let now_ms = chrono::Utc::now().timestamp_millis();
    let elapsed_secs = now_ms / 1_000 - market.start_ts;
    let timing = timing_for(timeframe, elapsed_secs);
    let (min_price, max_price, min_margin) = if timeframe == "5m" {
        (0.05, settings.max_entry_price, settings.min_edge)
    } else {
        (0.15, settings.max_entry_price, settings.min_edge)
    };
    if timing.reason_code == "entry_not_open" {
        set_timeframe_note(
            state,
            timeframe,
            &format!(
                "Waiting for entry window: {}s / {}-{}s",
                elapsed_secs, timing.entry_start_secs, timing.entry_end_secs
            ),
        )
        .await;
        return;
    }
    if timing.reason_code == "entry_deadline" {
        set_timeframe_note(
            state,
            timeframe,
            &format!(
                "Skip: entry deadline {}s / {}-{}s",
                elapsed_secs, timing.entry_start_secs, timing.entry_end_secs
            ),
        )
        .await;
        return;
    }
    if now_ms - signal.timestamp > MAX_SIGNAL_AGE_MS {
        let decision = stale_signal_decision();
        if let Err(error) = state
            .record_risk_decision_with_context(
                &market.slug,
                timeframe,
                &decision,
                serde_json::json!({
                    "signal_age_ms": now_ms - signal.timestamp,
                    "max_signal_age_ms": MAX_SIGNAL_AGE_MS,
                    "elapsed_secs": elapsed_secs,
                    "entry_layer": timing.layer.map(|layer| layer.as_str())
                }),
            )
            .await
        {
            state
                .halt_after_persistence_failure("stale signal risk persistence", &error)
                .await;
        }
        set_timeframe_note(state, timeframe, "Skip: central risk rejected stale_signal").await;
        return;
    }

    let direction = Direction::parse(&signal.direction);
    let target_fill_for_sizing = match direction.as_ref() {
        Some(Direction::Up) => market.up_expected_fill_price,
        Some(Direction::Down) => market.down_expected_fill_price,
        None => None,
    };
    let min_share_order_usd = match (target_fill_for_sizing, market.min_order_size) {
        (Some(price), Some(minimum_shares)) if price.is_finite() && minimum_shares.is_finite() => {
            price * minimum_shares
        }
        _ => state.runtime.configured_min_order_usd,
    };
    let mut trades = state.trades.write().await;
    let mut stats = state.stats.write().await;
    let mut stats_5m = state.stats_5m.write().await;
    let size_usd = (stats.current_capital * settings.risk_fraction)
        .max(state.runtime.configured_min_order_usd)
        .max(min_share_order_usd)
        .min(settings.max_order);
    let fee_usd = market
        .fee_rate_bps
        .map(|bps| size_usd * bps as f64 / 10_000.0)
        .unwrap_or(size_usd * state.runtime.configured_fee_pct);
    let mut risk_stats = if timeframe == "5m" {
        stats_5m.clone()
    } else {
        stats.clone()
    };
    risk_stats.current_capital = stats.current_capital;
    risk_stats.peak_capital = risk_stats.peak_capital.max(stats.peak_capital);
    risk_stats.max_drawdown = risk_stats.max_drawdown.max(stats.max_drawdown);
    let adjusted = microstructure_probability(&market, &signal, elapsed_secs, now_ms);
    let up_quote = market.up_expected_fill_price.map(|price| BuyQuote {
        average_price: price,
        shares: size_usd / price,
        available_depth_usd: market.up_executable_depth_usd,
    });
    let down_quote = market.down_expected_fill_price.map(|price| BuyQuote {
        average_price: price,
        shares: size_usd / price,
        available_depth_usd: market.down_executable_depth_usd,
    });
    let quote_decision = direction.clone().map(|direction| {
        executable_quote(QuoteInput {
            direction,
            probability_up: adjusted.adjusted_probability_up,
            up_quote,
            down_quote,
            up_best_bid: market.up_best_bid,
            down_best_bid: market.down_best_bid,
            min_edge: min_margin,
            requested_usd: size_usd,
            tick_size: market.tick_size.unwrap_or(0.01),
            timing,
            maker_time_in_force_ms: MAKER_TIME_IN_FORCE_MS,
        })
    });
    let (override_fill, override_depth, quote_reason, maker_bid) = match quote_decision.as_ref() {
        Some(QuoteDecision::Taker {
            price,
            depth_usd,
            edge: _,
            ..
        }) => (Some(*price), *depth_usd, "taker", None),
        Some(QuoteDecision::Maker {
            bid_price,
            reason_code,
            ..
        }) => (Some(*bid_price), size_usd, *reason_code, Some(*bid_price)),
        Some(QuoteDecision::Reject { reason_code }) => (None, 0.0, *reason_code, None),
        None => (None, 0.0, "invalid_direction", None),
    };

    let target_probability = match direction.as_ref() {
        Some(Direction::Up) => adjusted.adjusted_probability_up,
        Some(Direction::Down) => 1.0 - adjusted.adjusted_probability_up,
        None => 0.0,
    };

    let risk_decision = evaluate_trade_risk(
        state,
        &market,
        &signal,
        &trades,
        &risk_stats,
        size_usd,
        fee_usd,
        min_price,
        max_price,
        min_margin,
        timing.entry_start_secs,
        timing.entry_end_secs,
        override_fill,
        override_depth,
        target_probability,
    );
    let expected_fill_price = override_fill;
    let expected_shares = expected_fill_price
        .filter(|price| *price > 0.0)
        .map(|price| size_usd / price);
    let min_required_usd = match (expected_fill_price, market.min_order_size) {
        (Some(price), Some(minimum_shares)) => Some(price * minimum_shares),
        _ => None,
    };
    let audit_now_ms = chrono::Utc::now().timestamp_millis();
    let risk_context = serde_json::json!({
        "signal": {
            "direction": signal.direction.clone(),
            "confidence": signal.confidence,
            "reason": signal.reason.clone(),
            "timestamp_ms": signal.timestamp,
            "window_start_ts": signal.window_start_ts,
            "age_ms": audit_now_ms - signal.timestamp,
            "max_signal_age_ms": MAX_SIGNAL_AGE_MS
        },
        "model": {
            "model_probability_up": adjusted.model_probability_up,
            "adjusted_probability_up": adjusted.adjusted_probability_up,
            "target_probability": target_probability,
            "entry_layer": timing.layer.map(|layer| layer.as_str()),
            "quote_decision": quote_reason,
            "maker_bid": maker_bid
        },
        "sizing": {
            "requested_usd": size_usd,
            "fee_usd": fee_usd,
            "current_capital_usd": risk_stats.current_capital,
            "max_order_usd": settings.max_order,
            "configured_max_order_usd": state.runtime.configured_max_order_usd,
            "min_balance_reserve_usd": state.runtime.configured_min_balance_reserve_usd,
            "expected_fill_price": expected_fill_price,
            "expected_shares": expected_shares,
            "min_order_size_shares": market.min_order_size,
            "min_required_usd": min_required_usd
        },
        "entry_rules": {
            "min_entry_price": min_price,
            "max_entry_price": max_price,
            "min_model_margin": min_margin,
            "entry_window_start_secs": timing.entry_start_secs,
            "entry_window_end_secs": timing.entry_end_secs,
            "elapsed_secs": audit_now_ms / 1_000 - market.start_ts
        },
        "market_snapshot": {
            "data_status": market.data_status.as_str(),
            "data_detail": market.data_detail,
            "data_age_ms": audit_now_ms - market.captured_at_ms,
            "spread": market.spread,
            "fee_rate_bps": market.fee_rate_bps,
            "up_depth_usd": market.up_executable_depth_usd,
            "down_depth_usd": market.down_executable_depth_usd,
            "target_executable_depth_usd": override_depth,
            "book_age_ms": audit_now_ms - market.captured_at_ms,
            "max_orderbook_age_ms": state.runtime.configured_max_data_age_ms,
            "clock_drift_ms": market.clock_drift_ms
        }
    });
    let persistence_result = match state
        .record_risk_decision_with_context(&market.slug, timeframe, &risk_decision, risk_context)
        .await
    {
        Ok(()) => {
            state
                .record_forward_opportunity(&market, &signal, &risk_decision)
                .await
        }
        Err(error) => Err(error),
    };
    if let Err(error) = persistence_result {
        drop(stats_5m);
        drop(stats);
        drop(trades);
        state
            .halt_after_persistence_failure("unified risk persistence", &error)
            .await;
        set_timeframe_note(
            state,
            timeframe,
            "HALT: failed to persist unified risk decision",
        )
        .await;
        return;
    }
    let Some(intent) = risk_decision.intent.as_ref() else {
        drop(stats_5m);
        drop(stats);
        drop(trades);
        set_timeframe_note(
            state,
            timeframe,
            &format!("Skip: central risk rejected {}", risk_decision.reason_code),
        )
        .await;
        return;
    };
    if matches!(quote_decision, Some(QuoteDecision::Maker { .. })) {
        drop(stats_5m);
        drop(stats);
        drop(trades);
        set_timeframe_note(
            state,
            timeframe,
            &format!(
                "Maker fallback armed {} @ {:.2} for {}ms",
                signal.direction,
                maker_bid.unwrap_or_default(),
                MAKER_TIME_IN_FORCE_MS
            ),
        )
        .await;
        return;
    }
    match state.reserve_execution_intent(intent).await {
        Ok(true) => {}
        Ok(false) => {
            drop(stats_5m);
            drop(stats);
            drop(trades);
            set_timeframe_note(
                state,
                timeframe,
                "Skip: durable execution intent already exists",
            )
            .await;
            return;
        }
        Err(error) => {
            drop(stats_5m);
            drop(stats);
            drop(trades);
            state
                .halt_after_persistence_failure("unified intent reservation", &error)
                .await;
            set_timeframe_note(state, timeframe, "HALT: failed to persist execution intent").await;
            return;
        }
    }

    if auto_live_execution_enabled() {
        let intent = intent.clone();
        drop(stats_5m);
        drop(stats);
        drop(trades);
        submit_armed_live_signal(state, timeframe, &signal.direction, &intent).await;
        return;
    }

    let ask = intent.expected_fill_price.as_f64();
    let edge = intent.model_margin.as_f64();
    stats.current_capital -= size_usd + fee_usd;
    trades.push(state::TradeInfo {
        timestamp: chrono::Utc::now().timestamp_millis(),
        market_slug: market.slug.clone(),
        timeframe: timeframe.to_string(),
        direction: signal.direction.clone(),
        entry_price: ask,
        exit_price: None,
        shares: intent.expected_shares.as_f64(),
        size_usd,
        fee_usd,
        price_to_beat: market.price_to_beat,
        end_ts: market.end_ts,
        confidence: signal.confidence,
        edge,
        pnl: None,
        status: "open".to_string(),
    });
    refresh_stats(&mut stats, &trades, "15m");
    stats_5m.current_capital = stats.current_capital;
    stats_5m.peak_capital = stats_5m.peak_capital.max(stats.current_capital);
    refresh_stats(&mut stats_5m, &trades, "5m");
    drop(stats_5m);
    drop(stats);
    drop(trades);
    if let Err(error) = state
        .persist(
            "paper_trade_opened",
            serde_json::json!({
                "market_slug": &market.slug,
                "timeframe": timeframe,
                "direction": &signal.direction,
                "client_order_key": &intent.client_order_key,
                "size_usd": size_usd
            }),
        )
        .await
    {
        state
            .halt_after_persistence_failure("unified paper trade open", &error)
            .await;
    }
    set_timeframe_note(
        state,
        timeframe,
        &format!(
            "Paper BUY {} @ {:.2}, model margin {:.1}%, size ${:.2}",
            signal.direction,
            ask,
            edge * 100.0,
            size_usd
        ),
    )
    .await;
}

fn auto_live_execution_enabled() -> bool {
    std::env::var("POLYMARKET_AUTO_LIVE_EXECUTION")
        .map(|value| value == "I_UNDERSTAND_AUTO_LIVE_EXECUTION")
        .unwrap_or(false)
}

async fn submit_armed_live_signal(
    state: &AppState,
    timeframe: &str,
    direction: &str,
    intent: &crate::engine::risk::ExecutionIntent,
) {
    if let Err(error) = crate::production::run_preflight(&state.config).await {
        set_timeframe_note(
            state,
            timeframe,
            &format!("HALT: live preflight failed before submit: {error}"),
        )
        .await;
        return;
    }
    match state.store.latest_reconciliation_ready().await {
        Ok(true) => {}
        Ok(false) => {
            set_timeframe_note(
                state,
                timeframe,
                "HALT: fresh reconciliation required before live submit",
            )
            .await;
            return;
        }
        Err(error) => {
            state
                .halt_after_persistence_failure("live reconciliation readiness query", &error)
                .await;
            set_timeframe_note(
                state,
                timeframe,
                "HALT: failed to verify reconciliation readiness",
            )
            .await;
            return;
        }
    }

    let executor = crate::execution::live::SdkOrderExecutor::new(
        state.config.execution.heartbeat_interval_secs,
        state.config.risk.max_fee_rate_bps,
        state.config.risk.min_balance_reserve_usd,
    );
    match crate::execution::live::execute_armed_signal(&state.store, &executor, intent).await {
        Ok(outcome) => {
            set_timeframe_note(
                state,
                timeframe,
                &format!(
                    "LIVE BUY {direction} submitted: status={} order_id={}",
                    outcome.status, outcome.order_id
                ),
            )
            .await;
        }
        Err(error) => {
            let _ = state
                .store
                .set_runtime_state(
                    "halted",
                    "armed live signal submission requires reconciliation",
                )
                .await;
            let _ = state
                .store
                .open_incident(
                    &format!("armed-live-submit-{}", chrono::Utc::now().timestamp()),
                    "ambiguous_order_submission",
                    serde_json::json!({
                        "client_key": intent.client_order_key,
                        "error": error.to_string()
                    }),
                )
                .await;
            set_timeframe_note(
                state,
                timeframe,
                &format!("HALT: live submit failed; reconciliation required: {error}"),
            )
            .await;
        }
    }
}

async fn set_timeframe_note(state: &AppState, timeframe: &str, note: &str) {
    if timeframe == "5m" {
        set_5m_execution_note(state, note).await;
    } else {
        set_execution_note(state, note).await;
    }
}

async fn set_execution_note(state: &AppState, note: &str) {
    let changed = {
        let mut current = state.execution_note.write().await;
        if *current == note {
            false
        } else {
            *current = note.to_string();
            true
        }
    };
    if changed && should_audit_execution_note(note) {
        if let Err(error) = state
            .audit(
                "execution_note_changed",
                serde_json::json!({"timeframe": "15m", "note": note}),
            )
            .await
        {
            state
                .halt_after_persistence_failure("15m execution-note audit", &error)
                .await;
        }
    }
}

async fn set_5m_execution_note(state: &AppState, note: &str) {
    let changed = {
        let mut current = state.execution_note_5m.write().await;
        if *current == note {
            false
        } else {
            *current = note.to_string();
            true
        }
    };
    if changed && should_audit_execution_note(note) {
        if let Err(error) = state
            .audit(
                "execution_note_changed",
                serde_json::json!({"timeframe": "5m", "note": note}),
            )
            .await
        {
            state
                .halt_after_persistence_failure("5m execution-note audit", &error)
                .await;
        }
    }
}

fn should_audit_execution_note(note: &str) -> bool {
    note.starts_with("Skip:")
        || note.starts_with("HALT:")
        || note.starts_with("Paper BUY")
        || note.starts_with("Maker fallback")
        || note.starts_with("Already traded")
        || note.contains("circuit breaker")
        || note.contains("halted:")
}

fn stale_signal_decision() -> RiskDecision {
    RiskDecision {
        approved: false,
        reason_code: "stale_signal".to_string(),
        checks: vec![RiskCheck {
            code: "stale_signal".to_string(),
            passed: false,
            detail: "signal age exceeded max_signal_age_ms".to_string(),
        }],
        intent: None,
    }
}

fn microstructure_probability(
    market: &state::UpDownMarket,
    signal: &state::SignalInfo,
    elapsed_secs: i64,
    now_ms: i64,
) -> crate::engine::microstructure::ProbabilityEstimate {
    let tau_seconds = (market.end_ts - now_ms / 1_000).max(1) as f64;
    let side_bias = if signal.direction == "Up" { 1.0 } else { -1.0 };
    let momentum = ((signal.confidence - 0.5) * 0.10 * side_bias).clamp(-0.03, 0.03);
    let up_depth = market.up_executable_depth_usd.max(0.0);
    let down_depth = market.down_executable_depth_usd.max(0.0);
    let book_imbalance = if up_depth + down_depth > 0.0 {
        (up_depth - down_depth) / (up_depth + down_depth)
    } else {
        0.0
    };
    let price_scale = market.current_price.max(1.0);
    let minute_vol_return = 0.000_25_f64;
    let realized_vol = (price_scale * minute_vol_return / 60.0_f64.sqrt()).max(0.01);
    estimate_probability(ProbabilityInput {
        current_price: market.current_price,
        price_to_beat: market.price_to_beat,
        drift_per_second: momentum * price_scale / (tau_seconds.max(1.0) * 4.0),
        realized_vol_per_sqrt_second: realized_vol,
        tau_seconds,
        momentum,
        book_imbalance,
        spread: market.spread,
        latency_ms: now_ms - signal.timestamp + elapsed_secs.max(0) * 10,
    })
}

#[allow(clippy::too_many_arguments)]
fn evaluate_trade_risk(
    state: &AppState,
    market: &state::UpDownMarket,
    signal: &state::SignalInfo,
    trades: &[state::TradeInfo],
    stats: &state::StatsInfo,
    requested_usd: f64,
    fee_usd: f64,
    min_entry_price: f64,
    max_entry_price: f64,
    min_model_margin: f64,
    entry_window_start_secs: i64,
    entry_window_end_secs: i64,
    override_fill_price: Option<f64>,
    override_depth_usd: f64,
    model_confidence: f64,
) -> RiskDecision {
    let direction = Direction::parse(&signal.direction);
    let token_id = match direction {
        Some(Direction::Up) => market.up_token_id.clone(),
        Some(Direction::Down) => market.down_token_id.clone(),
        None => None,
    };
    let settled_for_strategy: Vec<&state::TradeInfo> = trades
        .iter()
        .rev()
        .filter(|trade| trade.timeframe == market.interval && trade.status == "settled")
        .take(state.runtime.configured_max_consecutive_losses)
        .collect();
    let consecutive_losses = settled_for_strategy
        .iter()
        .take_while(|trade| trade.pnl.unwrap_or(0.0) < 0.0)
        .count();
    let last_loss_timestamp_ms = settled_for_strategy
        .first()
        .filter(|trade| trade.pnl.unwrap_or(0.0) < 0.0)
        .map(|trade| trade.timestamp);
    let strategy_enabled = match market.interval.as_str() {
        "5m" => state.runtime.configured_enable_5m,
        "15m" => state.runtime.configured_enable_15m,
        _ => false,
    };

    RiskEngine::new(RiskPolicy {
        trading_enabled: state.runtime.configured_trading_enabled,
        strategy_enabled,
        max_open_positions: state.runtime.configured_max_open_positions,
        max_daily_orders: state.runtime.configured_max_daily_orders,
        max_daily_realized_loss_usd: state.runtime.configured_max_daily_loss_usd,
        max_drawdown: state.runtime.configured_max_drawdown,
        max_consecutive_losses: state.runtime.configured_max_consecutive_losses,
        max_order_usd: state.runtime.configured_max_order_usd,
        min_balance_reserve_usd: state.runtime.configured_min_balance_reserve_usd,
        max_spread: state.runtime.configured_max_spread,
        max_fee_rate_bps: state.runtime.configured_max_fee_rate_bps,
        max_data_age_ms: state.runtime.configured_max_data_age_ms as i64,
    })
    .evaluate(RiskRequest {
        now_ms: chrono::Utc::now().timestamp_millis(),
        market_slug: market.slug.clone(),
        token_id,
        timeframe: market.interval.clone(),
        direction,
        strategy_version: state.runtime.strategy_version.clone(),
        signal_timestamp_ms: signal.timestamp,
        signal_window_start_ts: signal.window_start_ts,
        market_start_ts: market.start_ts,
        market_snapshot_timestamp_ms: market.captured_at_ms,
        data_ready: validate_market_freshness(market, &state.runtime).is_ok(),
        price_to_beat: market.price_to_beat,
        expected_fill_price: override_fill_price,
        min_entry_price,
        max_entry_price,
        confidence: model_confidence,
        min_model_margin,
        spread: market.spread,
        executable_depth_usd: override_depth_usd,
        min_order_size_shares: market.min_order_size,
        fee_rate_bps: market.fee_rate_bps,
        requested_usd,
        fee_usd,
        current_capital_usd: stats.current_capital,
        open_positions: open_position_count(trades),
        daily_orders: daily_trade_count(trades),
        daily_realized_pnl_usd: daily_realized_pnl(trades),
        max_drawdown: stats.max_drawdown,
        consecutive_losses,
        last_loss_timestamp_ms,
        market_already_traded: trades.iter().any(|trade| trade.market_slug == market.slug),
        entry_window_start_secs,
        entry_window_end_secs,
        max_signal_age_ms: MAX_SIGNAL_AGE_MS,
        max_orderbook_age_ms: state.runtime.configured_max_data_age_ms as i64,
    })
}

fn validate_market_freshness(
    market: &state::UpDownMarket,
    runtime: &state::RuntimeInfo,
) -> Result<(), &'static str> {
    if !matches!(
        market.data_status,
        state::DataStatus::Ready | state::DataStatus::OneSided
    ) || !market.token_mapping_valid
    {
        return Err("is incomplete or invalid");
    }
    if market.up_best_ask.is_none()
        && market.down_best_ask.is_none()
        && market.up_best_bid.is_none()
        && market.down_best_bid.is_none()
    {
        return Err("has no executable or maker-side orderbook levels");
    }
    if market.tick_size.is_none()
        || market.min_order_size.is_none()
        || market.fee_rate_bps.is_none()
        || market
            .clock_drift_ms
            .map(|drift| drift.abs() > MAX_CLOCK_DRIFT_MS)
            .unwrap_or(true)
    {
        return Err("has an incomplete orderbook");
    }
    if chrono::Utc::now().timestamp_millis() - market.captured_at_ms
        > runtime.configured_max_data_age_ms as i64
    {
        return Err("is stale");
    }
    if market.price_to_beat <= 0.0 || market.current_price <= 0.0 {
        return Err("is missing reference price");
    }
    Ok(())
}

fn open_position_count(trades: &[state::TradeInfo]) -> usize {
    trades.iter().filter(|trade| trade.status == "open").count()
}

fn daily_trade_count(trades: &[state::TradeInfo]) -> usize {
    trades
        .iter()
        .filter(|trade| trade.timestamp >= current_utc_day_start_ms())
        .count()
}

fn daily_realized_pnl(trades: &[state::TradeInfo]) -> f64 {
    trades
        .iter()
        .filter(|trade| trade.timestamp >= current_utc_day_start_ms() && trade.status == "settled")
        .filter_map(|trade| trade.pnl)
        .sum()
}

fn current_utc_day_start_ms() -> i64 {
    chrono::Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("UTC midnight must be valid")
        .and_utc()
        .timestamp_millis()
}

fn refresh_stats(stats: &mut state::StatsInfo, trades: &[state::TradeInfo], timeframe: &str) {
    let completed: Vec<&state::TradeInfo> = trades
        .iter()
        .filter(|trade| trade.status == "settled" && trade.timeframe == timeframe)
        .collect();
    stats.total_trades = completed.len();
    stats.wins = completed
        .iter()
        .filter(|trade| trade.pnl.unwrap_or(0.0) > 0.0)
        .count();
    stats.losses = stats.total_trades.saturating_sub(stats.wins);
    stats.total_pnl = completed.iter().filter_map(|trade| trade.pnl).sum();
    stats.win_rate = if stats.total_trades == 0 {
        0.0
    } else {
        stats.wins as f64 / stats.total_trades as f64
    };

    let gross_profit: f64 = completed
        .iter()
        .filter_map(|trade| trade.pnl)
        .filter(|pnl| *pnl > 0.0)
        .sum();
    let gross_loss: f64 = completed
        .iter()
        .filter_map(|trade| trade.pnl)
        .filter(|pnl| *pnl < 0.0)
        .map(f64::abs)
        .sum();
    stats.avg_win = if stats.wins == 0 {
        0.0
    } else {
        gross_profit / stats.wins as f64
    };
    stats.avg_loss = if stats.losses == 0 {
        0.0
    } else {
        gross_loss / stats.losses as f64
    };
    stats.profit_factor = if gross_loss > 0.0 {
        gross_profit / gross_loss
    } else {
        0.0
    };
    stats.peak_capital = stats.peak_capital.max(stats.current_capital);
    if stats.peak_capital > 0.0 {
        stats.max_drawdown = stats
            .max_drawdown
            .max((stats.peak_capital - stats.current_capital) / stats.peak_capital);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trade(timestamp: i64, status: &str) -> state::TradeInfo {
        state::TradeInfo {
            timestamp,
            market_slug: "btc-updown-test".to_string(),
            timeframe: "5m".to_string(),
            direction: "Up".to_string(),
            entry_price: 0.50,
            exit_price: None,
            shares: 0.20,
            size_usd: 0.10,
            fee_usd: 0.0,
            price_to_beat: 1.0,
            end_ts: 0,
            confidence: 0.70,
            edge: 0.20,
            pnl: None,
            status: status.to_string(),
        }
    }

    #[test]
    fn counts_open_positions_across_timeframes() {
        let mut open_5m = trade(chrono::Utc::now().timestamp_millis(), "open");
        open_5m.timeframe = "5m".to_string();
        let mut open_15m = trade(chrono::Utc::now().timestamp_millis(), "open");
        open_15m.timeframe = "15m".to_string();
        let settled = trade(chrono::Utc::now().timestamp_millis(), "settled");

        assert_eq!(open_position_count(&[open_5m, open_15m, settled]), 2);
    }

    #[test]
    fn daily_order_count_ignores_previous_utc_day() {
        let now = chrono::Utc::now().timestamp_millis();
        let yesterday = now - 25 * 60 * 60 * 1_000;

        assert_eq!(
            daily_trade_count(&[trade(now, "open"), trade(yesterday, "settled")]),
            1
        );
    }

    #[test]
    fn audits_decisions_but_not_waiting_heartbeat_text() {
        assert!(should_audit_execution_note("Skip: spread too wide"));
        assert!(should_audit_execution_note("HALT: persistence failed"));
        assert!(!should_audit_execution_note(
            "Waiting for early entry window; elapsed 181s"
        ));
    }

    #[test]
    fn daily_realized_pnl_excludes_previous_day_and_open_trades() {
        let now = chrono::Utc::now().timestamp_millis();
        let yesterday = now - 25 * 60 * 60 * 1_000;
        let mut today_loss = trade(now, "settled");
        today_loss.pnl = Some(-0.10);
        let mut old_loss = trade(yesterday, "settled");
        old_loss.pnl = Some(-0.20);
        let mut open = trade(now, "open");
        open.pnl = Some(-1.0);
        let mut other_strategy_loss = trade(now, "settled");
        other_strategy_loss.timeframe = "15m".to_string();
        other_strategy_loss.pnl = Some(-0.05);

        let pnl = daily_realized_pnl(&[today_loss, old_loss, open, other_strategy_loss]);
        assert!((pnl - -0.15).abs() < f64::EPSILON);
    }

    #[test]
    fn rejects_stale_or_incomplete_market_data() {
        let config = crate::config::Config::default();
        let runtime = state::RuntimeInfo::from_config(&config);
        let mut market = unavailable_market(
            "btc",
            "5m",
            "btc-updown-5m-test",
            1,
            301,
            300,
            state::DataStatus::Incomplete,
            "missing book",
        );
        assert_eq!(
            validate_market_freshness(&market, &runtime),
            Err("is incomplete or invalid")
        );

        market.data_status = state::DataStatus::Ready;
        market.token_mapping_valid = true;
        market.up_best_ask = Some(0.51);
        market.up_best_bid = Some(0.49);
        market.down_best_ask = Some(0.52);
        market.down_best_bid = Some(0.48);
        market.tick_size = Some(0.01);
        market.min_order_size = Some(5.0);
        market.fee_rate_bps = Some(0);
        market.up_expected_fill_price = Some(0.51);
        market.down_expected_fill_price = Some(0.52);
        market.clock_drift_ms = Some(0);
        market.captured_at_ms =
            chrono::Utc::now().timestamp_millis() - runtime.configured_max_data_age_ms as i64 - 1;
        assert_eq!(
            validate_market_freshness(&market, &runtime),
            Err("is stale")
        );
    }

    #[test]
    fn accepts_complete_recent_candles() {
        let now = chrono::Utc::now().timestamp_millis();
        let candles = vec![
            Candle {
                timestamp: now - 60_000,
                open: 100.0,
                high: 102.0,
                low: 99.0,
                close: 101.0,
                volume: 1.0,
            },
            Candle {
                timestamp: now,
                open: 101.0,
                high: 103.0,
                low: 100.0,
                close: 102.0,
                volume: 1.0,
            },
        ];

        assert!(candles_are_complete_and_fresh(&candles, now));
    }

    #[test]
    fn rejects_stale_or_malformed_candles() {
        let now = chrono::Utc::now().timestamp_millis();
        let mut candles = vec![Candle {
            timestamp: now - 121_000,
            open: 100.0,
            high: 102.0,
            low: 99.0,
            close: 101.0,
            volume: 1.0,
        }];
        assert!(!candles_are_complete_and_fresh(&candles, now));

        candles[0].timestamp = now;
        candles[0].high = 90.0;
        assert!(!candles_are_complete_and_fresh(&candles, now));
    }
}
