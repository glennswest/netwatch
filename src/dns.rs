//! Minimal DNS PTR client — raw UDP, no external dependencies.

use anyhow::{bail, Result};
use std::net::UdpSocket;
use std::time::Duration;

const TYPE_PTR: u16 = 12;
const TYPE_NS: u16 = 2;
const CLASS_IN: u16 = 1;
const FLAGS_RD: u16 = 0x0100; // Recursion desired

/// Build a DNS PTR query packet for a reverse lookup.
fn encode_ptr_query(ip: &str) -> Result<(Vec<u8>, u16)> {
    let octets: Vec<&str> = ip.split('.').collect();
    if octets.len() != 4 {
        bail!("invalid IPv4 address: {}", ip);
    }

    // Reverse octets: d.c.b.a.in-addr.arpa
    let name = format!(
        "{}.{}.{}.{}.in-addr.arpa",
        octets[3], octets[2], octets[1], octets[0]
    );

    let id: u16 = rand::random();
    let mut packet = Vec::with_capacity(64);

    // Header: ID, FLAGS, QDCOUNT=1, ANCOUNT=0, NSCOUNT=0, ARCOUNT=0
    packet.extend(&id.to_be_bytes());
    packet.extend(&FLAGS_RD.to_be_bytes());
    packet.extend(&1u16.to_be_bytes()); // QDCOUNT
    packet.extend(&[0u8; 6]);           // AN, NS, AR counts

    // Question: encoded name + QTYPE + QCLASS
    for label in name.split('.') {
        let bytes = label.as_bytes();
        packet.push(bytes.len() as u8);
        packet.extend(bytes);
    }
    packet.push(0); // root label

    packet.extend(&TYPE_PTR.to_be_bytes());
    packet.extend(&CLASS_IN.to_be_bytes());

    Ok((packet, id))
}

/// Decode a DNS name from a packet, handling compression pointers.
fn decode_dns_name(data: &[u8], pos: &mut usize) -> Result<String> {
    let mut labels = Vec::new();
    let mut jumped = false;
    let mut return_pos = 0;

    loop {
        if *pos >= data.len() {
            bail!("truncated DNS name");
        }
        let len = data[*pos] as usize;

        if len == 0 {
            if !jumped {
                *pos += 1;
            } else {
                *pos = return_pos;
            }
            break;
        }

        // Compression pointer
        if len & 0xC0 == 0xC0 {
            if *pos + 1 >= data.len() {
                bail!("truncated compression pointer");
            }
            if !jumped {
                return_pos = *pos + 2;
            }
            let offset = ((len & 0x3F) << 8) | data[*pos + 1] as usize;
            *pos = offset;
            jumped = true;
            continue;
        }

        *pos += 1;
        if *pos + len > data.len() {
            bail!("label exceeds packet");
        }
        labels.push(String::from_utf8_lossy(&data[*pos..*pos + len]).to_string());
        *pos += len;
    }

    Ok(labels.join("."))
}

/// Parse a DNS response and extract the PTR hostname.
fn parse_ptr_response(data: &[u8], expected_id: u16) -> Result<String> {
    if data.len() < 12 {
        bail!("DNS response too short");
    }

    let id = u16::from_be_bytes([data[0], data[1]]);
    if id != expected_id {
        bail!("DNS response ID mismatch");
    }

    let flags = u16::from_be_bytes([data[2], data[3]]);
    let rcode = flags & 0x000F;
    if rcode != 0 {
        bail!("DNS error rcode={}", rcode);
    }

    let qdcount = u16::from_be_bytes([data[4], data[5]]) as usize;
    let ancount = u16::from_be_bytes([data[6], data[7]]) as usize;

    if ancount == 0 {
        bail!("no answer records");
    }

    // Skip question section
    let mut pos = 12;
    for _ in 0..qdcount {
        decode_dns_name(data, &mut pos)?;
        pos += 4; // QTYPE + QCLASS
    }

    // Read answer records
    for _ in 0..ancount {
        decode_dns_name(data, &mut pos)?; // name
        if pos + 10 > data.len() {
            bail!("truncated answer record");
        }
        let rtype = u16::from_be_bytes([data[pos], data[pos + 1]]);
        let rdlength = u16::from_be_bytes([data[pos + 8], data[pos + 9]]) as usize;
        pos += 10; // TYPE(2) + CLASS(2) + TTL(4) + RDLENGTH(2)

        if rtype == TYPE_PTR {
            let hostname = decode_dns_name(data, &mut pos)?;
            // Strip trailing dot
            let hostname = hostname.trim_end_matches('.').to_string();
            return Ok(hostname);
        }

        pos += rdlength;
    }

    bail!("no PTR record in response")
}

/// Send a PTR query to a specific DNS server and return the hostname.
pub fn ptr_lookup(ip: &str, dns_server: &str, timeout_ms: u64) -> Result<String> {
    let (query, id) = encode_ptr_query(ip)?;
    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_read_timeout(Some(Duration::from_millis(timeout_ms)))?;
    sock.send_to(&query, format!("{}:53", dns_server))?;
    let mut buf = [0u8; 512];
    let (n, _) = sock.recv_from(&mut buf)?;
    parse_ptr_response(&buf[..n], id)
}

/// Async wrapper for PTR lookup.
pub async fn async_ptr_lookup(
    ip: String,
    dns_server: String,
    timeout_ms: u64,
) -> Result<String> {
    tokio::task::spawn_blocking(move || ptr_lookup(&ip, &dns_server, timeout_ms)).await?
}

/// Try multiple DNS servers in order, fall back to system resolver.
pub async fn resolve_ptr(ip: &str, dns_servers: &[String], timeout_ms: u64) -> Option<String> {
    for server in dns_servers {
        if let Ok(name) = async_ptr_lookup(ip.to_string(), server.clone(), timeout_ms).await {
            if !name.is_empty() && name != ip {
                return Some(name);
            }
        }
    }
    // Fall back to system resolver
    let addr: std::net::IpAddr = ip.parse().ok()?;
    dns_lookup::lookup_addr(&addr).ok().filter(|h| h != ip)
}

/// Check if an IP runs a DNS server by sending a root NS query.
pub fn probe_dns_server(ip: &str, timeout_ms: u64) -> bool {
    let id: u16 = rand::random();
    let mut packet = Vec::with_capacity(17);
    packet.extend(&id.to_be_bytes());
    packet.extend(&FLAGS_RD.to_be_bytes());
    packet.extend(&1u16.to_be_bytes()); // QDCOUNT
    packet.extend(&[0u8; 6]);           // AN, NS, AR counts
    packet.push(0);                     // root name "."
    packet.extend(&TYPE_NS.to_be_bytes());
    packet.extend(&CLASS_IN.to_be_bytes());

    let sock = match UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return false,
    };
    sock.set_read_timeout(Some(Duration::from_millis(timeout_ms))).ok();
    if sock.send_to(&packet, format!("{}:53", ip)).is_err() {
        return false;
    }
    let mut buf = [0u8; 512];
    matches!(sock.recv_from(&mut buf), Ok((n, _)) if n >= 12)
}

/// Async wrapper for DNS server probe.
pub async fn async_probe_dns_server(ip: String, timeout_ms: u64) -> bool {
    tokio::task::spawn_blocking(move || probe_dns_server(&ip, timeout_ms))
        .await
        .unwrap_or(false)
}
