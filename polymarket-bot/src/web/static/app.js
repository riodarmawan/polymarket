let ws = null;
let updownMarkets = [];

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
            case 'signal_5m':
                updateSignal(data, '5m');
                break;
            case 'updown':
                updateUpDownMarkets(data.markets);
                break;
            case 'trades':
                updateTrades(data.trades);
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
    document.getElementById('price-source').textContent = data.source;
}

// Update signal display
function updateSignal(data, timeframe = '15m') {
    const suffix = timeframe === '5m' ? '-5m' : '';
    const signalEl = document.getElementById(`signal${suffix}`);
    const detailsEl = document.getElementById(`signal${suffix}-details`);
    
    if (data && data.direction === 'WAIT') {
        signalEl.innerHTML = '<span class="yellow">WAIT</span>';
        detailsEl.innerHTML = `<span class="text-gray-400">${data.timeframe} | ${data.reason}<br>${data.execution_note || ''}</span>`;
    } else if (data && data.direction) {
        const color = data.direction === 'Up' ? 'green' : 'red';
        const arrow = data.direction === 'Up' ? '▲' : '▼';
        signalEl.innerHTML = `<span class="${color}">${arrow} ${data.direction}</span> (${(data.confidence * 100).toFixed(0)}%)`;
        detailsEl.innerHTML = `<span class="text-gray-400">${data.timeframe} | ${data.reason}<br>${data.execution_note || ''}</span>`;
    } else {
        signalEl.innerHTML = 'Waiting for signal...';
        detailsEl.innerHTML = '';
    }
}

// Update BTC Up/Down markets
function updateUpDownMarkets(markets) {
    updownMarkets = markets || [];
    renderMarketGroup('updown-container', updownMarkets.filter(m => m.interval === '15m'));
    renderMarketGroup('updown-5m-container', updownMarkets.filter(m => m.interval === '5m'));
}

function renderMarketGroup(containerId, markets) {
    const container = document.getElementById(containerId);
    if (!markets.length) {
        container.innerHTML = '<div class="text-gray-500">Scanner belum menerima data market</div>';
        return;
    }
    container.innerHTML = markets.map(m => {
        const remaining = Math.max(0, Math.floor(m.end_ts - Date.now() / 1000));
        const mins = Math.floor(remaining / 60);
        const secs = remaining % 60;
        const isLive = m.status === 'live';
        const isEnding = remaining <= 120;
        const countdown = `${mins}:${secs.toString().padStart(2, '0')}`;
        
        let statusClass = 'text-gray-500';
        let statusText = `${m.status.replaceAll('_', ' ')} · ${countdown}`;
        if (isLive && isEnding) {
            statusClass = 'text-red-400 pulse';
            statusText = `${countdown} LEFT!`;
        } else if (isLive) {
            statusClass = 'text-green-400';
            statusText = countdown;
        }
        
        const upAsk = m.up_best_ask ? `$${m.up_best_ask.toFixed(2)}` : '-';
        const upBid = m.up_best_bid ? `$${m.up_best_bid.toFixed(2)}` : '-';
        const downAsk = m.down_best_ask ? `$${m.down_best_ask.toFixed(2)}` : '-';
        const downBid = m.down_best_bid ? `$${m.down_best_bid.toFixed(2)}` : '-';
        const priceToBeat = m.price_to_beat > 0 ? `$${m.price_to_beat.toFixed(2)}` : '-';
        const currentPrice = m.current_price > 0 ? `$${m.current_price.toFixed(2)}` : '-';
        
        return `
            <div class="bg-gray-700 rounded p-3 ${isEnding ? 'border border-red-500' : ''}">
                <div class="flex justify-between items-center">
                    <div>
                        <span class="font-bold text-yellow-400 uppercase">${m.asset}</span>
                        <span class="text-gray-400 ml-2">${m.interval}</span>
                    </div>
                    <div class="${statusClass} font-bold">${statusText}</div>
                </div>
                <div class="grid grid-cols-2 gap-4 mt-2 text-sm">
                    <div>
                        <span class="text-green-400">UP:</span>
                        <span class="ml-2">Ask ${upAsk}</span>
                        <span class="ml-2">Bid ${upBid}</span>
                    </div>
                    <div>
                        <span class="text-red-400">DOWN:</span>
                        <span class="ml-2">Ask ${downAsk}</span>
                        <span class="ml-2">Bid ${downBid}</span>
                    </div>
                </div>
                <div class="text-xs text-gray-500 mt-1">
                    Price to beat: ${priceToBeat} |
                    Current: ${currentPrice} |
                    Spread: ${(m.spread * 100).toFixed(1)}%
                </div>
            </div>
        `;
    }).join('');
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

function updateTrades(trades) {
    const historyEl = document.getElementById('trade-history');
    if (!trades || trades.length === 0) {
        historyEl.innerHTML = '<div class="text-gray-500">No trades yet</div>';
        return;
    }

    historyEl.innerHTML = trades.map(t => {
        const pnl = t.pnl ?? 0;
        const color = t.status === 'open' ? 'yellow' : (pnl >= 0 ? 'green' : 'red');
        const result = t.status === 'open' ? 'OPEN' : `$${pnl.toFixed(2)}`;
        return `<div class="flex justify-between ${color}">
            <span><b>${t.timeframe || '15m'}</b> · ${t.direction} @ ${t.entry_price.toFixed(2)} · edge ${(t.edge * 100).toFixed(1)}%</span>
            <span>${result}</span>
        </div>`;
    }).join('');
}

// Update stats display
function updateStats(data) {
    renderStats('stats-15m', data['15m']);
    renderStats('stats-5m', data['5m']);
    document.getElementById('capital').textContent = data.current_capital.toFixed(2);
}

function renderStats(id, stats) {
    if (!stats) return;
    const pnlClass = stats.total_pnl >= 0 ? 'text-green-400' : 'text-red-400';
    document.getElementById(id).innerHTML = `
        <div>Capital<br><b>$${stats.current_capital.toFixed(2)}</b></div>
        <div>Trades<br><b>${stats.total_trades}</b></div>
        <div>Win<br><b>${(stats.win_rate * 100).toFixed(1)}%</b></div>
        <div>PnL<br><b class="${pnlClass}">$${stats.total_pnl.toFixed(2)}</b></div>
        <div>DD<br><b>${(stats.max_drawdown * 100).toFixed(1)}%</b></div>
        <div>PF<br><b>${stats.profit_factor.toFixed(2)}</b></div>`;
}

// Load initial data
async function loadInitialData() {
    try {
        const [priceRes, updownRes, statsRes, settingsRes, signalRes] = await Promise.all([
            fetch('/api/price'),
            fetch('/api/updown'),
            fetch('/api/stats'),
            fetch('/api/settings'),
            fetch('/api/signals')
        ]);
        
        const price = await priceRes.json();
        updatePrice(price);
        
        const updownData = await updownRes.json();
        updateUpDownMarkets(updownData.markets);
        
        const stats = await statsRes.json();
        updateStats(stats);
        
        const settings = await settingsRes.json();
        loadSettings(settings);
        
        const signalData = await signalRes.json();
        if (signalData.signal) {
            updateSignal(signalData.signal);
        }
        if (signalData.signal_5m) {
            updateSignal(signalData.signal_5m, '5m');
        }
    } catch (err) {
        console.error('Failed to load initial data:', err);
    }
}

// Load settings into form
function loadSettings(settings) {
    document.getElementById('setting-capital').value = settings.capital;
    document.getElementById('setting-max-order').value = settings.max_order;
    document.getElementById('setting-auto-trade').checked = settings.auto_trade;
    document.getElementById('setting-min-edge').value = settings.min_edge * 100;
    document.getElementById('setting-max-entry').value = settings.max_entry_price;
    document.getElementById('setting-risk').value = settings.risk_fraction * 100;
}

// Save settings
document.getElementById('save-settings').addEventListener('click', async () => {
    const settings = {
        capital: parseFloat(document.getElementById('setting-capital').value),
        max_order: parseFloat(document.getElementById('setting-max-order').value),
        timeframe: '15m',
        auto_trade: document.getElementById('setting-auto-trade').checked,
        min_edge: parseFloat(document.getElementById('setting-min-edge').value) / 100,
        max_entry_price: parseFloat(document.getElementById('setting-max-entry').value),
        risk_fraction: parseFloat(document.getElementById('setting-risk').value) / 100
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

// Keep countdown moving locally between API/WebSocket updates.
setInterval(() => updateUpDownMarkets(updownMarkets), 1000);

// Refresh market metadata and orderbooks every 10 seconds.
setInterval(async () => {
    try {
        const res = await fetch('/api/updown');
        const data = await res.json();
        updateUpDownMarkets(data.markets);
    } catch (err) {
        console.error('Failed to refresh updown markets:', err);
    }
}, 10000);
