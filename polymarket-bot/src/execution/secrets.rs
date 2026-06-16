use alloy_signer_local::PrivateKeySigner;
use anyhow::{bail, Context, Result};
use polymarket_client_sdk_v2::auth::{Credentials, ExposeSecret, SecretString, Signer, Uuid};
use polymarket_client_sdk_v2::types::Address;
use polymarket_client_sdk_v2::POLYGON;
use std::fmt;
use std::str::FromStr;

pub trait SecretProvider {
    fn required(&self, name: &str) -> Result<SecretString>;
}

#[derive(Debug, Clone, Copy)]
pub struct EnvSecretProvider;

impl SecretProvider for EnvSecretProvider {
    fn required(&self, name: &str) -> Result<SecretString> {
        let value = std::env::var(name).with_context(|| format!("{name} is required"))?;
        validate(name, &value)?;
        Ok(SecretString::from(value))
    }
}

#[derive(Clone)]
pub struct TradingSecrets {
    private_key: SecretString,
    deposit_wallet: SecretString,
    api_key: SecretString,
    api_secret: SecretString,
    passphrase: SecretString,
}

impl TradingSecrets {
    pub fn load(provider: &impl SecretProvider) -> Result<Self> {
        Ok(Self {
            private_key: provider.required("POLYMARKET_PRIVATE_KEY")?,
            deposit_wallet: provider.required("POLYMARKET_DEPOSIT_WALLET_ADDRESS")?,
            api_key: provider.required("POLYMARKET_CLOB_API_KEY")?,
            api_secret: provider.required("POLYMARKET_CLOB_API_SECRET")?,
            passphrase: provider.required("POLYMARKET_CLOB_PASSPHRASE")?,
        })
    }

    pub fn signer(&self) -> Result<PrivateKeySigner> {
        Ok(PrivateKeySigner::from_str(self.private_key.expose_secret())
            .context("invalid owner/session signer private key")?
            .with_chain_id(Some(POLYGON)))
    }

    pub fn funder(&self) -> Result<Address> {
        Address::from_str(self.deposit_wallet.expose_secret())
            .context("invalid deposit wallet address")
    }

    pub fn credentials(&self) -> Result<Credentials> {
        Ok(Credentials::new(
            Uuid::from_str(self.api_key.expose_secret()).context("invalid CLOB API key UUID")?,
            self.api_secret.expose_secret().to_string(),
            self.passphrase.expose_secret().to_string(),
        ))
    }
}

impl fmt::Debug for TradingSecrets {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TradingSecrets")
            .field("private_key", &"[REDACTED]")
            .field("deposit_wallet", &"[REDACTED]")
            .field("api_key", &"[REDACTED]")
            .field("api_secret", &"[REDACTED]")
            .field("passphrase", &"[REDACTED]")
            .finish()
    }
}

fn validate(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() || value.contains("replace_me") {
        bail!("{name} is missing or still contains a placeholder");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    struct FakeProvider(HashMap<&'static str, &'static str>);

    impl SecretProvider for FakeProvider {
        fn required(&self, name: &str) -> Result<SecretString> {
            self.0
                .get(name)
                .map(|value| SecretString::from((*value).to_string()))
                .ok_or_else(|| anyhow::anyhow!("{name} missing"))
        }
    }

    #[test]
    fn debug_output_is_fully_redacted() {
        let provider = FakeProvider(HashMap::from([
            ("POLYMARKET_PRIVATE_KEY", "VALUE_A1"),
            ("POLYMARKET_DEPOSIT_WALLET_ADDRESS", "VALUE_B2"),
            ("POLYMARKET_CLOB_API_KEY", "VALUE_C3"),
            ("POLYMARKET_CLOB_API_SECRET", "VALUE_D4"),
            ("POLYMARKET_CLOB_PASSPHRASE", "VALUE_E5"),
        ]));
        let secrets = TradingSecrets::load(&provider).unwrap();
        let debug = format!("{secrets:?}");
        for value in ["VALUE_A1", "VALUE_B2", "VALUE_C3", "VALUE_D4", "VALUE_E5"] {
            assert!(!debug.contains(value));
        }
        assert!(debug.contains("[REDACTED]"));
    }
}
