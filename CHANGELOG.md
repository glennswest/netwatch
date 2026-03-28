# Changelog

## [Unreleased]

### 2026-03-27
- **fix:** Web UI hang — `get_latest_probe()` scanned entire probes table per service per device (O(N*M*P))
- **fix:** Add `LATEST_PROBES` redb table for O(1) latest-probe-per-service lookup
- **fix:** Wrap all blocking redb calls in `spawn_blocking` to prevent async runtime starvation
- **perf:** Dashboard, devices, map, services, alerts pages now respond instantly instead of hanging
- **perf:** Batch-load `build_service_rows` — single table scan instead of N+1 queries per service
- **perf:** Map filtered to Up/Degraded devices only — no more rendering 500+ down nodes
- **feat:** Sortable table columns — click headers to sort by status, name, IP, type, latency
- **feat:** Default sort by IP address (numeric octets) within each network group
- **fix:** Reduce WebSocket noise — only broadcast probe events for Down/Degraded status changes
- **feat:** Add g1, g8, g9 subnets (renamed gw to g1); now covers g1/g8/g9/g10/g11/gt
- **feat:** MAC address discovery via SNMP ifPhysAddress + ARP cache lookup
- **feat:** MAC column in devices table
- **feat:** Sortable table headers persist across HTMX auto-refresh
- **fix:** Suppress device_discovered and probe toast spam — only show alerts and discovery_complete
- **fix:** Force rust_embed to pick up updated static assets (sorting CSS/JS)
- **perf:** Discovery interval 300s→900s, monitoring concurrency 50→20 to reduce ping storms
- **feat:** MAC OUI-based vendor identification (MikroTik, Ubiquiti, Cisco, Amazon, Apple, etc.)
- **feat:** MAC OUI-based device type classification (router, AP, switch, camera, phone)
- **feat:** Only add monitoring services for infrastructure devices (router/switch/AP/firewall/server)
- **feat:** Device labels (key-value attributes, like Kubernetes labels)
- **feat:** Clear All Alerts button + API endpoint (`DELETE /api/alerts/clear`)
- **feat:** ARP cache MAC lookup for directly-connected devices
- **fix:** Existing devices get MAC backfilled on re-scan if missing
- **feat:** SNMP-based subnet auto-discovery from default gateway (ipAddrTable walk)
- **feat:** Version number displayed in navigation bar
- **refactor:** Remove hardcoded subnets from config — all subnets discovered dynamically via SNMP

### 2026-03-26
- **feat:** Initial project creation — complete network monitoring app
- **feat:** Auto-discovery engine (ICMP ping sweep, port scan, SNMP system info)
- **feat:** SNMP v1/v2c client (BER/ASN.1, GET/GETNEXT/WALK)
- **feat:** Multi-vendor device identification (MikroTik, Cisco, Ubiquiti, Juniper, etc.)
- **feat:** Service monitoring (ICMP, TCP, HTTP/HTTPS, DNS, SNMP probes)
- **feat:** Interactive SVG network topology map with drag-and-drop
- **feat:** Force-directed auto-layout for network map
- **feat:** Real-time alerts with email (SMTP) and webhook notifications
- **feat:** Performance tracking with Chart.js graphs
- **feat:** LLDP/CDP neighbor discovery for automatic link detection
- **feat:** HTMX web UI with dark theme (matches stormd/mkube design)
- **feat:** WebSocket live updates for alerts and discovery events
- **feat:** redb embedded database — pure Rust, no C dependencies
- **feat:** Configurable retention cleanup for probes, metrics, alerts
- **feat:** Single static binary for scratch containers
- **feat:** TOML configuration with sensible defaults
- **fix:** Replace native_db with redb (native_db derive macros incompatible)
- **fix:** Askama templates — use display helper methods instead of arithmetic
- **fix:** socket2 ICMP — use libc::SOCK_RAW and MaybeUninit buffers
- **fix:** Resolve all compiler warnings (unused imports)
- **feat:** ARM64 cross-compilation + stormdbase container deployment
- **feat:** Containerfile using stormd as PID 1 supervisor
- **feat:** Deploy config (stormd-config.toml, netwatch.toml with all subnets)
- **feat:** mkube pod spec with auto-update and DNS alias (netwatch.gt.lo)
- **chore:** Deployed to MikroTik via mkube at 192.168.200.23
- **feat:** Network-organized UI — devices and services grouped by subnet
- **feat:** Collapsible per-network sections with summary badges (devices/up/down/svc)
- **fix:** Changed default listen port from 8080 to 80
