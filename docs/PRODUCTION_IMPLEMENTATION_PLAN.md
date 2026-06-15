# Polymarket Production Implementation Plan

## 1. Objective

Prepare the application so it can progress safely from paper trading to a
manually approved live canary with a maximum funded capital of $2.

Production-ready means more than having a wallet and posting an order. The
program must be able to:

- make the same decision in paper, shadow, dry-signed, and live modes;
- prove why every order was placed or skipped;
- survive restarts without losing order or position state;
- reconcile local state with Polymarket before taking new risk;
- fail closed when data, authentication, heartbeat, or risk checks fail;
- prevent live mode from being enabled accidentally;
- support a controlled rollback and credential rotation.

This plan does not authorize live trading. Live execution remains blocked until
all go-live gates in this document pass.

## 2. Current State

### Working Today

- Live Gamma/CLOB public market discovery through the local proxy.
- Live Binance one-minute candles for the 5m and 15m signal models.
- Real CLOB orderbook asks/bids displayed in the dashboard.
- Separate 5m and 15m paper executors.
- Narrow entry windows and spread/ask guards.
- Paper settlement from official Polymarket outcomes.
- Production config template, environment template, runbook, and preflight
  command.
- A SQLite module exists for older collector/position flows.
- Dashboard and Gamma proxy can run as separate processes.

### Blocking Gaps

| Area | Current Gap | Production Impact |
|---|---|---|
| Execution | No authenticated CLOB V2 order executor | Cannot place live orders |
| Architecture | Web paper logic is embedded in `web/mod.rs` | Hard to test and reuse safely |
| Persistence | Dashboard trades, stats, signals, and breakers are in memory | State is lost on restart |
| Reconciliation | No startup comparison with CLOB orders/trades/positions | Duplicate or orphaned risk |
| Risk | Guards are distributed and partly hard-coded | Paper/live behavior may diverge |
| Configuration | Dashboard does not consistently use production config | Unsafe or surprising defaults |
| Credentials | No secret-provider abstraction | Risk of leaking or mishandling keys |
| Order lifecycle | No partial-fill, cancel, heartbeat, or recovery logic | Unmanaged open orders |
| Observability | No durable audit event stream or production metrics | Incidents cannot be reconstructed |
| Validation | Backtest lacks historical Polymarket books and Chainlink outcomes | Profit estimates are not reliable |
| Deployment | Only dashboard service example exists | Proxy/executor are not supervised |
| Operations | No canary command, kill switch, backup, or restore drill | Unsafe rollout and recovery |

## 3. Target Architecture

```text
Gamma/CLOB public APIs   Binance candles   Official outcomes
          |                    |                  |
          +---------- Market Data Layer ---------+
                               |
                     Strategy Decision Engine
                               |
                         Central Risk Engine
                               |
                   Execution Intent + Audit Store
                               |
       +-----------------------+-----------------------+
       |                       |                       |
  Paper Executor         Shadow Executor       CLOB V2 Executor
  simulated fills       no submission          signed/live modes
       |                       |                       |
       +---------------- Order Lifecycle -------------+
                               |
              SQLite state + reconciliation + metrics
                               |
                           Dashboard/API
```

### Required Invariants

1. Strategy code never submits orders directly.
2. Every proposed trade becomes a persisted execution intent before action.
3. Every executor receives the same validated intent.
4. The central risk engine is the final authority before submission.
5. Live submission requires an explicit runtime mode and a valid manual canary
   authorization.
6. Startup reconciliation completes before any new intent can execute.
7. Missing or stale critical data always results in `SKIP` or `HALT`.
8. No secret is returned through dashboard APIs, logs, panic messages, or
   diagnostics.

## 4. Runtime Modes

Use one explicit enum throughout the application:

| Mode | Behavior |
|---|---|
| `paper` | Simulates orders using real books and official outcomes |
| `shadow` | Builds live-ready intents and records expected orders, submits nothing |
| `dry_signed` | Creates and verifies signed CLOB orders locally, submits nothing |
| `canary` | Allows exactly one manually authorized live order within canary limits |
| `live` | Allows guarded live execution after every gate passes |

Rules:

- Default mode is always `paper`.
- Config files cannot silently elevate the mode.
- `canary` and `live` require environment confirmation plus a CLI command.
- The web dashboard must not contain an endpoint that enables `canary` or
  `live`.
- Any mode change is persisted as an audit event.

## 5. Delivery Phases

## Phase 0: Freeze Safety Baseline

**Goal:** establish a reproducible, fail-closed starting point.

**Status as of June 15, 2026:** foundation implemented. Typed runtime/risk/
execution/storage/dashboard config, paper-only startup validation, startup
identity logging, dashboard risk ceilings, and `production-readiness` are in
place. Build provenance/dirty-tree gating and full strategy-parameter
extraction remain before Phase 0 is complete.

### Tasks

- Keep all current execution paper-only.
- Add a `production-readiness` command that reports each phase/gate.
- Make startup print the runtime mode, config path, database path, build
  version, and strategy version.
- Add a strategy version constant and include it on every signal/trade.
- Remove hard-coded risk parameters from `web/mod.rs`; load them from typed
  config.
- Validate production config values and reject unsafe combinations.
- Reject `canary/live` when the git/build version is unknown or dirty unless an
  explicit development override is used outside production.

### Acceptance Criteria

- Application cannot enter live mode.
- Invalid or missing production config causes startup failure.
- Every dashboard signal exposes strategy version and runtime mode.
- Unit tests prove defaults are paper-only and fail closed.

## Phase 1: Durable State and Audit Log

**Goal:** make restarts harmless and every decision reconstructable.

**Status as of June 15, 2026:** Phase 1A and 1B implemented. Dashboard settings,
trades, statistics, breaker history, actionable signals/decisions, audit
events, and execution-intent uniqueness now survive restart in a versioned
SQLite store. WAL, foreign keys, busy timeout, atomic snapshots, consistent
`VACUUM INTO` backups, and reopen/restore tests are in place. Paper trades,
capital ledger entries, and per-day UTC risk state are also normalized into
query-oriented tables. Phase 1C also persists normalized market windows and
orderbook snapshots from every scanner pass. Reconciliation history remains
intentionally pending until the reconciliation service is implemented in
Phase 7.

### Tasks

- Create versioned SQLite migrations instead of runtime `ALTER TABLE` calls.
- Consolidate old storage and web state into a production schema.
- Enable SQLite WAL mode, foreign keys, busy timeout, and scheduled backups.
- Persist:
  - markets and token IDs;
  - orderbook snapshots used at decision time;
  - signals and model inputs;
  - execution intents and risk decisions;
  - orders, fills, trades, positions, and settlements;
  - capital ledger and daily risk counters;
  - circuit-breaker state;
  - reconciliation runs;
  - audit events and runtime-mode changes.

### Minimum Schema

| Table | Purpose |
|---|---|
| `schema_migrations` | Applied database versions |
| `market_windows` | Slug, timeframe, tokens, start/end, official result |
| `market_snapshots` | Ask, bid, spread, depth, fee, tick size, timestamps |
| `signals` | Direction, features, confidence, strategy version |
| `execution_intents` | Immutable proposed orders and skip/reject reasons |
| `risk_decisions` | Every guard result and limits at decision time |
| `orders` | Client key, CLOB order ID, status, requested values |
| `fills` | Fill price, shares, fee, transaction/order references |
| `positions` | Reconciled outcome-token positions |
| `capital_ledger` | Deposits, reserves, fills, payouts, fees, adjustments |
| `daily_risk_state` | Orders, realized loss, consecutive losses, halt state |
| `audit_events` | Structured append-only operational events |

### Acceptance Criteria

- Restart preserves paper trades, positions, statistics, and breakers.
- Duplicate execution intent for the same market/timeframe is rejected by a
  database uniqueness constraint.
- Database restore drill recovers an equivalent dashboard state.
- Integration tests cover crash/restart during open and settled paper trades.

## Phase 2: Market Data Reliability

**Goal:** ensure decisions are based on fresh, correctly identified data.

**Status (June 15, 2026): complete.** The dashboard now fails closed on stale
or incomplete books/candles, maps tokens by outcome name, records typed source
status, validates CLOB execution metadata and clock drift, and quotes paper
entries from available depth rather than the best ask alone.

### Tasks

- Replace silent `Option` failures with typed error/status categories:
  `not_found`, `timeout`, `rate_limited`, `invalid_payload`, `stale`,
  `unavailable`.
- Add request timeouts, bounded retries, exponential backoff, and jitter.
- Record source timestamps and reject stale books/candles.
- Validate token ordering against outcomes instead of assuming index order.
- Fetch and persist:
  - tick size;
  - minimum order size;
  - fee rate;
  - negative-risk flag;
  - best bid/ask and executable depth.
- Compute expected fill from actual depth, not only the best ask.
- Add clock-drift checks against CLOB server time.
- Add health status for Gamma, CLOB public, Binance, proxy, and database.

### Acceptance Criteria

- No intent is created from stale or incomplete market data.
- Token/outcome mapping has fixture tests.
- Simulated fills never exceed available recorded depth.
- Dashboard distinguishes market-not-found from API/network failure.

## Phase 3: Unified Strategy and Risk Pipeline

**Goal:** eliminate behavior differences between paper and live execution.

### Tasks

- Move signal generation out of `web/mod.rs` into strategy services.
- Move execution guards into a central `RiskEngine`.
- Produce an immutable `ExecutionIntent` containing:
  - market slug and token ID;
  - timeframe and direction;
  - strategy/model version;
  - signal/input timestamps;
  - requested amount and worst allowed price;
  - expected fill, fee, and model margin;
  - all risk-check results;
  - deterministic client order key.
- Implement risk limits from configuration:
  - maximum one open position across both strategies;
  - maximum $0.10 live order;
  - maximum three daily live orders;
  - maximum $0.30 daily realized loss;
  - three-loss circuit breaker;
  - spread, depth, fee, stale-data, and entry-window limits;
  - minimum balance reserve;
  - no martingale or averaging down.
- Add global, strategy-level, and market-level kill switches.
- Use decimal/fixed-point amounts for money, shares, and prices in production
  paths; avoid `f64` for signed order values and ledger accounting.

### Acceptance Criteria

- Paper and shadow modes create byte-equivalent intent payloads for identical
  input fixtures.
- Every skip/rejection is persisted with a machine-readable reason.
- Risk engine has boundary and property tests for every configured limit.
- Concurrent 5m/15m signals cannot exceed the global position/order limit.

## Phase 4: Forward-Test Collector and Honest Evaluation

**Goal:** generate evidence that reflects executable Polymarket conditions.

### Tasks

- Run paper executor against real CLOB snapshots and executable depth.
- Store official Polymarket outcomes and detect settlement mismatches.
- Record unfilled/skipped opportunities and their reasons.
- Produce daily and rolling reports:
  - raw direction accuracy;
  - executable trade accuracy;
  - fill ratio;
  - realized/simulated PnL after actual fee assumptions;
  - profit factor and drawdown;
  - Brier score/calibration;
  - results by timeframe, hour, ask band, spread band, and regime;
  - Binance-versus-official-outcome mismatch rate.
- Add a report command and JSON export suitable for automated evaluation.
- Do not auto-tune production thresholds. Parameter changes require a new
  strategy version and a documented comparison.

### Acceptance Criteria

- Minimum 200 settled forward paper trades using real CLOB books.
- No unresolved settlement mismatch.
- Reports can reproduce every aggregate from persisted rows.
- Promotion gate: win rate >= 68%, profit factor >= 1.40, max drawdown <= 20%.

## Phase 5: CLOB V2 Authentication and Dry-Signed Orders

**Goal:** prove the program can safely build valid orders without submitting.

### Tasks

- Add an authenticated CLOB V2 client behind an `OrderExecutor` trait.
- Prefer the official Rust SDK where its V2/deposit-wallet functionality is
  complete; otherwise isolate exact EIP-712/POLY_1271 signing in one module.
- Add a secret-provider interface:
  - environment file for initial local deployment;
  - future secret-manager adapter;
  - redacted debug output.
- Support owner/session signer plus deposit-wallet funder address.
- Derive or validate L2 API key, secret, and passphrase.
- Add balance/allowance checks and a read-only authenticated status command.
- Implement `dry_signed` mode that creates and locally verifies an order but
  cannot call the post-order endpoint.
- Add fixture tests against official SDK signing examples.

### Acceptance Criteria

- No secret appears in logs, API output, database, panic output, or test
  snapshots.
- A signed order can be generated and validated locally.
- Wrong signer, signature type, funder, chain ID, or credentials fail closed.
- Dry-signed mode has no network path capable of order submission.

## Phase 6: Live Order Lifecycle

**Goal:** correctly manage an order from intent through fill and settlement.

### Tasks

- Implement FOK first; add FAK only after partial-fill accounting is tested.
- Fetch tick size, neg-risk, fee rate, minimum size, and depth immediately
  before submission.
- Reserve capital atomically before posting.
- Persist intent and pending order before the network request.
- Submit with deterministic idempotency/client order key.
- Persist response and reconcile ambiguous timeouts before retrying.
- Add authenticated user WebSocket for order/fill updates.
- Implement:
  - open-order query;
  - cancel one/cancel all;
  - heartbeat;
  - partial/full fill accounting;
  - rejected/cancelled/expired states;
  - official settlement and redeem workflow.
- On shutdown: halt new intents, cancel open orders, reconcile, then exit.

### Acceptance Criteria

- Integration tests cover accepted, rejected, FOK-cancelled, partial,
  duplicate, timeout, disconnect, and restart scenarios.
- Ambiguous post-order timeout cannot create duplicate risk.
- Capital ledger equals reconciled CLOB balances/positions within documented
  tolerances.
- Heartbeat failure halts submissions and triggers cancel/reconciliation.

## Phase 7: Startup Reconciliation and Recovery

**Goal:** make process and host restarts safe.

### Tasks

- Add startup state machine:
  `BOOTING -> PREFLIGHT -> RECONCILING -> READY/HALTED`.
- Before accepting signals:
  - query CLOB open orders;
  - query fills/trades;
  - query balances/allowances;
  - query positions;
  - compare with local orders and ledger;
  - resolve pending/ambiguous requests;
  - preserve or activate risk halts.
- Add manual reconciliation command and report.
- Add immutable incident marker when automatic reconciliation cannot decide.
- Require operator resolution before returning to ready state.

### Acceptance Criteria

- Killing the process at every order-lifecycle stage and restarting never
  creates a duplicate order.
- Unknown remote order or position forces `HALTED`.
- Recovery tests run automatically in CI.

## Phase 8: Deployment, Security, and Observability

**Goal:** operate the system predictably on a dedicated host.

### Tasks

- Create separate supervised services:
  - `polymarket-proxy`;
  - `polymarket-trader`;
  - `polymarket-dashboard`.
- Bind services to localhost; use authenticated TLS reverse proxy only if
  remote dashboard access is required.
- Run as a dedicated non-root OS user.
- Store secrets outside Git with mode `0600`.
- Add structured JSON logs with secret redaction and rotation.
- Add health/readiness endpoints that expose no secrets.
- Add metrics/alerts for:
  - process exit/restart;
  - API errors and stale data;
  - geoblock/preflight change;
  - heartbeat and user-WebSocket failure;
  - reconciliation mismatch;
  - order rejection;
  - daily loss/circuit breaker;
  - database backup failure.
- Add database backup/retention and restore automation.
- Pin dependencies, generate an SBOM, and run dependency/security audits in CI.

### Acceptance Criteria

- Services start automatically after host reboot in the correct dependency
  order.
- Trader remains halted until proxy/data dependencies and reconciliation are
  ready.
- Restore, credential rotation, and rollback drills are documented and tested.
- Dashboard cannot enable live trading.

## Phase 9: Canary and Live Promotion

**Goal:** permit the smallest possible controlled live experiment.

### Canary Requirements

- All previous phase acceptance criteria pass.
- Production preflight passes on the actual host.
- Geographic eligibility is confirmed without bypassing restrictions.
- New uncompromised signer/deposit wallet is configured.
- Deposit wallet holds no more than the approved $2 experiment amount.
- Balance and approvals are confirmed.
- At least 200 settled forward-test trades pass promotion metrics.
- Operator reviews the exact canary intent before submission.

### Canary Behavior

- One order only.
- Maximum amount: $0.10.
- FOK only.
- Manual CLI command with intent ID and short-lived confirmation token.
- Automatic transition to `HALTED` after the canary completes or fails.
- Mandatory reconciliation and operator review afterward.

### Live Promotion

Do not promote beyond canary until multiple canaries have reconciled cleanly.
Initial live mode retains:

- maximum one open position;
- maximum $0.10 per order;
- maximum three orders per day;
- maximum $0.30 daily realized loss;
- manual daily enablement;
- automatic halt on any reconciliation, heartbeat, or data-quality failure.

## 6. Proposed Module Layout

```text
src/
  application/
    coordinator.rs
    runtime_mode.rs
  domain/
    execution_intent.rs
    order.rs
    fill.rs
    position.rs
    risk.rs
  market_data/
    gamma.rs
    clob_public.rs
    binance.rs
    freshness.rs
  strategy/
    five_minute.rs
    fifteen_minute.rs
    version.rs
  execution/
    mod.rs
    paper.rs
    shadow.rs
    dry_signed.rs
    clob_v2.rs
    lifecycle.rs
    reconciliation.rs
  risk/
    engine.rs
    limits.rs
    kill_switch.rs
  storage/
    migrations/
    repository.rs
    audit.rs
    ledger.rs
  operations/
    preflight.rs
    health.rs
    reports.rs
    canary.rs
```

The web module should become a read-oriented presentation layer over these
services, not the owner of strategy or execution behavior.

## 7. Configuration Changes

Create explicit typed sections:

```toml
[runtime]
mode = "paper"
strategy_version = "btc-updown-v2"

[risk]
max_open_positions = 1
max_order_usd = 0.10
max_daily_orders = 3
max_daily_realized_loss_usd = 0.30
max_consecutive_losses = 3
max_spread = 0.04
max_data_age_ms = 3000

[execution]
order_type = "FOK"
heartbeat_interval_secs = 5
reconcile_before_ready = true
cancel_on_shutdown = true

[storage]
database_path = "data-production/trading.db"
backup_directory = "data-production/backups"

[dashboard]
bind = "127.0.0.1:3001"
allow_live_mode_changes = false
```

Production validation must reject:

- non-local dashboard bind without authentication/TLS configuration;
- live/canary mode with paper/dry-run flags;
- missing reconciliation or cancel-on-shutdown;
- max order or daily loss above approved limits;
- unknown strategy version;
- zero/negative stale-data thresholds;
- missing production database/backup paths.

## 8. Testing Strategy

### Unit Tests

- Strategy feature calculations and thresholds.
- Risk limits and boundary values.
- Order rounding, fee, and amount conversions.
- Token/outcome mapping.
- State transitions and idempotency keys.
- Secret redaction.

### Integration Tests

- SQLite migrations and restart recovery.
- Public API timeout/rate-limit/stale-data behavior.
- Paper/shadow intent equivalence.
- Signed-order fixtures.
- Order lifecycle using a deterministic fake CLOB server.
- Reconciliation across local/remote mismatch cases.

### Failure and Chaos Tests

- Kill process before/after post-order request.
- Network timeout after remote acceptance.
- User WebSocket disconnect.
- Heartbeat failure.
- Database locked/full/corrupt.
- Stale Binance or CLOB data.
- Proxy/API unavailable.
- Host reboot with an open order or position.

### Release Gates

- `cargo fmt --check`
- `cargo clippy -- -D warnings` for new production modules
- unit/integration/recovery tests
- dependency and secret scan
- build artifact checksum/version
- production readiness report

## 9. Implementation Order and Dependencies

| Priority | Work Item | Depends On |
|---|---|---|
| P0 | Runtime modes and fail-closed config validation | None |
| P0 | Durable schema, migrations, audit log, capital ledger | Runtime modes |
| P0 | Unified execution intent and central risk engine | Durable state |
| P0 | Paper/shadow executor migration | Intent and risk engine |
| P1 | Forward-test collector and reports | Paper/shadow persistence |
| P1 | Market-data freshness, metadata, and depth validation | Durable state |
| P1 | CLOB V2 authenticated client and dry-signed mode | Secret provider |
| P1 | Order lifecycle, heartbeat, user WebSocket | CLOB V2 client |
| P1 | Reconciliation and recovery | Order lifecycle and storage |
| P2 | Supervised deployment, metrics, alerts, backups | Stable services |
| P2 | Canary command and operator workflow | Every P0/P1 item |

No CLOB live-post code should be enabled before the P0 items and dry-signed mode
are complete.

## 10. Definition of Production Ready

The program is ready for a manual $0.10 canary only when:

- all runtime modes and config gates fail closed;
- state, ledger, breakers, and audit logs survive restart;
- paper and shadow use the same execution intents as live;
- 200+ forward trades meet promotion metrics;
- official settlement mismatches are zero;
- CLOB V2 dry-signed order verification passes;
- balances, approvals, tick size, minimum size, fee, and depth are validated;
- heartbeat, cancel-all, reconciliation, and shutdown behavior pass tests;
- deployment, backups, alerts, rollback, and credential rotation are tested;
- production preflight passes on the actual host;
- the operator manually approves the exact canary intent.

Until then, the correct runtime mode is `paper` or `shadow`.
