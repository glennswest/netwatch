# CLAUDE.md — Netwatch Project Instructions

## Project Overview

**Netwatch** — Network monitoring and topology mapping tool. Replaces MikroTik Dude.
Discovers devices via ICMP/SNMP/LLDP, monitors services, displays interactive SVG topology map.

## Current Version: `v0.4.0`

### Version Location
```
Cargo.toml → version = "0.4.0"
```
Version is read at compile time via `env!("CARGO_PKG_VERSION")` in `src/lib.rs`.

---

## Tech Stack

- **Language:** Rust (edition 2021)
- **Web framework:** axum 0.8 (REST API + WebSocket)
- **Templates:** Askama 0.12 (HTMX web UI)
- **Database:** redb 2 (embedded, pure Rust)
- **Async runtime:** tokio
- **Config:** TOML (serde)
- **HTTP client:** reqwest (rustls-tls)
- **Email:** lettre (SMTP with rustls)
- **Target:** aarch64-unknown-linux-musl (ARM64 static binary for scratch container)

## Key Directories

```
src/main.rs        — CLI entry point (clap)
src/lib.rs         — Library root, VERSION constant
src/config.rs      — TOML config parsing with defaults
src/models.rs      — Device, Service, Subnet, Alert, Probe data models
src/db.rs          — redb database layer, retention cleanup
src/dns.rs         — Custom DNS PTR client (raw UDP, compression pointer support)
src/snmp.rs        — SNMP v1/v2c client (BER/ASN.1, GET/GETNEXT/WALK)
src/discovery.rs   — Auto-discovery engine (ICMP sweep, SNMP, LLDP/CDP)
src/monitor.rs     — Service monitoring (ICMP, TCP, HTTP, DNS, SNMP probes)
src/alert.rs       — Alert engine (email + webhook notifications)
src/topo.rs        — Topology / link discovery
src/web/mod.rs     — Axum router, static asset embedding (rust-embed)
src/web/api.rs     — REST API handlers
src/web/pages.rs   — HTML page handlers (Askama templates)
src/web/ws.rs      — WebSocket live update handler
templates/         — Askama HTML templates (HTMX)
static/            — JS and CSS (app.js, style.css, map.js)
deploy/            — Container deployment configs (stormd-config.toml, netwatch.toml)
Containerfile      — Container image build (stormdbase + netwatch binary)
Dockerfile         — Alternate scratch-based container image
```

## Build & Deploy

### Build Commands
```bash
# Cross-compile for ARM64 (container target)
cargo build --release --target aarch64-unknown-linux-musl

# Force re-embed static files before build
touch src/web/mod.rs

# Build container image
podman build --platform linux/arm64 -t registry.gt.lo:5000/netwatch:edge -f Containerfile .

# Push to registry
podman push --tls-verify=false registry.gt.lo:5000/netwatch:edge

# Trigger redeploy
curl -s -X POST http://192.168.200.2:8082/api/v1/images/redeploy \
  -H 'Content-Type: application/json' \
  -d '{"image":"registry.gt.lo:5000/netwatch:edge"}'
```

### Build & Deploy Checklist
1. Edit code
2. `touch src/web/mod.rs` (force rust_embed to re-embed static files)
3. `cargo build --release --target aarch64-unknown-linux-musl`
4. `podman build --platform linux/arm64 -t registry.gt.lo:5000/netwatch:edge -f Containerfile .`
5. `podman push --tls-verify=false registry.gt.lo:5000/netwatch:edge`
6. `curl -s -X POST http://192.168.200.2:8082/api/v1/images/redeploy -H 'Content-Type: application/json' -d '{"image":"registry.gt.lo:5000/netwatch:edge"}'`
7. Wait ~45s for container restart
8. git commit + push + tag if version bump

### Important Build Notes
- Image tag is **`edge`**, NOT `latest`
- Pod listens on port **80** (not 8080)
- Pod is in namespace `infra`, name `netwatch`, static IP `192.168.200.7`
- Registry: `registry.gt.lo:5000`
- mkube API: `http://192.168.200.2:8082`
- `touch src/web/mod.rs` before build to force rust_embed re-embed

## Infrastructure Context

- **mkube** = container orchestrator at `http://192.168.200.2:8082` — use REST API, NEVER ssh
- **rose/rose1** = physical MikroTik rack appliance (gateway 192.168.200.1), NOT a container
- Netwatch monitors multiple subnets (gw, gt, g8, g9, g10, g11, g88, g216)
- rose1 is split into physical device + virtual per-subnet bridge devices
- Virtual devices have `is_virtual: true` — skipped by LLDP and multi-homed consolidation

## Work Plan

### Completed (v0.4.0)
- Auto-discovery (ICMP sweep, SNMP, LLDP/CDP neighbor detection)
- Service monitoring (ICMP, TCP, HTTP/HTTPS, DNS, SNMP)
- Interactive SVG network topology map with drag-and-drop
- Topology-driven BFS tree layout
- Real-time alerts (email SMTP + webhook)
- Performance tracking with Chart.js
- WebSocket live updates
- HTMX web UI with dark theme
- redb embedded database with retention cleanup
- Multi-homed device consolidation (SNMP sysName + hostname-stem)
- Per-subnet DNS discovery and PTR resolution
- Internet node with latency monitoring
- Virtual device support (rose1 physical + per-subnet bridges)
- Cross-subnet hostname-stem consolidation

### Pending
- [ ] Further map layout improvements
- [ ] Device grouping / custom map annotations

### Release History

| Version | Date | Summary |
|---------|------|---------|
| v0.2.0 | 2026-03-27 | SNMP discovery, sortable tables, MAC OUI, device labels |
| v0.3.0 | 2026-03-27 | Infrastructure page, DNS discovery, multi-homed devices, SVG icons |
| v0.4.0 | 2026-03-29 | Internet node, BFS tree layout, virtual devices, cross-subnet consolidation |
