# Changelog

## [Unreleased]

### 2026-03-29
- **feat:** Columnar subnet map layout — each /24 subnet gets a vertical column with its switch on top and devices stacked in rows of 4 below
- **feat:** Internet → Routers → Subnet columns hierarchy (top to bottom)
- **feat:** Auto-fit viewBox — map auto-zooms to fit all devices with 120px padding on load
- **feat:** Persistent initial placement — new devices get hierarchical positions saved to DB on first map load
- **fix:** Icon rendering — replaced fragile heuristic path detection with explicit fill/stroke icon format
- **fix:** WiFi arcs, globe longitude curves, camera dome correctly stroked instead of filled
- **fix:** "Reset View" button now fits to content instead of hardcoded 1200x800
- **fix:** "Auto Layout" uses columnar subnet placement instead of force-directed (better for network topologies)
- **feat:** UpdateDevice API now supports `additional_ips` field for multi-homed device management
- **fix:** Merged duplicate rose1 device (192.168.216.1 consolidated into rose1 at 192.168.9.1)
- **fix:** Removed 5 bogus/duplicate LLDP links from duplicate rose1 device
- **feat:** `is_virtual` boolean field on Device — distinguishes physical switches (configman backups) from virtual bridges
- **fix:** Reclassified router.gw.lo (192.168.1.254) as switch1.gw.lo (Switch type)
- **feat:** Created virtual bridge.gt.lo (rose1 internal bridge for g200/gt network)
- **feat:** Created virtual switch8.gw.lo (rose1 internal bridge for g8 network)
- **feat:** Split rose1 multi-homed device into physical rose1.gw.lo + virtual routers per subnet (g8, g9, g10, g11, g88, g216, gt)
- **feat:** Master rose1 device as central hub — all virtual subnet bridges connect to it
- **fix:** LLDP discovery skips virtual devices — prevents bogus cross-links between virtual bridges
- **fix:** Multi-homed consolidation skips virtual devices — prevents discovery from merging IPs into manually-curated devices
- **fix:** Gateway auto-detection picks lowest RFC1918 IP from SNMP ipAddrTable — avoids using virtual bridge IPs as canonical gateway
- **feat:** Master `rose1` device as central hub with per-subnet virtual routers (rose1.gw.lo, rose1.g8.lo, rose1.g9.lo, rose1.g10.lo, rose1.g11.lo, rose1.g88.lo, rose1.g216.lo, rose1.gt.lo)

## [v0.4.0] — 2026-03-29

### Added
- Internet node — virtual device pinging 1.1.1.1 (configurable) with latency indicator on map
- Internet device auto-links to gateway router on network map
- `internet_target` config option in `[discovery]` section (default: `1.1.1.1`)
- All discovered devices now get ICMP Ping monitoring (not just infrastructure)
- Map shows Unknown-status devices (gray ring) — no longer hidden until first probe
- New `Internet` device type with globe icon and green/teal color (#43b581)
- Redesigned all SVG device icons — router (directional arrows), switch (wide chassis with ports), server (rack units with LEDs), firewall (shield with checkmark), AP (WiFi arcs), printer, camera (dome), phone, internet (globe with grid), other (monitor)

### Fixed
- Devices classified as "Other" were invisible — no ICMP service, `monitor=false` label, status stuck on Unknown
- Removed `monitor=false` label from non-infrastructure devices

### 2026-03-28
- **fix:** Ping sweep connected sockets — each ping uses connect() to filter replies to target IP only
- **fix:** ICMP reply handling — retry loop handles destination-unreachable (type 3) and skips non-matching responses
- **fix:** Proper IP header offset detection for DGRAM vs RAW ICMP sockets
- **fix:** Run initial subnet scan immediately on startup instead of waiting 15 minutes
- **fix:** Skip non-RFC1918 networks from ping sweep (was scanning public 24.158.x.x/22)
- **fix:** DNS PTR resolution — try per-subnet DNS servers (non-gateway) before system resolver
- **fix:** Exclude gateway (.1) from DNS server discovery — gateways forward DNS but lack PTR records
- **fix:** MikroTik device classification — detect switches (CSS/CRS/GS models) and APs (CAP/wAP) from sysDescr
- **fix:** LLDP link routing — route AP-to-AP links through subnet switch instead of direct false connections

## [v0.3.0] — 2026-03-27

### Added
- Infrastructure page (`/ui/infrastructure`) — SNMP status for routers, switches, APs, firewalls, servers
- SNMP reachability tracking (`snmp_reachable`, `snmp_last_checked` fields)
- On-demand SNMP probe API (`POST /api/devices/{id}/snmp-probe`)
- Vendor-specific SNMP enable commands (MikroTik, Cisco, Ubiquiti, Juniper, HPE/Aruba, Fortinet, Linux)
- Per-network DNS discovery — probes common IPs for port 53 on each subnet
- Custom DNS PTR client (`src/dns.rs`) — raw UDP queries with compression pointer support
- Per-network PTR lookups — uses discovered DNS servers before system resolver
- Multi-homed device consolidation — devices with same SNMP sysName share one record
- `additional_ips` field for multi-homed devices (e.g. rose1 MikroTik bridges)
- `dns_servers` field on Subnet — auto-discovered DNS servers per network
- SVG device icons on network map (router, switch, server, firewall, AP, printer, camera, phone)
- Multi-homed devices rendered larger on map with port dots and all IPs listed
- Discovery page shows discovered DNS servers per subnet
- Device detail and device table show additional IPs

### Fixed
- Skip network (.0) and broadcast (.255) addresses during subnet scan
- **feat:** Multi-homed device consolidation — devices with same SNMP sysName share one record
- **feat:** `additional_ips` field on Device for multi-homed devices (rose1 bridges)
- **feat:** `dns_servers` field on Subnet — auto-discovered DNS servers per network
- **feat:** SVG device icons on network map (router, switch, server, firewall, AP, printer, camera, phone)
- **feat:** Multi-homed devices shown larger on map with port dots and all IPs listed
- **feat:** Discovery page shows discovered DNS servers per subnet
- **feat:** Device detail and device table show additional IPs for multi-homed devices

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
