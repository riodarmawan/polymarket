use crate::web::state::AppState;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use serde_json::json;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut _receiver) = socket.split();

    let state_clone = state.clone();
    let mut send_task = tokio::spawn(async move {
        let mut last_price = 0.0;
        let mut last_signal_time = 0i64;
        let mut last_signal_5m_time = 0i64;

        loop {
            // Send price updates
            let price = state_clone.price.read().await;
            if price.price != last_price {
                let msg = json!({
                    "type": "price",
                    "price": price.price,
                    "change_pct": price.change_pct,
                    "timestamp": price.timestamp,
                    "source": price.source
                });
                if sender
                    .send(Message::Text(msg.to_string().into()))
                    .await
                    .is_err()
                {
                    break;
                }
                last_price = price.price;
            }
            drop(price);

            // Send signal updates
            let signal_time = state_clone.last_signal_time.read().await;
            if *signal_time != last_signal_time {
                let signal = state_clone.last_signal.read().await;
                if let Some(s) = &*signal {
                    let execution_note = state_clone.execution_note.read().await;
                    let msg = json!({
                        "type": "signal",
                        "direction": s.direction,
                        "confidence": s.confidence,
                        "timeframe": s.timeframe,
                        "reason": s.reason,
                        "execution_note": *execution_note,
                        "timestamp": s.timestamp
                        ,"window_start_ts": s.window_start_ts
                    });
                    if sender
                        .send(Message::Text(msg.to_string().into()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                last_signal_time = *signal_time;
            }
            drop(signal_time);

            let signal_5m_time = state_clone.last_signal_5m_time.read().await;
            if *signal_5m_time != last_signal_5m_time {
                let signal = state_clone.last_signal_5m.read().await;
                if let Some(s) = &*signal {
                    let execution_note = state_clone.execution_note_5m.read().await;
                    let msg = json!({
                        "type": "signal_5m",
                        "direction": s.direction,
                        "confidence": s.confidence,
                        "timeframe": s.timeframe,
                        "reason": s.reason,
                        "execution_note": *execution_note,
                        "timestamp": s.timestamp
                        ,"window_start_ts": s.window_start_ts
                    });
                    if sender.send(Message::Text(msg.to_string().into())).await.is_err() {
                        break;
                    }
                }
                last_signal_5m_time = *signal_5m_time;
            }
            drop(signal_5m_time);

            // Send updown market updates
            let updown = state_clone.updown_markets.read().await;
            let updown_json: Vec<serde_json::Value> = updown
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
            let msg = json!({
                "type": "updown",
                "markets": updown_json
            });
            if sender
                .send(Message::Text(msg.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
            drop(updown);

            // Send trade updates
            let trades = state_clone.trades.read().await;
            let trades_json: Vec<serde_json::Value> = trades
                .iter()
                .rev()
                .take(10)
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
                        "confidence": t.confidence,
                        "edge": t.edge,
                        "pnl": t.pnl,
                        "status": t.status
                    })
                })
                .collect();
            let msg = json!({
                "type": "trades",
                "trades": trades_json
            });
            if sender
                .send(Message::Text(msg.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
            drop(trades);

            // Send stats updates
            let stats = state_clone.stats.read().await;
            let stats_5m = state_clone.stats_5m.read().await;
            let msg = json!({
                "type": "stats",
                "15m": stats_to_json(&stats),
                "5m": stats_to_json(&stats_5m),
                "current_capital": stats.current_capital + stats_5m.current_capital
            });
            if sender
                .send(Message::Text(msg.to_string().into()))
                .await
                .is_err()
            {
                break;
            }
            drop(stats);
            drop(stats_5m);

            tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;
        }
    });

    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(_msg)) = _receiver.next().await {
            // Client messages ignored for now
        }
    });

    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}

fn stats_to_json(stats: &crate::web::state::StatsInfo) -> serde_json::Value {
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
