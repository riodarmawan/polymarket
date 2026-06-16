use crate::engine::risk::ExecutionIntent;
use crate::execution::lifecycle::{OrderRecord, OrderState};
use crate::execution::secrets::{EnvSecretProvider, TradingSecrets};
use crate::storage::dashboard::DashboardStore;
use alloy_signer_local::PrivateKeySigner;
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use polymarket_client_sdk_v2::clob::types::request::{
    BalanceAllowanceRequest, OrderBookSummaryRequest,
};
use polymarket_client_sdk_v2::clob::types::{
    AssetType, OrderStatusType, OrderType, Side, SignatureType,
};
use polymarket_client_sdk_v2::clob::{Client, Config};
use polymarket_client_sdk_v2::types::{Decimal, U256};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::time::Duration;

pub const CANCEL_ALL_CONFIRMATION: &str = "CANCEL_ALL_OPEN_ORDERS";
pub const SUBMIT_CANARY_CONFIRMATION: &str = "SUBMIT_AUTHORIZED_CANARY";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveOrderRequest {
    pub client_key: String,
    pub market_slug: String,
    pub token_id: String,
    pub price: String,
    pub size: String,
    pub order_usd: f64,
}

impl LiveOrderRequest {
    pub fn from_intent(intent: &ExecutionIntent) -> Result<Self> {
        let price = intent.worst_allowed_price.as_f64();
        if price <= 0.0 {
            bail!("worst allowed price must be positive");
        }
        let order_usd = intent.requested_usd.as_f64();
        Ok(Self {
            client_key: intent.client_order_key.clone(),
            market_slug: intent.market_slug.clone(),
            token_id: intent.token_id.clone(),
            price: price.to_string(),
            size: (order_usd / price).to_string(),
            order_usd,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PreSubmitValidation {
    pub collateral_balance: String,
    pub allowance_contracts: usize,
    pub min_order_size: String,
    pub tick_size: String,
    pub neg_risk: bool,
    pub fee_rate_bps: u32,
    pub executable_ask_size: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionOutcome {
    pub success: bool,
    pub order_id: String,
    pub state: OrderState,
    pub filled_size: String,
    pub status: String,
    pub pre_submit: Option<PreSubmitValidation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelAllOutcome {
    pub canceled: Vec<String>,
    pub not_canceled: usize,
}

#[async_trait]
pub trait OrderExecutor: Send + Sync {
    async fn submit_fok(&self, request: &LiveOrderRequest) -> Result<SubmissionOutcome>;
    async fn cancel_all(&self) -> Result<CancelAllOutcome>;
}

#[derive(Debug, Clone)]
pub struct SdkOrderExecutor {
    heartbeat_interval_secs: u64,
    max_fee_rate_bps: u64,
    min_balance_reserve_usd: f64,
}

impl SdkOrderExecutor {
    pub fn new(
        heartbeat_interval_secs: u64,
        max_fee_rate_bps: u64,
        min_balance_reserve_usd: f64,
    ) -> Self {
        Self {
            heartbeat_interval_secs,
            max_fee_rate_bps,
            min_balance_reserve_usd,
        }
    }
}

#[async_trait]
impl OrderExecutor for SdkOrderExecutor {
    async fn submit_fok(&self, request: &LiveOrderRequest) -> Result<SubmissionOutcome> {
        let auth = AuthMaterial::from_environment()?;
        let signer = auth.signer()?;
        let client = auth.client(&signer, self.heartbeat_interval_secs).await?;
        let token_id = U256::from_str(&request.token_id).context("invalid token ID")?;
        let price = Decimal::from_str(&request.price).context("invalid order price")?;
        let size = Decimal::from_str(&request.size).context("invalid order size")?;
        let requested_usd =
            Decimal::from_str(&request.order_usd.to_string()).context("invalid order USD")?;
        let required_balance = requested_usd
            + Decimal::from_str(&self.min_balance_reserve_usd.to_string())
                .context("invalid configured balance reserve")?;
        let balance = client
            .balance_allowance(
                BalanceAllowanceRequest::builder()
                    .asset_type(AssetType::Collateral)
                    .build(),
            )
            .await?;
        if balance.balance < required_balance {
            bail!("insufficient collateral balance after configured reserve");
        }
        if balance.allowances.is_empty() {
            bail!("no collateral allowances are available");
        }
        for allowance in balance.allowances.values() {
            let allowance =
                Decimal::from_str(allowance).context("invalid collateral allowance value")?;
            if allowance < required_balance {
                bail!("a required collateral allowance is below the requested amount and reserve");
            }
        }
        let book = client
            .order_book(
                &OrderBookSummaryRequest::builder()
                    .token_id(token_id)
                    .build(),
            )
            .await?;
        if size < book.min_order_size {
            bail!("order size is below current market minimum");
        }
        let executable_ask_size: Decimal = book
            .asks
            .iter()
            .filter(|level| level.price <= price)
            .map(|level| level.size)
            .sum();
        if executable_ask_size < size {
            bail!("insufficient executable ask depth at the worst allowed price");
        }
        let tick = client.tick_size(token_id).await?;
        let neg_risk = client.neg_risk(token_id).await?;
        let fee = client.fee_rate_bps(token_id).await?;
        if u64::from(fee.base_fee) > self.max_fee_rate_bps {
            bail!("current fee rate exceeds configured maximum");
        }
        let validation = PreSubmitValidation {
            collateral_balance: balance.balance.to_string(),
            allowance_contracts: balance.allowances.len(),
            min_order_size: book.min_order_size.to_string(),
            tick_size: tick.minimum_tick_size.to_string(),
            neg_risk: neg_risk.neg_risk,
            fee_rate_bps: fee.base_fee,
            executable_ask_size: executable_ask_size.to_string(),
        };
        let order = client
            .limit_order()
            .token_id(token_id)
            .side(Side::Buy)
            .price(price)
            .size(size)
            .order_type(OrderType::FOK)
            .build()
            .await?;
        let signed = client.sign(&signer, order).await?;
        let response = client.post_order(signed).await?;
        let state = match &response.status {
            OrderStatusType::Matched => OrderState::Filled,
            OrderStatusType::Canceled => OrderState::Cancelled,
            OrderStatusType::Live | OrderStatusType::Delayed | OrderStatusType::Unmatched => {
                OrderState::Submitted
            }
            _ => OrderState::UnknownRemote,
        };
        Ok(SubmissionOutcome {
            success: response.success,
            order_id: response.order_id,
            state: if response.success {
                state
            } else {
                OrderState::Rejected
            },
            filled_size: response.taking_amount.to_string(),
            status: response.status.to_string(),
            pre_submit: Some(validation),
        })
    }

    async fn cancel_all(&self) -> Result<CancelAllOutcome> {
        let auth = AuthMaterial::from_environment()?;
        let signer = auth.signer()?;
        let client = auth.client(&signer, self.heartbeat_interval_secs).await?;
        let response = client.cancel_all_orders().await?;
        Ok(CancelAllOutcome {
            canceled: response.canceled,
            not_canceled: response.not_canceled.len(),
        })
    }
}

pub async fn execute_authorized_canary<E: OrderExecutor>(
    store: &DashboardStore,
    executor: &E,
    authorization_id: &str,
    intent: &ExecutionIntent,
) -> Result<SubmissionOutcome> {
    let request = LiveOrderRequest::from_intent(intent)?;
    let record = OrderRecord {
        client_key: request.client_key.clone(),
        market_slug: request.market_slug.clone(),
        token_id: request.token_id.clone(),
        side: "BUY".to_string(),
        requested_price: request.price.clone(),
        requested_size: request.size.clone(),
        state: OrderState::IntentPersisted,
        clob_order_id: None,
        filled_size: "0".to_string(),
        updated_at: Utc::now().to_rfc3339(),
    };
    if !store
        .create_order_and_reserve(&record, request.order_usd, serde_json::to_value(&request)?)
        .await?
    {
        bail!("order already exists for this deterministic client key");
    }
    if !store
        .consume_canary_authorization(authorization_id, &request.client_key, request.order_usd)
        .await?
    {
        store
            .transition_order(
                &request.client_key,
                OrderState::Rejected,
                None,
                "0",
                serde_json::json!({"reason": "invalid_or_consumed_canary_authorization"}),
            )
            .await?;
        store
            .update_capital_reservation(&request.client_key, "released")
            .await?;
        bail!("canary authorization is invalid, expired, mismatched, or already consumed");
    }
    store
        .transition_order(
            &request.client_key,
            OrderState::Signed,
            None,
            "0",
            serde_json::json!({"submitted": false}),
        )
        .await?;

    match executor.submit_fok(&request).await {
        Ok(outcome) => {
            store
                .transition_order(
                    &request.client_key,
                    outcome.state,
                    Some(&outcome.order_id),
                    &outcome.filled_size,
                    serde_json::to_value(&outcome)?,
                )
                .await?;
            let reservation_status = match outcome.state {
                OrderState::Filled => "filled",
                OrderState::Cancelled | OrderState::Rejected => "released",
                _ => "held",
            };
            store
                .update_capital_reservation(&request.client_key, reservation_status)
                .await?;
            Ok(outcome)
        }
        Err(error) => {
            store
                .transition_order(
                    &request.client_key,
                    OrderState::UnknownRemote,
                    None,
                    "0",
                    serde_json::json!({"reason": "ambiguous_submit_failure"}),
                )
                .await?;
            Err(error.context(
                "submission outcome is ambiguous; authorization consumed and reconciliation required",
            ))
        }
    }
}

struct AuthMaterial {
    secrets: TradingSecrets,
    host: String,
}

impl AuthMaterial {
    fn from_environment() -> Result<Self> {
        Ok(Self {
            secrets: TradingSecrets::load(&EnvSecretProvider)?,
            host: std::env::var("POLYMARKET_CLOB_V2_URL")
                .unwrap_or_else(|_| "https://clob-v2.polymarket.com".to_string()),
        })
    }

    fn signer(&self) -> Result<PrivateKeySigner> {
        self.secrets.signer()
    }

    async fn client(
        &self,
        signer: &PrivateKeySigner,
        heartbeat_interval_secs: u64,
    ) -> Result<
        Client<
            polymarket_client_sdk_v2::auth::state::Authenticated<
                polymarket_client_sdk_v2::auth::Normal,
            >,
        >,
    > {
        Ok(Client::new(
            &self.host,
            Config::builder()
                .use_server_time(true)
                .heartbeat_interval(Duration::from_secs(heartbeat_interval_secs.max(1)))
                .build(),
        )?
        .authentication_builder(signer)
        .credentials(self.secrets.credentials()?)
        .funder(self.secrets.funder()?)
        .signature_type(SignatureType::Poly1271)
        .authenticate()
        .await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::risk::{Direction, Fixed};
    use tempfile::TempDir;

    enum FakeOutcome {
        Success(SubmissionOutcome),
        Failure(&'static str),
    }

    struct FakeExecutor {
        outcome: FakeOutcome,
    }

    #[async_trait]
    impl OrderExecutor for FakeExecutor {
        async fn submit_fok(&self, _request: &LiveOrderRequest) -> Result<SubmissionOutcome> {
            match &self.outcome {
                FakeOutcome::Success(outcome) => Ok(outcome.clone()),
                FakeOutcome::Failure(message) => bail!(*message),
            }
        }

        async fn cancel_all(&self) -> Result<CancelAllOutcome> {
            Ok(CancelAllOutcome {
                canceled: vec![],
                not_canceled: 0,
            })
        }
    }

    fn intent(client_key: &str) -> ExecutionIntent {
        ExecutionIntent {
            client_order_key: client_key.to_string(),
            market_slug: "btc-updown-5m-test".to_string(),
            token_id: "123".to_string(),
            timeframe: "5m".to_string(),
            direction: Direction::Up,
            strategy_version: "test".to_string(),
            signal_timestamp_ms: 1,
            market_snapshot_timestamp_ms: 1,
            requested_usd: Fixed::from_f64(0.10).unwrap(),
            worst_allowed_price: Fixed::from_f64(0.50).unwrap(),
            expected_fill_price: Fixed::from_f64(0.50).unwrap(),
            expected_fee_usd: Fixed::from_f64(0.0).unwrap(),
            model_margin: Fixed::from_f64(0.20).unwrap(),
            expected_shares: Fixed::from_f64(0.20).unwrap(),
            risk_checks: vec![],
        }
    }

    #[tokio::test]
    async fn canary_is_idempotent_and_persists_success() {
        let temp = TempDir::new().unwrap();
        let store = DashboardStore::open(&temp.path().join("canary.db"))
            .await
            .unwrap();
        let authorization = store
            .issue_canary_authorization("client-1", 0.10, Utc::now().timestamp_millis() + 60_000)
            .await
            .unwrap();
        let executor = FakeExecutor {
            outcome: FakeOutcome::Success(SubmissionOutcome {
                success: true,
                order_id: "remote-1".to_string(),
                state: OrderState::Filled,
                filled_size: "0.20".to_string(),
                status: "MATCHED".to_string(),
                pre_submit: None,
            }),
        };
        assert!(
            execute_authorized_canary(&store, &executor, &authorization, &intent("client-1"))
                .await
                .is_ok()
        );
        assert!(
            execute_authorized_canary(&store, &executor, &authorization, &intent("client-1"))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn ambiguous_failure_halts_retry() {
        let temp = TempDir::new().unwrap();
        let store = DashboardStore::open(&temp.path().join("ambiguous.db"))
            .await
            .unwrap();
        let authorization = store
            .issue_canary_authorization("client-2", 0.10, Utc::now().timestamp_millis() + 60_000)
            .await
            .unwrap();
        let executor = FakeExecutor {
            outcome: FakeOutcome::Failure("timeout"),
        };
        assert!(
            execute_authorized_canary(&store, &executor, &authorization, &intent("client-2"))
                .await
                .is_err()
        );
        assert_eq!(store.local_non_terminal_order_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn restart_preserves_ambiguous_order_and_blocks_duplicate_retry() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("restart-ambiguous.db");
        let store = DashboardStore::open(&path).await.unwrap();
        let authorization = store
            .issue_canary_authorization(
                "restart-client",
                0.10,
                Utc::now().timestamp_millis() + 60_000,
            )
            .await
            .unwrap();
        let executor = FakeExecutor {
            outcome: FakeOutcome::Failure("timeout after remote acceptance"),
        };
        assert!(execute_authorized_canary(
            &store,
            &executor,
            &authorization,
            &intent("restart-client"),
        )
        .await
        .is_err());
        drop(store);

        let reopened = DashboardStore::open(&path).await.unwrap();
        assert_eq!(
            reopened.order_state("restart-client").await.unwrap(),
            Some(OrderState::UnknownRemote)
        );
        assert!(execute_authorized_canary(
            &reopened,
            &executor,
            &authorization,
            &intent("restart-client"),
        )
        .await
        .is_err());
        assert_eq!(reopened.local_non_terminal_order_count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn rejected_and_cancelled_fok_release_reserved_capital() {
        for (client_key, state) in [
            ("rejected-client", OrderState::Rejected),
            ("cancelled-client", OrderState::Cancelled),
        ] {
            let temp = TempDir::new().unwrap();
            let store = DashboardStore::open(&temp.path().join(format!("{client_key}.db")))
                .await
                .unwrap();
            let authorization = store
                .issue_canary_authorization(
                    client_key,
                    0.10,
                    Utc::now().timestamp_millis() + 60_000,
                )
                .await
                .unwrap();
            let executor = FakeExecutor {
                outcome: FakeOutcome::Success(SubmissionOutcome {
                    success: state != OrderState::Rejected,
                    order_id: format!("remote-{client_key}"),
                    state,
                    filled_size: "0".to_string(),
                    status: state.to_string(),
                    pre_submit: None,
                }),
            };
            execute_authorized_canary(&store, &executor, &authorization, &intent(client_key))
                .await
                .unwrap();
            assert_eq!(store.held_capital_usd().await.unwrap(), 0.0);
            assert_eq!(store.order_state(client_key).await.unwrap(), Some(state));
        }
    }
}
