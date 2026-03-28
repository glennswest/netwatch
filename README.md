# Netwatch

Network monitoring and topology mapping for MikroTik-based networks. Replaces MikroTik Dude with a modern web UI, SNMP auto-discovery, and smart alerting.

## Features

- **SNMP subnet auto-discovery** — walks the default gateway's ipAddrTable to find all networks
- **Device classification** — MAC OUI + SNMP sysDescr identify routers, switches, APs, cameras, phones
- **Smart monitoring** — only probes infrastructure devices (routers, switches, APs, firewalls, servers)
- **Device labels** — key-value attributes (like Kubernetes labels) for classification
- **Multi-probe types** — ICMP, TCP, HTTP/HTTPS, DNS, SNMP
- **Interactive network map** — SVG topology with LLDP neighbor discovery
- **Alerting** — email (SMTP) and webhook notifications with cooldown
- **Dark theme web UI** — HTMX + Askama, real-time WebSocket updates
- **Network filter** — dropdown to focus on a specific network
- **Sortable tables** — click column headers, persists across auto-refresh

## Architecture

```
netwatch (single static ARM64 binary)
├── Discovery engine      — SNMP walk, ping sweep, port scan, ARP lookup
├── Monitor engine        — scheduled probes (ICMP/TCP/HTTP/DNS/SNMP)
├── Alert engine          — consecutive failure detection, cooldown, notifications
├── Web UI (port 80)      — Askama templates + HTMX + WebSocket
├── REST API (/api/*)     — JSON CRUD for devices, services, subnets, alerts
└── redb database         — embedded key-value store (pure Rust, no C deps)
```

Runs inside a `stormdbase` container on MikroTik RouterOS via mkube.

## Deployment

### Prerequisites

- MikroTik router with RouterOS containers enabled
- [mkube](https://github.com/glennswest/mkube) running on the router
- Local registry at `registry.gt.lo:5000`
- Rust toolchain with `aarch64-unknown-linux-musl` target
- Podman for container builds

### Build

```bash
# Cross-compile for ARM64
cargo build --release --target aarch64-unknown-linux-musl

# Build container image (MUST use edge tag)
podman build --platform linux/arm64 \
  -t registry.gt.lo:5000/netwatch:edge \
  -f Containerfile .

# Push to registry
podman push --tls-verify=false registry.gt.lo:5000/netwatch:edge
```

### Deploy to mkube

```bash
# First-time deploy — apply the pod spec
python3 -c "import yaml,json; print(json.dumps(yaml.safe_load(open('deploy/pod.yaml'))))" \
  | curl -s -X POST http://192.168.200.2:8082/api/v1/namespaces/infra/pods \
    -H 'Content-Type: application/json' -d @-

# Subsequent deploys — trigger image redeploy after pushing new image
curl -s -X POST http://192.168.200.2:8082/api/v1/images/redeploy \
  -H 'Content-Type: application/json' \
  -d '{"image":"registry.gt.lo:5000/netwatch:edge"}'
```

mkube will pull the new image, recreate the container, and assign it a static IP on the gt network.

### Static Files

When modifying CSS/JS in `static/`, touch the web module before building to force rust_embed to re-embed:

```bash
touch src/web/mod.rs
cargo build --release --target aarch64-unknown-linux-musl
```

### Pod Spec

The pod spec lives at `deploy/pod.yaml`. Key annotations:

| Annotation | Value | Purpose |
|---|---|---|
| `vkube.io/network` | `gt` | Deploy on the gt management network |
| `vkube.io/image-policy` | `auto` | Auto-update when image digest changes |
| `vkube.io/aliases` | `netwatch.gt.lo` | DNS alias for the pod |

## Configuration

Configuration file: `deploy/netwatch.toml` (embedded in the container at `/etc/netwatch/netwatch.toml`).

```toml
[server]
listen = "0.0.0.0:80"
data_dir = "/data"

[discovery]
interval_secs = 900          # 15 minutes between discovery cycles
snmp_community = "public"
snmp_timeout_ms = 2000
auto_add_services = true
scan_ports = [22, 23, 53, 80, 161, 443, 8080, 8291, 8728, 8729]

[monitoring]
default_interval_secs = 60   # probe every 60 seconds
default_timeout_ms = 5000
concurrency = 20             # max concurrent probes

[alerting]
cooldown_secs = 300          # 5 min cooldown per service
consecutive_failures = 3     # alert after 3 consecutive failures

[retention]
probe_days = 30
metric_days = 90
alert_days = 180
```

Subnets are auto-discovered from the default gateway via SNMP. No manual subnet configuration needed.

## API Reference

Base URL: `http://netwatch.gt.lo`

### Devices
```
GET    /api/devices              List all devices
POST   /api/devices              Create device
GET    /api/devices/{id}         Get device
PUT    /api/devices/{id}         Update device
DELETE /api/devices/{id}         Delete device
GET    /api/devices/{id}/interfaces    List interfaces
GET    /api/devices/{id}/services      List services
GET    /api/devices/{id}/metrics       List metrics
```

### Services
```
GET    /api/services             List all services
POST   /api/services             Create service
DELETE /api/services/{id}        Delete service
GET    /api/services/{id}/probes List probe results
```

### Alerts
```
GET    /api/alerts               List alerts
POST   /api/alerts/{id}/ack      Acknowledge alert
DELETE /api/alerts/{id}          Delete alert
DELETE /api/alerts/clear         Clear all alerts
```

### Subnets
```
GET    /api/subnets              List subnets
POST   /api/subnets              Create subnet
DELETE /api/subnets/{id}         Delete subnet
```

### Operations
```
POST   /api/discovery/scan       Trigger immediate scan
DELETE /api/reset                Wipe all data (full DB reset)
GET    /api/metrics              Query metrics
```

### Map
```
GET    /api/map/positions        Get device positions
PUT    /api/map/positions        Update position
POST   /api/map/auto-layout      Auto-layout devices
GET    /api/links                List network links
POST   /api/links                Create link
DELETE /api/links/{id}           Delete link
```

### WebSocket
```
ws://netwatch.gt.lo/ws          Live events (alerts, discovery)
```

## How Discovery Works

1. **Gateway detection** — reads `/proc/net/route` for the default gateway IP
2. **Subnet discovery** — SNMP walks the gateway's `ipAddrTable` (OID 1.3.6.1.2.1.4.20) to find all connected subnets
3. **Ping sweep** — ICMP echo request to every IP in each subnet (100 concurrent)
4. **SNMP identification** — queries sysName, sysDescr, sysObjectID on responding hosts
5. **MAC discovery** — SNMP ifPhysAddress walk + ARP cache lookup
6. **OUI classification** — MAC prefix identifies vendor (MikroTik, Ubiquiti, Cisco, Amazon, etc.) and device type
7. **Infrastructure filtering** — only routers, switches, APs, firewalls, and servers get monitoring services
8. **Port scan** — infrastructure devices get TCP port scans for service auto-detection
9. **LLDP neighbors** — walks LLDP MIB to discover network links for the topology map

## Infrastructure IPs

| Component | IP | Port |
|---|---|---|
| rose1 (gateway) | 192.168.200.1 | — |
| mkube | 192.168.200.2 | 8082 |
| registry | 192.168.200.3 | 5000 |
| netwatch | 192.168.200.107 | 80 |

## Version History

| Version | Date | Summary |
|---------|------|---------|
| v0.2.0 | 2026-03-27 | SNMP auto-discovery, MAC OUI classification, network filter, version display, DB reset |
| v0.1.0 | 2026-03-26 | Initial release — full monitoring app with web UI |
