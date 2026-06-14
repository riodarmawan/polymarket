# Live Trading Dashboard Design

## Overview

A real-time terminal UI dashboard for paper trading BTC Up/Down markets on Polymarket. The dashboard connects to Binance WebSocket for live BTC price data, generates M15 trading signals using the regime-adaptive strategy, and simulates virtual trades with dynamic risk management.

## Goals

- Provide a live trading simulation experience
- Validate the M15 strategy in real-time conditions
- Track virtual positions and PnL without risking real money

## Non-Goals

- Real money trading
- Polymarket API integration (paper trading only)
- Multi-asset trading (BTC only)

## Architecture

```
polymarket-bot live --capital 10.0
         │
         ▼
┌─────────────────────────────────────────┐
│           LiveDashboard                 │
├─────────────────────────────────────────┤
│  ┌─────────────┐  ┌──────────────────┐ │
│  │ BinanceWs   │  │ SignalEngine     │ │
│  │ (real-time  │  │ (M15 only,       │ │
│  │  BTC price) │  │  regime-adaptive)│ │
│  └──────┬──────┘  └────────┬─────────┘ │
│         │                  │           │
│         ▼                  ▼           │
│  ┌─────────────────────────────────┐   │
│  │     Paper Trading Engine        │   │
│  │  - Virtual position tracking    │   │
│  │  - Dynamic risk management      │   │
│  │  - PnL calculation              │   │
│  └─────────────────────────────────┘   │
│                   │                    │
│                   ▼                    │
│  ┌─────────────────────────────────┐   │
│  │       TUI Renderer (ratatui)    │   │
│  │  - Price chart                  │   │
│  │  - Signal display               │   │
│  │  - Trade history                │   │
│  │  - Portfolio stats              │   │
│  └─────────────────────────────────┘   │
└─────────────────────────────────────────┘
```

### Components

1. **LiveDashboard** — Main orchestrator, coordinates all components
2. **BinanceWsClient** — Reuse existing WebSocket client for real-time BTC price
3. **SignalEngine** — Reuse existing signal engine (M15 only mode)
4. **PaperTradingEngine** — New: virtual positions, PnL, risk management
5. **TuiRenderer** — New: ratatui-based terminal UI

## Data Flow

```
Binance WebSocket (1m candles)
         │
         ▼
    Buffer 15 candles → Aggregate to M15
         │
         ▼
    SignalEngine.generate_signal()
         │
         ▼
    PaperTradingEngine.execute_signal()
         │
         ▼
    TuiRenderer.update()
```

### Timing

- **BTC Price**: Real-time updates (every tick from WebSocket)
- **Signal Check**: Every 15 minutes (when M15 candle closes)
- **Dashboard Refresh**: Every 1 second

## TUI Layout

```
╔══════════════════════════════════════════════════════════════╗
║  LIVE TRADING DASHBOARD                    Capital: $10.00  ║
╠══════════════════════════════════════════════════════════════╣
║                                                              ║
║  BTC/USDT: $78,432.50  ▲ +0.12%    Regime: TRENDING         ║
║  ─────────────────────────────────────────────────────────── ║
║                                                              ║
║  CURRENT SIGNAL:                                             ║
║  ┌─────────────────────────────────────────────────────────┐ ║
║  │ Direction: UP (confidence: 0.78)                        │ ║
║  │ Timeframe: M15    Reason: Mom:0.0012 Trending           │ ║
║  │ Entry: $78,432    Target: $78,500    Stop: $78,365      │ ║
║  └─────────────────────────────────────────────────────────┘ ║
║                                                              ║
║  OPEN POSITION:                                              ║
║  ┌─────────────────────────────────────────────────────────┐ ║
║  │ UP @ $78,432  Size: $0.80  PnL: +$0.12 (+15.0%)       │ ║
║  │ Duration: 12m 30s          SL: $78,365  TP: $78,500    │ ║
║  └─────────────────────────────────────────────────────────┘ ║
║                                                              ║
║  STATS:                                                      ║
║  ┌─────────────────────────────────────────────────────────┐ ║
║  │ Trades: 45  Win: 24 (53.3%)  PnL: +$3.42 (+34.2%)     │ ║
║  │ Drawdown: 8.2%  Max DD: 15.3%  PF: 1.52               │ ║
║  │ Win Streak: 3  Loss Streak: 2                           │ ║
║  └─────────────────────────────────────────────────────────┘ ║
║                                                              ║
║  TRADE HISTORY (last 5):                                     ║
║  ┌─────────────────────────────────────────────────────────┐ ║
║  │ 14:30  UP   @ 78,400 → 78,450  +$0.42  ✓              │ ║
║  │ 14:15  DOWN @ 78,500 → 78,420  +$0.38  ✓              │ ║
║  │ 14:00  UP   @ 78,350 → 78,400  +$0.35  ✓              │ ║
║  │ 13:45  DOWN @ 78,550 → 78,500  +$0.40  ✓              │ ║
║  │ 13:30  UP   @ 78,300 → 78,250  -$0.50  ✗              │ ║
║  └─────────────────────────────────────────────────────────┘ ║
║                                                              ║
║  Last signal: 2m ago    Next check: 13m    Uptime: 1h 23m   ║
╚══════════════════════════════════════════════════════════════╝
```

## Trading Logic

### Signal Generation

- **Timeframe**: M15 only (no multi-timeframe)
- **Regime Detection**: Trending → Momentum, Ranging → Mean Reversion
- **Skip**: Choppy regime (no trades)

### Position Sizing

- **Base Fraction**: 10% of capital
- **Drawdown Scaling**:
  - DD > 25%: 50% size
  - DD > 15%: 75% size
  - DD ≤ 15%: 100% size
- **Min Order**: $0.10
- **Max Order**: Configurable (default $0.50)

### Exit Logic

- **Stop Loss**: 0.5% from entry price
- **Take Profit**: 1.0% from entry price (1:2 R:R)
- **Timeout**: Close after 15 minutes if neither SL nor TP hit

## CLI Interface

```bash
# Basic usage
polymarket-bot live --capital 10.0

# With custom risk settings
polymarket-bot live --capital 20.0 --max-order 1.0

# With trade logging
polymarket-bot live --capital 10.0 --log trades.json
```

### Arguments

| Argument | Default | Description |
|----------|---------|-------------|
| `--capital` | 2.0 | Initial virtual capital in USD |
| `--max-order` | 0.50 | Maximum order size in USD |
| `--log` | None | Path to trade log file (JSON) |

## Error Handling

- **WebSocket Disconnect**: Auto-reconnect with exponential backoff (1s, 2s, 4s, max 30s)
- **Binance API Error**: Show warning banner, continue with last known price
- **No Signal**: Display "Waiting for signal..." with countdown to next M15 close
- **Position Timeout**: Auto-close position, log as timeout trade

## File Structure

```
src/crypto/
├── live/
│   ├── mod.rs          # LiveDashboard orchestrator
│   ├── paper_trading.rs # PaperTradingEngine
│   └── tui.rs          # TuiRenderer (ratatui)
├── binance_ws.rs       # Reuse existing
├── signals.rs          # Reuse existing
├── indicators.rs       # Reuse existing
└── mod.rs              # Update with new module
```

## Dependencies

- `ratatui` — Terminal UI framework
- `crossterm` — Terminal manipulation
- `tokio` — Async runtime (already in use)
- `tungstenite` — WebSocket client (already in use)

## Testing

1. **Unit Tests**: Paper trading engine logic (position sizing, PnL calculation)
2. **Integration Test**: Run dashboard for 30 minutes, verify:
   - WebSocket connects and receives price data
   - Signals generate at M15 close times
   - Trades execute with correct sizing
   - PnL calculates correctly
   - Dashboard renders without errors
3. **Manual Test**: Visual inspection of TUI layout and real-time updates
