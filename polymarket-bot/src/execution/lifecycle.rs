use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

pub const CANARY_CONFIRMATION: &str = "AUTHORIZE_ONE_LIVE_CANARY";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderState {
    IntentPersisted,
    Signed,
    Submitted,
    PartiallyFilled,
    Filled,
    CancelPending,
    Cancelled,
    Rejected,
    UnknownRemote,
}

impl OrderState {
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Filled | Self::Cancelled | Self::Rejected)
    }

    pub fn can_transition_to(self, next: Self) -> bool {
        if self == next {
            return true;
        }
        matches!(
            (self, next),
            (Self::IntentPersisted, Self::Signed | Self::Rejected)
                | (
                    Self::Signed,
                    Self::Submitted
                        | Self::CancelPending
                        | Self::PartiallyFilled
                        | Self::Filled
                        | Self::Cancelled
                        | Self::Rejected
                        | Self::UnknownRemote
                )
                | (
                    Self::Submitted,
                    Self::PartiallyFilled
                        | Self::Filled
                        | Self::CancelPending
                        | Self::Cancelled
                        | Self::Rejected
                        | Self::UnknownRemote
                )
                | (
                    Self::PartiallyFilled,
                    Self::Filled | Self::CancelPending | Self::Cancelled | Self::UnknownRemote
                )
                | (
                    Self::CancelPending,
                    Self::Submitted
                        | Self::Cancelled
                        | Self::PartiallyFilled
                        | Self::Filled
                        | Self::UnknownRemote
                )
                | (
                    Self::UnknownRemote,
                    Self::Submitted
                        | Self::PartiallyFilled
                        | Self::Filled
                        | Self::CancelPending
                        | Self::Cancelled
                        | Self::Rejected
                )
        )
    }

    pub fn validate_transition(self, next: Self) -> Result<()> {
        if !self.can_transition_to(next) {
            bail!("invalid order-state transition: {self} -> {next}");
        }
        Ok(())
    }
}

impl fmt::Display for OrderState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = serde_json::to_value(self).map_err(|_| fmt::Error)?;
        formatter.write_str(value.as_str().ok_or(fmt::Error)?)
    }
}

impl FromStr for OrderState {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        Ok(serde_json::from_value(serde_json::Value::String(
            value.to_string(),
        ))?)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRecord {
    pub client_key: String,
    pub market_slug: String,
    pub token_id: String,
    pub side: String,
    pub requested_price: String,
    pub requested_size: String,
    pub state: OrderState,
    pub clob_order_id: Option<String>,
    pub filled_size: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconciliationReport {
    pub ready: bool,
    pub remote_checked: bool,
    pub local_non_terminal_orders: i64,
    pub mismatch_count: i64,
    pub reason: String,
}

pub fn validate_canary_authorization(
    confirmation: &str,
    requested_max_usd: f64,
    configured_max_usd: f64,
    promotion_ready: bool,
    reconciliation_ready: bool,
    live_switch_enabled: bool,
) -> Result<()> {
    if confirmation != CANARY_CONFIRMATION {
        bail!("manual canary confirmation phrase is invalid");
    }
    if !requested_max_usd.is_finite()
        || requested_max_usd <= 0.0
        || requested_max_usd > configured_max_usd
    {
        bail!("canary amount exceeds the configured order ceiling");
    }
    if !promotion_ready {
        bail!("forward-test promotion gates have not passed");
    }
    if !reconciliation_ready {
        bail!("a successful remote reconciliation is required");
    }
    if !live_switch_enabled {
        bail!("explicit live-trading environment switch is locked");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{validate_canary_authorization, OrderState, CANARY_CONFIRMATION};

    #[test]
    fn accepts_idempotent_and_valid_lifecycle_transitions() {
        assert!(OrderState::IntentPersisted
            .validate_transition(OrderState::Signed)
            .is_ok());
        assert!(OrderState::Signed
            .validate_transition(OrderState::Submitted)
            .is_ok());
        assert!(OrderState::Submitted
            .validate_transition(OrderState::PartiallyFilled)
            .is_ok());
        assert!(OrderState::PartiallyFilled
            .validate_transition(OrderState::Filled)
            .is_ok());
        assert!(OrderState::Filled
            .validate_transition(OrderState::Filled)
            .is_ok());
    }

    #[test]
    fn rejects_regression_from_terminal_state() {
        assert!(OrderState::Filled
            .validate_transition(OrderState::Submitted)
            .is_err());
        assert!(OrderState::Cancelled
            .validate_transition(OrderState::PartiallyFilled)
            .is_err());
        assert!(OrderState::Rejected
            .validate_transition(OrderState::Submitted)
            .is_err());
    }

    #[test]
    fn supports_fok_rejection_cancellation_and_partial_evidence() {
        assert!(OrderState::Signed
            .validate_transition(OrderState::Rejected)
            .is_ok());
        assert!(OrderState::Signed
            .validate_transition(OrderState::Cancelled)
            .is_ok());
        assert!(OrderState::Submitted
            .validate_transition(OrderState::Cancelled)
            .is_ok());
        assert!(OrderState::Submitted
            .validate_transition(OrderState::PartiallyFilled)
            .is_ok());
        assert!(OrderState::PartiallyFilled
            .validate_transition(OrderState::CancelPending)
            .is_ok());
        assert!(OrderState::CancelPending
            .validate_transition(OrderState::Cancelled)
            .is_ok());
    }

    #[test]
    fn canary_authorization_requires_every_gate() {
        assert!(
            validate_canary_authorization(CANARY_CONFIRMATION, 0.10, 0.10, true, true, true)
                .is_ok()
        );
        assert!(
            validate_canary_authorization(CANARY_CONFIRMATION, 0.10, 0.10, false, true, true)
                .is_err()
        );
        assert!(
            validate_canary_authorization(CANARY_CONFIRMATION, 0.11, 0.10, true, true, true)
                .is_err()
        );
    }
}
