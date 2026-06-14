# Web Trading Dashboard Design

## Overview

A web-based live trading dashboard for paper trading BTC Up/Down markets on Polymarket. Replaces the TUI dashboard with a full-featured web UI using Axum backend and Tailwind CSS frontend. Includes BTC price proxy, real-time WebSocket updates, settings panel, risk management controls, and trade export.

## Goals

- Provide a full-featured web dashboard for live trading simulation
- Support real-time updates via WebSocket
- Include settings, risk management, and export functionality
- Run as single binary with embedded static files

## Non-Goals

- Real money trading
- Multi-user authentication
- Public internet deployment (local only)

## Architecture

```
polymarket-bot web --port 3001
         │
         ▼
┌─────────────────────────────────────────┐
│           Axum Server (port 3001)       │
├─────────────────────────────────────────┤
│  ┌─────────────┐  ┌──────────────────┐ │
│  │ REST API    │  │ WebSocket        │ │
│  │ /api/*      │  │ /ws              │ │
│  └──────┬──────┘  └────────┬─────────┘ │
│         │                  │           │
│         ▼                  ▼           │
│  ┌─────────────────────────────────┐   │
│  │     Trading Engine              │   │
│  │  - Binance BTC price proxy      │   │
│  │  - Signal engine (M15)          │   │
│  │  - Paper trading engine         │   │
│  │  - Risk management              │   │
│  └─────────────────────────────────┘   │
│                   │                    │
│                   ▼                    │
│  ┌─────────────────────────────────┐   │
│  │     Static Files (embedded)     │   │
│  │  - index.html                   │   │
│  │  - app.js                       │   │
│  │  - style.css (Tailwind)         │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

## Backend API

### REST Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/price` | Current BTC price |
| GET | `/api/markets` | Polymarket BTC markets |
| GET | `/api/signals` | Recent signals |
| GET | `/api/trades` | Trade history |
| GET | `/api/stats` | Trading statistics |
| GET | `/api/settings` | Current settings |
| POST | `/api/settings` | Update settings |
| POST | `/api/trades/export` | Export trades as JSON |

### WebSocket Events

| Event | Data | Frequency |
|-------|------|-----------|
| `price` | `{price, change_pct, timestamp}` | Every tick |
| `signal` | `{direction, confidence, timeframe}` | On M15 close |
| `trade` | `{entry, exit, pnl, status}` | On trade |
| `stats` | `{win_rate, pnl, drawdown}` | After trade |

## Frontend Layout

```
┌──────────────────────────────────────────────────────────────┐
│  LIVE TRADING DASHBOARD                       Capital: $10  │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌─────────────────────┐  ┌──────────────────────────────┐  │
│  │ BTC/USDT            │  │ MARKET                       │  │
│  │ $80,432.50 +0.12%  │  │ [15m] YES: $0.485 NO: $0.515│  │
│  │ Regime: TRENDING    │  │ [1h]  YES: $0.520 NO: $0.480│  │
│  └─────────────────────┘  └──────────────────────────────┘  │
│                                                              │
│  ┌─────────────────────┐  ┌──────────────────────────────┐  │
│  │ SIGNAL              │  │ OPEN POSITION                │  │
│  │ M15 UP (0.78)       │  │ UP @ $80,432 Size: $0.80    │  │
│  │ Mom:0.0012 Trending │  │ PnL: +$0.12 (+15%)          │  │
│  └─────────────────────┘  └──────────────────────────────┘  │
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ STATS                                                   ││
│  │ Trades: 45  Win: 53.3%  PnL: +$3.42  DD: 8.2%  PF:1.52││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ TRADE HISTORY                                           ││
│  │ 14:30 UP @ 78,400 → 78,450 +$0.42 ✓                   ││
│  │ 14:15 DOWN @ 78,500 → 78,420 +$0.38 ✓                 ││
│  └─────────────────────────────────────────────────────────┘│
│                                                              │
│  ┌─────────────────────────────────────────────────────────┐│
│  │ SETTINGS                    │ RISK MANAGEMENT           ││
│  │ Capital: [____] $10.00     │ Max Order: [____] $0.50  ││
│  │ Timeframe: [M15 ▼]        │ Stop Loss: [____] 0.5%   ││
│  │ Auto-trade: [✓]           │ Take Profit: [____] 1.0% ││
│  │                            │ [Save] [Export Trades]    ││
│  └─────────────────────────────────────────────────────────┘│
└──────────────────────────────────────────────────────────────┘
```

## Real-time Updates

### WebSocket Connection

```javascript
const ws = new WebSocket('ws://localhost:3001/ws');
ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  switch(data.type) {
    case 'price': updatePrice(data); break;
    case 'signal': updateSignal(data); break;
    case 'trade': updateTrade(data); break;
    case 'stats': updateStats(data); break;
  }
};
```

### Update Flow

1. Binance WebSocket → Backend receive price
2. Backend broadcast to all connected WebSocket clients
3. Frontend update DOM in real-time
4. No page refresh needed

## Settings & Controls

### Settings Panel

- Capital amount (editable)
- Timeframe selector (M15, H1, etc.)
- Auto-trade toggle
- Max order size
- Stop loss / Take profit percentages
- Save button → POST /api/settings

### Risk Management Panel

- Current drawdown display
- Max drawdown limit
- Position size calculator
- Emergency stop button

### Export

- Export trades as JSON file
- Download button → POST /api/trades/export

## BTC Price Proxy

### Embedded Proxy

- Fetch from Binance REST API every 10 seconds
- Cache price in memory
- Serve via `/api/price` endpoint
- Fallback to mock data if Binance unreachable

```rust
async fn fetch_btc_price() -> Result<f64> {
    let resp = reqwest::get("https://api.binance.com/api/v3/ticker/price?symbol=BTCUSDT").await?;
    let data: PriceResponse = resp.json().await?;
    Ok(data.price.parse()?)
}
```

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

## Dependencies

- `axum` — Web framework
- `tower-http` — Static file serving, CORS
- `tokio-tungstenite` — WebSocket support
- `rust-embed` — Embed static files in binary
- `tailwindcss` — CSS generation (build step)

## Testing

1. **Unit Tests**: API handlers, price proxy, state management
2. **Integration Test**: Run server, verify:
   - Static files serve correctly
   - REST endpoints return correct data
   - WebSocket broadcasts price updates
   - Settings save/load correctly
3. **Manual Test**: Open browser, verify all features work
