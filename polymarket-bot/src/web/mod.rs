pub mod api;
pub mod price_proxy;
pub mod state;
pub mod ws;

use crate::config::Config;
use crate::crypto::binance_ws::{BinanceRestClient, Candle};
use crate::crypto::indicators::Timeframe;
use crate::crypto::live::gamma_client::{
    generate_updown_slug, get_current_interval_start, get_remaining_seconds, ClobClient,
    GammaClient,
};
use crate::crypto::strategy::{
    diagnose_five_minute_continuation, predict_early_window, predict_five_minute_continuation,
};
use axum::Router;
use state::AppState;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

// Assets to scan for Up/Down markets
const CRYPTO_ASSETS: &[&str] = &["btc", "eth", "sol", "xrp", "doge", "bnb"];
const MAX_CLOCK_DRIFT_MS: i64 = 5_000;

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
        .route("/api/health", axum::routing::get(api::get_health))
        .route("/api/settings", axum::routing::get(api::get_settings))
        .route("/api/settings", axum::routing::post(api::update_settings))
        .route("/ws", axum::routing::get(ws::ws_handler))
        .fallback_service(ServeDir::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/web/static"),
        ))
        .layer(cors)
        .with_state(state);

    // WSL host browsers need the service exposed on the VM interface for
    // localhost forwarding to reach it.
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("Web dashboard running at http://localhost:{}", port);

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
                    let down_quote =
                        down_book.quote_buy_usd(&down_token, state.runtime.configured_max_order_usd);
                    let tick_size = match (up_book.tick_size(), down_book.tick_size()) {
                        (Some(up), Some(down)) if (up - down).abs() < f64::EPSILON => Some(up),
                        _ => None,
                    };
                    let min_order_size = match (up_book.min_order_size(), down_book.min_order_size())
                    {
                        (Some(up), Some(down)) => Some(up.max(down)),
                        _ => None,
                    };
                    let fee_rate_bps = match (up_fee, down_fee) {
                        (Ok(up), Ok(down)) => Some(up.max(down)),
                        _ => None,
                    };
                    let book_timestamp_ms =
                        match (up_book.timestamp_ms(), down_book.timestamp_ms()) {
                            (Some(up), Some(down)) => up.min(down),
                            _ => chrono::Utc::now().timestamp_millis(),
                        };
                    let one_sided = up_bid.is_none()
                        || up_ask.is_none()
                        || down_bid.is_none()
                        || down_ask.is_none();
                    let metadata_complete = tick_size.is_some()
                        && min_order_size.is_some()
                        && fee_rate_bps.is_some()
                        && up_quote.is_some()
                        && down_quote.is_some()
                        && !one_sided
                        && clock_drift_ms
                            .map(|drift| drift.abs() <= MAX_CLOCK_DRIFT_MS)
                            .unwrap_or(false);
                    let spread = match (up_ask, down_ask, up_bid, down_bid) {
                        (Some(up_ask), Some(down_ask), _, _) => {
                            (up_ask + down_ask - 1.0).abs()
                        }
                        (_, Some(down_ask), Some(up_bid), _) => {
                            (down_ask - (1.0 - up_bid)).abs()
                        }
                        (Some(up_ask), _, _, Some(down_bid)) => {
                            (up_ask - (1.0 - down_bid)).abs()
                        }
                        _ => 0.0,
                    };

                    let current_price = if asset == "btc" { btc_price } else { 0.0 };
                    let price_to_beat = previous_prices
                        .get(&slug)
                        .copied()
                        .filter(|price| *price > 0.0)
                        .unwrap_or(current_price);
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
    let rest_client = BinanceRestClient::new();

    loop {
        let now = chrono::Utc::now();
        let window_start_ts = get_current_interval_start(15);
        let elapsed = now.timestamp() - window_start_ts;
        let candle_request = tokio::time::timeout(
            tokio::time::Duration::from_secs(8),
            rest_client.fetch_recent_candles("BTCUSDT", "1m", 90),
        )
        .await;

        let signal_info = match candle_request {
            Ok(Ok(candles)) if candles_are_complete_and_fresh(&candles, now.timestamp_millis()) => {
                let window_index = candles
                    .iter()
                    .position(|candle| candle.timestamp == window_start_ts * 1000);
                match window_index {
                    Some(index) if elapsed >= 180 && index + 2 < candles.len() => {
                        let entry_index = index + 2;
                        let history_start = entry_index.saturating_sub(60);
                        let prices: Vec<f64> = candles[history_start..=entry_index]
                            .iter()
                            .map(|candle| candle.close)
                            .collect();
                        let window_open = candles[index].open;
                        let current_slug = generate_updown_slug("btc", "15m", window_start_ts);
                        if let Some(market) = state
                            .updown_markets
                            .write()
                            .await
                            .iter_mut()
                            .find(|market| market.slug == current_slug)
                        {
                            market.price_to_beat = window_open;
                        }

                        match predict_early_window(&prices) {
                            Some(signal) => state::SignalInfo {
                                direction: signal.direction.to_string(),
                                confidence: signal.confidence,
                                timeframe: Timeframe::M15.as_str().to_string(),
                                reason: format!("fixed minute-3 model | {}", signal.reason),
                                timestamp: now.timestamp_millis(),
                                window_start_ts,
                            },
                            None => state::SignalInfo {
                                direction: "WAIT".to_string(),
                                confidence: 0.0,
                                timeframe: "15m".to_string(),
                                reason: "Minute-3 model found no aligned momentum setup"
                                    .to_string(),
                                timestamp: now.timestamp_millis(),
                                window_start_ts,
                            },
                        }
                    }
                    _ => state::SignalInfo {
                        direction: "WAIT".to_string(),
                        confidence: 0.0,
                        timeframe: "15m".to_string(),
                        reason: format!("Waiting for minute-3 close; elapsed {}s", elapsed),
                        timestamp: now.timestamp_millis(),
                        window_start_ts,
                    },
                }
            }
            Ok(Ok(_)) => state::SignalInfo {
                direction: "WAIT".to_string(),
                confidence: 0.0,
                timeframe: "15m".to_string(),
                reason: "Binance 1m candle payload is incomplete or stale".to_string(),
                timestamp: now.timestamp_millis(),
                window_start_ts,
            },
            Ok(Err(error)) => state::SignalInfo {
                direction: "WAIT".to_string(),
                confidence: 0.0,
                timeframe: "15m".to_string(),
                reason: format!("Binance 1m data unavailable: {}", error),
                timestamp: now.timestamp_millis(),
                window_start_ts,
            },
            Err(_) => state::SignalInfo {
                direction: "WAIT".to_string(),
                confidence: 0.0,
                timeframe: "15m".to_string(),
                reason: "Binance 1m request timed out after 8s".to_string(),
                timestamp: now.timestamp_millis(),
                window_start_ts,
            },
        };

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
                    .halt_after_persistence_failure("15m signal audit", &error)
                    .await;
            }
        }
        *state.last_signal.write().await = Some(signal_info);
        *state.last_signal_time.write().await = signal_time;

        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}

async fn run_5m_signal_generator(state: AppState) {
    let rest_client = BinanceRestClient::new();

    loop {
        let now = chrono::Utc::now();
        let current_start = get_current_interval_start(5);
        let elapsed = now.timestamp() - current_start;
        let candle_request = tokio::time::timeout(
            tokio::time::Duration::from_secs(8),
            rest_client.fetch_recent_candles("BTCUSDT", "1m", 40),
        )
        .await;

        let signal_info = match candle_request {
            Ok(Ok(candles)) if candles_are_complete_and_fresh(&candles, now.timestamp_millis()) => {
                let window_index = candles
                    .iter()
                    .position(|candle| candle.timestamp == current_start * 1000);

                match window_index {
                    Some(index) if elapsed >= 60 && index > 0 => {
                        let prices: Vec<f64> = candles[..=index]
                            .iter()
                            .map(|candle| candle.close)
                            .collect();
                        let window_open = candles[index].open;
                        let current_slug = generate_updown_slug("btc", "5m", current_start);
                        if let Some(market) = state
                            .updown_markets
                            .write()
                            .await
                            .iter_mut()
                            .find(|market| market.slug == current_slug)
                        {
                            market.price_to_beat = window_open;
                        }
                        match predict_five_minute_continuation(&prices, window_open) {
                            Some(signal) => state::SignalInfo {
                                direction: signal.direction.to_string(),
                                confidence: signal.confidence,
                                timeframe: "5m".to_string(),
                                reason: signal.reason,
                                timestamp: now.timestamp_millis(),
                                window_start_ts: current_start,
                            },
                            None => state::SignalInfo {
                                direction: "WAIT".to_string(),
                                confidence: 0.0,
                                timeframe: "5m".to_string(),
                                reason: diagnose_five_minute_continuation(&prices, window_open),
                                timestamp: now.timestamp_millis(),
                                window_start_ts: current_start,
                            },
                        }
                    }
                    _ => state::SignalInfo {
                        direction: "WAIT".to_string(),
                        confidence: 0.0,
                        timeframe: "5m".to_string(),
                        reason: format!("Waiting for first minute close; elapsed {}s", elapsed),
                        timestamp: now.timestamp_millis(),
                        window_start_ts: current_start,
                    },
                }
            }
            Ok(Ok(_)) => state::SignalInfo {
                direction: "WAIT".to_string(),
                confidence: 0.0,
                timeframe: "5m".to_string(),
                reason: "Binance 1m candle payload is incomplete or stale".to_string(),
                timestamp: now.timestamp_millis(),
                window_start_ts: current_start,
            },
            Ok(Err(error)) => state::SignalInfo {
                direction: "WAIT".to_string(),
                confidence: 0.0,
                timeframe: "5m".to_string(),
                reason: format!("Binance 1m data unavailable: {}", error),
                timestamp: now.timestamp_millis(),
                window_start_ts: current_start,
            },
            Err(_) => state::SignalInfo {
                direction: "WAIT".to_string(),
                confidence: 0.0,
                timeframe: "5m".to_string(),
                reason: "Binance 1m request timed out after 8s".to_string(),
                timestamp: now.timestamp_millis(),
                window_start_ts: current_start,
            },
        };

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
                    .halt_after_persistence_failure("5m signal audit", &error)
                    .await;
            }
        }
        *state.last_signal_5m.write().await = Some(signal_info);
        *state.last_signal_5m_time.write().await = signal_time;
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}

async fn run_paper_executor(state: AppState) {
    loop {
        settle_finished_trades(&state).await;
        try_open_trade(&state).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}

async fn run_5m_paper_executor(state: AppState) {
    loop {
        settle_finished_5m_trades(&state).await;
        try_open_5m_trade(&state).await;
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

    let mut trades = state.trades.write().await;
    let mut stats = state.stats.write().await;
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
        drop(stats);
        drop(trades);
        if let Err(error) = state
            .persist("paper_trades_settled", serde_json::json!({"timeframe": "15m"}))
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

    let mut trades = state.trades.write().await;
    let mut stats = state.stats_5m.write().await;
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
        refresh_stats(&mut stats, &trades, "5m");
        drop(stats);
        drop(trades);
        if let Err(error) = state
            .persist("paper_trades_settled", serde_json::json!({"timeframe": "5m"}))
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

async fn try_open_trade(state: &AppState) {
    let settings = state.settings.read().await.clone();
    if !settings.auto_trade {
        set_execution_note(state, "Auto-trade is disabled").await;
        return;
    }
    let price = state.price.read().await.clone();
    if price.source != "live"
        || chrono::Utc::now().timestamp_millis() - price.timestamp
            > state.runtime.configured_max_data_age_ms as i64
    {
        set_execution_note(state, "Skip: live BTC price is unavailable or stale").await;
        return;
    }

    let signal = match state.last_signal.read().await.clone() {
        Some(signal) if signal.direction == "Up" || signal.direction == "Down" => signal,
        _ => {
            set_execution_note(state, "Waiting for a valid UP/DOWN signal").await;
            return;
        }
    };
    if chrono::Utc::now().timestamp_millis() - signal.timestamp > 30_000 {
        set_execution_note(state, "Signal is stale; waiting for refresh").await;
        return;
    }

    let market = match state
        .updown_markets
        .read()
        .await
        .iter()
        .find(|market| market.asset == "btc" && market.interval == "15m" && market.status == "live")
        .cloned()
    {
        Some(market) => market,
        None => {
            set_execution_note(state, "Waiting for the active BTC 15m market").await;
            return;
        }
    };
    if let Err(reason) = validate_market_freshness(&market, &state.runtime) {
        set_execution_note(state, &format!("Skip: 15m market data {reason}")).await;
        return;
    }
    if signal.window_start_ts != market.start_ts {
        set_execution_note(
            state,
            "Waiting for a 15m signal from the active market window",
        )
        .await;
        return;
    }
    if market.spread > state.runtime.configured_max_spread {
        set_execution_note(
            state,
            &format!(
                "Skip: 15m spread {:.1}% exceeds {:.1}%",
                market.spread * 100.0,
                state.runtime.configured_max_spread * 100.0
            ),
        )
        .await;
        return;
    }

    // Match the backtest: decide after the third one-minute candle closes.
    let elapsed = chrono::Utc::now().timestamp() - market.start_ts;
    if !(180..=210).contains(&elapsed) || market.price_to_beat <= 0.0 {
        set_execution_note(
            state,
            &format!("Waiting for early entry window; elapsed {}s", elapsed),
        )
        .await;
        return;
    }

    let ask = if signal.direction == "Up" {
        market.up_expected_fill_price
    } else {
        market.down_expected_fill_price
    };
    let ask = match ask {
        Some(ask) if ask >= 0.50 && ask <= settings.max_entry_price => ask,
        Some(ask) => {
            set_execution_note(
                state,
                &format!(
                    "Skip: 15m ask {:.2} outside 0.50-{:.2}; market does not confirm direction",
                    ask, settings.max_entry_price
                ),
            )
            .await;
            return;
        }
        None => {
            set_execution_note(state, "Skip: executable orderbook depth unavailable").await;
            return;
        }
    };
    let edge = signal.confidence - ask;
    if edge < settings.min_edge {
        set_execution_note(
            state,
            &format!(
                "Skip: model margin {:.1}% below required {:.1}%",
                edge * 100.0,
                settings.min_edge * 100.0
            ),
        )
        .await;
        return;
    }

    let mut trades = state.trades.write().await;
    if trades.iter().any(|trade| trade.market_slug == market.slug) {
        set_execution_note(state, "Already traded this 15m window").await;
        return;
    }
    if open_position_count(&trades) >= state.runtime.configured_max_open_positions {
        set_execution_note(state, "Skip: maximum open-position limit reached").await;
        return;
    }
    if daily_trade_count(&trades) >= state.runtime.configured_max_daily_orders {
        set_execution_note(state, "Skip: maximum daily-order limit reached").await;
        return;
    }

    let mut stats = state.stats.write().await;
    let daily_pnl = daily_realized_pnl(&trades, "15m");
    if daily_pnl <= -state.runtime.configured_max_daily_loss_usd
        || stats.max_drawdown >= state.runtime.configured_max_drawdown
    {
        set_execution_note(
            state,
            &format!(
                "15m halted: PnL ${:.2}, drawdown {:.1}% exceeds safety limit",
                daily_pnl,
                stats.max_drawdown * 100.0
            ),
        )
        .await;
        return;
    }
    let recent_15m: Vec<&state::TradeInfo> = trades
        .iter()
        .rev()
        .filter(|trade| trade.timeframe == "15m" && trade.status == "settled")
        .take(state.runtime.configured_max_consecutive_losses)
        .collect();
    if recent_15m.len() == state.runtime.configured_max_consecutive_losses
        && recent_15m
            .iter()
            .all(|trade| trade.pnl.unwrap_or(0.0) < 0.0)
    {
        let last_timestamp = recent_15m[0].timestamp;
        if chrono::Utc::now().timestamp_millis() - last_timestamp < 90 * 60 * 1000 {
            set_execution_note(
                state,
                &format!(
                    "15m circuit breaker active after {} losses",
                    state.runtime.configured_max_consecutive_losses
                ),
            )
            .await;
            return;
        }
    }
    let size_usd = (stats.current_capital * settings.risk_fraction)
        .max(state.runtime.configured_min_order_usd)
        .min(settings.max_order);
    if market
        .min_order_size
        .map(|minimum_shares| size_usd / ask < minimum_shares)
        .unwrap_or(true)
    {
        set_execution_note(
            state,
            "Skip: configured order is below the CLOB minimum share size",
        )
        .await;
        return;
    }
    let fee_usd = size_usd * state.runtime.configured_fee_pct;
    if stats.current_capital < size_usd + fee_usd {
        set_execution_note(state, "Skip: insufficient paper capital").await;
        return;
    }
    match state
        .reserve_execution_intent(
            &market.slug,
            "15m",
            serde_json::json!({
                "direction": &signal.direction,
                "ask": ask,
                "confidence": signal.confidence,
                "edge": edge,
                "size_usd": size_usd,
                "fee_usd": fee_usd,
                "expected_fill_price": ask,
                "tick_size": market.tick_size,
                "min_order_size": market.min_order_size,
                "fee_rate_bps": market.fee_rate_bps,
                "negative_risk": market.negative_risk
            }),
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            set_execution_note(state, "Skip: durable 15m intent already exists").await;
            return;
        }
        Err(error) => {
            drop(stats);
            drop(trades);
            state
                .halt_after_persistence_failure("15m intent reservation", &error)
                .await;
            set_execution_note(state, "HALT: failed to persist 15m execution intent").await;
            return;
        }
    }

    stats.current_capital -= size_usd + fee_usd;
    trades.push(state::TradeInfo {
        timestamp: chrono::Utc::now().timestamp_millis(),
        market_slug: market.slug.clone(),
        timeframe: "15m".to_string(),
        direction: signal.direction.clone(),
        entry_price: ask,
        exit_price: None,
        shares: size_usd / ask,
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
    drop(stats);
    drop(trades);
    if let Err(error) = state
        .persist(
            "paper_trade_opened",
            serde_json::json!({
                "market_slug": &market.slug,
                "timeframe": "15m",
                "direction": &signal.direction,
                "size_usd": size_usd
            }),
        )
        .await
    {
        state
            .halt_after_persistence_failure("15m trade open", &error)
            .await;
    }
    set_execution_note(
        state,
        &format!(
            "Paper BUY {} @ {:.2}, model margin {:.1}%, size ${:.2}",
            signal.direction,
            ask,
            edge * 100.0,
            size_usd
        ),
    )
    .await;

    tracing::info!(
        "Paper BUY {} {} @ {:.2}, model margin {:.1}%, size ${:.2}",
        market.slug,
        signal.direction,
        ask,
        edge * 100.0,
        size_usd
    );
}

async fn try_open_5m_trade(state: &AppState) {
    let settings = state.settings.read().await.clone();
    if !settings.auto_trade {
        set_5m_execution_note(state, "Auto-trade is disabled").await;
        return;
    }
    let price = state.price.read().await.clone();
    if price.source != "live"
        || chrono::Utc::now().timestamp_millis() - price.timestamp
            > state.runtime.configured_max_data_age_ms as i64
    {
        set_5m_execution_note(state, "Skip: live BTC price is unavailable or stale").await;
        return;
    }

    let signal = match state.last_signal_5m.read().await.clone() {
        Some(signal) if signal.direction == "Up" || signal.direction == "Down" => signal,
        _ => {
            set_5m_execution_note(state, "Waiting for a strong 5m continuation signal").await;
            return;
        }
    };
    if chrono::Utc::now().timestamp_millis() - signal.timestamp > 30_000 {
        set_5m_execution_note(state, "5m signal is stale; waiting for refresh").await;
        return;
    }

    let market = match state
        .updown_markets
        .read()
        .await
        .iter()
        .find(|market| market.asset == "btc" && market.interval == "5m" && market.status == "live")
        .cloned()
    {
        Some(market) => market,
        None => {
            set_5m_execution_note(state, "Waiting for the active BTC 5m market").await;
            return;
        }
    };
    if let Err(reason) = validate_market_freshness(&market, &state.runtime) {
        set_5m_execution_note(state, &format!("Skip: 5m market data {reason}")).await;
        return;
    }
    if signal.window_start_ts != market.start_ts {
        set_5m_execution_note(
            state,
            "Waiting for a 5m signal from the active market window",
        )
        .await;
        return;
    }

    let elapsed = chrono::Utc::now().timestamp() - market.start_ts;
    if !(60..=90).contains(&elapsed) || market.price_to_beat <= 0.0 {
        set_5m_execution_note(
            state,
            &format!("Waiting for 5m entry window; elapsed {}s", elapsed),
        )
        .await;
        return;
    }
    if market.spread > state.runtime.configured_max_spread {
        set_5m_execution_note(
            state,
            &format!(
                "Skip: 5m spread {:.1}% exceeds {:.1}%",
                market.spread * 100.0,
                state.runtime.configured_max_spread * 100.0
            ),
        )
        .await;
        return;
    }

    let ask = if signal.direction == "Up" {
        market.up_expected_fill_price
    } else {
        market.down_expected_fill_price
    };
    let ask = match ask {
        Some(ask) if (0.15..=0.62).contains(&ask) => ask,
        Some(ask) => {
            set_5m_execution_note(state, &format!("Skip: 5m ask {:.2} outside 0.15-0.62", ask))
                .await;
            return;
        }
        None => {
            set_5m_execution_note(state, "Skip: 5m executable orderbook depth unavailable").await;
            return;
        }
    };
    let edge = signal.confidence - ask;
    if edge < 0.08 {
        set_5m_execution_note(
            state,
            &format!(
                "Skip: 5m model margin {:.1}% below required 8.0%",
                edge * 100.0
            ),
        )
        .await;
        return;
    }

    let mut trades = state.trades.write().await;
    if trades.iter().any(|trade| trade.market_slug == market.slug) {
        set_5m_execution_note(state, "Already traded this 5m window").await;
        return;
    }
    if open_position_count(&trades) >= state.runtime.configured_max_open_positions {
        set_5m_execution_note(state, "Skip: maximum open-position limit reached").await;
        return;
    }
    if daily_trade_count(&trades) >= state.runtime.configured_max_daily_orders {
        set_5m_execution_note(state, "Skip: maximum daily-order limit reached").await;
        return;
    }

    let recent_5m: Vec<&state::TradeInfo> = trades
        .iter()
        .rev()
        .filter(|trade| trade.timeframe == "5m" && trade.status == "settled")
        .take(state.runtime.configured_max_consecutive_losses)
        .collect();
    if recent_5m.len() == state.runtime.configured_max_consecutive_losses
        && recent_5m
            .iter()
            .all(|trade| trade.pnl.unwrap_or(0.0) < 0.0)
    {
        let last_timestamp = recent_5m[0].timestamp;
        if chrono::Utc::now().timestamp_millis() - last_timestamp < 90 * 60 * 1000 {
            set_5m_execution_note(
                state,
                &format!(
                    "5m circuit breaker active after {} losses",
                    state.runtime.configured_max_consecutive_losses
                ),
            )
            .await;
            return;
        }
    }

    let mut stats = state.stats_5m.write().await;
    let daily_pnl = daily_realized_pnl(&trades, "5m");
    if daily_pnl <= -state.runtime.configured_max_daily_loss_usd
        || stats.max_drawdown >= state.runtime.configured_max_drawdown
    {
        set_5m_execution_note(
            state,
            &format!(
                "5m halted: PnL ${:.2}, drawdown {:.1}% exceeds safety limit",
                daily_pnl,
                stats.max_drawdown * 100.0
            ),
        )
        .await;
        return;
    }
    let size_usd = (stats.current_capital * settings.risk_fraction)
        .max(state.runtime.configured_min_order_usd)
        .min(settings.max_order);
    if market
        .min_order_size
        .map(|minimum_shares| size_usd / ask < minimum_shares)
        .unwrap_or(true)
    {
        set_5m_execution_note(
            state,
            "Skip: configured 5m order is below the CLOB minimum share size",
        )
        .await;
        return;
    }
    let fee_usd = size_usd * state.runtime.configured_fee_pct;
    if stats.current_capital < size_usd + fee_usd {
        set_5m_execution_note(state, "Skip: insufficient 5m paper capital").await;
        return;
    }
    match state
        .reserve_execution_intent(
            &market.slug,
            "5m",
            serde_json::json!({
                "direction": &signal.direction,
                "ask": ask,
                "confidence": signal.confidence,
                "edge": edge,
                "size_usd": size_usd,
                "fee_usd": fee_usd,
                "expected_fill_price": ask,
                "tick_size": market.tick_size,
                "min_order_size": market.min_order_size,
                "fee_rate_bps": market.fee_rate_bps,
                "negative_risk": market.negative_risk
            }),
        )
        .await
    {
        Ok(true) => {}
        Ok(false) => {
            set_5m_execution_note(state, "Skip: durable 5m intent already exists").await;
            return;
        }
        Err(error) => {
            drop(stats);
            drop(trades);
            state
                .halt_after_persistence_failure("5m intent reservation", &error)
                .await;
            set_5m_execution_note(state, "HALT: failed to persist 5m execution intent").await;
            return;
        }
    }

    stats.current_capital -= size_usd + fee_usd;
    trades.push(state::TradeInfo {
        timestamp: chrono::Utc::now().timestamp_millis(),
        market_slug: market.slug.clone(),
        timeframe: "5m".to_string(),
        direction: signal.direction.clone(),
        entry_price: ask,
        exit_price: None,
        shares: size_usd / ask,
        size_usd,
        fee_usd,
        price_to_beat: market.price_to_beat,
        end_ts: market.end_ts,
        confidence: signal.confidence,
        edge,
        pnl: None,
        status: "open".to_string(),
    });
    refresh_stats(&mut stats, &trades, "5m");
    drop(stats);
    drop(trades);
    if let Err(error) = state
        .persist(
            "paper_trade_opened",
            serde_json::json!({
                "market_slug": &market.slug,
                "timeframe": "5m",
                "direction": &signal.direction,
                "size_usd": size_usd
            }),
        )
        .await
    {
        state
            .halt_after_persistence_failure("5m trade open", &error)
            .await;
    }
    set_5m_execution_note(
        state,
        &format!(
            "Paper BUY 5m {} @ {:.2}, model margin {:.1}%, size ${:.2}",
            signal.direction,
            ask,
            edge * 100.0,
            size_usd
        ),
    )
    .await;
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
        || note.starts_with("Already traded")
        || note.contains("circuit breaker")
        || note.contains("halted:")
}

fn validate_market_freshness(
    market: &state::UpDownMarket,
    runtime: &state::RuntimeInfo,
) -> Result<(), &'static str> {
    if market.data_status != state::DataStatus::Ready || !market.token_mapping_valid {
        return Err("is incomplete or invalid");
    }
    if market.up_best_ask.is_none()
        || market.up_best_bid.is_none()
        || market.down_best_ask.is_none()
        || market.down_best_bid.is_none()
        || market.up_expected_fill_price.is_none()
        || market.down_expected_fill_price.is_none()
        || market.tick_size.is_none()
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
    Ok(())
}

fn open_position_count(trades: &[state::TradeInfo]) -> usize {
    trades
        .iter()
        .filter(|trade| trade.status == "open")
        .count()
}

fn daily_trade_count(trades: &[state::TradeInfo]) -> usize {
    trades
        .iter()
        .filter(|trade| trade.timestamp >= current_utc_day_start_ms())
        .count()
}

fn daily_realized_pnl(trades: &[state::TradeInfo], timeframe: &str) -> f64 {
    trades
        .iter()
        .filter(|trade| {
            trade.timestamp >= current_utc_day_start_ms()
                && trade.timeframe == timeframe
                && trade.status == "settled"
        })
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

        assert_eq!(daily_trade_count(&[trade(now, "open"), trade(yesterday, "settled")]), 1);
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

        assert_eq!(daily_realized_pnl(&[today_loss, old_loss, open], "5m"), -0.10);
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
        assert_eq!(validate_market_freshness(&market, &runtime), Err("is stale"));
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
