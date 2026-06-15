use crate::config::{Config, RuntimeEnvironment, RuntimeMode};
use anyhow::{bail, Context, Result};
use serde::Deserialize;

const LIVE_CONFIRMATION: &str = "I_UNDERSTAND_LIVE_TRADING";

pub fn print_readiness(config: &Config) {
    println!("Polymarket production readiness");
    println!("  Build version:    {}", env!("CARGO_PKG_VERSION"));
    println!("  Strategy version: {}", config.runtime.strategy_version);
    println!("  Runtime mode:     {}", config.runtime.mode);
    println!("  Config source:    {}", config.source_label());
    println!("  Database path:    {}", config.storage.database_path);
    println!();

    readiness_check(
        config.runtime.mode == RuntimeMode::Paper,
        "Phase 0 runtime remains paper-only",
    );
    readiness_check(
        !config.dashboard.allow_live_mode_changes,
        "Phase 0 dashboard cannot enable live mode",
    );
    readiness_check(
        true,
        "Phase 1A durable dashboard state, audit log, backups, and intent uniqueness",
    );
    readiness_check(
        true,
        "Phase 1B normalized paper trades, capital ledger, and daily risk state",
    );
    readiness_check(
        true,
        "Phase 1C normalized market windows and orderbook snapshots",
    );
    readiness_check(
        true,
        "Phase 2 market freshness, outcome mapping, depth, fees, metadata, and clock checks",
    );
    readiness_check(false, "Phase 3 unified intent and central risk engine");
    readiness_check(false, "Phase 4 forward-test promotion metrics");
    readiness_check(false, "Phase 5 CLOB V2 dry-signed executor");
    readiness_check(false, "Phase 6 live order lifecycle");
    readiness_check(false, "Phase 7 startup reconciliation and recovery");
    readiness_check(false, "Phase 8 production deployment and observability");
    readiness_check(false, "Phase 9 manually approved canary");
    println!();
    println!("BLOCKED: live execution is intentionally unavailable.");
}

fn readiness_check(ok: bool, label: &str) {
    println!("[{}] {}", if ok { "OK" } else { "PENDING" }, label);
}

#[derive(Debug, Deserialize)]
struct GeoblockResponse {
    blocked: bool,
    country: String,
    region: String,
}

pub async fn run_preflight(config: &Config) -> Result<()> {
    println!("Polymarket production preflight (no orders will be placed)");
    if config.runtime.environment != RuntimeEnvironment::Production {
        bail!("production-check requires an explicit production configuration");
    }
    let mut failures = Vec::new();

    check_secret("POLYMARKET_PRIVATE_KEY", true, &mut failures);
    check_address("POLYMARKET_DEPOSIT_WALLET_ADDRESS", &mut failures);
    check_secret("POLYMARKET_CLOB_API_KEY", false, &mut failures);
    check_secret("POLYMARKET_CLOB_API_SECRET", false, &mut failures);
    check_secret("POLYMARKET_CLOB_PASSPHRASE", false, &mut failures);
    check_secret("POLYMARKET_RELAYER_API_KEY", false, &mut failures);
    check_address("POLYMARKET_RELAYER_API_KEY_ADDRESS", &mut failures);
    check_secret("POLYMARKET_RPC_URL", false, &mut failures);

    let live_enabled = std::env::var("POLYMARKET_LIVE_TRADING_ENABLED")
        .map(|value| value == LIVE_CONFIRMATION)
        .unwrap_or(false);
    print_check(
        live_enabled,
        "explicit live-trading confirmation",
        "set POLYMARKET_LIVE_TRADING_ENABLED=I_UNDERSTAND_LIVE_TRADING only after paper validation",
    );
    if !live_enabled {
        failures.push("live-trading confirmation is locked".to_string());
    }

    let client = reqwest::Client::new();
    match client
        .get("https://polymarket.com/api/geoblock")
        .send()
        .await
        .context("geoblock request failed")
    {
        Ok(response) => match response.json::<GeoblockResponse>().await {
            Ok(geo) => {
                let eligible = !geo.blocked;
                print_check(
                    eligible,
                    "geographic eligibility",
                    &format!("country={} region={}", geo.country, geo.region),
                );
                if !eligible {
                    failures.push("current server IP is blocked from order placement".to_string());
                }
            }
            Err(error) => failures.push(format!("could not parse geoblock response: {error}")),
        },
        Err(error) => failures.push(format!("could not verify geoblock: {error}")),
    }

    let clob_ok = client
        .get("https://clob.polymarket.com/time")
        .send()
        .await
        .map(|response| response.status().is_success())
        .unwrap_or(false);
    print_check(
        clob_ok,
        "CLOB V2 production endpoint",
        "https://clob.polymarket.com",
    );
    if !clob_ok {
        failures.push("CLOB V2 production endpoint unavailable".to_string());
    }

    println!();
    if failures.is_empty() {
        println!(
            "READY: prerequisites are present. Live order execution is still not implemented."
        );
        Ok(())
    } else {
        println!(
            "BLOCKED: {} production prerequisite(s) failed:",
            failures.len()
        );
        for failure in failures {
            println!("  - {failure}");
        }
        bail!("production preflight failed; no live orders are permitted")
    }
}

fn check_secret(name: &str, private_key: bool, failures: &mut Vec<String>) {
    let value = std::env::var(name).unwrap_or_default();
    let valid = if private_key {
        value
            .strip_prefix("0x")
            .map(|key| {
                key.len() == 64 && key.chars().all(|character| character.is_ascii_hexdigit())
            })
            .unwrap_or(false)
    } else {
        !value.trim().is_empty() && !value.contains("replace_me")
    };
    print_check(valid, name, "value is never printed");
    if !valid {
        failures.push(format!("{name} is missing or invalid"));
    }
}

fn check_address(name: &str, failures: &mut Vec<String>) {
    let value = std::env::var(name).unwrap_or_default();
    let valid = value
        .strip_prefix("0x")
        .map(|address| {
            address.len() == 40
                && address
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
        })
        .unwrap_or(false);
    print_check(valid, name, "Ethereum address format");
    if !valid {
        failures.push(format!("{name} is missing or invalid"));
    }
}

fn print_check(ok: bool, name: &str, detail: &str) {
    println!("[{}] {} ({})", if ok { "OK" } else { "FAIL" }, name, detail);
}
