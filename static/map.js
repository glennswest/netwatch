// Netwatch — Interactive SVG Network Map

const SVG_NS = 'http://www.w3.org/2000/svg';
const NODE_RADIUS = 24;
const STATUS_COLORS = {
    up: '#4caf7d',
    down: '#e55b5b',
    degraded: '#e5a54b',
    unknown: '#6b7084',
};

// SVG icon paths for each device type, designed for a 48x48 viewBox centered at 0,0
const DEVICE_ICONS = {
    router: [
        // Two curved arrows (routing symbol)
        'M-3,-8 L3,-8 L3,-4 L-3,-4 Z',
        'M-3,4 L3,4 L3,8 L-3,8 Z',
        'M-8,-3 L-4,-3 L-4,3 L-8,3 Z',
        'M4,-3 L8,-3 L8,3 L4,3 Z',
        'M-1,-4 L1,-4 L1,4 L-1,4 Z',
        'M-4,-1 L4,-1 L4,1 L-4,1 Z',
    ],
    switch: [
        // Horizontal box with port indicators
        'M-9,-4 L9,-4 L9,4 L-9,4 Z',
        'M-7,-2 L-5,-2 L-5,0 L-7,0 Z',
        'M-3,-2 L-1,-2 L-1,0 L-3,0 Z',
        'M1,-2 L3,-2 L3,0 L1,0 Z',
        'M5,-2 L7,-2 L7,0 L5,0 Z',
        'M6,1 L7,1 L7,2.5 L6,2.5 Z',
    ],
    server: [
        // Stacked rack units
        'M-6,-8 L6,-8 L6,-3 L-6,-3 Z',
        'M-6,-2 L6,-2 L6,3 L-6,3 Z',
        'M-6,4 L6,4 L6,8 L-6,8 Z',
        'M-4,-6.5 L-3,-6.5 L-3,-4.5 L-4,-4.5 Z',
        'M-4,-0.5 L-3,-0.5 L-3,1.5 L-4,1.5 Z',
        'M-4,5.5 L-3,5.5 L-3,6.5 L-4,6.5 Z',
    ],
    firewall: [
        // Shield shape
        'M0,-9 L8,-5.5 L8,1 Q8,7 0,9.5 Q-8,7 -8,1 L-8,-5.5 Z',
        // Inner line
        'M0,-5 L0,6',
        'M-4,-3 L4,-3',
    ],
    ap: [
        // Radio tower with waves
        'M-1,3 L1,3 L2,8 L-2,8 Z',
        'M-4,8 L4,8 L4,9 L-4,9 Z',
        'M0,3 L0,-2',
        // Waves (arcs)
        'M-4,-2 Q0,-7 4,-2',
        'M-7,-1 Q0,-9 7,-1',
    ],
    printer: [
        // Paper tray + body
        'M-4,-7 L4,-7 L4,-3 L-4,-3 Z',
        'M-7,-3 L7,-3 L7,3 L-7,3 Z',
        'M-4,3 L4,3 L4,7 L-4,7 Z',
        'M4,-1 L6,-1 L6,1 L4,1 Z',
    ],
    camera: [
        // Camera body + lens
        'M-7,-4 L4,-4 L4,4 L-7,4 Z',
        'M4,-2.5 L8,-5 L8,5 L4,2.5 Z',
    ],
    phone: [
        // Smartphone
        'M-3,-8 L3,-8 Q4,-8 4,-7 L4,7 Q4,8 3,8 L-3,8 Q-4,8 -4,7 L-4,-7 Q-4,-8 -3,-8 Z',
        'M-3,-6 L3,-6 L3,5 L-3,5 Z',
    ],
    other: [
        // Circle with question mark
        'M0,-9 A9,9 0 1,0 0.01,-9 Z',
        'M-2,-3 Q-2,-6 0,-6 Q2,-6 2,-3 Q2,-1 0,0 L0,2',
        'M-0.8,4 L0.8,4 L0.8,5.5 L-0.8,5.5 Z',
    ],
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

function renderDeviceIcon(g, device, radius) {
    const paths = DEVICE_ICONS[device.type] || DEVICE_ICONS.other;
    const iconScale = radius / 14; // scale to fit inside circle

    const iconG = createSvgElement('g', {
        transform: `scale(${iconScale})`,
        fill: '#fff',
        stroke: '#fff',
        'stroke-width': '0.5',
        'stroke-linejoin': 'round',
    });

    for (const d of paths) {
        // Detect if path is a line (moveto + lineto only, no area)
        const isStroke = /^M[^Z]*$/.test(d) && !d.includes('L') || (d.match(/[ML]/g) || []).length <= 2 && !d.includes('Z') && !d.includes('Q') && !d.includes('A');
        const el = createSvgElement('path', {
            d: d,
            fill: d.includes('Z') || d.includes('A') || d.includes('Q') ? '#fff' : 'none',
            stroke: '#fff',
            'stroke-width': d.includes('Z') ? '0' : '1.5',
            'stroke-linecap': 'round',
        });
        iconG.appendChild(el);
    }

    g.appendChild(iconG);
}

function renderNodes() {
    nodesGroup.innerHTML = '';
    for (const device of MAP_DEVICES) {
        const isMultiHomed = device.additional_ips && device.additional_ips.length > 0;
        const nodeRadius = isMultiHomed ? NODE_RADIUS + 8 : NODE_RADIUS;

        const g = createSvgElement('g', {
            'data-device-id': device.id,
            transform: `translate(${device.x}, ${device.y})`,
            style: 'cursor: pointer;',
        });

        // Status ring
        const statusColor = STATUS_COLORS[device.status] || STATUS_COLORS.unknown;
        g.appendChild(createSvgElement('circle', {
            cx: 0, cy: 0, r: nodeRadius + 3,
            fill: 'none',
            stroke: statusColor,
            'stroke-width': '2.5',
            opacity: '0.8',
        }));

        // Device circle
        g.appendChild(createSvgElement('circle', {
            cx: 0, cy: 0, r: nodeRadius,
            fill: device.icon_color,
            opacity: '0.9',
        }));

        // SVG device icon
        renderDeviceIcon(g, device, nodeRadius);

        // Port dots for multi-homed devices
        if (isMultiHomed) {
            const totalPorts = device.additional_ips.length + 1;
            for (let i = 0; i < totalPorts; i++) {
                const angle = (2 * Math.PI * i) / totalPorts - Math.PI / 2;
                const px = (nodeRadius + 1) * Math.cos(angle);
                const py = (nodeRadius + 1) * Math.sin(angle);
                g.appendChild(createSvgElement('circle', {
                    cx: px, cy: py, r: 3,
                    fill: '#fff',
                    stroke: device.icon_color,
                    'stroke-width': '1.5',
                }));
            }
        }

        // Name label
        let labelY = nodeRadius + 16;
        const label = createSvgElement('text', {
            x: 0, y: labelY,
            'text-anchor': 'middle',
            fill: '#e4e6ed',
            'font-size': '11',
            'font-family': '-apple-system, sans-serif',
        });
        label.textContent = device.name;
        g.appendChild(label);

        // Primary IP label
        labelY += 12;
        const ipLabel = createSvgElement('text', {
            x: 0, y: labelY,
            'text-anchor': 'middle',
            fill: '#6b7084',
            'font-size': '10',
            'font-family': 'SF Mono, monospace',
        });
        ipLabel.textContent = device.ip;
        g.appendChild(ipLabel);

        // Additional IPs for multi-homed devices
        if (isMultiHomed) {
            for (const addIp of device.additional_ips) {
                labelY += 11;
                const addIpLabel = createSvgElement('text', {
                    x: 0, y: labelY,
                    'text-anchor': 'middle',
                    fill: '#6b7084',
                    'font-size': '9',
                    'font-family': 'SF Mono, monospace',
                    opacity: '0.7',
                });
                addIpLabel.textContent = addIp;
                g.appendChild(addIpLabel);
            }
        }

        // Latency badge (if available)
        if (device.latency_us) {
            const ms = Math.round(device.latency_us / 1000);
            const badge = createSvgElement('text', {
                x: nodeRadius + 4, y: -nodeRadius + 4,
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
