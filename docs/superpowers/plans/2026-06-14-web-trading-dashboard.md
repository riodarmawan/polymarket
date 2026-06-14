# Web Trading Dashboard Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a web-based live trading dashboard with Axum backend and Tailwind CSS frontend for paper trading BTC markets.

**Architecture:** Axum server on port 3001 serves embedded static files and REST/WebSocket APIs. Trading engine reuses existing signal and paper trading modules. BTC price fetched from Binance REST API with fallback to mock data.

**Tech Stack:** Axum, tower-http, tokio-tungstenite, rust-embed, Tailwind CSS, vanilla JavaScript

---

## File Structure

```
src/web/
├── mod.rs              # Web server entry point
├── api.rs              # REST API handlers
├── ws.rs               # WebSocket handler
├── state.rs            # Shared application state
├── price_proxy.rs      # Binance BTC price fetcher
└── static/
    ├── index.html      # Main dashboard page
    ├── app.js          # Frontend JavaScript
    └── style.css       # Tailwind CSS (generated)
```

---

## Task 1: Add Dependencies

**Files:**
- Modify: `/home/kucingsakti/polymarket/polymarket-bot/Cargo.toml`

- [ ] **Step 1: Add web dependencies to Cargo.toml**

Add after the existing `[dependencies]` section:

```toml
axum = "0.8"
tower-http = { version = "0.6", features = ["fs", "cors"] }
tokio-tungstenite = "0.26"
rust-embed = "8"
mime = "0.3"
```

- [ ] **Step 2: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors (warnings OK)

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml Cargo.lock
git commit -m "deps: add axum, tower-http, tokio-tungstenite, rust-embed for web dashboard"
```

---

## Task 2: Create Web Module Structure

**Files:**
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/web/mod.rs`
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/web/state.rs`
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/web/price_proxy.rs`
- Modify: `/home/kucingsakti/polymarket/polymarket-bot/src/main.rs`

- [ ] **Step 1: Create web/mod.rs with server entry point**

```rust
pub mod api;
pub mod ws;
pub mod state;
pub mod price_proxy;

use axum::Router;
use std::net::SocketAddr;
use tower_http::services::ServeDir;
use tower_http::cors::{CorsLayer, Any};
use state::AppState;
use tokio::net::TcpListener;

pub async fn run_web_server(port: u16) -> anyhow::Result<()> {
    let state = AppState::new();
    
    // Start price proxy
    let state_clone = state.clone();
    tokio::spawn(async move {
        price_proxy::run_price_proxy(state_clone).await;
    });

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/api/price", axum::routing::get(api::get_price))
        .route("/api/markets", axum::routing::get(api::get_markets))
        .route("/api/signals", axum::routing::get(api::get_signals))
        .route("/api/trades", axum::routing::get(api::get_trades))
        .route("/api/stats", axum::routing::get(api::get_stats))
        .route("/api/settings", axum::routing::get(api::get_settings))
        .route("/api/settings", axum::routing::post(api::update_settings))
        .route("/ws", axum::routing::get(ws::ws_handler))
        .fallback_service(ServeDir::new("src/web/static"))
        .layer(cors)
        .with_state(state);

    let addr = SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!("Web dashboard running at http://localhost:{}", port);
    
    let listener = TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}
```

- [ ] **Step 2: Create web/state.rs with shared state**

```rust
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::crypto::live::paper_trading::{Stats, Trade};
use crate::crypto::live::gamma_client::GammaMarket;

#[derive(Debug, Clone)]
pub struct Settings {
    pub capital: f64,
    pub max_order: f64,
    pub timeframe: String,
    pub auto_trade: bool,
    pub stop_loss_pct: f64,
    pub take_profit_pct: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            capital: 2.0,
            max_order: 0.50,
            timeframe: "15m".to_string(),
            auto_trade: true,
            stop_loss_pct: 0.005,
            take_profit_pct: 0.01,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PriceData {
    pub price: f64,
    pub change_pct: f64,
    pub timestamp: i64,
}

impl Default for PriceData {
    fn default() -> Self {
        Self {
            price: 0.0,
            change_pct: 0.0,
            timestamp: 0,
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub price: Arc<RwLock<PriceData>>,
    pub settings: Arc<RwLock<Settings>>,
    pub trades: Arc<RwLock<Vec<Trade>>>,
    pub stats: Arc<RwLock<Stats>>,
    pub markets: Arc<RwLock<Vec<GammaMarket>>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            price: Arc::new(RwLock::new(PriceData::default())),
            settings: Arc::new(RwLock::new(Settings::default())),
            trades: Arc::new(RwLock::new(Vec::new())),
            stats: Arc::new(RwLock::new(Stats {
                total_trades: 0,
                wins: 0,
                losses: 0,
                win_rate: 0.0,
                total_pnl: 0.0,
                avg_win: 0.0,
                avg_loss: 0.0,
                profit_factor: 0.0,
                max_drawdown: 0.0,
                current_capital: 2.0,
            })),
            markets: Arc::new(RwLock::new(Vec::new())),
        }
    }
}
```

- [ ] **Step 3: Add `pub mod web;` to main.rs**

Add to `/home/kucingsakti/polymarket/polymarket-bot/src/main.rs`:

```rust
pub mod web;
```

- [ ] **Step 4: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 5: Commit**

```bash
git add src/web/mod.rs src/web/state.rs src/main.rs
git commit -m "feat: add web module structure with AppState"
```

---

## Task 3: Implement Price Proxy

**Files:**
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/web/price_proxy.rs`

- [ ] **Step 1: Create price_proxy.rs**

```rust
use crate::web::state::{AppState, PriceData};
use std::time::Duration;

#[derive(serde::Deserialize)]
struct BinancePriceResponse {
    price: String,
}

pub async fn fetch_btc_price() -> Option<f64> {
    let url = "https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT";
    let client = reqwest::Client::new();
    
    match client.get(url).timeout(Duration::from_secs(5)).send().await {
        Ok(resp) => {
            if let Ok(data) = resp.json::<BinancePriceResponse>().await {
                data.price.parse().ok()
            } else {
                None
            }
        }
        Err(_) => None,
    }
}

pub async fn run_price_proxy(state: AppState) {
    let mut last_price = 0.0f64;
    let mut mock_price = 80000.0f64;
    
    loop {
        // Try to fetch real price from Binance
        if let Some(price) = fetch_btc_price().await {
            let change = if last_price > 0.0 {
                ((price - last_price) / last_price) * 100.0
            } else {
                0.0
            };
            
            let mut price_data = state.price.write().await;
            *price_data = PriceData {
                price,
                change_pct: change,
                timestamp: chrono::Utc::now().timestamp_millis(),
            };
            last_price = price;
        } else {
            // Fallback to mock data
            let change = (rand::random::<f64>() - 0.5) * 100.0;
            mock_price += change;
            mock_price = mock_price.max(75000.0).min(85000.0);
            
            let change_pct = if last_price > 0.0 {
                ((mock_price - last_price) / last_price) * 100.0
            } else {
                0.0
            };
            
            let mut price_data = state.price.write().await;
            *price_data = PriceData {
                price: mock_price,
                change_pct,
                timestamp: chrono::Utc::now().timestamp_millis(),
            };
            last_price = mock_price;
        }
        
        tokio::time::sleep(Duration::from_secs(10)).await;
    }
}
```

- [ ] **Step 2: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 3: Commit**

```bash
git add src/web/price_proxy.rs
git commit -m "feat: add Binance BTC price proxy with mock fallback"
```

---

## Task 4: Implement REST API Handlers

**Files:**
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/web/api.rs`

- [ ] **Step 1: Create api.rs with all handlers**

```rust
use axum::extract::State;
use axum::Json;
use crate::web::state::{AppState, Settings};
use serde_json::{json, Value};

pub async fn get_price(State(state): State<AppState>) -> Json<Value> {
    let price = state.price.read().await;
    Json(json!({
        "price": price.price,
        "change_pct": price.change_pct,
        "timestamp": price.timestamp
    }))
}

pub async fn get_markets(State(state): State<AppState>) -> Json<Value> {
    let markets = state.markets.read().await;
    let markets_json: Vec<Value> = markets.iter().map(|m| {
        json!({
            "id": m.id,
            "question": m.question,
            "yes_price": m.yes_price(),
            "no_price": m.no_price(),
            "volume": m.volume_usd()
        })
    }).collect();
    Json(json!({ "markets": markets_json }))
}

pub async fn get_signals(State(state): State<AppState>) -> Json<Value> {
    let trades = state.trades.read().await;
    let recent_signals: Vec<Value> = trades.iter().rev().take(10).map(|t| {
        json!({
            "direction": format!("{:?}", t.direction),
            "timeframe": format!("{:?}", t.timeframe),
            "timestamp": t.timestamp
        })
    }).collect();
    Json(json!({ "signals": recent_signals }))
}

pub async fn get_trades(State(state): State<AppState>) -> Json<Value> {
    let trades = state.trades.read().await;
    let trades_json: Vec<Value> = trades.iter().map(|t| {
        json!({
            "timestamp": t.timestamp,
            "direction": format!("{:?}", t.direction),
            "entry_price": t.entry_price,
            "exit_price": t.exit_price,
            "size_usd": t.size_usd,
            "pnl": t.pnl,
            "status": format!("{:?}", t.status)
        })
    }).collect();
    Json(json!({ "trades": trades_json }))
}

pub async fn get_stats(State(state): State<AppState>) -> Json<Value> {
    let stats = state.stats.read().await;
    Json(json!({
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
    }))
}

pub async fn get_settings(State(state): State<AppState>) -> Json<Value> {
    let settings = state.settings.read().await;
    Json(json!({
        "capital": settings.capital,
        "max_order": settings.max_order,
        "timeframe": settings.timeframe,
        "auto_trade": settings.auto_trade,
        "stop_loss_pct": settings.stop_loss_pct,
        "take_profit_pct": settings.take_profit_pct
    }))
}

pub async fn update_settings(
    State(state): State<AppState>,
    Json(new_settings): Json<Settings>,
) -> Json<Value> {
    let mut settings = state.settings.write().await;
    *settings = new_settings;
    Json(json!({ "status": "ok" }))
}
```

- [ ] **Step 2: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 3: Commit**

```bash
git add src/web/api.rs
git commit -m "feat: add REST API handlers for price, markets, trades, stats, settings"
```

---

## Task 5: Implement WebSocket Handler

**Files:**
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/web/ws.rs`

- [ ] **Step 1: Create ws.rs**

```rust
use axum::extract::ws::{WebSocket, WebSocketUpgrade, Message};
use axum::extract::State;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use crate::web::state::AppState;
use serde_json::json;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(|socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    
    // Spawn task to send price updates
    let state_clone = state.clone();
    let mut send_task = tokio::spawn(async move {
        let mut last_price = 0.0;
        loop {
            let price = state_clone.price.read().await;
            if price.price != last_price {
                let msg = json!({
                    "type": "price",
                    "price": price.price,
                    "change_pct": price.change_pct,
                    "timestamp": price.timestamp
                });
                if sender.send(Message::Text(msg.to_string().into())).await.is_err() {
                    break;
                }
                last_price = price.price;
            }
            drop(price);
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    });

    // Handle incoming messages
    let state_clone = state.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(_msg)) = receiver.next().await {
            // Client messages ignored for now
        }
    });

    // Wait for either task to finish
    tokio::select! {
        _ = &mut send_task => recv_task.abort(),
        _ = &mut recv_task => send_task.abort(),
    }
}
```

- [ ] **Step 2: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 3: Commit**

```bash
git add src/web/ws.rs
git commit -m "feat: add WebSocket handler for real-time price updates"
```

---

## Task 6: Create Frontend HTML

**Files:**
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/web/static/index.html`

- [ ] **Step 1: Create index.html with Tailwind CSS**

```html
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Live Trading Dashboard</title>
    <script src="https://cdn.tailwindcss.com"></script>
    <style>
        .green { color: #22c55e; }
        .red { color: #ef4444; }
        .cyan { color: #06b6d4; }
        .yellow { color: #eab308; }
        .magenta { color: #d946ef; }
    </style>
</head>
<body class="bg-gray-900 text-white min-h-screen">
    <div class="container mx-auto p-4">
        <!-- Header -->
        <div class="bg-gray-800 rounded-lg p-4 mb-4 flex justify-between items-center">
            <h1 class="text-xl font-bold cyan">LIVE TRADING DASHBOARD</h1>
            <div class="text-green-400 font-bold">
                Capital: $<span id="capital">2.00</span>
            </div>
        </div>

        <!-- Price & Market Row -->
        <div class="grid grid-cols-2 gap-4 mb-4">
            <!-- BTC Price -->
            <div class="bg-gray-800 rounded-lg p-4">
                <h2 class="text-sm text-gray-400 mb-2">BTC/USDT</h2>
                <div class="text-2xl font-bold">$<span id="btc-price">0.00</span></div>
                <div id="btc-change" class="text-sm">+0.00%</div>
                <div class="text-sm text-yellow-400">Regime: <span id="regime">UNKNOWN</span></div>
            </div>

            <!-- Market -->
            <div class="bg-gray-800 rounded-lg p-4">
                <h2 class="text-sm text-gray-400 mb-2">POLYMARKET</h2>
                <div id="markets-container">
                    <div class="text-gray-500">Loading markets...</div>
                </div>
            </div>
        </div>

        <!-- Signal & Position Row -->
        <div class="grid grid-cols-2 gap-4 mb-4">
            <!-- Signal -->
            <div class="bg-gray-800 rounded-lg p-4">
                <h2 class="text-sm text-gray-400 mb-2">SIGNAL</h2>
                <div id="signal" class="text-lg font-bold">Waiting for signal...</div>
            </div>

            <!-- Open Position -->
            <div class="bg-gray-800 rounded-lg p-4">
                <h2 class="text-sm text-gray-400 mb-2">OPEN POSITION</h2>
                <div id="position" class="text-lg">No open position</div>
            </div>
        </div>

        <!-- Stats -->
        <div class="bg-gray-800 rounded-lg p-4 mb-4">
            <h2 class="text-sm text-gray-400 mb-2">STATS</h2>
            <div class="grid grid-cols-6 gap-4 text-sm">
                <div>Trades: <span id="total-trades" class="font-bold">0</span></div>
                <div>Win: <span id="win-rate" class="font-bold">0%</span></div>
                <div>PnL: $<span id="total-pnl" class="font-bold">0.00</span></div>
                <div>DD: <span id="drawdown" class="font-bold">0%</span></div>
                <div>PF: <span id="profit-factor" class="font-bold">0.00</span></div>
                <div>Avg: $<span id="avg-win" class="font-bold">0.00</span></div>
            </div>
        </div>

        <!-- Trade History -->
        <div class="bg-gray-800 rounded-lg p-4 mb-4">
            <h2 class="text-sm text-gray-400 mb-2">TRADE HISTORY</h2>
            <div id="trade-history" class="text-sm max-h-48 overflow-y-auto">
                <div class="text-gray-500">No trades yet</div>
            </div>
        </div>

        <!-- Settings & Risk -->
        <div class="grid grid-cols-2 gap-4">
            <!-- Settings -->
            <div class="bg-gray-800 rounded-lg p-4">
                <h2 class="text-sm text-gray-400 mb-2">SETTINGS</h2>
                <div class="space-y-2 text-sm">
                    <div class="flex justify-between">
                        <span>Capital:</span>
                        <input id="setting-capital" type="number" value="2.0" class="bg-gray-700 rounded px-2 w-24">
                    </div>
                    <div class="flex justify-between">
                        <span>Max Order:</span>
                        <input id="setting-max-order" type="number" value="0.5" class="bg-gray-700 rounded px-2 w-24">
                    </div>
                    <div class="flex justify-between">
                        <span>Auto-trade:</span>
                        <input id="setting-auto-trade" type="checkbox" checked class="rounded">
                    </div>
                    <button id="save-settings" class="bg-blue-600 hover:bg-blue-700 rounded px-4 py-1 w-full mt-2">Save</button>
                </div>
            </div>

            <!-- Risk Management -->
            <div class="bg-gray-800 rounded-lg p-4">
                <h2 class="text-sm text-gray-400 mb-2">RISK MANAGEMENT</h2>
                <div class="space-y-2 text-sm">
                    <div class="flex justify-between">
                        <span>Stop Loss:</span>
                        <input id="setting-stop-loss" type="number" value="0.5" step="0.1" class="bg-gray-700 rounded px-2 w-24">
                    </div>
                    <div class="flex justify-between">
                        <span>Take Profit:</span>
                        <input id="setting-take-profit" type="number" value="1.0" step="0.1" class="bg-gray-700 rounded px-2 w-24">
                    </div>
                    <button id="export-trades" class="bg-green-600 hover:bg-green-700 rounded px-4 py-1 w-full mt-2">Export Trades</button>
                </div>
            </div>
        </div>
    </div>

    <script src="app.js"></script>
</body>
</html>
```

- [ ] **Step 2: Verify file exists**

Run: `ls -la src/web/static/index.html`
Expected: File exists

- [ ] **Step 3: Commit**

```bash
git add src/web/static/index.html
git commit -m "feat: add dashboard HTML with Tailwind CSS layout"
```

---

## Task 7: Create Frontend JavaScript

**Files:**
- Create: `/home/kucingsakti/polymarket/polymarket-bot/src/web/static/app.js`

- [ ] **Step 1: Create app.js**

```javascript
let ws = null;

// WebSocket connection
function connectWebSocket() {
    ws = new WebSocket(`ws://${window.location.host}/ws`);
    
    ws.onmessage = (event) => {
        const data = JSON.parse(event.data);
        switch(data.type) {
            case 'price':
                updatePrice(data);
                break;
            case 'signal':
                updateSignal(data);
                break;
            case 'trade':
                updateTrade(data);
                break;
            case 'stats':
                updateStats(data);
                break;
        }
    };
    
    ws.onclose = () => {
        setTimeout(connectWebSocket, 1000);
    };
}

// Update price display
function updatePrice(data) {
    document.getElementById('btc-price').textContent = data.price.toFixed(2);
    const changeEl = document.getElementById('btc-change');
    changeEl.textContent = `${data.change_pct >= 0 ? '+' : ''}${data.change_pct.toFixed(2)}%`;
    changeEl.className = data.change_pct >= 0 ? 'text-green-400' : 'text-red-400';
}

// Update signal display
function updateSignal(data) {
    const signalEl = document.getElementById('signal');
    const color = data.direction === 'Up' ? 'green' : 'red';
    signalEl.innerHTML = `<span class="${color}">${data.direction}</span> (${data.confidence.toFixed(2)})`;
}

// Update trade display
function updateTrade(data) {
    const historyEl = document.getElementById('trade-history');
    const statusIcon = data.pnl >= 0 ? '✓' : '✗';
    const statusColor = data.pnl >= 0 ? 'green' : 'red';
    
    const tradeHtml = `
        <div class="flex justify-between ${statusColor}">
            <span>${statusIcon} ${data.direction} @ $${data.entry_price.toFixed(2)}</span>
            <span>$${data.pnl.toFixed(2)}</span>
        </div>
    `;
    
    if (historyEl.querySelector('.text-gray-500')) {
        historyEl.innerHTML = '';
    }
    historyEl.insertAdjacentHTML('afterbegin', tradeHtml);
}

// Update stats display
function updateStats(data) {
    document.getElementById('total-trades').textContent = data.total_trades;
    document.getElementById('win-rate').textContent = `${(data.win_rate * 100).toFixed(1)}%`;
    
    const pnlEl = document.getElementById('total-pnl');
    pnlEl.textContent = data.total_pnl.toFixed(2);
    pnlEl.className = data.total_pnl >= 0 ? 'font-bold text-green-400' : 'font-bold text-red-400';
    
    document.getElementById('drawdown').textContent = `${(data.max_drawdown * 100).toFixed(1)}%`;
    document.getElementById('profit-factor').textContent = data.profit_factor.toFixed(2);
    document.getElementById('avg-win').textContent = data.avg_win.toFixed(2);
    document.getElementById('capital').textContent = data.current_capital.toFixed(2);
}

// Load initial data
async function loadInitialData() {
    try {
        const [priceRes, marketsRes, statsRes, settingsRes] = await Promise.all([
            fetch('/api/price'),
            fetch('/api/markets'),
            fetch('/api/stats'),
            fetch('/api/settings')
        ]);
        
        const price = await priceRes.json();
        updatePrice(price);
        
        const markets = await marketsRes.json();
        updateMarkets(markets.markets);
        
        const stats = await statsRes.json();
        updateStats(stats);
        
        const settings = await settingsRes.json();
        loadSettings(settings);
    } catch (err) {
        console.error('Failed to load initial data:', err);
    }
}

// Update markets display
function updateMarkets(markets) {
    const container = document.getElementById('markets-container');
    if (markets.length === 0) {
        container.innerHTML = '<div class="text-gray-500">No BTC markets found</div>';
        return;
    }
    
    container.innerHTML = markets.map(m => `
        <div class="flex justify-between text-sm mb-1">
            <span class="text-yellow-400">[${m.timeframe || '?'}]</span>
            <span>YES: $${m.yes_price.toFixed(3)}</span>
            <span>NO: $${m.no_price.toFixed(3)}</span>
        </div>
    `).join('');
}

// Load settings into form
function loadSettings(settings) {
    document.getElementById('setting-capital').value = settings.capital;
    document.getElementById('setting-max-order').value = settings.max_order;
    document.getElementById('setting-auto-trade').checked = settings.auto_trade;
    document.getElementById('setting-stop-loss').value = settings.stop_loss_pct * 100;
    document.getElementById('setting-take-profit').value = settings.take_profit_pct * 100;
}

// Save settings
document.getElementById('save-settings').addEventListener('click', async () => {
    const settings = {
        capital: parseFloat(document.getElementById('setting-capital').value),
        max_order: parseFloat(document.getElementById('setting-max-order').value),
        timeframe: '15m',
        auto_trade: document.getElementById('setting-auto-trade').checked,
        stop_loss_pct: parseFloat(document.getElementById('setting-stop-loss').value) / 100,
        take_profit_pct: parseFloat(document.getElementById('setting-take-profit').value) / 100
    };
    
    await fetch('/api/settings', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings)
    });
    
    alert('Settings saved!');
});

// Export trades
document.getElementById('export-trades').addEventListener('click', async () => {
    const res = await fetch('/api/trades');
    const data = await res.json();
    
    const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `trades-${new Date().toISOString().slice(0,10)}.json`;
    a.click();
});

// Initialize
connectWebSocket();
loadInitialData();
```

- [ ] **Step 2: Verify file exists**

Run: `ls -la src/web/static/app.js`
Expected: File exists

- [ ] **Step 3: Commit**

```bash
git add src/web/static/app.js
git commit -m "feat: add dashboard JavaScript with WebSocket and API integration"
```

---

## Task 8: Add CLI Command

**Files:**
- Modify: `/home/kucingsakti/polymarket/polymarket-bot/src/cli.rs`
- Modify: `/home/kucingsakti/polymarket/polymarket-bot/src/main.rs`

- [ ] **Step 1: Add Web command to CLI**

Add to `Commands` enum in `cli.rs`:

```rust
    /// Web trading dashboard
    Web {
        /// Port to serve on
        #[arg(long, default_value = "3001")]
        port: u16,
    },
```

- [ ] **Step 2: Add Web handler to main.rs**

Add to `main.rs` match block:

```rust
        Commands::Web { port } => {
            tracing::info!("Starting web trading dashboard on port {}...", port);
            polymarket_bot::web::run_web_server(port).await?;
        }
```

- [ ] **Step 3: Verify build compiles**

Run: `cargo check 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 4: Commit**

```bash
git add src/cli.rs src/main.rs
git commit -m "feat: add 'web' CLI command for web dashboard"
```

---

## Task 9: Build and Test

**Files:**
- Verify: `/home/kucingsakti/polymarket/polymarket-bot/target/release/polymarket-bot`

- [ ] **Step 1: Build release binary**

Run: `cargo build --release 2>&1 | tail -5`
Expected: `Finished` with no errors

- [ ] **Step 2: Test run dashboard**

Run: `./target/release/polymarket-bot web --port 3001`
Expected: Server starts, shows "Web dashboard running at http://localhost:3001"

- [ ] **Step 3: Open browser**

Navigate to: `http://localhost:3001`
Expected: Dashboard loads, shows BTC price, markets, stats

- [ ] **Step 4: Final commit**

```bash
git add -A
git commit -m "feat: complete web trading dashboard implementation"
```

---

## Summary

| Task | Description | Files Changed |
|------|-------------|---------------|
| 1 | Add dependencies | Cargo.toml |
| 2 | Web module structure | src/web/mod.rs, state.rs |
| 3 | Price proxy | src/web/price_proxy.rs |
| 4 | REST API handlers | src/web/api.rs |
| 5 | WebSocket handler | src/web/ws.rs |
| 6 | Frontend HTML | src/web/static/index.html |
| 7 | Frontend JavaScript | src/web/static/app.js |
| 8 | CLI command | src/cli.rs, src/main.rs |
| 9 | Build and test | target/release/polymarket-bot |

**Total estimated time:** 60-90 minutes
