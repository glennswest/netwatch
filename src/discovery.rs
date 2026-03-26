//! Network discovery engine — ping sweep, port scan, SNMP probe, neighbor detection.

use crate::config::Config;
use crate::db::Db;
use crate::models::*;
use crate::snmp;
use anyhow::Result;
use ipnetwork::IpNetwork;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// Background discovery loop.
pub async fn run(
    db: Arc<Db>,
    config: Arc<Config>,
    ws_tx: broadcast::Sender<String>,
) {
    // Initial delay to let the system start up
    tokio::time::sleep(Duration::from_secs(5)).await;

    let mut interval =
        tokio::time::interval(Duration::from_secs(config.discovery.interval_secs));

    loop {
        interval.tick().await;

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

    // Collect all IPs to scan
    let ips: Vec<Ipv4Addr> = match network {
        IpNetwork::V4(net) => net.iter().collect(),
        _ => return Ok(0),
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

        // Check if device already exists
        if db.get_device_by_ip(&ip_str)?.is_some() {
            // Update last_seen
            if let Some(old) = db.get_device_by_ip(&ip_str)? {
                let mut new = old.clone();
                new.last_seen = Some(chrono::Utc::now().to_rfc3339());
                let _ = db.update_device(old, new);
            }
            continue;
        }

        // New device — try SNMP
        let mut name = ip_str.clone();
        let mut device_type = DeviceType::Other;
        let mut vendor = None;
        let mut sys_descr = None;
        let mut sys_object_id = None;
        let mut location = None;

        if let Ok((sname, sdescr, soid, sloc, _uptime)) =
            snmp::snmp_system_info(&ip_str, &comm, timeout).await
        {
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

        // Try reverse DNS
        if name == ip_str {
            if let Ok(hostname) = dns_reverse(&ip_str) {
                name = hostname;
            }
        }

        let now = chrono::Utc::now().to_rfc3339();
        let device = Device {
            id: uuid::Uuid::new_v4().to_string(),
            ip: ip_str.clone(),
            name,
            mac: None,
            vendor,
            device_type,
            snmp_community: Some(comm.clone()),
            snmp_version: 2,
            sys_descr,
            sys_object_id,
            location,
            notes: None,
            enabled: true,
            last_seen: Some(now.clone()),
            created_at: now.clone(),
            updated_at: now,
        };

        let device_id = device.id.clone();
        if let Err(e) = db.insert_device(device) {
            tracing::warn!("discovery: failed to insert device {}: {}", ip_str, e);
            continue;
        }

        tracing::info!("discovery: found new device {} ({})", ip_str, device_id);

        // Auto-add ICMP service
        if auto_add {
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

        // Port scan and auto-add services
        if auto_add {
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
    use socket2::{Domain, Protocol, Socket, Type};

    let addr: std::net::IpAddr = match ip.parse() {
        Ok(a) => a,
        Err(_) => return false,
    };

    // Try raw ICMP socket first, fall back to UDP probe
    let sock = match Socket::new(Domain::IPV4, Type::RAW, Some(Protocol::ICMPV4)) {
        Ok(s) => s,
        Err(_) => {
            // Fallback: try DGRAM ICMP (unprivileged, Linux >= 3.0)
            match Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::ICMPV4)) {
                Ok(s) => s,
                Err(_) => {
                    // Final fallback: TCP connect to common port
                    return tcp_probe_sync(ip, 80, timeout_ms)
                        || tcp_probe_sync(ip, 443, timeout_ms)
                        || tcp_probe_sync(ip, 22, timeout_ms);
                }
            }
        }
    };

    sock.set_read_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok();
    sock.set_write_timeout(Some(Duration::from_millis(timeout_ms)))
        .ok();

    let dest = SocketAddr::new(addr, 0);

    // Build ICMP echo request
    let id = (std::process::id() & 0xFFFF) as u16;
    let seq = 1u16;
    let mut packet = vec![
        8,    // type: echo request
        0,    // code
        0, 0, // checksum (placeholder)
        (id >> 8) as u8,
        (id & 0xFF) as u8,
        (seq >> 8) as u8,
        (seq & 0xFF) as u8,
    ];
    // Payload
    packet.extend_from_slice(b"netwatch\x00");

    // Calculate checksum
    let cksum = icmp_checksum(&packet);
    packet[2] = (cksum >> 8) as u8;
    packet[3] = (cksum & 0xFF) as u8;

    if sock.send_to(&packet, &dest.into()).is_err() {
        return false;
    }

    let mut buf = [0u8; 256];
    match sock.recv(&mut buf) {
        Ok(n) if n >= 8 => {
            // Check for echo reply (type 0)
            // Raw socket includes IP header (20 bytes), DGRAM does not
            let offset = if buf[0] >> 4 == 4 { 20 } else { 0 };
            if offset < n && buf[offset] == 0 {
                return true;
            }
            // Some systems return type 0 at offset 0
            buf[0] == 0
        }
        _ => false,
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

/// Reverse DNS lookup.
fn dns_reverse(ip: &str) -> Result<String> {
    let addr: IpAddr = ip.parse()?;
    let sa: SocketAddr = SocketAddr::new(addr, 0);
    let hostname = dns_lookup::lookup_addr(&sa.ip())?;
    if hostname != ip {
        Ok(hostname)
    } else {
        anyhow::bail!("no reverse DNS")
    }
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
