use crate::web::state::AppState;
use crate::web::state::Settings;
use axum::extract::{Json, State};
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
            })
        })
        .collect();
    axum::Json(json!({ "markets": updown_json }))
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
            "timestamp": s.timestamp
            ,"window_start_ts": s.window_start_ts
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
            "timestamp": s.timestamp
            ,"window_start_ts": s.window_start_ts
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
                "status": t.status
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
        "risk_fraction": settings.risk_fraction
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(settings): Json<Settings>,
) -> axum::Json<Value> {
    *state.settings.write().await = settings.clone();
    if state.trades.read().await.is_empty() {
        let mut stats = state.stats.write().await;
        stats.current_capital = settings.capital;
        stats.peak_capital = settings.capital;
        let mut stats_5m = state.stats_5m.write().await;
        stats_5m.current_capital = settings.capital;
        stats_5m.peak_capital = settings.capital;
    }
    axum::Json(json!({ "status": "ok" }))
}
