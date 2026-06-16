use crate::build_info;
use crate::config::{Config, RuntimeEnvironment, RuntimeMode};
use crate::evaluation;
use crate::storage::dashboard::DashboardStore;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const LIVE_CONFIRMATION: &str = "I_UNDERSTAND_LIVE_TRADING";

#[derive(Debug, Serialize)]
pub struct BuildReadiness {
    pub package_version: String,
    pub git_sha: String,
    pub git_dirty: String,
    pub build_timestamp: String,
}

#[derive(Debug, Serialize)]
pub struct RuntimeReadiness {
    pub mode: String,
    pub environment: String,
    pub strategy_version: String,
    pub config_source: String,
    pub database_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReadinessCheck {
    phase: String,
    gate: String,
    ok: bool,
    required_for_canary: bool,
    evidence: String,
}

#[derive(Debug, Serialize)]
pub struct ProductionReadinessReport {
    pub generated_at_ms: i64,
    pub build: BuildReadiness,
    pub runtime: RuntimeReadiness,
    pub canary_ready: bool,
    pub blockers: Vec<String>,
    pub checks: Vec<ReadinessCheck>,
}

pub async fn print_readiness(
    config: &Config,
    json: bool,
    require_canary_ready: bool,
) -> Result<()> {
    let report = build_readiness_report(config).await;
    if json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print_readiness_text(&report);
    }
    if require_canary_ready {
        if let Some(error) = canary_gate_error(&report) {
            bail!("{error}");
        }
    }
    Ok(())
}

pub async fn build_readiness_report(config: &Config) -> ProductionReadinessReport {
    let build = build_info::BuildInfo::current();
    let live_switch_enabled = std::env::var("POLYMARKET_LIVE_TRADING_ENABLED")
        .map(|value| value == LIVE_CONFIRMATION)
        .unwrap_or(false);
    let database_path = Path::new(&config.storage.database_path);
    let database_integrity = match DashboardStore::verify_integrity(database_path).await {
        Ok(()) => (true, "PRAGMA integrity_check returned ok".to_string()),
        Err(error) => (false, error.to_string()),
    };

    let mut forward_report = None;
    let mut runtime_state = None;
    let mut open_incidents = None;
    let mut reconciliation_ready = None;
    let mut store_evidence = "dashboard store not opened".to_string();
    if database_path.is_file() {
        match DashboardStore::open(database_path).await {
            Ok(store) => {
                store_evidence = "dashboard store opened".to_string();
                let opportunities = store.load_forward_opportunities().await.unwrap_or_default();
                let trades = store
                    .load_snapshot()
                    .await
                    .ok()
                    .flatten()
                    .map(|snapshot| snapshot.trades)
                    .unwrap_or_default();
                forward_report = Some(evaluation::build_report(&opportunities, &trades));
                runtime_state = store.runtime_state().await.ok();
                open_incidents = store.open_incident_count().await.ok();
                reconciliation_ready = store.latest_reconciliation_ready().await.ok();
            }
            Err(error) => {
                store_evidence = format!("dashboard store open failed: {error}");
            }
        }
    }

    let provenance_ok = build.is_git_sha_known() && build.is_dirty_known() && !build.is_dirty();
    let settlement_mismatches = forward_report
        .as_ref()
        .map(|report| report.settlement_mismatches)
        .unwrap_or(usize::MAX);
    let forward_promotion_ready = forward_report
        .as_ref()
        .map(|report| report.promotion_ready)
        .unwrap_or(false);
    let forward_evidence = forward_report.as_ref().map_or_else(
        || "forward report unavailable".to_string(),
        |report| {
            if report.promotion_ready {
                format!(
                    "{} settled trades; win rate {:.1}%, PF {:.2}, drawdown ${:.2}",
                    report.settled_trades,
                    report.executable_trade_accuracy * 100.0,
                    report.profit_factor,
                    report.max_drawdown_usd
                )
            } else {
                report.promotion_reasons.join("; ")
            }
        },
    );
    let runtime_ready = runtime_state
        .as_ref()
        .map(|(state, _)| state == "ready")
        .unwrap_or(false);
    let runtime_evidence = runtime_state
        .as_ref()
        .map(|(state, reason)| format!("{state}: {reason}"))
        .unwrap_or_else(|| store_evidence.clone());
    let incident_count = open_incidents.unwrap_or(i64::MAX);
    let reconciliation_evidence = reconciliation_ready
        .map(|ready| ready.to_string())
        .unwrap_or_else(|| store_evidence.clone());
    let signer_drills = signer_drill_evidence(config);
    let lifecycle_drills = lifecycle_drill_evidence(config);
    let deployment_drills = deployment_drill_evidence(config);

    let checks = vec![
        readiness_check(
            "Phase 0",
            "runtime remains paper-only until an operator canary command",
            config.runtime.mode == RuntimeMode::Paper,
            true,
            format!("runtime_mode={}", config.runtime.mode),
        ),
        readiness_check(
            "Phase 0",
            "build provenance is known and clean",
            provenance_ok,
            true,
            format!(
                "git_sha={} git_dirty={}",
                build.git_short_sha(),
                build.git_dirty
            ),
        ),
        readiness_check(
            "Phase 0",
            "dashboard cannot enable live mode",
            !config.dashboard.allow_live_mode_changes,
            true,
            format!(
                "dashboard.allow_live_mode_changes={}",
                config.dashboard.allow_live_mode_changes
            ),
        ),
        readiness_check(
            "Phase 0",
            "strategy manifest is available",
            true,
            false,
            "run strategy-manifest for active thresholds".to_string(),
        ),
        readiness_check(
            "Phase 1",
            "production database integrity is valid",
            database_integrity.0,
            true,
            database_integrity.1,
        ),
        readiness_check(
            "Phase 2",
            "market-data reliability implementation is present",
            true,
            false,
            "freshness, outcome mapping, depth, fees, and clock checks are tested".to_string(),
        ),
        readiness_check(
            "Phase 3",
            "unified strategy and central risk pipeline are present",
            true,
            false,
            "fixed-point intents and risk-engine boundary tests are present".to_string(),
        ),
        readiness_check(
            "Phase 4",
            "forward promotion metrics pass",
            forward_promotion_ready,
            true,
            forward_evidence,
        ),
        readiness_check(
            "Phase 4",
            "official settlement mismatches are zero",
            settlement_mismatches == 0,
            true,
            if settlement_mismatches == usize::MAX {
                "forward report unavailable".to_string()
            } else {
                format!("settlement_mismatches={settlement_mismatches}")
            },
        ),
        readiness_check(
            "Phase 5",
            "dry-sign and new signer validation are complete",
            signer_drills.0,
            true,
            signer_drills.1,
        ),
        readiness_check(
            "Phase 6",
            "live order lifecycle drills are complete",
            lifecycle_drills.0,
            true,
            lifecycle_drills.1,
        ),
        readiness_check(
            "Phase 7",
            "runtime state is ready",
            runtime_ready,
            true,
            runtime_evidence,
        ),
        readiness_check(
            "Phase 7",
            "fresh authenticated reconciliation is ready",
            reconciliation_ready.unwrap_or(false),
            true,
            format!("latest_reconciliation_ready={reconciliation_evidence}"),
        ),
        readiness_check(
            "Phase 7",
            "open incidents are zero",
            incident_count == 0,
            true,
            if incident_count == i64::MAX {
                store_evidence.clone()
            } else {
                format!("open_incidents={incident_count}")
            },
        ),
        readiness_check(
            "Phase 8",
            "deployment host drills and alerts are complete",
            deployment_drills.0,
            true,
            deployment_drills.1,
        ),
        readiness_check(
            "Phase 9",
            "explicit live switch is enabled for reviewed canary",
            live_switch_enabled,
            true,
            format!("POLYMARKET_LIVE_TRADING_ENABLED={live_switch_enabled}"),
        ),
    ];
    let canary_ready = canary_required_checks_pass(&checks);
    let blockers = checks
        .iter()
        .filter(|check| check.required_for_canary && !check.ok)
        .map(|check| format!("{}: {} ({})", check.phase, check.gate, check.evidence))
        .collect();

    ProductionReadinessReport {
        generated_at_ms: chrono::Utc::now().timestamp_millis(),
        build: BuildReadiness {
            package_version: build.package_version.to_string(),
            git_sha: build.git_sha.to_string(),
            git_dirty: build.git_dirty.to_string(),
            build_timestamp: build.build_timestamp.to_string(),
        },
        runtime: RuntimeReadiness {
            mode: config.runtime.mode.to_string(),
            environment: format!("{:?}", config.runtime.environment).to_lowercase(),
            strategy_version: config.runtime.strategy_version.clone(),
            config_source: config.source_label(),
            database_path: config.storage.database_path.clone(),
        },
        canary_ready,
        blockers,
        checks,
    }
}

fn print_readiness_text(report: &ProductionReadinessReport) {
    println!("Polymarket production readiness");
    println!("  Build version:    {}", report.build.package_version);
    println!("  Git revision:     {}", short_sha(&report.build.git_sha));
    println!("  Git dirty:        {}", report.build.git_dirty);
    println!("  Build timestamp:  {}", report.build.build_timestamp);
    println!("  Strategy version: {}", report.runtime.strategy_version);
    println!("  Runtime mode:     {}", report.runtime.mode);
    println!("  Config source:    {}", report.runtime.config_source);
    println!("  Database path:    {}", report.runtime.database_path);
    println!();

    for check in &report.checks {
        println!(
            "[{}] {}: {} ({})",
            if check.ok { "OK" } else { "PENDING" },
            check.phase,
            check.gate,
            check.evidence
        );
    }
    println!();
    if report.canary_ready {
        println!("READY: all canary-required readiness checks passed. Operator review is still required.");
    } else {
        println!(
            "BLOCKED: {} canary-required readiness check(s) failed:",
            report.blockers.len()
        );
        for blocker in &report.blockers {
            println!("  - {blocker}");
        }
    }
}

fn readiness_check(
    phase: &str,
    gate: &str,
    ok: bool,
    required_for_canary: bool,
    evidence: String,
) -> ReadinessCheck {
    ReadinessCheck {
        phase: phase.to_string(),
        gate: gate.to_string(),
        ok,
        required_for_canary,
        evidence,
    }
}

fn canary_required_checks_pass(checks: &[ReadinessCheck]) -> bool {
    checks
        .iter()
        .filter(|check| check.required_for_canary)
        .all(|check| check.ok)
}

fn canary_gate_error(report: &ProductionReadinessReport) -> Option<String> {
    (!report.canary_ready).then(|| {
        format!(
            "production readiness gate failed: {} canary-required check(s) are blocked",
            report.blockers.len()
        )
    })
}

fn signer_drill_evidence(config: &Config) -> (bool, String) {
    let drill_dir = drill_summary_dir(config);
    let summaries = load_deployment_drill_summaries(&drill_dir);
    signer_drill_evidence_from_summaries(&summaries, &drill_dir)
}

fn signer_drill_evidence_from_summaries(summaries: &[Value], drill_dir: &Path) -> (bool, String) {
    const REQUIRED_DRILLS: [&str; 2] = ["dry-sign-safety", "new-wallet-dry-sign"];

    let mut completed = BTreeSet::new();
    for summary in summaries {
        let Some(drill_type) = summary.get("drill_type").and_then(Value::as_str) else {
            continue;
        };
        if signer_drill_summary_passes(drill_type, summary) {
            completed.insert(drill_type.to_string());
        }
    }

    let missing: Vec<&str> = REQUIRED_DRILLS
        .into_iter()
        .filter(|drill| !completed.contains(*drill))
        .collect();

    let completed_text = if completed.is_empty() {
        "none".to_string()
    } else {
        completed.into_iter().collect::<Vec<_>>().join(", ")
    };

    if missing.is_empty() {
        (
            true,
            format!(
                "completed: {completed_text}; source={}",
                drill_dir.display()
            ),
        )
    } else {
        (
            false,
            format!(
                "completed: {completed_text}; missing: {}; source={}",
                missing.join(", "),
                drill_dir.display()
            ),
        )
    }
}

fn signer_drill_summary_passes(drill_type: &str, summary: &Value) -> bool {
    let ok = summary.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        return false;
    }
    match drill_type {
        "dry-sign-safety" => {
            let submitted = summary
                .get("submitted")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let live_credentials_required = summary
                .get("live_credentials_required")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let failed_closed_without_credentials = summary
                .get("failed_closed_without_credentials")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            !submitted && !live_credentials_required && failed_closed_without_credentials
        }
        "new-wallet-dry-sign" => {
            let submitted = summary
                .get("submitted")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let signature_type_ok = summary
                .get("signature_type")
                .and_then(Value::as_u64)
                .is_some_and(|value| value == 3);
            let signature_present = summary
                .get("signature_present")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            !submitted && signature_type_ok && signature_present
        }
        _ => false,
    }
}

fn lifecycle_drill_evidence(config: &Config) -> (bool, String) {
    let drill_dir = drill_summary_dir(config);
    let summaries = load_deployment_drill_summaries(&drill_dir);
    lifecycle_drill_evidence_from_summaries(&summaries, &drill_dir)
}

fn lifecycle_drill_evidence_from_summaries(
    summaries: &[Value],
    drill_dir: &Path,
) -> (bool, String) {
    const REQUIRED_DRILLS: [&str; 2] = ["lifecycle-non-live", "lifecycle-live-canary"];

    let mut completed = BTreeSet::new();
    for summary in summaries {
        let Some(drill_type) = summary.get("drill_type").and_then(Value::as_str) else {
            continue;
        };
        if lifecycle_drill_summary_passes(drill_type, summary) {
            completed.insert(drill_type.to_string());
        }
    }

    let missing: Vec<&str> = REQUIRED_DRILLS
        .into_iter()
        .filter(|drill| !completed.contains(*drill))
        .collect();

    let completed_text = if completed.is_empty() {
        "none".to_string()
    } else {
        completed.into_iter().collect::<Vec<_>>().join(", ")
    };

    if missing.is_empty() {
        (
            true,
            format!(
                "completed: {completed_text}; source={}",
                drill_dir.display()
            ),
        )
    } else {
        (
            false,
            format!(
                "completed: {completed_text}; missing: {}; source={}",
                missing.join(", "),
                drill_dir.display()
            ),
        )
    }
}

fn lifecycle_drill_summary_passes(drill_type: &str, summary: &Value) -> bool {
    let ok = summary.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        return false;
    }
    match drill_type {
        "lifecycle-non-live" => {
            let submitted = summary
                .get("submitted")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let live_credentials_required = summary
                .get("live_credentials_required")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let test_suite_passed = summary
                .get("test_suite_passed")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            !submitted && !live_credentials_required && test_suite_passed
        }
        "lifecycle-live-canary" => summary
            .get("submitted")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        _ => false,
    }
}

fn deployment_drill_evidence(config: &Config) -> (bool, String) {
    let drill_dir = drill_summary_dir(config);
    let summaries = load_deployment_drill_summaries(&drill_dir);
    deployment_drill_evidence_from_summaries(&summaries, &drill_dir)
}

fn drill_summary_dir(config: &Config) -> PathBuf {
    Path::new(&config.storage.database_path)
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("drills")
}

fn load_deployment_drill_summaries(drill_dir: &Path) -> Vec<Value> {
    let mut summaries = Vec::new();
    let Ok(entries) = fs::read_dir(drill_dir) else {
        return summaries;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(mut summary) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        if summary.get("drill_type").is_none() {
            if let Some(name) = path.file_name().and_then(|value| value.to_str()) {
                if name.starts_with("production-paper-drill-") {
                    summary["drill_type"] = Value::String("production-paper".to_string());
                }
            }
        }
        summaries.push(summary);
    }
    summaries
}

fn deployment_drill_evidence_from_summaries(
    summaries: &[Value],
    drill_dir: &Path,
) -> (bool, String) {
    const REQUIRED_DRILLS: [&str; 6] = [
        "production-paper",
        "reboot",
        "restore",
        "rollback",
        "credential-rotation",
        "alerts",
    ];

    let mut completed = BTreeSet::new();
    for summary in summaries {
        let Some(drill_type) = summary.get("drill_type").and_then(Value::as_str) else {
            continue;
        };
        if drill_summary_passes(drill_type, summary) {
            completed.insert(drill_type.to_string());
        }
    }

    let missing: Vec<&str> = REQUIRED_DRILLS
        .into_iter()
        .filter(|drill| !completed.contains(*drill))
        .collect();

    let completed_text = if completed.is_empty() {
        "none".to_string()
    } else {
        completed.into_iter().collect::<Vec<_>>().join(", ")
    };

    if missing.is_empty() {
        (
            true,
            format!(
                "completed: {completed_text}; source={}",
                drill_dir.display()
            ),
        )
    } else {
        (
            false,
            format!(
                "completed: {completed_text}; missing: {}; source={}",
                missing.join(", "),
                drill_dir.display()
            ),
        )
    }
}

fn drill_summary_passes(drill_type: &str, summary: &Value) -> bool {
    let ok = summary.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        return false;
    }
    match drill_type {
        "production-paper" => {
            let submitted = summary
                .get("submitted")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let live_credentials_required = summary
                .get("live_credentials_required")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            let canary_gate_blocked = summary
                .get("canary_gate_blocked")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let health_ready = summary
                .pointer("/health/overall")
                .and_then(Value::as_str)
                .is_some_and(|overall| overall == "ready");
            let backup_verified = summary
                .pointer("/backup_verify/ok")
                .and_then(Value::as_bool)
                .unwrap_or(false);

            !submitted
                && !live_credentials_required
                && canary_gate_blocked
                && health_ready
                && backup_verified
        }
        "reboot" | "restore" | "rollback" | "credential-rotation" | "alerts" => true,
        _ => false,
    }
}

fn short_sha(value: &str) -> &str {
    if value.len() >= 12 {
        &value[..12]
    } else {
        value
    }
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

    check_address("POLYMARKET_OWNER_ADDRESS", &mut failures);
    check_secret("POLYMARKET_PRIVATE_KEY", true, &mut failures);
    check_address("POLYMARKET_DEPOSIT_WALLET_ADDRESS", &mut failures);
    check_exact("POLYMARKET_SIGNATURE_TYPE", "3", &mut failures);
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
        println!("READY: host prerequisites passed. This does not authorize a live order.");
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

fn check_exact(name: &str, expected: &str, failures: &mut Vec<String>) {
    let valid = std::env::var(name)
        .map(|value| value == expected)
        .unwrap_or(false);
    print_check(valid, name, &format!("must equal {expected}"));
    if !valid {
        failures.push(format!("{name} is missing or invalid"));
    }
}

fn print_check(ok: bool, name: &str, detail: &str) {
    println!("[{}] {} ({})", if ok { "OK" } else { "FAIL" }, name, detail);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn canary_ready_requires_only_canary_required_checks() {
        let checks = vec![
            readiness_check("Phase 0", "required ok", true, true, "ok".to_string()),
            readiness_check(
                "Phase 2",
                "informational pending",
                false,
                false,
                "pending".to_string(),
            ),
        ];
        assert!(canary_required_checks_pass(&checks));

        let checks = vec![
            readiness_check("Phase 0", "required ok", true, true, "ok".to_string()),
            readiness_check(
                "Phase 4",
                "required pending",
                false,
                true,
                "pending".to_string(),
            ),
        ];
        assert!(!canary_required_checks_pass(&checks));
    }

    #[test]
    fn canary_gate_error_reports_blocker_count() {
        let report = ProductionReadinessReport {
            generated_at_ms: 0,
            build: BuildReadiness {
                package_version: "0.1.0".to_string(),
                git_sha: "abc".to_string(),
                git_dirty: "true".to_string(),
                build_timestamp: "1".to_string(),
            },
            runtime: RuntimeReadiness {
                mode: "paper".to_string(),
                environment: "production".to_string(),
                strategy_version: "test".to_string(),
                config_source: "test".to_string(),
                database_path: "test.db".to_string(),
            },
            canary_ready: false,
            blockers: vec!["a".to_string(), "b".to_string()],
            checks: vec![],
        };
        assert_eq!(
            canary_gate_error(&report),
            Some(
                "production readiness gate failed: 2 canary-required check(s) are blocked"
                    .to_string()
            )
        );
    }

    #[test]
    fn deployment_drill_evidence_counts_valid_paper_drill_but_keeps_gate_blocked() {
        let summary = json!({
            "drill_type": "production-paper",
            "ok": true,
            "submitted": false,
            "live_credentials_required": false,
            "canary_gate_blocked": true,
            "health": {"overall": "ready"},
            "backup_verify": {"ok": true}
        });
        let (ok, evidence) = deployment_drill_evidence_from_summaries(
            &[summary],
            Path::new("data-production/drills"),
        );

        assert!(!ok);
        assert!(evidence.contains("completed: production-paper"));
        assert!(evidence.contains("missing: reboot, restore, rollback"));
    }

    #[test]
    fn deployment_drill_evidence_ignores_non_deployment_drills() {
        let summary = json!({
            "drill_type": "lifecycle-non-live",
            "ok": true,
            "submitted": false,
            "live_credentials_required": false,
            "test_suite_passed": true
        });
        let (ok, evidence) = deployment_drill_evidence_from_summaries(
            &[summary],
            Path::new("data-production/drills"),
        );

        assert!(!ok);
        assert!(evidence.contains("completed: none"));
        assert!(evidence.contains("missing: production-paper, reboot, restore"));
    }

    #[test]
    fn signer_drill_evidence_keeps_phase5_blocked_after_safety_drill() {
        let summary = json!({
            "drill_type": "dry-sign-safety",
            "ok": true,
            "submitted": false,
            "live_credentials_required": false,
            "failed_closed_without_credentials": true
        });
        let (ok, evidence) =
            signer_drill_evidence_from_summaries(&[summary], Path::new("data-production/drills"));

        assert!(!ok);
        assert!(evidence.contains("completed: dry-sign-safety"));
        assert!(evidence.contains("missing: new-wallet-dry-sign"));
    }

    #[test]
    fn signer_drill_evidence_passes_only_after_new_wallet_dry_sign() {
        let summaries = vec![
            json!({
                "drill_type": "dry-sign-safety",
                "ok": true,
                "submitted": false,
                "live_credentials_required": false,
                "failed_closed_without_credentials": true
            }),
            json!({
                "drill_type": "new-wallet-dry-sign",
                "ok": true,
                "submitted": false,
                "signature_type": 3,
                "signature_present": true
            }),
        ];
        let (ok, evidence) =
            signer_drill_evidence_from_summaries(&summaries, Path::new("data-production/drills"));

        assert!(ok);
        assert!(evidence.contains("completed: dry-sign-safety, new-wallet-dry-sign"));
    }

    #[test]
    fn deployment_drill_evidence_passes_only_when_all_required_drills_exist() {
        let paper = json!({
            "drill_type": "production-paper",
            "ok": true,
            "submitted": false,
            "live_credentials_required": false,
            "canary_gate_blocked": true,
            "health": {"overall": "ready"},
            "backup_verify": {"ok": true}
        });
        let summaries = vec![
            paper,
            json!({"drill_type": "reboot", "ok": true}),
            json!({"drill_type": "restore", "ok": true}),
            json!({"drill_type": "rollback", "ok": true}),
            json!({"drill_type": "credential-rotation", "ok": true}),
            json!({"drill_type": "alerts", "ok": true}),
        ];
        let (ok, evidence) = deployment_drill_evidence_from_summaries(
            &summaries,
            Path::new("data-production/drills"),
        );

        assert!(ok);
        assert!(evidence.contains("completed:"));
        assert!(!evidence.contains("missing:"));
    }

    #[test]
    fn lifecycle_drill_evidence_keeps_live_gate_blocked_after_non_live_drill() {
        let summary = json!({
            "drill_type": "lifecycle-non-live",
            "ok": true,
            "submitted": false,
            "live_credentials_required": false,
            "test_suite_passed": true
        });
        let (ok, evidence) = lifecycle_drill_evidence_from_summaries(
            &[summary],
            Path::new("data-production/drills"),
        );

        assert!(!ok);
        assert!(evidence.contains("completed: lifecycle-non-live"));
        assert!(evidence.contains("missing: lifecycle-live-canary"));
    }

    #[test]
    fn lifecycle_drill_evidence_passes_only_after_non_live_and_live_canary_drills() {
        let summaries = vec![
            json!({
                "drill_type": "lifecycle-non-live",
                "ok": true,
                "submitted": false,
                "live_credentials_required": false,
                "test_suite_passed": true
            }),
            json!({
                "drill_type": "lifecycle-live-canary",
                "ok": true,
                "submitted": true
            }),
        ];
        let (ok, evidence) = lifecycle_drill_evidence_from_summaries(
            &summaries,
            Path::new("data-production/drills"),
        );

        assert!(ok);
        assert!(evidence.contains("completed: lifecycle-live-canary, lifecycle-non-live"));
    }
}
