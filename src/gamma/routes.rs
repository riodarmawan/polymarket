use axum::extract::{Path, Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use tracing::instrument;

use super::client::{ClobClient, GammaClient};

// ── Query params ──────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
}

#[derive(Debug, Deserialize)]
pub struct MarketsQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default)]
    pub offset: u32,
    pub tag: Option<String>,
    pub closed: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub query: String,
}

#[derive(Debug, Deserialize)]
pub struct PriceHistoryQuery {
    pub market: String,
    #[serde(default = "default_interval")]
    pub interval: String,
    #[serde(default = "default_fidelity")]
    pub fidelity: u32,
}

#[derive(Debug, Deserialize)]
pub struct TokenQuery {
    pub token_id: String,
}

#[derive(Debug, Deserialize)]
pub struct PriceQuery {
    pub token_id: String,
    #[serde(default = "default_side")]
    pub side: String,
}

fn default_side() -> String {
    "BUY".to_string()
}

fn default_interval() -> String {
    "1d".to_string()
}

fn default_fidelity() -> u32 {
    60
}

fn default_limit() -> u32 {
    20
}

// ── App state ─────────────────────────────────────────────

#[derive(Clone)]
pub struct AppState {
    pub gamma: GammaClient,
    pub clob: ClobClient,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            gamma: GammaClient::new(),
            clob: ClobClient::new(),
        }
    }
}

// ── Error helper ──────────────────────────────────────────

type AppResult<T> = Result<Json<T>, (axum::http::StatusCode, String)>;

fn internal(err: impl ToString) -> (axum::http::StatusCode, String) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        err.to_string(),
    )
}

fn json<T: serde::Serialize>(
    v: T,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, String)> {
    serde_json::to_value(v).map(Json).map_err(|e| internal(e))
}

// ── Router ────────────────────────────────────────────────

pub fn router() -> Router {
    let state = AppState::new();

    Router::new()
        // Events
        .route("/api/events", get(list_events))
        .route("/api/events/slug/{slug}", get(get_event_by_slug))
        .route("/api/events/{id}", get(get_event))
        // Markets
        .route("/api/markets", get(list_markets))
        .route("/api/markets/{id}", get(get_market))
        // Search
        .route("/api/search", get(search))
        // Tags
        .route("/api/tags", get(list_tags))
        // CLOB
        .route("/api/book", get(orderbook))
        .route("/api/price", get(price))
        .route("/api/fee-rate", get(fee_rate))
        .route("/api/time", get(server_time))
        .route("/api/prices-history", get(price_history))
        .with_state(state)
}

// ── Handlers ──────────────────────────────────────────────

#[instrument(skip(state))]
async fn list_events(
    Query(q): Query<PaginationQuery>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .gamma
        .list_events(q.limit, q.offset)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn get_event(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .gamma
        .get_event(&id)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn get_event_by_slug(
    Path(slug): Path<String>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .gamma
        .get_event_by_slug(&slug)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn list_markets(
    Query(q): Query<MarketsQuery>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .gamma
        .list_markets(q.limit, q.offset, q.tag.as_deref(), q.closed)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn get_market(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .gamma
        .get_market(&id)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn search(
    Query(q): Query<SearchQuery>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .gamma
        .search(&q.query)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn list_tags(State(state): State<AppState>) -> AppResult<serde_json::Value> {
    state
        .gamma
        .list_tags()
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn price_history(
    Query(q): Query<PriceHistoryQuery>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .clob
        .fetch_price_history(&q.market, &q.interval, q.fidelity)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn orderbook(
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .clob
        .fetch_orderbook(&q.token_id)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn price(
    Query(q): Query<PriceQuery>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .clob
        .fetch_price(&q.token_id, &q.side)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn fee_rate(
    Query(q): Query<TokenQuery>,
    State(state): State<AppState>,
) -> AppResult<serde_json::Value> {
    state
        .clob
        .fetch_fee_rate(&q.token_id)
        .await
        .map_err(internal)
        .and_then(json)
}

#[instrument(skip(state))]
async fn server_time(State(state): State<AppState>) -> AppResult<serde_json::Value> {
    state
        .clob
        .fetch_server_time()
        .await
        .map_err(internal)
        .and_then(json)
}
