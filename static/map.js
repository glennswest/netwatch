// Netwatch — Interactive SVG Network Map

const SVG_NS = 'http://www.w3.org/2000/svg';
const NODE_RADIUS = 24;
const STATUS_COLORS = {
    up: '#4caf7d',
    down: '#e55b5b',
    degraded: '#e5a54b',
    unknown: '#6b7084',
};

let viewBox = { x: 0, y: 0, w: 1200, h: 800 };
let scale = 1;
let isPanning = false;
let panStart = { x: 0, y: 0 };
let dragNode = null;
let dragOffset = { x: 0, y: 0 };
let svg, linksGroup, nodesGroup;

function initMap() {
    svg = document.getElementById('network-map');
    if (!svg) return;

    // Set initial viewBox
    svg.setAttribute('viewBox', `${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`);

    // Create layer groups
    linksGroup = createSvgElement('g', { id: 'links-layer' });
    nodesGroup = createSvgElement('g', { id: 'nodes-layer' });
    svg.appendChild(linksGroup);
    svg.appendChild(nodesGroup);

    // Render
    renderLinks();
    renderNodes();

    // Event listeners
    svg.addEventListener('mousedown', onMouseDown);
    svg.addEventListener('mousemove', onMouseMove);
    svg.addEventListener('mouseup', onMouseUp);
    svg.addEventListener('wheel', onWheel, { passive: false });

    // Touch support
    svg.addEventListener('touchstart', onTouchStart, { passive: false });
    svg.addEventListener('touchmove', onTouchMove, { passive: false });
    svg.addEventListener('touchend', onTouchEnd);
}

function createSvgElement(tag, attrs) {
    const el = document.createElementNS(SVG_NS, tag);
    if (attrs) {
        for (const [k, v] of Object.entries(attrs)) {
            el.setAttribute(k, v);
        }
    }
    return el;
}

function renderLinks() {
    linksGroup.innerHTML = '';
    for (const link of MAP_LINKS) {
        const src = MAP_DEVICES.find(d => d.id === link.source);
        const tgt = MAP_DEVICES.find(d => d.id === link.target);
        if (!src || !tgt) continue;

        const line = createSvgElement('line', {
            x1: src.x, y1: src.y,
            x2: tgt.x, y2: tgt.y,
            stroke: '#353a4d',
            'stroke-width': '2',
            'data-link-id': link.id,
            'data-source': link.source,
            'data-target': link.target,
        });
        linksGroup.appendChild(line);
    }
}

function renderNodes() {
    nodesGroup.innerHTML = '';
    for (const device of MAP_DEVICES) {
        const g = createSvgElement('g', {
            'data-device-id': device.id,
            transform: `translate(${device.x}, ${device.y})`,
            style: 'cursor: pointer;',
        });

        // Status ring
        const statusColor = STATUS_COLORS[device.status] || STATUS_COLORS.unknown;
        g.appendChild(createSvgElement('circle', {
            cx: 0, cy: 0, r: NODE_RADIUS + 3,
            fill: 'none',
            stroke: statusColor,
            'stroke-width': '2.5',
            opacity: '0.8',
        }));

        // Device circle
        g.appendChild(createSvgElement('circle', {
            cx: 0, cy: 0, r: NODE_RADIUS,
            fill: device.icon_color,
            opacity: '0.9',
        }));

        // Icon letter
        const text = createSvgElement('text', {
            x: 0, y: 1,
            'text-anchor': 'middle',
            'dominant-baseline': 'middle',
            fill: '#fff',
            'font-size': '16',
            'font-weight': '700',
            'font-family': '-apple-system, sans-serif',
        });
        text.textContent = device.icon_letter;
        g.appendChild(text);

        // Name label
        const label = createSvgElement('text', {
            x: 0, y: NODE_RADIUS + 16,
            'text-anchor': 'middle',
            fill: '#e4e6ed',
            'font-size': '11',
            'font-family': '-apple-system, sans-serif',
        });
        label.textContent = device.name;
        g.appendChild(label);

        // IP label
        const ipLabel = createSvgElement('text', {
            x: 0, y: NODE_RADIUS + 28,
            'text-anchor': 'middle',
            fill: '#6b7084',
            'font-size': '10',
            'font-family': 'SF Mono, monospace',
        });
        ipLabel.textContent = device.ip;
        g.appendChild(ipLabel);

        // Latency badge (if available)
        if (device.latency_us) {
            const ms = Math.round(device.latency_us / 1000);
            const badge = createSvgElement('text', {
                x: NODE_RADIUS + 4, y: -NODE_RADIUS + 4,
                fill: ms > 100 ? '#e5a54b' : '#4caf7d',
                'font-size': '9',
                'font-family': 'SF Mono, monospace',
            });
            badge.textContent = ms + 'ms';
            g.appendChild(badge);
        }

        nodesGroup.appendChild(g);
    }
}

function updateLinkPositions() {
    const lines = linksGroup.querySelectorAll('line');
    for (const line of lines) {
        const srcId = line.getAttribute('data-source');
        const tgtId = line.getAttribute('data-target');
        const src = MAP_DEVICES.find(d => d.id === srcId);
        const tgt = MAP_DEVICES.find(d => d.id === tgtId);
        if (src && tgt) {
            line.setAttribute('x1', src.x);
            line.setAttribute('y1', src.y);
            line.setAttribute('x2', tgt.x);
            line.setAttribute('y2', tgt.y);
        }
    }
}

// ── Mouse interactions ──

function svgPoint(clientX, clientY) {
    const pt = svg.createSVGPoint();
    pt.x = clientX;
    pt.y = clientY;
    return pt.matrixTransform(svg.getScreenCTM().inverse());
}

function onMouseDown(e) {
    const nodeG = e.target.closest('g[data-device-id]');
    if (nodeG) {
        const deviceId = nodeG.getAttribute('data-device-id');
        const device = MAP_DEVICES.find(d => d.id === deviceId);
        if (device) {
            dragNode = device;
            const pt = svgPoint(e.clientX, e.clientY);
            dragOffset.x = pt.x - device.x;
            dragOffset.y = pt.y - device.y;
            e.preventDefault();
            return;
        }
    }

    // Pan
    isPanning = true;
    panStart.x = e.clientX;
    panStart.y = e.clientY;
}

function onMouseMove(e) {
    if (dragNode) {
        const pt = svgPoint(e.clientX, e.clientY);
        dragNode.x = pt.x - dragOffset.x;
        dragNode.y = pt.y - dragOffset.y;

        const g = nodesGroup.querySelector(`g[data-device-id="${dragNode.id}"]`);
        if (g) {
            g.setAttribute('transform', `translate(${dragNode.x}, ${dragNode.y})`);
        }
        updateLinkPositions();
        e.preventDefault();
        return;
    }

    if (isPanning) {
        const dx = (e.clientX - panStart.x) / scale;
        const dy = (e.clientY - panStart.y) / scale;
        viewBox.x -= dx;
        viewBox.y -= dy;
        svg.setAttribute('viewBox', `${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`);
        panStart.x = e.clientX;
        panStart.y = e.clientY;
    }
}

function onMouseUp(e) {
    if (dragNode) {
        // Save position to server
        savePosition(dragNode.id, dragNode.x, dragNode.y);
        dragNode = null;
    }
    isPanning = false;
}

function onWheel(e) {
    e.preventDefault();
    const pt = svgPoint(e.clientX, e.clientY);
    const factor = e.deltaY > 0 ? 1.1 : 0.9;

    viewBox.x = pt.x - (pt.x - viewBox.x) * factor;
    viewBox.y = pt.y - (pt.y - viewBox.y) * factor;
    viewBox.w *= factor;
    viewBox.h *= factor;
    scale /= factor;

    svg.setAttribute('viewBox', `${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`);
}

// ── Touch interactions ──

function onTouchStart(e) {
    if (e.touches.length === 1) {
        const touch = e.touches[0];
        const nodeG = touch.target.closest('g[data-device-id]');
        if (nodeG) {
            const deviceId = nodeG.getAttribute('data-device-id');
            const device = MAP_DEVICES.find(d => d.id === deviceId);
            if (device) {
                dragNode = device;
                const pt = svgPoint(touch.clientX, touch.clientY);
                dragOffset.x = pt.x - device.x;
                dragOffset.y = pt.y - device.y;
                e.preventDefault();
            }
        }
    }
}

function onTouchMove(e) {
    if (dragNode && e.touches.length === 1) {
        const pt = svgPoint(e.touches[0].clientX, e.touches[0].clientY);
        dragNode.x = pt.x - dragOffset.x;
        dragNode.y = pt.y - dragOffset.y;
        const g = nodesGroup.querySelector(`g[data-device-id="${dragNode.id}"]`);
        if (g) g.setAttribute('transform', `translate(${dragNode.x}, ${dragNode.y})`);
        updateLinkPositions();
        e.preventDefault();
    }
}

function onTouchEnd() {
    if (dragNode) {
        savePosition(dragNode.id, dragNode.x, dragNode.y);
        dragNode = null;
    }
}

// ── Actions ──

async function savePosition(deviceId, x, y) {
    try {
        await fetch('/api/map/positions', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ device_id: deviceId, x: Math.round(x * 10) / 10, y: Math.round(y * 10) / 10 }),
        });
    } catch (e) {
        console.error('Failed to save position:', e);
    }
}

async function autoLayout() {
    try {
        const resp = await fetch('/api/map/auto-layout', { method: 'POST' });
        const positions = await resp.json();
        for (const pos of positions) {
            const device = MAP_DEVICES.find(d => d.id === pos.device_id);
            if (device) {
                device.x = pos.x;
                device.y = pos.y;
            }
        }
        renderNodes();
        updateLinkPositions();
    } catch (e) {
        console.error('Auto layout failed:', e);
    }
}

function zoomIn() {
    const cx = viewBox.x + viewBox.w / 2;
    const cy = viewBox.y + viewBox.h / 2;
    viewBox.w *= 0.8;
    viewBox.h *= 0.8;
    viewBox.x = cx - viewBox.w / 2;
    viewBox.y = cy - viewBox.h / 2;
    scale /= 0.8;
    svg.setAttribute('viewBox', `${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`);
}

function zoomOut() {
    const cx = viewBox.x + viewBox.w / 2;
    const cy = viewBox.y + viewBox.h / 2;
    viewBox.w *= 1.25;
    viewBox.h *= 1.25;
    viewBox.x = cx - viewBox.w / 2;
    viewBox.y = cy - viewBox.h / 2;
    scale /= 1.25;
    svg.setAttribute('viewBox', `${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`);
}

function resetView() {
    viewBox = { x: 0, y: 0, w: 1200, h: 800 };
    scale = 1;
    svg.setAttribute('viewBox', `${viewBox.x} ${viewBox.y} ${viewBox.w} ${viewBox.h}`);
}

// Initialize when DOM is ready
document.addEventListener('DOMContentLoaded', initMap);
