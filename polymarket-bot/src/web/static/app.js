let ws = null;
let updownMarkets = [];
let latestSignals = {};
let auditRows = [];
let auditPage = 1;
let auditPageSize = 15;
let auditFilters = {
    timeframe: 'all',
    status: 'all',
    reason: ''
};

function htmlEscape(value) {
    return String(value ?? '')
        .replaceAll('&', '&amp;')
        .replaceAll('<', '&lt;')
        .replaceAll('>', '&gt;')
        .replaceAll('"', '&quot;')
        .replaceAll("'", '&#039;');
}

function formatMoney(value, decimals = 2) {
    if (value === null || value === undefined) return '-';
    return Number.isFinite(Number(value)) ? `$${Number(value).toFixed(decimals)}` : '-';
}

function formatPct(value, decimals = 1) {
    if (value === null || value === undefined) return '-';
    return Number.isFinite(Number(value)) ? `${(Number(value) * 100).toFixed(decimals)}%` : '-';
}

function fixedToFloat(value) {
    if (value === null || value === undefined) return null;
    const number = Number(value);
    if (!Number.isFinite(number)) return null;
    return Math.abs(number) >= 10000 ? number / 1000000 : number;
}

function formatFixedMoney(value, decimals = 2) {
    return formatMoney(fixedToFloat(value), decimals);
}

function formatFixedPct(value, decimals = 1) {
    return formatPct(fixedToFloat(value), decimals);
}

function formatDate(value) {
    if (!value) return '-';
    const date = typeof value === 'number' ? new Date(value) : new Date(value);
    return Number.isNaN(date.getTime()) ? htmlEscape(value) : date.toLocaleTimeString();
}

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
    latestSignals[timeframe] = data;
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
    renderModelRiskSnapshot();
}

function renderModelRiskSnapshot() {
    const el = document.getElementById('model-risk-snapshot');
    if (!el) return;
    const s15 = latestSignals['15m'];
    const s5 = latestSignals['5m'];
    const parts = [];
    if (s15) {
        parts.push(`15m ${htmlEscape(s15.direction)} ${formatPct(s15.confidence, 0)} · ${htmlEscape(s15.execution_note || s15.reason || '')}`);
    }
    if (s5) {
        parts.push(`5m ${htmlEscape(s5.direction)} ${formatPct(s5.confidence, 0)} · ${htmlEscape(s5.execution_note || s5.reason || '')}`);
    }
    el.innerHTML = parts.length ? parts.join('<br>') : 'Waiting for model state...';
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
        const dataReady = m.data_status === 'ready';
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
            <div class="panel-soft p-3 ${isEnding ? 'border-red-500' : (dataReady ? '' : 'border-yellow-600')}">
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
                <div class="text-xs ${dataReady ? 'text-green-400' : 'text-yellow-400'} mt-1">
                    Data: ${m.data_status || 'unknown'} · ${m.data_detail || ''}
                </div>
            </div>
        `;
    }).join('');
}

async function refreshHealth() {
    try {
        const health = await (await fetch('/api/health')).json();
        const el = document.getElementById('data-health');
        const autoLive = health.production_control?.auto_live_execution_enabled
            ? 'AUTO LIVE ARMED'
            : 'AUTO LIVE OFF';
        const walletReady = health.production_control?.reconciliation_ready
            ? 'wallet reconciled'
            : 'wallet not reconciled';
        const liveSwitch = health.production_control?.live_switch_enabled
            ? 'live switch on'
            : 'paper/no live orders';
        el.textContent = `DATA ${health.overall.toUpperCase()} · markets ${health.markets.ready}/${health.markets.total} · ${autoLive} · ${walletReady} · ${liveSwitch}`;
        el.className = health.overall === 'ready' && health.production_control?.reconciliation_ready
            ? 'text-green-400 text-sm font-bold'
            : 'text-yellow-400 text-sm font-bold';
    } catch (err) {
        const el = document.getElementById('data-health');
        el.textContent = 'DATA UNAVAILABLE';
        el.className = 'text-red-400 text-sm font-bold';
    }
}

async function refreshAccount() {
    const statusEl = document.getElementById('account-status');
    const detailEl = document.getElementById('account-detail');
    try {
        const account = await (await fetch('/api/account')).json();
        if (!account.connected) {
            const reason = account.reason || 'Remote account could not be checked';
            const isMissingSecret = reason.includes('POLYMARKET_PRIVATE_KEY') || reason.includes('required');
            const isTls = reason.includes('certificate') || reason.includes('TLS') || reason.includes('clob.polymarket.com');
            statusEl.textContent = isMissingSecret
                ? 'Paper dashboard: wallet credentials not loaded'
                : 'Remote account unavailable';
            statusEl.className = 'text-sm text-red-400';
            detailEl.textContent = isMissingSecret
                ? 'Current balance shown in Shared Capital is local paper accounting. Wallet USDC is not being used by this dashboard process.'
                : (isTls
                    ? 'Authenticated CLOB check is blocked by network/TLS verification. Wallet balance cannot be verified safely from here.'
                    : `Reason: ${reason}`);
            detailEl.className = isMissingSecret || isTls ? 'text-xs text-yellow-400 mt-2' : 'text-xs text-gray-500 mt-2';
            document.getElementById('remote-balance').textContent = '-';
            document.getElementById('remote-allowances').textContent = '-';
            document.getElementById('remote-open-orders').textContent = '-';
            document.getElementById('remote-positions').textContent = '-';
            return;
        }
        statusEl.textContent = 'Authenticated CLOB account connected';
        statusEl.className = 'text-sm text-green-400';
        detailEl.textContent = 'Wallet data is read from the authenticated Polymarket CLOB account.';
        detailEl.className = 'text-xs text-green-400 mt-2';
        const balance = account.collateral_balance_usd ?? account.collateral_balance ?? 0;
        document.getElementById('remote-balance').textContent = `$${Number(balance).toFixed(2)}`;
        document.getElementById('remote-allowances').textContent = account.allowance_contracts ?? '-';
        document.getElementById('remote-open-orders').textContent = account.open_orders ?? '-';
        document.getElementById('remote-positions').textContent = (account.positions || []).length;
    } catch (err) {
        statusEl.textContent = 'Remote account check failed';
        statusEl.className = 'text-sm text-red-400';
        detailEl.textContent = 'The dashboard could not call the account endpoint.';
        detailEl.className = 'text-xs text-red-400 mt-2';
    }
}

async function refreshProductionReadiness() {
    try {
        const [readiness, forward] = await Promise.all([
            fetch('/api/production-readiness').then(r => r.json()),
            fetch('/api/forward-report').then(r => r.json())
        ]);
        const runtimeEl = document.getElementById('runtime-status');
        runtimeEl.textContent = `${readiness.runtime.environment} · ${readiness.runtime.mode} · ${readiness.runtime.strategy_version}`;
        const capitalLabel = document.getElementById('capital-label');
        const capitalSubtitle = document.getElementById('capital-subtitle');
        if (capitalLabel && capitalSubtitle) {
            if (readiness.runtime.mode === 'paper') {
                capitalLabel.textContent = 'Local Shared Capital';
                capitalSubtitle.textContent = 'Paper balance only; wallet USDC is not changing';
                capitalSubtitle.className = 'text-xs text-yellow-400';
            } else {
                capitalLabel.textContent = 'Shared Capital';
                capitalSubtitle.textContent = 'Runtime is not paper; verify wallet panel before trading';
                capitalSubtitle.className = 'text-xs text-green-400';
            }
        }

        const canaryEl = document.getElementById('canary-status');
        const blockersEl = document.getElementById('canary-blockers');
        if (readiness.canary_ready) {
            canaryEl.textContent = 'READY for operator-live review';
            canaryEl.className = 'text-sm text-green-400 font-bold';
            blockersEl.innerHTML = '<div class="text-green-400">All required operator-live gates passed.</div>';
        } else {
            canaryEl.textContent = `${readiness.blockers.length} blocker(s) remain`;
            canaryEl.className = 'text-sm text-yellow-400 font-bold';
            blockersEl.innerHTML = readiness.blockers
                .map(b => `<div class="mb-1">• ${b}</div>`)
                .join('');
        }

        const forwardEl = document.getElementById('forward-status');
        const detailEl = document.getElementById('forward-detail');
        forwardEl.textContent = forward.promotion_ready
            ? 'Promotion metrics ready'
            : 'Collecting forward evidence';
        forwardEl.className = forward.promotion_ready
            ? 'text-sm text-green-400 font-bold'
            : 'text-sm text-yellow-400 font-bold';
        detailEl.innerHTML = `
            <div>Settled: <b>${forward.settled_trades}</b> / 200</div>
            <div>Opportunities: <b>${forward.opportunities}</b>, approved: <b>${forward.approved}</b></div>
            <div>${(forward.promotion_reasons || []).join('<br>')}</div>
        `;
        renderRejectionSummary(forward.rejection_reasons || {});
    } catch (err) {
        document.getElementById('canary-status').textContent = 'Readiness unavailable';
        document.getElementById('canary-status').className = 'text-sm text-red-400';
    }
}

function renderRejectionSummary(reasons) {
    const el = document.getElementById('rejection-summary');
    if (!el) return;
    const entries = Object.entries(reasons).sort((a, b) => b[1] - a[1]);
    if (!entries.length) {
        el.innerHTML = '<div class="text-gray-500">No rejected opportunities recorded yet</div>';
        return;
    }
    el.innerHTML = entries.map(([reason, count]) => `
        <div class="flex justify-between gap-3 mb-1">
            <span class="text-gray-300">${htmlEscape(reason)}</span>
            <b class="text-yellow-400">${count}</b>
        </div>
    `).join('');
}

async function refreshExecutionAudit() {
    const statusEl = document.getElementById('execution-audit-status');
    try {
        const data = await fetch('/api/execution-audit').then(r => r.json());
        auditRows = data.rows || [];
        renderExecutionAudit();
        statusEl.textContent = `${auditRows.length} latest risk decisions loaded`;
        statusEl.className = 'text-xs text-green-400';
    } catch (err) {
        statusEl.textContent = 'Execution audit unavailable';
        statusEl.className = 'text-xs text-red-400';
    }
}

function filteredAuditRows() {
    const needle = auditFilters.reason.trim().toLowerCase();
    return auditRows.filter(row => {
        if (auditFilters.timeframe !== 'all' && row.timeframe !== auditFilters.timeframe) {
            return false;
        }
        if (auditFilters.status === 'approved' && !row.approved) {
            return false;
        }
        if (auditFilters.status === 'rejected' && row.approved) {
            return false;
        }
        if (!needle) {
            return true;
        }
        const failed = (row.failed_checks || []).map(check => `${check.code} ${check.detail}`).join(' ');
        return `${row.reason_code} ${failed} ${row.market_slug}`.toLowerCase().includes(needle);
    });
}

function renderExecutionAudit() {
    const body = document.getElementById('execution-audit-table');
    if (!body) return;
    const rows = filteredAuditRows();
    const totalPages = Math.max(1, Math.ceil(rows.length / auditPageSize));
    auditPage = Math.min(Math.max(1, auditPage), totalPages);
    const start = (auditPage - 1) * auditPageSize;
    const pageRows = rows.slice(start, start + auditPageSize);
    updateAuditPagination(rows.length, totalPages);

    if (!pageRows.length) {
        body.innerHTML = '<tr><td colspan="7" class="text-gray-500">No risk decisions match the current filters</td></tr>';
        return;
    }
    body.innerHTML = pageRows.map(row => {
        const failed = row.failed_checks || [];
        const firstFailed = failed[0];
        const failedText = firstFailed
            ? `<b class="text-yellow-400">${htmlEscape(firstFailed.code)}</b><br><span class="text-gray-500">${htmlEscape(firstFailed.detail)}</span>`
            : '<span class="text-green-400">all checks passed</span>';
        const extraFailed = failed.length > 1
            ? `<div class="text-gray-500 mt-1">+${failed.length - 1} other failed check(s)</div>`
            : '';
        const approvedClass = row.approved ? 'text-green-400 border-green-700' : 'text-yellow-400 border-yellow-700';
        const approval = row.approved ? 'APPROVED' : 'REJECTED';
        const meta = row.execution_metadata || {};
        const market = row.market_data || {};
        const intent = row.intent || {};
        const context = row.context || {};
        const sizing = context.sizing || {};
        const entryRules = context.entry_rules || {};
        const requested = intent.requested_usd ?? sizing.requested_usd ?? null;
        const modelMargin = intent.model_margin ?? null;
        const depth = row.direction === 'Up' ? meta.up_executable_depth_usd : meta.down_executable_depth_usd;
        const expectedPrice = sizing.expected_fill_price ?? row.expected_fill_price ?? (row.direction === 'Up' ? meta.up_expected_fill_price : meta.down_expected_fill_price);
        const expectedShares = sizing.expected_shares ?? null;
        const minShares = sizing.min_order_size_shares ?? meta.min_order_size ?? null;
        const minRequired = sizing.min_required_usd ?? null;
        const entryWindow = entryRules.entry_window_start_secs !== undefined
            ? `${entryRules.elapsed_secs ?? '-'}s / ${entryRules.entry_window_start_secs}-${entryRules.entry_window_end_secs}s`
            : '-';

        return `
            <tr>
                <td class="text-gray-300 whitespace-nowrap">
                    ${formatDate(row.created_at)}<br>
                    <span class="text-gray-500">#${row.id}</span>
                </td>
                <td><span class="pill text-cyan-300">${htmlEscape(row.timeframe)}</span></td>
                <td>
                    <b class="${row.direction === 'Down' ? 'text-red-400' : 'text-green-400'}">${htmlEscape(row.direction || '-')}</b>
                    <div class="text-gray-500">conf ${formatPct(row.confidence, 0)}</div>
                    <div class="text-gray-500 max-w-[220px] truncate" title="${htmlEscape(row.market_slug)}">${htmlEscape(row.market_slug)}</div>
                </td>
                <td>
                    <span class="pill ${approvedClass}">${approval}</span>
                    <div class="text-gray-500 mt-1">${htmlEscape(row.reason_code)}</div>
                </td>
                <td>${failedText}${extraFailed}</td>
                <td>
                    <div>${htmlEscape(market.status || 'unknown')} · ${htmlEscape(market.detail || '')}</div>
                    <div class="text-gray-500">token map: ${market.token_mapping_valid === true ? 'ok' : 'no'}</div>
                    <div class="text-gray-500">spread ${formatPct(row.spread)} · clock ${meta.clock_drift_ms ?? '-'}ms</div>
                    <div class="text-gray-500">entry ${htmlEscape(entryWindow)}</div>
                </td>
                <td>
                    <div>ask ${formatMoney(expectedPrice, 3)} · depth ${formatMoney(depth, 2)}</div>
                    <div class="text-gray-500">shares ${expectedShares?.toFixed ? expectedShares.toFixed(2) : '-'} / min ${minShares ?? '-'} · min ${formatMoney(minRequired, 2)}</div>
                    <div class="text-gray-500">fee ${row.fee_rate_bps ?? meta.fee_rate_bps ?? '-'} bps</div>
                    <div class="text-gray-500">request ${formatFixedMoney(requested, 2)} · margin ${formatFixedPct(modelMargin)}</div>
                </td>
            </tr>
        `;
    }).join('');
}

function updateAuditPagination(totalRows, totalPages) {
    const info = document.getElementById('audit-page-info');
    const prev = document.getElementById('audit-prev');
    const next = document.getElementById('audit-next');
    if (info) {
        const start = totalRows === 0 ? 0 : (auditPage - 1) * auditPageSize + 1;
        const end = Math.min(totalRows, auditPage * auditPageSize);
        info.textContent = `${start}-${end} of ${totalRows} · page ${auditPage}/${totalPages}`;
    }
    if (prev) prev.disabled = auditPage <= 1;
    if (next) next.disabled = auditPage >= totalPages;
    [prev, next].filter(Boolean).forEach(button => {
        button.classList.toggle('opacity-40', button.disabled);
        button.classList.toggle('cursor-not-allowed', button.disabled);
    });
}

function wireAuditControls() {
    const timeframe = document.getElementById('audit-filter-timeframe');
    const status = document.getElementById('audit-filter-status');
    const reason = document.getElementById('audit-filter-reason');
    const pageSize = document.getElementById('audit-page-size');
    const prev = document.getElementById('audit-prev');
    const next = document.getElementById('audit-next');

    timeframe?.addEventListener('change', () => {
        auditFilters.timeframe = timeframe.value;
        auditPage = 1;
        renderExecutionAudit();
    });
    status?.addEventListener('change', () => {
        auditFilters.status = status.value;
        auditPage = 1;
        renderExecutionAudit();
    });
    reason?.addEventListener('input', () => {
        auditFilters.reason = reason.value;
        auditPage = 1;
        renderExecutionAudit();
    });
    pageSize?.addEventListener('change', () => {
        auditPageSize = parseInt(pageSize.value, 10) || 15;
        auditPage = 1;
        renderExecutionAudit();
    });
    prev?.addEventListener('click', () => {
        auditPage = Math.max(1, auditPage - 1);
        renderExecutionAudit();
    });
    next?.addEventListener('click', () => {
        auditPage += 1;
        renderExecutionAudit();
    });
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
            <span><b>${t.timeframe || '15m'}</b> · ${t.direction} @ ${t.entry_price.toFixed(2)} · model margin ${(t.edge * 100).toFixed(1)}%</span>
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
        <div class="metric">Shared Capital<br><b>$${stats.current_capital.toFixed(2)}</b></div>
        <div class="metric">Trades<br><b>${stats.total_trades}</b></div>
        <div class="metric">Win<br><b>${(stats.win_rate * 100).toFixed(1)}%</b></div>
        <div class="metric">PnL<br><b class="${pnlClass}">$${stats.total_pnl.toFixed(2)}</b></div>
        <div class="metric">DD<br><b>${(stats.max_drawdown * 100).toFixed(1)}%</b></div>
        <div class="metric">PF<br><b>${stats.profit_factor.toFixed(2)}</b></div>`;
}

// Load initial data
async function loadInitialData() {
    const loadJson = async (url) => {
        const res = await fetch(url);
        if (!res.ok) throw new Error(`${url} returned ${res.status}`);
        return res.json();
    };

    const loaders = [
        loadJson('/api/price').then(updatePrice),
        loadJson('/api/updown').then(data => updateUpDownMarkets(data.markets)),
        loadJson('/api/stats').then(updateStats),
        loadJson('/api/settings').then(loadSettings),
        loadJson('/api/signals').then(data => {
            if (data.signal) updateSignal(data.signal);
            if (data.signal_5m) updateSignal(data.signal_5m, '5m');
        }),
        loadJson('/api/trades').then(data => updateTrades(data.trades)),
    ];

    const results = await Promise.allSettled(loaders);
    const failed = results.filter(result => result.status === 'rejected');
    if (failed.length) {
        console.warn('Some dashboard panels failed to load', failed.map(result => result.reason?.message || result.reason));
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
    const statusEl = document.getElementById('settings-save-status');
    const settings = {
        capital: parseFloat(document.getElementById('setting-capital').value),
        max_order: parseFloat(document.getElementById('setting-max-order').value),
        timeframe: '15m',
        auto_trade: document.getElementById('setting-auto-trade').checked,
        min_edge: parseFloat(document.getElementById('setting-min-edge').value) / 100,
        max_entry_price: parseFloat(document.getElementById('setting-max-entry').value),
        risk_fraction: parseFloat(document.getElementById('setting-risk').value) / 100
    };

    const res = await fetch('/api/settings', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(settings)
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok || data.status === 'rejected') {
        statusEl.textContent = data.reason || 'settings rejected';
        statusEl.className = 'text-xs text-red-400';
        await loadInitialData();
        return;
    }

    statusEl.textContent = 'Settings saved';
    statusEl.className = 'text-xs text-green-400';
    await loadInitialData();
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
wireAuditControls();
connectWebSocket();
loadInitialData();
refreshHealth();
refreshAccount();
refreshProductionReadiness();
refreshExecutionAudit();

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

setInterval(refreshHealth, 5000);
setInterval(refreshProductionReadiness, 15000);
setInterval(refreshAccount, 30000);
setInterval(refreshExecutionAudit, 10000);
