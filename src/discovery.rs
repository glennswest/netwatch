//! Network discovery engine — ping sweep, port scan, SNMP probe, neighbor detection.

use crate::config::Config;
use crate::db::Db;
use crate::models::*;
use crate::snmp;
use anyhow::Result;
use ipnetwork::IpNetwork;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Background discovery loop.
pub async fn run(
    db: Arc<Db>,
    config: Arc<Config>,
    ws_tx: broadcast::Sender<String>,
) {
    // Initial delay to let the system start up
    tokio::time::sleep(Duration::from_secs(5)).await;

    // Auto-discover subnets from the default gateway on first run
    if let Err(e) = discover_gateway_subnets(&db, &config).await {
        tracing::warn!("discovery: gateway subnet discovery failed: {}", e);
    }

    let interval_secs = config.discovery.interval_secs;
    let mut first_run = true;

    loop {
        if first_run {
            first_run = false;
            tracing::info!("discovery: running initial scan of all subnets");
        } else {
            tokio::time::sleep(Duration::from_secs(interval_secs)).await;
            // Re-discover subnets from gateway each cycle
            if let Err(e) = discover_gateway_subnets(&db, &config).await {
                tracing::warn!("discovery: gateway subnet discovery failed: {}", e);
            }
        }

        let subnets = match db.list_subnets() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("discovery: failed to list subnets: {}", e);
                continue;
            }
        };

        for subnet in &subnets {
            if !subnet.scan_enabled {
                continue;
            }
            tracing::info!("discovery: scanning {}", subnet.cidr);
            if let Err(e) = scan_subnet(&db, &config, subnet, &ws_tx).await {
                tracing::error!("discovery: scan error for {}: {}", subnet.cidr, e);
            }

            // Update last scan time
            let now = chrono::Utc::now().to_rfc3339();
            let _ = db.update_subnet_last_scan(&subnet.id, &now);
        }

        // Run neighbor discovery on all devices with SNMP
        if let Err(e) = discover_neighbors(&db, &config).await {
            tracing::error!("discovery: neighbor detection error: {}", e);
        }

        let _ = ws_tx.send(r#"{"event":"discovery_complete"}"#.to_string());
    }
}

/// Discover subnets by SNMP-walking the default gateway's IP address table.
pub async fn discover_gateway_subnets(db: &Db, config: &Config) -> Result<()> {
    let gateway = match read_default_gateway() {
        Some(gw) => gw,
        None => {
            tracing::warn!("discovery: cannot determine default gateway");
            return Ok(());
        }
    };

    tracing::info!("discovery: querying gateway {} for subnets via SNMP", gateway);
    let community = &config.discovery.snmp_community;
    let timeout = config.discovery.snmp_timeout_ms;

    // Walk IP addresses on the router
    let addrs = snmp::async_snmp_walk(
        gateway.clone(),
        community.clone(),
        snmp::OID_IP_ADDR_ENTRY_ADDR.to_string(),
        timeout,
    )
    .await?;

    // Walk subnet masks
    let masks = snmp::async_snmp_walk(
        gateway.clone(),
        community.clone(),
        snmp::OID_IP_ADDR_ENTRY_MASK.to_string(),
        timeout,
    )
    .await?;

    // Build map of index → mask
    let mask_map: std::collections::HashMap<String, String> = masks
        .into_iter()
        .map(|(oid, val)| {
            // OID suffix is the IP address itself
            let suffix = oid.strip_prefix(snmp::OID_IP_ADDR_ENTRY_MASK)
                .unwrap_or(&oid)
                .trim_start_matches('.');
            (suffix.to_string(), val.as_string())
        })
        .collect();

    for (oid, val) in &addrs {
        let ip_str = val.as_string();

        // Skip loopback, link-local, and non-private networks
        if ip_str.starts_with("127.") || ip_str.starts_with("169.254.") {
            continue;
        }
        // Only scan RFC1918 private networks
        if !ip_str.starts_with("10.")
            && !ip_str.starts_with("172.16.") && !ip_str.starts_with("172.17.")
            && !ip_str.starts_with("172.18.") && !ip_str.starts_with("172.19.")
            && !ip_str.starts_with("172.2") && !ip_str.starts_with("172.30.")
            && !ip_str.starts_with("172.31.")
            && !ip_str.starts_with("192.168.")
        {
            tracing::debug!("discovery: skipping non-private subnet for {}", ip_str);
            continue;
        }

        // Get the OID suffix (which is the IP)
        let suffix = oid.strip_prefix(snmp::OID_IP_ADDR_ENTRY_ADDR)
            .unwrap_or(oid)
            .trim_start_matches('.');

        let mask_str = match mask_map.get(suffix) {
            Some(m) => m.clone(),
            None => continue,
        };

        // Convert mask to prefix length
        let prefix_len = mask_to_prefix_len(&mask_str);
        if prefix_len == 0 || prefix_len > 30 {
            continue;
        }

        // Compute network address
        let ip: Ipv4Addr = match ip_str.parse() {
            Ok(a) => a,
            Err(_) => continue,
        };
        let mask: Ipv4Addr = match mask_str.parse() {
            Ok(a) => a,
            Err(_) => continue,
        };

        let net_octets: Vec<u8> = ip.octets().iter().zip(mask.octets().iter())
            .map(|(i, m)| i & m)
            .collect();
        let network = Ipv4Addr::new(net_octets[0], net_octets[1], net_octets[2], net_octets[3]);
        let cidr = format!("{}/{}", network, prefix_len);

        // Generate a name from the network (e.g., "192.168.1" -> auto-name)
        let name = subnet_name_from_ip(&ip_str);

        tracing::info!("discovery: found subnet {} ({})", cidr, name);
        let _ = db.upsert_subnet_by_cidr(&cidr, &name, community);
    }

    Ok(())
}

/// Read the default gateway from /proc/net/route.
fn read_default_gateway() -> Option<String> {
    let content = std::fs::read_to_string("/proc/net/route").ok()?;
    for line in content.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 3 { continue; }
        // Default route has destination 00000000
        if fields[1] == "00000000" {
            let hex_gw = fields[2];
            if let Ok(gw) = u32::from_str_radix(hex_gw, 16) {
                let ip = Ipv4Addr::from(u32::from_be(gw));
                return Some(ip.to_string());
            }
        }
    }
    None
}

/// Convert dotted subnet mask to prefix length.
fn mask_to_prefix_len(mask: &str) -> u32 {
    let parts: Vec<u8> = mask.split('.').filter_map(|s| s.parse().ok()).collect();
    if parts.len() != 4 { return 0; }
    let bits = u32::from_be_bytes([parts[0], parts[1], parts[2], parts[3]]);
    bits.count_ones()
}

/// Generate a human-readable subnet name from an IP.
fn subnet_name_from_ip(ip: &str) -> String {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() != 4 { return ip.to_string(); }
    // Common naming: "gX" for 192.168.X.0/24 networks
    if parts[0] == "192" && parts[1] == "168" {
        return format!("g{}", parts[2]);
    }
    // For 10.x.x.x or other networks
    format!("net-{}-{}", parts[0], parts[1])
}

/// Trigger an immediate scan of a specific subnet.
pub async fn scan_single(
    db: &Arc<Db>,
    config: &Arc<Config>,
    subnet: &Subnet,
    ws_tx: &broadcast::Sender<String>,
) -> Result<usize> {
    scan_subnet(db, config, subnet, ws_tx).await
}

/// Scan a subnet: ping sweep + SNMP probe + optional port scan.
async fn scan_subnet(
    db: &Arc<Db>,
    config: &Config,
    subnet: &Subnet,
    ws_tx: &broadcast::Sender<String>,
) -> Result<usize> {
    let network: IpNetwork = subnet.cidr.parse()?;
    let timeout = config.discovery.snmp_timeout_ms;
    let community = subnet.snmp_community.clone();
    let scan_ports = config.discovery.scan_ports.clone();
    let auto_add = config.discovery.auto_add_services;

    let mut found = 0usize;

    // Collect all IPs to scan (skip network and broadcast addresses)
    let ips: Vec<Ipv4Addr> = match network {
        IpNetwork::V4(net) => {
            let prefix = net.prefix();
            if prefix >= 31 {
                // /31 and /32 — include all addresses
                net.iter().collect()
            } else {
                let net_addr = net.network();
                let bcast = net.broadcast();
                net.iter().filter(|ip| *ip != net_addr && *ip != bcast).collect()
            }
        }
        _ => return Ok(0),
    };

    // DNS server discovery — probe common IPs for port 53
    let dns_servers = {
        let mut found_dns: Vec<String> = Vec::new();
        if let IpNetwork::V4(net) = network {
            let base = net.network().octets();
            let candidates = [1, 199, 252, 253];
            for last in candidates {
                let candidate = format!("{}.{}.{}.{}", base[0], base[1], base[2], last);
                if crate::dns::async_probe_dns_server(candidate.clone(), 1500).await {
                    tracing::info!("discovery: found DNS server {} on {}", candidate, subnet.cidr);
                    found_dns.push(candidate);
                }
            }
        }
        if !found_dns.is_empty() {
            let _ = db.update_subnet_dns_servers(&subnet.id, found_dns.clone());
        }
        found_dns
    };

    // Ping sweep in parallel batches
    let semaphore = Arc::new(tokio::sync::Semaphore::new(100));
    let mut handles = Vec::new();

    for ip in ips {
        let permit = semaphore.clone().acquire_owned().await?;
        let ip_str = ip.to_string();
        let comm = community.clone();

        let handle = tokio::spawn(async move {
            let _permit = permit;
            let reachable = ping_host(&ip_str, 2000).await;
            (ip_str, comm, reachable)
        });
        handles.push(handle);
    }

    for handle in handles {
        let (ip_str, comm, reachable) = handle.await?;
        if !reachable {
            continue;
        }

        found += 1;

        // Check if device already exists (primary or additional IP)
        if let Some(old) = db.get_device_by_any_ip(&ip_str)? {
            let mut new = old.clone();
            new.last_seen = Some(chrono::Utc::now().to_rfc3339());
            // Fill in MAC from ARP if missing
            if new.mac.is_none() {
                new.mac = arp_lookup(&ip_str);
            }
            // Probe SNMP reachability on re-scan
            let snmp_comm = new.snmp_community.clone().unwrap_or_else(|| community.clone());
            let snmp_ok = snmp::snmp_probe(&ip_str, &snmp_comm, timeout).await.is_some();
            new.snmp_reachable = Some(snmp_ok);
            new.snmp_last_checked = Some(chrono::Utc::now().to_rfc3339());
            let _ = db.update_device(old, new);
            continue;
        }

        // New device — try SNMP
        let mut name = ip_str.clone();
        let mut device_type = DeviceType::Other;
        let mut vendor = None;
        let mut sys_descr = None;
        let mut sys_object_id = None;
        let mut location = None;
        let mut snmp_reachable = false;

        if let Ok((sname, sdescr, soid, sloc, _uptime)) =
            snmp::snmp_system_info(&ip_str, &comm, timeout).await
        {
            snmp_reachable = true;
            if !sname.is_empty() {
                name = sname;
            }
            device_type = snmp::guess_device_type(&sdescr, &soid);
            vendor = snmp::guess_vendor(&sdescr);
            if !sdescr.is_empty() {
                sys_descr = Some(sdescr);
            }
            if !soid.is_empty() {
                sys_object_id = Some(soid);
            }
            if !sloc.is_empty() {
                location = Some(sloc);
            }
        }

        // Multi-homed consolidation: if SNMP sysName matches an existing device,
        // add this IP as an additional_ip instead of creating a new device
        if snmp_reachable && name != ip_str {
            if let Ok(Some(existing)) = db.get_device_by_name(&name) {
                if existing.ip != ip_str && !existing.additional_ips.contains(&ip_str) {
                    let mut updated = existing.clone();
                    updated.additional_ips.push(ip_str.clone());
                    updated.last_seen = Some(chrono::Utc::now().to_rfc3339());
                    updated.snmp_reachable = Some(true);
                    updated.snmp_last_checked = Some(chrono::Utc::now().to_rfc3339());
                    let _ = db.update_device(existing, updated);
                    tracing::info!(
                        "discovery: consolidated {} as additional IP on {}",
                        ip_str, name
                    );
                    // Fetch interfaces for the additional IP
                    if let Ok(Some(dev)) = db.get_device_by_name(&name) {
                        let _ = fetch_interfaces(db, &dev.id, &ip_str, &comm, timeout).await;
                    }
                    let _ = ws_tx.send(format!(
                        r#"{{"event":"device_consolidated","ip":"{}","name":"{}"}}"#,
                        ip_str, name
                    ));
                    continue;
                }
            }
        }

        // Try to get MAC from ARP cache
        let mac = arp_lookup(&ip_str);

        // MAC-based vendor/type identification (when SNMP didn't identify)
        if let Some(ref m) = mac {
            if vendor.is_none() {
                vendor = oui_vendor(m);
            }
            if device_type == DeviceType::Other {
                device_type = oui_device_type(m);
            }
        }

        // Try per-network DNS first, fall back to system resolver
        if name == ip_str {
            if let Some(hostname) = crate::dns::resolve_ptr(&ip_str, &dns_servers, 2000).await {
                name = hostname;
            }
        }

        let is_infrastructure = device_type.is_infrastructure();

        let mut labels = std::collections::HashMap::new();
        if !is_infrastructure {
            labels.insert("monitor".to_string(), "false".to_string());
        }
        if let Some(ref v) = vendor {
            labels.insert("vendor".to_string(), v.to_lowercase());
        }
        let type_str = device_type.as_str().to_string();

        let now = chrono::Utc::now().to_rfc3339();
        let device = Device {
            id: uuid::Uuid::new_v4().to_string(),
            ip: ip_str.clone(),
            additional_ips: Vec::new(),
            name,
            mac,
            vendor,
            device_type,
            snmp_community: Some(comm.clone()),
            snmp_version: 2,
            sys_descr,
            sys_object_id,
            location,
            notes: None,
            labels,
            enabled: true,
            last_seen: Some(now.clone()),
            snmp_reachable: Some(snmp_reachable),
            snmp_last_checked: Some(now.clone()),
            created_at: now.clone(),
            updated_at: now,
        };

        let device_id = device.id.clone();
        if let Err(e) = db.insert_device(device) {
            tracing::warn!("discovery: failed to insert device {}: {}", ip_str, e);
            continue;
        }

        tracing::info!("discovery: found {} {} ({})", type_str, ip_str, device_id);

        // Only add monitoring services for infrastructure devices
        if auto_add && is_infrastructure {
            let svc = Service {
                id: uuid::Uuid::new_v4().to_string(),
                device_id: device_id.clone(),
                name: "Ping".to_string(),
                probe_type: ProbeType::Icmp,
                host: Some(ip_str.clone()),
                port: None,
                url: None,
                interval_secs: 60,
                timeout_ms: 5000,
                enabled: true,
            };
            let _ = db.insert_service(svc);
        }

        // Port scan and auto-add services — only for infrastructure
        if auto_add && is_infrastructure {
            let open_ports = scan_ports_fast(&ip_str, &scan_ports, 1500).await;
            for port in open_ports {
                let (svc_name, probe_type) = port_to_service(port);
                let svc = Service {
                    id: uuid::Uuid::new_v4().to_string(),
                    device_id: device_id.clone(),
                    name: svc_name.to_string(),
                    probe_type,
                    host: Some(ip_str.clone()),
                    port: Some(port),
                    url: if probe_type == ProbeType::Http {
                        Some(format!("http://{}:{}", ip_str, port))
                    } else if probe_type == ProbeType::Https {
                        Some(format!("https://{}:{}", ip_str, port))
                    } else {
                        None
                    },
                    interval_secs: 60,
                    timeout_ms: 5000,
                    enabled: true,
                };
                let _ = db.insert_service(svc);
            }
        }

        // Fetch SNMP interfaces
        let _ = fetch_interfaces(db, &device_id, &ip_str, &comm, timeout).await;

        let _ = ws_tx.send(format!(
            r#"{{"event":"device_discovered","ip":"{}","id":"{}"}}"#,
            ip_str, device_id
        ));
    }

    Ok(found)
}

/// Fetch interface table via SNMP and store.
async fn fetch_interfaces(
    db: &Db,
    device_id: &str,
    ip: &str,
    community: &str,
    timeout: u64,
) -> Result<()> {
    let descrs = snmp::async_snmp_walk(
        ip.to_string(),
        community.to_string(),
        snmp::OID_IF_DESCR.to_string(),
        timeout,
    )
    .await?;

    for (oid, val) in &descrs {
        let if_index: i32 = oid
            .rsplit('.')
            .next()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let iface_name = val.as_string();
        let iface = NetInterface {
            id: format!("{}:{}", device_id, if_index),
            device_id: device_id.to_string(),
            name: iface_name,
            if_index: Some(if_index),
            ip: None,
            mac: None,
            speed_mbps: None,
            status: "unknown".to_string(),
            if_type: None,
            in_octets: None,
            out_octets: None,
        };
        let _ = db.upsert_interface(iface);
    }

    // Fetch MAC addresses (ifPhysAddress)
    let mut first_mac: Option<String> = None;
    if let Ok(macs) = snmp::async_snmp_walk(
        ip.to_string(),
        community.to_string(),
        snmp::OID_IF_PHYS_ADDR.to_string(),
        timeout,
    )
    .await
    {
        for (oid, val) in macs {
            let if_index: i32 = oid.rsplit('.').next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let id = format!("{}:{}", device_id, if_index);
            if let snmp::SnmpValue::OctetString(ref bytes) = val {
                if bytes.len() == 6 && bytes.iter().any(|&b| b != 0) {
                    let mac_str = format!(
                        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
                        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]
                    );
                    if first_mac.is_none() {
                        first_mac = Some(mac_str.clone());
                    }
                    if let Ok(Some(old)) = db.get_interface(&id) {
                        let mut new = old.clone();
                        new.mac = Some(mac_str);
                        let _ = db.upsert_interface(new);
                    }
                }
            }
        }
    }

    // Set device MAC to first real interface MAC found
    if let Some(mac) = first_mac {
        if let Ok(Some(old_dev)) = db.get_device(device_id) {
            if old_dev.mac.is_none() {
                let mut new_dev = old_dev.clone();
                new_dev.mac = Some(mac);
                let _ = db.update_device(old_dev, new_dev);
            }
        }
    }

    // Fetch speeds
    if let Ok(speeds) = snmp::async_snmp_walk(
        ip.to_string(),
        community.to_string(),
        snmp::OID_IF_SPEED.to_string(),
        timeout,
    )
    .await
    {
        for (oid, val) in speeds {
            let if_index: i32 = oid.rsplit('.').next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let id = format!("{}:{}", device_id, if_index);
            if let Ok(Some(old)) = db.get_interface(&id) {
                let mut new = old.clone();
                if let snmp::SnmpValue::Gauge32(speed) = val {
                    new.speed_mbps = Some(speed as i64 / 1_000_000);
                } else if let snmp::SnmpValue::Integer(speed) = val {
                    new.speed_mbps = Some(speed / 1_000_000);
                }
                let _ = db.upsert_interface(new);
            }
        }
    }

    // Fetch oper status
    if let Ok(statuses) = snmp::async_snmp_walk(
        ip.to_string(),
        community.to_string(),
        snmp::OID_IF_OPER_STATUS.to_string(),
        timeout,
    )
    .await
    {
        for (oid, val) in statuses {
            let if_index: i32 = oid.rsplit('.').next().and_then(|s| s.parse().ok()).unwrap_or(0);
            let id = format!("{}:{}", device_id, if_index);
            if let Ok(Some(old)) = db.get_interface(&id) {
                let mut new = old.clone();
                new.status = match val {
                    snmp::SnmpValue::Integer(1) => "up".to_string(),
                    snmp::SnmpValue::Integer(2) => "down".to_string(),
                    _ => "unknown".to_string(),
                };
                let _ = db.upsert_interface(new);
            }
        }
    }

    Ok(())
}

/// Discover LLDP neighbors via SNMP and create links.
async fn discover_neighbors(db: &Db, config: &Config) -> Result<()> {
    let devices = db.list_devices()?;
    let timeout = config.discovery.snmp_timeout_ms;

    for device in &devices {
        let community = device
            .snmp_community
            .as_deref()
            .unwrap_or(&config.discovery.snmp_community);

        // Try LLDP
        if let Ok(neighbors) = snmp::async_snmp_walk(
            device.ip.clone(),
            community.to_string(),
            snmp::OID_LLDP_REM_SYS_NAME.to_string(),
            timeout,
        )
        .await
        {
            for (_oid, val) in neighbors {
                let neighbor_name = val.as_string();
                if neighbor_name.is_empty() {
                    continue;
                }

                // Find the neighbor device by name
                let all_devices = db.list_devices()?;
                if let Some(neighbor) = all_devices.iter().find(|d| d.name == neighbor_name) {
                    // Check if link already exists
                    let links = db.list_links()?;
                    let exists = links.iter().any(|l| {
                        (l.source_device_id == device.id && l.target_device_id == neighbor.id)
                            || (l.source_device_id == neighbor.id
                                && l.target_device_id == device.id)
                    });

                    if !exists {
                        let link = Link {
                            id: uuid::Uuid::new_v4().to_string(),
                            source_device_id: device.id.clone(),
                            target_device_id: neighbor.id.clone(),
                            source_if_id: None,
                            target_if_id: None,
                            link_type: "ethernet".to_string(),
                            bandwidth_mbps: None,
                        };
                        let _ = db.insert_link(link);
                        tracing::info!(
                            "discovery: link {} <-> {} (LLDP)",
                            device.name,
                            neighbor_name
                        );
                    }
                }
            }
        }
    }

    Ok(())
}

// ── Utility functions ──

/// Ping a host using ICMP (raw socket). Returns true if reachable.
pub async fn ping_host(ip: &str, timeout_ms: u64) -> bool {
    let ip = ip.to_string();
    tokio::task::spawn_blocking(move || ping_host_sync(&ip, timeout_ms))
        .await
        .unwrap_or(false)
}

pub fn ping_host_sync(ip: &str, timeout_ms: u64) -> bool {
    use socket2::{Domain, Protocol, Socket, Type, SockAddr};
    use std::mem::MaybeUninit;

    let addr: std::net::IpAddr = match ip.parse() {
        Ok(a) => a,
        Err(_) => return false,
    };

    let target_v4: Ipv4Addr = match ip.parse() {
        Ok(a) => a,
        Err(_) => return false,
    };

    // Try DGRAM ICMP first (unprivileged), then RAW, then TCP fallback
    let raw_type = Type::from(libc::SOCK_RAW);
    let dgram_type = Type::DGRAM;
    let icmp_proto = Protocol::ICMPV4;

    let (sock, is_dgram) = match Socket::new(Domain::IPV4, dgram_type, Some(icmp_proto)) {
        Ok(s) => (s, true),
        Err(_) => match Socket::new(Domain::IPV4, raw_type, Some(icmp_proto)) {
            Ok(s) => (s, false),
            Err(_) => {
                return tcp_probe_sync(ip, 80, timeout_ms)
                    || tcp_probe_sync(ip, 443, timeout_ms)
                    || tcp_probe_sync(ip, 22, timeout_ms);
            }
        },
    };

    sock.set_read_timeout(Some(Duration::from_millis(timeout_ms))).ok();
    sock.set_write_timeout(Some(Duration::from_millis(timeout_ms))).ok();

    // Connect to target so recv only gets replies from this IP
    let dest: SockAddr = SocketAddr::new(addr, 0).into();
    let _ = sock.connect(&dest);

    // Build ICMP echo request
    let id = (std::process::id() & 0xFFFF) as u16;
    let seq: u16 = (target_v4.octets()[2] as u16) << 8 | target_v4.octets()[3] as u16;
    let mut packet = vec![
        8, 0, 0, 0, // type=echo, code=0, checksum placeholder
        (id >> 8) as u8, (id & 0xFF) as u8,
        (seq >> 8) as u8, (seq & 0xFF) as u8,
    ];
    packet.extend_from_slice(b"netwatch\x00");

    let cksum = icmp_checksum(&packet);
    packet[2] = (cksum >> 8) as u8;
    packet[3] = (cksum & 0xFF) as u8;

    if sock.send(&packet).is_err() {
        return false;
    }

    // Read responses — retry a few times to skip error responses
    let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
    let mut buf = [MaybeUninit::<u8>::uninit(); 512];
    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        sock.set_read_timeout(Some(remaining)).ok();

        match sock.recv(&mut buf) {
            Ok(n) if n >= 4 => {
                let data: Vec<u8> = buf[..n].iter().map(|b| unsafe { b.assume_init() }).collect();

                // DGRAM sockets strip IP header; RAW sockets include it
                let offset = if !is_dgram && data[0] >> 4 == 4 {
                    ((data[0] & 0x0F) as usize) * 4
                } else {
                    0
                };

                if offset >= n { return false; }
                let icmp_type = data[offset];

                // Type 0 = echo reply — success
                if icmp_type == 0 {
                    return true;
                }
                // Type 3 = destination unreachable — host doesn't exist
                if icmp_type == 3 {
                    return false;
                }
                // Other ICMP type — keep waiting for echo reply
                continue;
            }
            _ => return false,
        }
    }
}

fn icmp_checksum(data: &[u8]) -> u16 {
    let mut sum = 0u32;
    let mut i = 0;
    while i < data.len() - 1 {
        sum += ((data[i] as u32) << 8) | data[i + 1] as u32;
        i += 2;
    }
    if data.len() % 2 != 0 {
        sum += (data[data.len() - 1] as u32) << 8;
    }
    while sum >> 16 != 0 {
        sum = (sum & 0xFFFF) + (sum >> 16);
    }
    !sum as u16
}

fn tcp_probe_sync(ip: &str, port: u16, timeout_ms: u64) -> bool {
    let addr: SocketAddr = match format!("{}:{}", ip, port).parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(timeout_ms)).is_ok()
}

/// Scan a list of ports on a host, returning those that are open.
pub async fn scan_ports_fast(ip: &str, ports: &[u16], timeout_ms: u64) -> Vec<u16> {
    let semaphore = Arc::new(tokio::sync::Semaphore::new(50));
    let mut handles = Vec::new();

    for &port in ports {
        let ip = ip.to_string();
        let permit = semaphore.clone().acquire_owned().await;
        let handle = tokio::spawn(async move {
            let _permit = permit;
            let addr: SocketAddr = format!("{}:{}", ip, port).parse().ok()?;
            match tokio::time::timeout(
                Duration::from_millis(timeout_ms),
                tokio::net::TcpStream::connect(addr),
            )
            .await
            {
                Ok(Ok(_)) => Some(port),
                _ => None,
            }
        });
        handles.push(handle);
    }

    let mut open = Vec::new();
    for handle in handles {
        if let Ok(Some(port)) = handle.await {
            open.push(port);
        }
    }
    open
}

/// Identify vendor from MAC OUI prefix.
fn oui_vendor(mac: &str) -> Option<String> {
    let prefix = mac.replace(':', "").replace('-', "").to_uppercase();
    if prefix.len() < 6 { return None; }
    let oui = &prefix[..6];
    let vendor = match oui {
        // MikroTik
        "2C:CF:67" | "2CCF67" | "4C5E0C" | "6C3B6B" | "D4CA6D" | "E4:8D:8C" | "E48D8C"
        | "48A98A" | "74:4D:28" | "744D28" | "CC2DE0" | "B8:69:F4" | "B869F4" => Some("MikroTik"),
        // Ubiquiti
        "24A43C" | "788A20" | "802AA8" | "B4FBE4" | "DC9FDB" | "F09FC2"
        | "FCECDA" | "AC8BA9" | "E063DA" | "245A4C" | "687251" | "18E829" => Some("Ubiquiti"),
        // Cisco
        "000C29" | "001B2A" | "00265E" | "002CC8" | "005056" | "00A2EE"
        | "0CD996" | "503DE5" | "58971E" | "7C21D8" | "881DFC" | "F4CFE2" => Some("Cisco"),
        // Juniper
        "0005860" | "001256" | "002283" | "3C6104" | "5C4527" | "8071B2"
        | "883FD3" | "9C7D14" | "F01C2D" | "F4B52F" => Some("Juniper"),
        // Amazon (Blink, Echo, Ring, Fire)
        "0C47C9" | "18744F" | "34D270" | "40B4CD" | "44D9E7" | "50F5DA"
        | "68542B" | "6854FD" | "747548" | "84D612" | "A002DC" | "FC65DE"
        | "F0F0A4" | "FCA183" | "ACE348" | "38F73D" | "940069" | "8871B1" => Some("Amazon"),
        // Apple
        "3C22FB" | "A4B197" | "D0817A" | "F0D4F6" | "28FF3C" | "7CD1C3"
        | "8866A5" | "A860B6" | "D087E2" | "F0B479" | "3C06A7" | "A4D1D2" => Some("Apple"),
        // Samsung
        "00265D" | "08D42B" | "1432D1" | "3423BA" | "ACE215" | "C0BDD1"
        | "D0176A" | "E4B021" | "F0D7AA" | "5CE0C5" => Some("Samsung"),
        // TP-Link
        "1C3BF3" | "50C7BF" | "60E327" | "B09575" | "C0E42D" | "E8DE27"
        | "F4F26D" | "147590" | "A842A1" | "549F13" | "30B49E" => Some("TP-Link"),
        // Synology
        "001132" | "0011320" => Some("Synology"),
        // Dell
        "002564" | "00B0D0" | "149197" | "204747" | "246E96" | "34E6D7"
        | "4C7625" | "842B2B" | "B083FE" | "D4AE52" | "F48E38" | "F8BC12" => Some("Dell"),
        // HP
        "001083" | "001321" | "00215A" | "0025B3" | "002655" | "3CA82A"
        | "68B599" | "9457A5" | "A01D48" | "B499BA" | "D4C9EF" | "EC8EB5" => Some("HP"),
        // Aruba
        "000B86" | "24DEC6" | "6CF37F" | "94B40F" | "D8C7C8" | "20A6CD" => Some("Aruba"),
        // Intel
        "001517" | "0019D1" | "001B21" | "001E65" | "002332" | "3C970E"
        | "485B39" | "A0369F" | "B4969" | "F8F21E" => Some("Intel"),
        // Google (Nest, Chromecast)
        "20DF B9" | "54:60:09" | "546009" | "A47733" | "F4F5D8" | "30FD38" => Some("Google"),
        _ => None,
    };
    vendor.map(String::from)
}

/// Classify device type from MAC OUI prefix.
fn oui_device_type(mac: &str) -> DeviceType {
    let prefix = mac.replace(':', "").replace('-', "").to_uppercase();
    if prefix.len() < 6 { return DeviceType::Other; }
    let oui = &prefix[..6];
    match oui {
        // MikroTik → Router
        "2CCF67" | "4C5E0C" | "6C3B6B" | "D4CA6D" | "E48D8C"
        | "48A98A" | "744D28" | "CC2DE0" | "B869F4" => DeviceType::Router,
        // Ubiquiti → AP (most common deployment)
        "24A43C" | "788A20" | "802AA8" | "B4FBE4" | "DC9FDB" | "F09FC2"
        | "FCECDA" | "AC8BA9" | "E063DA" | "245A4C" | "687251" | "18E829" => DeviceType::Ap,
        // Aruba → AP
        "000B86" | "24DEC6" | "6CF37F" | "94B40F" | "D8C7C8" | "20A6CD" => DeviceType::Ap,
        // Cisco → Switch (common in enterprise)
        "000C29" | "001B2A" | "00265E" | "002CC8" | "005056" | "00A2EE"
        | "0CD996" | "503DE5" | "58971E" | "7C21D8" | "881DFC" | "F4CFE2" => DeviceType::Switch,
        // Amazon (Blink, Echo, Ring) → Camera
        "0C47C9" | "18744F" | "34D270" | "40B4CD" | "44D9E7" | "50F5DA"
        | "68542B" | "6854FD" | "747548" | "84D612" | "A002DC" | "FC65DE"
        | "F0F0A4" | "FCA183" | "ACE348" | "38F73D" | "940069" | "8871B1" => DeviceType::Camera,
        // Printers (HP, common OUIs)
        "001083" | "001321" => DeviceType::Printer,
        // Apple, Samsung, Google → Phone/Other
        "3C22FB" | "A4B197" | "D0817A" | "F0D4F6" | "28FF3C" | "7CD1C3"
        | "8866A5" | "A860B6" | "D087E2" | "F0B479" | "3C06A7" | "A4D1D2" => DeviceType::Phone,
        _ => DeviceType::Other,
    }
}

/// Look up MAC address from the local ARP cache (/proc/net/arp).
fn arp_lookup(ip: &str) -> Option<String> {
    let arp = std::fs::read_to_string("/proc/net/arp").ok()?;
    for line in arp.lines().skip(1) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() >= 4 && fields[0] == ip {
            let mac = fields[3].to_uppercase();
            if mac != "00:00:00:00:00:00" {
                return Some(mac);
            }
        }
    }
    None
}

/// Map a port number to a service name and probe type.
fn port_to_service(port: u16) -> (&'static str, ProbeType) {
    match port {
        22 => ("SSH", ProbeType::Tcp),
        23 => ("Telnet", ProbeType::Tcp),
        25 => ("SMTP", ProbeType::Tcp),
        53 => ("DNS", ProbeType::Dns),
        80 => ("HTTP", ProbeType::Http),
        110 => ("POP3", ProbeType::Tcp),
        143 => ("IMAP", ProbeType::Tcp),
        161 => ("SNMP", ProbeType::Snmp),
        443 => ("HTTPS", ProbeType::Https),
        445 => ("SMB", ProbeType::Tcp),
        993 => ("IMAPS", ProbeType::Tcp),
        995 => ("POP3S", ProbeType::Tcp),
        3306 => ("MySQL", ProbeType::Tcp),
        5432 => ("PostgreSQL", ProbeType::Tcp),
        6379 => ("Redis", ProbeType::Tcp),
        8080 => ("HTTP-Alt", ProbeType::Http),
        8443 => ("HTTPS-Alt", ProbeType::Https),
        8291 => ("WinBox", ProbeType::Tcp),
        8728 => ("MikroTik API", ProbeType::Tcp),
        8729 => ("MikroTik API-SSL", ProbeType::Tcp),
        _ => ("TCP", ProbeType::Tcp),
    }
}
