pub mod api;
pub mod price_proxy;
pub mod state;
pub mod ws;

use crate::crypto::indicators::Timeframe;
use crate::crypto::live::gamma_client::{
    generate_updown_slug, get_current_interval_start, get_remaining_seconds, ClobClient,
    GammaClient,
};
use crate::crypto::binance_ws::BinanceRestClient;
use crate::crypto::strategy::{
    diagnose_five_minute_continuation, predict_early_window, predict_five_minute_continuation,
};
use axum::Router;
use state::AppState;
use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

// Assets to scan for Up/Down markets
const CRYPTO_ASSETS: &[&str] = &["btc", "eth", "sol", "xrp", "doge", "bnb"];
const MARKET_PROXY_URL: &str = "http://localhost:3000";

pub async fn run_web_server(port: u16) -> anyhow::Result<()> {
    let state = AppState::new();

    // Start price proxy
    let state_clone = state.clone();
    tokio::spawn(async move {
        price_proxy::run_price_proxy(state_clone).await;
    });

    // Start BTC Up/Down market scanner
    let state_clone = state.clone();
    tokio::spawn(async move {
        run_updown_scanner(state_clone).await;
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
        .route("/api/settings", axum::routing::get(api::get_settings))
        .route("/api/settings", axum::routing::post(api::update_settings))
        .route("/ws", axum::routing::get(ws::ws_handler))
        .fallback_service(ServeDir::new("polymarket-bot/src/web/static"))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Web dashboard running at http://localhost:{}", port);

    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn run_updown_scanner(state: AppState) {
    let gamma_client = GammaClient::new(MARKET_PROXY_URL);
    let clob_client = ClobClient::new(MARKET_PROXY_URL);

    loop {
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
                    if let Some(markets) = &event.markets {
                        for market in markets {
                            let token_ids = market.get_token_ids();
                            let up_token = token_ids.first().cloned();
                            let down_token = token_ids.get(1).cloned();

                            // Fetch orderbook for UP token
                            let (up_ask, up_bid, down_ask, down_bid, spread) =
                                if let Some(ref up_id) = up_token {
                                    let up_book = clob_client.fetch_orderbook(up_id).await.ok();
                                    let down_book = if let Some(ref dt) = down_token {
                                        clob_client.fetch_orderbook(dt).await.ok()
                                    } else {
                                        None
                                    };

                                    let up_ask = up_book
                                        .as_ref()
                                        .and_then(|b| b.asks.last())
                                        .and_then(|a| a.price.parse::<f64>().ok());
                                    let up_bid = up_book
                                        .as_ref()
                                        .and_then(|b| b.bids.last())
                                        .and_then(|b| b.price.parse::<f64>().ok());
                                    let down_ask = down_book
                                        .as_ref()
                                        .and_then(|b| b.asks.last())
                                        .and_then(|a| a.price.parse::<f64>().ok());
                                    let down_bid = down_book
                                        .as_ref()
                                        .and_then(|b| b.bids.last())
                                        .and_then(|b| b.price.parse::<f64>().ok());

                                    let spread = match (up_ask, down_ask) {
                                        (Some(ua), Some(da)) => (ua + da - 1.0f64).abs(),
                                        _ => 0.0f64,
                                    };

                                    (up_ask, up_bid, down_ask, down_bid, spread)
                                } else {
                                    (None, None, None, None, 0.0f64)
                                };

                            // Only BTC has a live spot-price feed in the current dashboard.
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
                                up_token_id: up_token,
                                down_token_id: down_token,
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
                            });
                        }
                    }
                }
                Ok(None) => {
                    // Market not found, create placeholder
                    updown_markets.push(state::UpDownMarket {
                        asset: asset.to_string(),
                        slug: slug.clone(),
                        interval: interval.to_string(),
                        start_ts: current_start,
                        end_ts,
                        remaining_seconds: remaining,
                        up_token_id: None,
                        down_token_id: None,
                        up_best_ask: None,
                        up_best_bid: None,
                        down_best_ask: None,
                        down_best_bid: None,
                        spread: 0.0,
                        status: "not_found".to_string(),
                        price_to_beat: 0.0,
                        current_price: 0.0,
                    });
                }
                Err(e) => {
                    tracing::error!("Failed to fetch event {}: {}", slug, e);
                    updown_markets.push(state::UpDownMarket {
                        asset: asset.to_string(),
                        slug: slug.clone(),
                        interval: interval.to_string(),
                        start_ts: current_start,
                        end_ts,
                        remaining_seconds: remaining,
                        up_token_id: None,
                        down_token_id: None,
                        up_best_ask: None,
                        up_best_bid: None,
                        down_best_ask: None,
                        down_best_bid: None,
                        spread: 0.0,
                        status: "api_unavailable".to_string(),
                        price_to_beat: 0.0,
                        current_price: 0.0,
                    });
                }
            }
        }

        // Update state
        let mut state_markets = state.updown_markets.write().await;
        *state_markets = updown_markets;

        let mut last_scan = state.last_scan_at.write().await;
        *last_scan = chrono::Utc::now().timestamp_millis();

        tracing::info!(
            "Scanned {} Up/Down market targets",
            CRYPTO_ASSETS.len() + 1
        );

        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
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
            Ok(Ok(candles)) => {
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
                        let current_slug =
                            generate_updown_slug("btc", "15m", window_start_ts);
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
                                reason: "Minute-3 model found no aligned momentum setup".to_string(),
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
            Ok(Ok(candles)) => {
                let window_index = candles
                    .iter()
                    .position(|candle| candle.timestamp == current_start * 1000);

                match window_index {
                    Some(index) if elapsed >= 60 && index > 0 => {
                        let prices: Vec<f64> =
                            candles[..=index].iter().map(|candle| candle.close).collect();
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
        *state.last_signal_5m.write().await = Some(signal_info);
        *state.last_signal_5m_time.write().await = signal_time;
        tokio::time::sleep(tokio::time::Duration::from_secs(10)).await;
    }
}

async fn run_paper_executor(state: AppState) {
    loop {
        settle_finished_trades(&state).await;
        try_open_trade(&state).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

async fn run_5m_paper_executor(state: AppState) {
    loop {
        settle_finished_5m_trades(&state).await;
        try_open_5m_trade(&state).await;
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
    }
}

async fn settle_finished_trades(state: &AppState) {
    let now = chrono::Utc::now().timestamp();
    let price_data = state.price.read().await.clone();
    let current_btc_price = price_data.price;
    if current_btc_price <= 0.0 || price_data.source != "live" {
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

        let won = if trade.direction == "Up" {
            current_btc_price >= trade.price_to_beat
        } else {
            current_btc_price < trade.price_to_beat
        };
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
    }
}

async fn settle_finished_5m_trades(state: &AppState) {
    let now = chrono::Utc::now().timestamp();
    let price_data = state.price.read().await.clone();
    let current_btc_price = price_data.price;
    if current_btc_price <= 0.0 || price_data.source != "live" {
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
        let won = if trade.direction == "Up" {
            current_btc_price >= trade.price_to_beat
        } else {
            current_btc_price < trade.price_to_beat
        };
        let payout = if won { trade.shares } else { 0.0 };
        trade.exit_price = Some(if won { 1.0 } else { 0.0 });
        trade.pnl = Some(payout - trade.size_usd - trade.fee_usd);
        trade.status = "settled".to_string();
        stats.current_capital += payout;
        changed = true;
    }
    if changed {
        refresh_stats(&mut stats, &trades, "5m");
    }
}

async fn try_open_trade(state: &AppState) {
    let settings = state.settings.read().await.clone();
    if !settings.auto_trade {
        set_execution_note(state, "Auto-trade is disabled").await;
        return;
    }
    if state.price.read().await.source != "live" {
        set_execution_note(state, "Skip: live BTC price source unavailable").await;
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
        .find(|market| {
            market.asset == "btc" && market.interval == "15m" && market.status == "live"
        })
        .cloned()
    {
        Some(market) => market,
        None => {
            set_execution_note(state, "Waiting for the active BTC 15m market").await;
            return;
        }
    };
    if signal.window_start_ts != market.start_ts {
        set_execution_note(state, "Waiting for a 15m signal from the active market window").await;
        return;
    }
    if market.spread > 0.04 {
        set_execution_note(
            state,
            &format!("Skip: 15m spread {:.1}% exceeds 4.0%", market.spread * 100.0),
        )
        .await;
        return;
    }

    // Match the backtest: decide after the third one-minute candle closes.
    let elapsed = chrono::Utc::now().timestamp() - market.start_ts;
    if !(180..=300).contains(&elapsed) || market.price_to_beat <= 0.0 {
        set_execution_note(
            state,
            &format!("Waiting for early entry window; elapsed {}s", elapsed),
        )
        .await;
        return;
    }

    let ask = if signal.direction == "Up" {
        market.up_best_ask
    } else {
        market.down_best_ask
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
            set_execution_note(state, "Skip: orderbook ask unavailable").await;
            return;
        }
    };
    let edge = signal.confidence - ask;
    if edge < settings.min_edge {
        set_execution_note(
            state,
            &format!(
                "Skip: edge {:.1}% below required {:.1}%",
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

    let mut stats = state.stats.write().await;
    if stats.total_pnl <= -0.30 || stats.max_drawdown >= 0.20 {
        set_execution_note(
            state,
            &format!(
                "15m halted: PnL ${:.2}, drawdown {:.1}% exceeds safety limit",
                stats.total_pnl,
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
        .take(3)
        .collect();
    if recent_15m.len() == 3 && recent_15m.iter().all(|trade| trade.pnl.unwrap_or(0.0) < 0.0) {
        let last_timestamp = recent_15m[0].timestamp;
        if chrono::Utc::now().timestamp_millis() - last_timestamp < 90 * 60 * 1000 {
            set_execution_note(state, "15m circuit breaker active after 3 losses").await;
            return;
        }
    }
    let size_usd = (stats.current_capital * settings.risk_fraction)
        .max(0.10)
        .min(settings.max_order);
    let fee_usd = size_usd * 0.02;
    if stats.current_capital < size_usd + fee_usd {
        set_execution_note(state, "Skip: insufficient paper capital").await;
        return;
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
    set_execution_note(
        state,
        &format!(
            "Paper BUY {} @ {:.2}, edge {:.1}%, size ${:.2}",
            signal.direction,
            ask,
            edge * 100.0,
            size_usd
        ),
    )
    .await;

    tracing::info!(
        "Paper BUY {} {} @ {:.2}, edge {:.1}%, size ${:.2}",
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
    if state.price.read().await.source != "live" {
        set_5m_execution_note(state, "Skip: live BTC price source unavailable").await;
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
        .find(|market| {
            market.asset == "btc" && market.interval == "5m" && market.status == "live"
        })
        .cloned()
    {
        Some(market) => market,
        None => {
            set_5m_execution_note(state, "Waiting for the active BTC 5m market").await;
            return;
        }
    };
    if signal.window_start_ts != market.start_ts {
        set_5m_execution_note(state, "Waiting for a 5m signal from the active market window").await;
        return;
    }

    let elapsed = chrono::Utc::now().timestamp() - market.start_ts;
    if !(60..=120).contains(&elapsed) || market.price_to_beat <= 0.0 {
        set_5m_execution_note(
            state,
            &format!("Waiting for 5m entry window; elapsed {}s", elapsed),
        )
        .await;
        return;
    }
    if market.spread > 0.04 {
        set_5m_execution_note(
            state,
            &format!("Skip: 5m spread {:.1}% exceeds 4.0%", market.spread * 100.0),
        )
        .await;
        return;
    }

    let ask = if signal.direction == "Up" {
        market.up_best_ask
    } else {
        market.down_best_ask
    };
    let ask = match ask {
        Some(ask) if (0.15..=0.62).contains(&ask) => ask,
        Some(ask) => {
            set_5m_execution_note(state, &format!("Skip: 5m ask {:.2} outside 0.15-0.62", ask))
                .await;
            return;
        }
        None => {
            set_5m_execution_note(state, "Skip: 5m orderbook ask unavailable").await;
            return;
        }
    };
    let edge = signal.confidence - ask;
    if edge < 0.08 {
        set_5m_execution_note(
            state,
            &format!("Skip: 5m edge {:.1}% below required 8.0%", edge * 100.0),
        )
        .await;
        return;
    }

    let mut trades = state.trades.write().await;
    if trades.iter().any(|trade| trade.market_slug == market.slug) {
        set_5m_execution_note(state, "Already traded this 5m window").await;
        return;
    }

    let recent_5m: Vec<&state::TradeInfo> = trades
        .iter()
        .rev()
        .filter(|trade| trade.timeframe == "5m" && trade.status == "settled")
        .take(3)
        .collect();
    if recent_5m.len() == 3 && recent_5m.iter().all(|trade| trade.pnl.unwrap_or(0.0) < 0.0) {
        let last_timestamp = recent_5m[0].timestamp;
        if chrono::Utc::now().timestamp_millis() - last_timestamp < 90 * 60 * 1000 {
            set_5m_execution_note(state, "5m circuit breaker active after 3 losses").await;
            return;
        }
    }

    let mut stats = state.stats_5m.write().await;
    let size_usd = (stats.current_capital * 0.03).max(0.10).min(0.25);
    let fee_usd = size_usd * 0.02;
    if stats.current_capital < size_usd + fee_usd {
        set_5m_execution_note(state, "Skip: insufficient 5m paper capital").await;
        return;
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
    set_5m_execution_note(
        state,
        &format!(
            "Paper BUY 5m {} @ {:.2}, edge {:.1}%, size ${:.2}",
            signal.direction,
            ask,
            edge * 100.0,
            size_usd
        ),
    )
    .await;
}

async fn set_execution_note(state: &AppState, note: &str) {
    *state.execution_note.write().await = note.to_string();
}

async fn set_5m_execution_note(state: &AppState, note: &str) {
    *state.execution_note_5m.write().await = note.to_string();
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
