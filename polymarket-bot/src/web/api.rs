use crate::web::state::AppState;
use crate::web::state::Settings;
use axum::extract::{Json, State};
use axum::http::StatusCode;
use serde_json::{json, Value};

pub async fn get_price(State(state): State<AppState>) -> axum::Json<Value> {
    let price = state.price.read().await;
    axum::Json(json!({
        "price": price.price,
        "change_pct": price.change_pct,
        "timestamp": price.timestamp,
        "source": price.source
    }))
}

pub async fn get_markets(State(state): State<AppState>) -> axum::Json<Value> {
    let markets = state.markets.read().await;
    let markets_json: Vec<Value> = markets
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "question": m.question,
                "yes_price": m.yes_price,
                "no_price": m.no_price,
                "volume": m.volume,
                "liquidity": m.liquidity,
                "enable_order_book": m.enable_order_book,
                "end_date": m.end_date,
                "tags": m.tags,
                "minutes_left": m.minutes_left
            })
        })
        .collect();
    axum::Json(json!({ "markets": markets_json }))
}

pub async fn get_updown_markets(State(state): State<AppState>) -> axum::Json<Value> {
    let updown = state.updown_markets.read().await;
    let updown_json: Vec<Value> = updown
        .iter()
        .map(|m| {
            let age_ms = chrono::Utc::now().timestamp_millis() - m.captured_at_ms;
            let data_status = if age_ms > state.runtime.configured_max_data_age_ms as i64 {
                "stale"
            } else {
                m.data_status.as_str()
            };
            json!({
                "asset": m.asset,
                "slug": m.slug,
                "interval": m.interval,
                "start_ts": m.start_ts,
                "end_ts": m.end_ts,
                "remaining_seconds": m.remaining_seconds,
                "status": m.status,
                "up_best_ask": m.up_best_ask,
                "up_best_bid": m.up_best_bid,
                "down_best_ask": m.down_best_ask,
                "down_best_bid": m.down_best_bid,
                "spread": m.spread,
                "price_to_beat": m.price_to_beat,
                "current_price": m.current_price
                ,"captured_at_ms": m.captured_at_ms
                ,"data_age_ms": age_ms
                ,"data_status": data_status
                ,"data_detail": m.data_detail
                ,"token_mapping_valid": m.token_mapping_valid
                ,"tick_size": m.tick_size
                ,"min_order_size": m.min_order_size
                ,"fee_rate_bps": m.fee_rate_bps
                ,"negative_risk": m.negative_risk
                ,"up_executable_depth_usd": m.up_executable_depth_usd
                ,"down_executable_depth_usd": m.down_executable_depth_usd
                ,"up_expected_fill_price": m.up_expected_fill_price
                ,"down_expected_fill_price": m.down_expected_fill_price
                ,"clock_drift_ms": m.clock_drift_ms
            })
        })
        .collect();
    axum::Json(json!({ "markets": updown_json }))
}

pub async fn get_health(State(state): State<AppState>) -> axum::Json<Value> {
    let now = chrono::Utc::now().timestamp_millis();
    let price = state.price.read().await;
    let scanner_at = *state.last_scan_at.read().await;
    let markets = state.updown_markets.read().await;
    let ready_markets = markets
        .iter()
        .filter(|market| {
            market.data_status == crate::web::state::DataStatus::Ready
                && now - market.captured_at_ms <= state.runtime.configured_max_data_age_ms as i64
        })
        .count();
    let stale_markets = markets
        .iter()
        .filter(|market| {
            now - market.captured_at_ms > state.runtime.configured_max_data_age_ms as i64
        })
        .count();
    let price_ready = price.source == "live"
        && now - price.timestamp <= state.runtime.configured_max_data_age_ms as i64;
    let scanner_ready =
        scanner_at > 0 && now - scanner_at <= state.runtime.configured_max_data_age_ms as i64;
    let overall = if price_ready && scanner_ready && ready_markets > 0 {
        "ready"
    } else if scanner_at == 0 || markets.is_empty() {
        "starting"
    } else {
        "degraded"
    };
    let (startup_state, startup_reason) = state.store.runtime_state().await.unwrap_or_else(|_| {
        (
            "unavailable".to_string(),
            "database query failed".to_string(),
        )
    });
    let open_incidents = state.store.open_incident_count().await.unwrap_or(-1);
    let reconciliation_ready = state
        .store
        .latest_reconciliation_ready()
        .await
        .unwrap_or(false);

    axum::Json(json!({
        "overall": overall,
        "max_data_age_ms": state.runtime.configured_max_data_age_ms,
        "price": {
            "status": if price_ready { "ready" } else { "stale_or_unavailable" },
            "source": price.source,
            "age_ms": now - price.timestamp
        },
        "scanner": {
            "status": if scanner_ready { "ready" } else { "stale_or_unavailable" },
            "age_ms": if scanner_at > 0 { now - scanner_at } else { -1 }
        },
        "clob_clock": {
            "status": if markets.iter().any(|market| market.clock_drift_ms.map(|drift| drift.abs() <= 5_000).unwrap_or(false)) {
                "ready"
            } else {
                "unavailable_or_drifted"
            },
            "max_observed_drift_ms": markets.iter().filter_map(|market| market.clock_drift_ms).map(i64::abs).max()
        },
        "markets": {
            "ready": ready_markets,
            "stale": stale_markets,
            "total": markets.len()
        },
        "database": {"status": "ready"}
        ,"production_control": {
            "startup_state": startup_state,
            "startup_reason": startup_reason,
            "open_incidents": open_incidents,
            "reconciliation_ready": reconciliation_ready,
            "live_switch_enabled": std::env::var("POLYMARKET_LIVE_TRADING_ENABLED")
                .map(|value| value == "I_UNDERSTAND_LIVE_TRADING")
                .unwrap_or(false)
        }
    }))
}

pub async fn get_signals(State(state): State<AppState>) -> axum::Json<Value> {
    let signal = state.last_signal.read().await;
    let signal_5m = state.last_signal_5m.read().await;
    let execution_note = state.execution_note.read().await;
    let execution_note_5m = state.execution_note_5m.read().await;
    let signal_json = match &*signal {
        Some(s) => json!({
            "direction": s.direction,
            "confidence": s.confidence,
            "timeframe": s.timeframe,
            "reason": s.reason,
            "execution_note": *execution_note,
            "timestamp": s.timestamp,
            "window_start_ts": s.window_start_ts,
            "runtime_mode": state.runtime.mode,
            "strategy_version": state.runtime.strategy_version
        }),
        None => json!(null),
    };
    let signal_5m_json = match &*signal_5m {
        Some(s) => json!({
            "direction": s.direction,
            "confidence": s.confidence,
            "timeframe": s.timeframe,
            "reason": s.reason,
            "execution_note": *execution_note_5m,
            "timestamp": s.timestamp,
            "window_start_ts": s.window_start_ts,
            "runtime_mode": state.runtime.mode,
            "strategy_version": state.runtime.strategy_version
        }),
        None => json!(null),
    };
    axum::Json(json!({ "signal": signal_json, "signal_5m": signal_5m_json }))
}

pub async fn get_trades(State(state): State<AppState>) -> axum::Json<Value> {
    let trades = state.trades.read().await;
    let trades_json: Vec<Value> = trades
        .iter()
        .map(|t| {
            json!({
                "timestamp": t.timestamp,
                "market_slug": t.market_slug,
                "timeframe": t.timeframe,
                "direction": t.direction,
                "entry_price": t.entry_price,
                "exit_price": t.exit_price,
                "shares": t.shares,
                "size_usd": t.size_usd,
                "fee_usd": t.fee_usd,
                "price_to_beat": t.price_to_beat,
                "end_ts": t.end_ts,
                "confidence": t.confidence,
                "edge": t.edge,
                "pnl": t.pnl,
                "status": t.status,
                "runtime_mode": state.runtime.mode,
                "strategy_version": state.runtime.strategy_version
            })
        })
        .collect();
    axum::Json(json!({ "trades": trades_json }))
}

pub async fn get_stats(State(state): State<AppState>) -> axum::Json<Value> {
    let stats = state.stats.read().await;
    let stats_5m = state.stats_5m.read().await;
    axum::Json(json!({
        "15m": stats_json(&stats),
        "5m": stats_json(&stats_5m),
        "current_capital": stats.current_capital + stats_5m.current_capital
    }))
}

pub async fn get_forward_report(State(state): State<AppState>) -> (StatusCode, axum::Json<Value>) {
    match state.forward_report().await {
        Ok(report) => (
            StatusCode::OK,
            axum::Json(
                serde_json::to_value(report)
                    .unwrap_or_else(|_| json!({"error": "serialization failed"})),
            ),
        ),
        Err(error) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({"error": error.to_string()})),
        ),
    }
}

fn stats_json(stats: &crate::web::state::StatsInfo) -> Value {
    json!({
        "total_trades": stats.total_trades,
        "wins": stats.wins,
        "losses": stats.losses,
        "win_rate": stats.win_rate,
        "total_pnl": stats.total_pnl,
        "avg_win": stats.avg_win,
        "avg_loss": stats.avg_loss,
        "profit_factor": stats.profit_factor,
        "max_drawdown": stats.max_drawdown,
        "current_capital": stats.current_capital
    })
}

pub async fn get_settings(State(state): State<AppState>) -> axum::Json<Value> {
    let settings = state.settings.read().await;
    axum::Json(json!({
        "capital": settings.capital,
        "max_order": settings.max_order,
        "timeframe": settings.timeframe,
        "auto_trade": settings.auto_trade,
        "min_edge": settings.min_edge,
        "max_entry_price": settings.max_entry_price,
        "risk_fraction": settings.risk_fraction,
        "runtime_mode": state.runtime.mode,
        "runtime_environment": state.runtime.environment,
        "strategy_version": state.runtime.strategy_version,
        "build_version": state.runtime.build_version,
        "configured_max_order_usd": state.runtime.configured_max_order_usd
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(settings): Json<Settings>,
) -> (StatusCode, axum::Json<Value>) {
    if !settings.respects(&state.runtime) {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({
                "status": "rejected",
                "reason": "settings exceed configured capital or risk limits"
            })),
        );
    }

    *state.settings.write().await = settings.clone();
    if state.trades.read().await.is_empty() {
        {
            let mut stats = state.stats.write().await;
            stats.current_capital = settings.capital;
            stats.peak_capital = settings.capital;
        }
        {
            let mut stats_5m = state.stats_5m.write().await;
            stats_5m.current_capital = settings.capital;
            stats_5m.peak_capital = settings.capital;
        }
    }
    if let Err(error) = state
        .persist(
            "settings_updated",
            json!({
                "capital": settings.capital,
                "max_order": settings.max_order,
                "auto_trade": settings.auto_trade
            }),
        )
        .await
    {
        state
            .halt_after_persistence_failure("settings update", &error)
            .await;
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            axum::Json(json!({"status": "error", "reason": "persistence failed"})),
        );
    }
    (StatusCode::OK, axum::Json(json!({ "status": "ok" })))
}
