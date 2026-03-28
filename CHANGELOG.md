# Changelog

## [Unreleased]

## [v0.2.0] — 2026-03-27

### Added
- SNMP-based subnet auto-discovery from default gateway (ipAddrTable walk)
- Version number displayed in navigation bar
- Sortable table columns — click headers to sort by status, name, IP, type, latency
- Default sort by IP address (numeric octets) within each network group
- Sortable table headers persist across HTMX auto-refresh
- MAC address discovery via SNMP ifPhysAddress + ARP cache lookup
- MAC column in devices table
- MAC OUI-based vendor identification (MikroTik, Ubiquiti, Cisco, Amazon, Apple, etc.)
- MAC OUI-based device type classification (router, AP, switch, camera, phone)
- Only add monitoring services for infrastructure devices (router/switch/AP/firewall/server)
- Device labels (key-value attributes, like Kubernetes labels)
- Clear All Alerts button + API endpoint (`DELETE /api/alerts/clear`)
- ARP cache MAC lookup for directly-connected devices
- `LATEST_PROBES` redb table for O(1) latest-probe-per-service lookup

### Fixed
- Web UI hang — `get_latest_probe()` scanned entire probes table per service per device (O(N*M*P))
- Wrap all blocking redb calls in `spawn_blocking` to prevent async runtime starvation
- Reduce WebSocket noise — only broadcast probe events for Down/Degraded status changes
- Suppress device_discovered and probe toast spam — only show alerts and discovery_complete
- Existing devices get MAC backfilled on re-scan if missing

### Changed
- Remove hardcoded subnets from config — all subnets discovered dynamically via SNMP
- Discovery interval 300s→900s, monitoring concurrency 50→20 to reduce ping storms
- Batch-load `build_service_rows` — single table scan instead of N+1 queries per service
- Map filtered to Up/Degraded devices only

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
