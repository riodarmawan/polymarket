use crate::execution::lifecycle::OrderState;
use crate::execution::secrets::{EnvSecretProvider, TradingSecrets};
use crate::storage::dashboard::DashboardStore;
use anyhow::{bail, Result};
use futures::StreamExt;
use polymarket_client_sdk_v2::clob::types::OrderStatusType;
use polymarket_client_sdk_v2::clob::ws::{Client, WsMessage};
use polymarket_client_sdk_v2::types::B256;

pub async fn monitor(store: &DashboardStore, max_events: usize) -> Result<usize> {
    let secrets = TradingSecrets::load(&EnvSecretProvider)?;
    let client = Client::default().authenticate(secrets.credentials()?, secrets.funder()?)?;
    let mut stream = std::pin::pin!(client.subscribe_user_events(Vec::<B256>::new())?);
    let mut events = 0;
    store
        .audit_event(
            "user_websocket_connected",
            "live_control_plane",
            "n/a",
            serde_json::json!({"authenticated": true}),
        )
        .await?;

    while let Some(event) = stream.next().await {
        events += 1;
        match event {
            Ok(WsMessage::Order(order)) => {
                let mut next = order
                    .status
                    .as_ref()
                    .map(map_order_status)
                    .unwrap_or(OrderState::UnknownRemote);
                if order
                    .size_matched
                    .zip(order.original_size)
                    .is_some_and(|(matched, original)| {
                        matched > Default::default() && matched < original
                    })
                {
                    next = OrderState::PartiallyFilled;
                }
                let known = store
                    .transition_order_by_remote_id(
                        &order.id,
                        next,
                        &order
                            .size_matched
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "0".to_string()),
                        serde_json::json!({
                            "source": "authenticated_user_websocket",
                            "event_type": "order",
                            "status": order.status.as_ref().map(ToString::to_string)
                        }),
                    )
                    .await?;
                if !known {
                    halt_unknown_order(store, &order.id).await?;
                }
            }
            Ok(WsMessage::Trade(trade)) => {
                let transaction_hash = trade.transaction_hash.as_ref().map(ToString::to_string);
                if let Some(order_id) = trade.taker_order_id {
                    let known = store
                        .transition_order_by_remote_id(
                            &order_id,
                            OrderState::Filled,
                            &trade.size.to_string(),
                            serde_json::json!({
                                "source": "authenticated_user_websocket",
                                "event_type": "trade",
                                "trade_id": trade.id
                            }),
                        )
                        .await?;
                    if !known {
                        halt_unknown_order(store, &order_id).await?;
                    } else {
                        store
                            .record_live_fill(
                                &format!("{}:{order_id}", trade.id),
                                &order_id,
                                &trade.asset_id.to_string(),
                                &trade.price.to_string(),
                                &trade.size.to_string(),
                                transaction_hash.as_deref(),
                            )
                            .await?;
                    }
                }
                for maker in trade.maker_orders {
                    let known = store
                        .transition_order_by_remote_id(
                            &maker.order_id,
                            OrderState::Filled,
                            &maker.matched_amount.to_string(),
                            serde_json::json!({
                                "source": "authenticated_user_websocket",
                                "event_type": "trade",
                                "trade_id": trade.id
                            }),
                        )
                        .await?;
                    if known {
                        store
                            .record_live_fill(
                                &format!("{}:{}", trade.id, maker.order_id),
                                &maker.order_id,
                                &maker.asset_id.to_string(),
                                &maker.price.to_string(),
                                &maker.matched_amount.to_string(),
                                transaction_hash.as_deref(),
                            )
                            .await?;
                    }
                }
            }
            Ok(_) => {}
            Err(error) => {
                store
                    .set_runtime_state("halted", "authenticated user WebSocket failed")
                    .await?;
                store
                    .open_incident(
                        "user-websocket-failed",
                        "user_websocket_failure",
                        serde_json::json!({"message": error.to_string()}),
                    )
                    .await?;
                bail!("authenticated user WebSocket failed: {error}");
            }
        }
        if max_events > 0 && events >= max_events {
            break;
        }
    }
    if max_events == 0 || events < max_events {
        store
            .set_runtime_state("halted", "authenticated user WebSocket disconnected")
            .await?;
        store
            .open_incident(
                "user-websocket-disconnected",
                "user_websocket_failure",
                serde_json::json!({}),
            )
            .await?;
        bail!("authenticated user WebSocket disconnected");
    }
    Ok(events)
}

fn map_order_status(status: &OrderStatusType) -> OrderState {
    match status {
        OrderStatusType::Live | OrderStatusType::Delayed | OrderStatusType::Unmatched => {
            OrderState::Submitted
        }
        OrderStatusType::Matched => OrderState::Filled,
        OrderStatusType::Canceled => OrderState::Cancelled,
        _ => OrderState::UnknownRemote,
    }
}

async fn halt_unknown_order(store: &DashboardStore, order_id: &str) -> Result<()> {
    store
        .set_runtime_state("halted", "unknown remote order observed on user WebSocket")
        .await?;
    store
        .open_incident(
            &format!("unknown-remote-order-{order_id}"),
            "unknown_remote_order",
            serde_json::json!({"order_id": order_id}),
        )
        .await
}

#[cfg(test)]
mod tests {
    use super::map_order_status;
    use crate::execution::lifecycle::OrderState;
    use polymarket_client_sdk_v2::clob::types::OrderStatusType;

    #[test]
    fn maps_remote_order_status_fail_closed() {
        assert_eq!(
            map_order_status(&OrderStatusType::Live),
            OrderState::Submitted
        );
        assert_eq!(
            map_order_status(&OrderStatusType::Matched),
            OrderState::Filled
        );
        assert_eq!(
            map_order_status(&OrderStatusType::Canceled),
            OrderState::Cancelled
        );
    }
}
