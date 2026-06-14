# Crypto Cross-Timeframe Trend Following Design

## Overview

Redesign the Polymarket bot to trade crypto prediction markets (BTC Up/Down) using a cross-timeframe trend following strategy. Focus on statistical/quantitative approach with $0.50 per trade, $2 total capital.

## Problem Statement

The current backtesting engine targets long-term prediction markets (GTA VI, FIFA World Cup) which have low volatility and generate 0 trades. The user wants to shift to high-frequency crypto markets that resolve every 5 minutes.

## Market Structure

Polymarket offers "BTC Up or Down" markets at multiple timeframes:
- **5 Min** — $14M volume, resolves every 5 minutes
- **15 Min** — 7 markets
- **1 Hour** — 9 markets
- **4 Hours** — 7 markets
- **Daily** — 11 markets

Resolution: "Up" if BTC price at end >= price at start (Chainlink BTC/USD data stream).

## Design: Cross-Timeframe Trend Following

### Core Concept

Use longer timeframe trends (1h, 4h) to predict shorter timeframe direction (5m, 15m). If BTC is trending up on 1h chart, bet "Up" on 5m markets.

### Components

#### 1. BTC Price Feed (Binance)
- **Source:** Binance BTC/USDT public websocket
- **Endpoint:** `wss://stream.binance.com:9443/ws/btcusdt@kline_1m`
- **Candles:** 1m, 5m, 15m, 1h, 4h, daily
- **No API key needed** for public market data

#### 2. Signal Engine
- **Trend Detection:** EMA 20 vs EMA 50 crossover
- **Strength:** ADX > 25 = trending, < 20 = ranging
- **Filter:** RSI 30-70 = neutral zone
- **Cross-timeframe:** 1h trend confirms 5m direction

#### 3. Market Matcher
- Fetch active Polymarket crypto markets via proxy
- Match signal timeframe to market timeframe
- Example: 5m "Up" signal → bet on 5m Up market

#### 4. Trade Execution
- $0.50 per trade
- Max 1 trade per timeframe per hour
- Stop loss: if capital < $0.50, stop trading

### Data Flow

```
1. Binance WebSocket → Real-time BTC price
2. Calculate indicators per timeframe
3. Generate signal: {timeframe: "5m", direction: "Up", confidence: 0.72}
4. Find matching Polymarket market: "BTC Up or Down 5m - 3:00-3:05"
5. Check: is market price < (1 - confidence)? → Edge exists
6. Execute: buy "Up" at $0.50 if edge > 10%
```

### Risk Management

- **Position Size:** $0.50 fixed
- **Max Trades:** 1 per timeframe per hour
- **Capital Reserve:** Stop if cash < $0.50
- **Diversification:** Spread across timeframes (5m/15m/1h/4h)

### Key Metrics

- Win rate per timeframe
- Avg edge (confidence - market_price)
- Total P&L
- Sharpe ratio

## Implementation Plan

### Phase 1: Data Layer
1. Add Binance websocket client for real-time BTC price
2. Store candles in SQLite database
3. Calculate technical indicators (EMA, ADX, RSI)

### Phase 2: Signal Engine
1. Implement cross-timeframe signal generation
2. Add confidence scoring
3. Backtest with historical data

### Phase 3: Trade Execution
1. Monitor Polymarket crypto markets via proxy
2. Match signals to markets
3. Execute trades via CLOB API

### Phase 4: Monitoring
1. Add real-time dashboard
2. Track performance metrics
3. Alert on significant events

## Technical Stack

- **Language:** Rust
- **Database:** SQLite (existing)
- **Price Feed:** Binance WebSocket (no API key)
- **Market Data:** Polymarket Gamma API via proxy
- **Trading:** Polymarket CLOB API via proxy

## Success Criteria

- Win rate > 55% across all timeframes
- Positive P&L after 100 trades
- Sharpe ratio > 1.0
- Max drawdown < 20%
