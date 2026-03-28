// Netwatch — common JavaScript

// WebSocket for live updates
let ws = null;
let wsReconnectTimer = null;

function connectWebSocket() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    ws = new WebSocket(`${proto}//${location.host}/ws`);

    ws.onopen = () => {
        console.log('WebSocket connected');
        if (wsReconnectTimer) {
            clearTimeout(wsReconnectTimer);
            wsReconnectTimer = null;
        }
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleWsMessage(msg);
        } catch (e) {
            console.warn('WS parse error:', e);
        }
    };

    ws.onclose = () => {
        console.log('WebSocket disconnected, reconnecting...');
        wsReconnectTimer = setTimeout(connectWebSocket, 3000);
    };

    ws.onerror = () => {
        ws.close();
    };
}

function handleWsMessage(msg) {
    switch (msg.event) {
        case 'alert':
            showToast(msg.message, msg.severity === 'critical' ? 'error' : 'info');
            break;
        case 'device_discovered':
            showToast(`New device discovered: ${msg.ip}`, 'success');
            break;
        case 'probe':
            // Could update specific device status indicators
            break;
        case 'discovery_complete':
            showToast('Discovery scan complete', 'info');
            break;
    }
}

function showToast(message, level) {
    const container = document.getElementById('toast-container');
    if (!container) return;

    const toast = document.createElement('div');
    toast.className = `toast toast-${level || 'info'}`;
    toast.textContent = message;
    container.appendChild(toast);

    setTimeout(() => {
        toast.style.opacity = '0';
        toast.style.transform = 'translateX(20px)';
        toast.style.transition = 'all 0.3s';
        setTimeout(() => toast.remove(), 300);
    }, 5000);
}

// HTMX event handling
document.addEventListener('htmx:afterRequest', (event) => {
    if (event.detail.failed) {
        showToast('Request failed: ' + (event.detail.xhr?.statusText || 'unknown error'), 'error');
    }
});

document.addEventListener('htmx:afterSwap', (event) => {
    // Success feedback for POST/PUT/DELETE
    const method = event.detail.requestConfig?.verb;
    if (method === 'post' || method === 'put' || method === 'delete') {
        if (!event.detail.failed) {
            showToast('Operation successful', 'success');
        }
    }
});

// ── Table sorting ──

function sortTable(th) {
    const table = th.closest('table');
    const tbody = table.querySelector('tbody');
    const rows = Array.from(tbody.querySelectorAll('tr'));
    const colIdx = Array.from(th.parentNode.children).indexOf(th);
    const sortType = th.dataset.sort || 'text';
    const asc = th.classList.contains('sort-asc');

    // Clear sort indicators from sibling headers
    th.parentNode.querySelectorAll('th').forEach(h => h.classList.remove('sort-asc', 'sort-desc'));
    th.classList.add(asc ? 'sort-desc' : 'sort-asc');

    rows.sort((a, b) => {
        const aText = a.children[colIdx]?.textContent.trim() || '';
        const bText = b.children[colIdx]?.textContent.trim() || '';
        let cmp = 0;

        if (sortType === 'ip') {
            cmp = compareIp(aText, bText);
        } else if (sortType === 'num') {
            cmp = parseNum(aText) - parseNum(bText);
        } else {
            cmp = aText.localeCompare(bText, undefined, { sensitivity: 'base' });
        }
        return asc ? -cmp : cmp;
    });

    rows.forEach(r => tbody.appendChild(r));
}

function compareIp(a, b) {
    const pa = a.split('.').map(Number);
    const pb = b.split('.').map(Number);
    for (let i = 0; i < 4; i++) {
        if ((pa[i] || 0) !== (pb[i] || 0)) return (pa[i] || 0) - (pb[i] || 0);
    }
    return 0;
}

function parseNum(s) {
    const n = parseFloat(s);
    return isNaN(n) ? -1 : n;
}

// Initialize
document.addEventListener('DOMContentLoaded', () => {
    connectWebSocket();
});
