// Netwatch — common JavaScript

// WebSocket for live updates
let ws = null;
let wsReconnectTimer = null;

function connectWebSocket() {
    const proto = location.protocol === 'https:' ? 'wss:' : 'ws:';
    ws = new WebSocket(`${proto}//${location.host}/ws`);

    ws.onopen = () => {
        if (wsReconnectTimer) {
            clearTimeout(wsReconnectTimer);
            wsReconnectTimer = null;
        }
    };

    ws.onmessage = (event) => {
        try {
            const msg = JSON.parse(event.data);
            handleWsMessage(msg);
        } catch (e) {}
    };

    ws.onclose = () => {
        wsReconnectTimer = setTimeout(connectWebSocket, 5000);
    };

    ws.onerror = () => { ws.close(); };
}

function handleWsMessage(msg) {
    switch (msg.event) {
        case 'alert':
            showToast(msg.message, msg.severity === 'critical' ? 'error' : 'info');
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
    const method = event.detail.requestConfig?.verb;
    if (method === 'post' || method === 'put' || method === 'delete') {
        if (!event.detail.failed) {
            showToast('Operation successful', 'success');
        }
    }
    // Re-apply sort state after HTMX table refresh
    reapplySorts();
});

// ── Table sorting ──

// Track sort state per content container
const sortState = {};

function sortTable(th) {
    const table = th.closest('table');
    const section = table.closest('details');
    const tbody = table.querySelector('tbody');
    const rows = Array.from(tbody.querySelectorAll('tr'));
    const colIdx = Array.from(th.parentNode.children).indexOf(th);
    const sortType = th.dataset.sort || 'text';
    const asc = th.classList.contains('sort-asc');
    const dir = asc ? 'desc' : 'asc';

    // Clear sort indicators from sibling headers
    th.parentNode.querySelectorAll('th').forEach(h => h.classList.remove('sort-asc', 'sort-desc'));
    th.classList.add('sort-' + dir);

    // Save state keyed by network section name
    const sectionName = section ? section.querySelector('.network-name')?.textContent.trim() : '_global';
    const contentDiv = th.closest('[id]');
    const pageKey = contentDiv ? contentDiv.id : location.pathname;
    sortState[pageKey] = sortState[pageKey] || {};
    sortState[pageKey][sectionName] = { colIdx, sortType, dir };

    doSort(tbody, rows, colIdx, sortType, dir);
}

function doSort(tbody, rows, colIdx, sortType, dir) {
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
        return dir === 'asc' ? cmp : -cmp;
    });
    rows.forEach(r => tbody.appendChild(r));
}

function reapplySorts() {
    for (const [pageKey, sections] of Object.entries(sortState)) {
        for (const [sectionName, state] of Object.entries(sections)) {
            // Find the matching table
            document.querySelectorAll('details.network-section, .table-wrap').forEach(el => {
                const name = el.querySelector('.network-name')?.textContent.trim() || '_global';
                if (name !== sectionName) return;
                const table = el.querySelector('table');
                if (!table) return;
                const tbody = table.querySelector('tbody');
                const rows = Array.from(tbody.querySelectorAll('tr'));
                const ths = table.querySelectorAll('thead th');

                // Set visual indicator
                ths.forEach(h => h.classList.remove('sort-asc', 'sort-desc'));
                if (ths[state.colIdx]) ths[state.colIdx].classList.add('sort-' + state.dir);

                doSort(tbody, rows, state.colIdx, state.sortType, state.dir);
            });
        }
    }
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
