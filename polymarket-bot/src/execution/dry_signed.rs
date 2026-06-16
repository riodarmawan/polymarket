use crate::execution::secrets::{EnvSecretProvider, TradingSecrets};
use anyhow::{Context, Result};
use polymarket_client_sdk_v2::clob::types::request::{
    BalanceAllowanceRequest, OrdersRequest, TradesRequest,
};
use polymarket_client_sdk_v2::clob::types::{AssetType, OrderType, Side, SignatureType};
use polymarket_client_sdk_v2::clob::{Client, Config};
use polymarket_client_sdk_v2::data::types::request::PositionsRequest;
use polymarket_client_sdk_v2::data::Client as DataClient;
use polymarket_client_sdk_v2::types::{Decimal, U256};
use serde::Serialize;
use std::str::FromStr;

#[derive(Debug, Serialize)]
pub struct DrySignedReport {
    pub ok: bool,
    pub submitted: bool,
    pub signature_type: u8,
    pub funder: String,
    pub token_id: String,
    pub price: String,
    pub size: String,
    pub order_type: String,
    pub signature_present: bool,
}

#[derive(Debug, Serialize)]
pub struct RemoteAccountSnapshot {
    pub remote_checked: bool,
    pub open_orders: usize,
    pub open_order_ids: Vec<String>,
    pub recent_trades: usize,
    pub trade_order_ids: Vec<String>,
    pub collateral_balance: String,
    pub allowance_contracts: usize,
    pub positions: Vec<RemotePositionSnapshot>,
}

#[derive(Debug, Serialize)]
pub struct RemotePositionSnapshot {
    pub token_id: String,
    pub condition_id: String,
    pub market_slug: String,
    pub size: String,
    pub average_price: String,
    pub redeemable: bool,
}

pub async fn create_from_environment(
    token_id: &str,
    price: &str,
    size: &str,
) -> Result<DrySignedReport> {
    let secrets = TradingSecrets::load(&EnvSecretProvider)?;
    let funder = secrets.funder()?;
    let signer = secrets.signer()?;
    let credentials = secrets.credentials()?;
    let token_id = U256::from_str(token_id).context("invalid token ID")?;
    let price_value = Decimal::from_str(price).context("invalid order price")?;
    let size_value = Decimal::from_str(size).context("invalid order size")?;
    let host = std::env::var("POLYMARKET_CLOB_V2_URL")
        .unwrap_or_else(|_| "https://clob-v2.polymarket.com".to_string());
    let client = Client::new(&host, Config::builder().use_server_time(true).build())?
        .authentication_builder(&signer)
        .credentials(credentials)
        .funder(funder)
        .signature_type(SignatureType::Poly1271)
        .authenticate()
        .await?;
    let order = client
        .limit_order()
        .token_id(token_id)
        .side(Side::Buy)
        .price(price_value)
        .size(size_value)
        .order_type(OrderType::FOK)
        .build()
        .await?;
    let signed = client.sign(&signer, order).await?;
    let payload = serde_json::to_value(&signed)?;

    let signature_present = payload.to_string().contains("signature");
    if !signature_present {
        anyhow::bail!("SDK returned a signed order without signature material");
    }

    Ok(DrySignedReport {
        ok: true,
        submitted: false,
        signature_type: 3,
        funder: funder.to_string(),
        token_id: token_id.to_string(),
        price: price.to_string(),
        size: size.to_string(),
        order_type: "FOK".to_string(),
        signature_present,
    })
}

pub async fn reconcile_from_environment() -> Result<RemoteAccountSnapshot> {
    let secrets = TradingSecrets::load(&EnvSecretProvider)?;
    let funder = secrets.funder()?;
    let signer = secrets.signer()?;
    let credentials = secrets.credentials()?;
    let host = std::env::var("POLYMARKET_CLOB_V2_URL")
        .unwrap_or_else(|_| "https://clob-v2.polymarket.com".to_string());
    let client = Client::new(&host, Config::builder().use_server_time(true).build())?
        .authentication_builder(&signer)
        .credentials(credentials)
        .funder(funder)
        .signature_type(SignatureType::Poly1271)
        .authenticate()
        .await?;

    let orders = client
        .orders(&OrdersRequest::builder().build(), None)
        .await?;
    let trades = client
        .trades(&TradesRequest::builder().build(), None)
        .await?;
    let balance = client
        .balance_allowance(
            BalanceAllowanceRequest::builder()
                .asset_type(AssetType::Collateral)
                .build(),
        )
        .await?;
    let positions = DataClient::default()
        .positions(&PositionsRequest::builder().user(funder).limit(500)?.build())
        .await?;

    Ok(RemoteAccountSnapshot {
        remote_checked: true,
        open_orders: orders.data.len(),
        open_order_ids: orders.data.iter().map(|order| order.id.clone()).collect(),
        recent_trades: trades.data.len(),
        trade_order_ids: trades
            .data
            .iter()
            .flat_map(|trade| {
                std::iter::once(trade.taker_order_id.clone()).chain(
                    trade
                        .maker_orders
                        .iter()
                        .map(|maker| maker.order_id.clone()),
                )
            })
            .collect(),
        collateral_balance: balance.balance.to_string(),
        allowance_contracts: balance.allowances.len(),
        positions: positions
            .into_iter()
            .map(|position| RemotePositionSnapshot {
                token_id: position.asset.to_string(),
                condition_id: position.condition_id.to_string(),
                market_slug: position.slug,
                size: position.size.to_string(),
                average_price: position.avg_price.to_string(),
                redeemable: position.redeemable,
            })
            .collect(),
    })
}
