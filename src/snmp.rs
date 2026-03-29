//! Minimal SNMP v1/v2c client — BER/ASN.1 encoding, UDP transport.

use anyhow::{bail, Result};
use std::net::UdpSocket;
use std::time::Duration;

// ASN.1 BER tags
const TAG_INTEGER: u8 = 0x02;
const TAG_OCTET_STRING: u8 = 0x04;
const TAG_NULL: u8 = 0x05;
const TAG_OID: u8 = 0x06;
const TAG_SEQUENCE: u8 = 0x30;
const TAG_GET_REQUEST: u8 = 0xA0;
const TAG_GET_NEXT_REQUEST: u8 = 0xA1;
const TAG_GET_RESPONSE: u8 = 0xA2;
const TAG_COUNTER32: u8 = 0x41;
const TAG_GAUGE32: u8 = 0x42;
const TAG_TIMETICKS: u8 = 0x43;
const TAG_COUNTER64: u8 = 0x46;
const TAG_NO_SUCH_OBJECT: u8 = 0x80;
const TAG_NO_SUCH_INSTANCE: u8 = 0x81;
const TAG_END_OF_MIB_VIEW: u8 = 0x82;
const TAG_IP_ADDRESS: u8 = 0x40;

/// SNMP value types returned from queries.
#[derive(Debug, Clone)]
pub enum SnmpValue {
    Integer(i64),
    OctetString(Vec<u8>),
    Null,
    Oid(String),
    IpAddress([u8; 4]),
    Counter32(u32),
    Gauge32(u32),
    TimeTicks(u32),
    Counter64(u64),
    NoSuchObject,
    NoSuchInstance,
    EndOfMibView,
}

impl SnmpValue {
    pub fn as_string(&self) -> String {
        match self {
            Self::Integer(v) => v.to_string(),
            Self::OctetString(v) => String::from_utf8_lossy(v).to_string(),
            Self::Null => "null".into(),
            Self::Oid(v) => v.clone(),
            Self::IpAddress(v) => format!("{}.{}.{}.{}", v[0], v[1], v[2], v[3]),
            Self::Counter32(v) => v.to_string(),
            Self::Gauge32(v) => v.to_string(),
            Self::TimeTicks(v) => format!("{:.2}s", *v as f64 / 100.0),
            Self::Counter64(v) => v.to_string(),
            Self::NoSuchObject => "noSuchObject".into(),
            Self::NoSuchInstance => "noSuchInstance".into(),
            Self::EndOfMibView => "endOfMibView".into(),
        }
    }

    pub fn is_end(&self) -> bool {
        matches!(
            self,
            Self::NoSuchObject | Self::NoSuchInstance | Self::EndOfMibView
        )
    }
}

// ── Well-known OIDs ──

pub const OID_SYS_DESCR: &str = "1.3.6.1.2.1.1.1.0";
pub const OID_SYS_OBJECT_ID: &str = "1.3.6.1.2.1.1.2.0";
pub const OID_SYS_UPTIME: &str = "1.3.6.1.2.1.1.3.0";
pub const OID_SYS_CONTACT: &str = "1.3.6.1.2.1.1.4.0";
pub const OID_SYS_NAME: &str = "1.3.6.1.2.1.1.5.0";
pub const OID_SYS_LOCATION: &str = "1.3.6.1.2.1.1.6.0";

// Interface table
pub const OID_IF_TABLE: &str = "1.3.6.1.2.1.2.2";
pub const OID_IF_DESCR: &str = "1.3.6.1.2.1.2.2.1.2";
pub const OID_IF_TYPE: &str = "1.3.6.1.2.1.2.2.1.3";
pub const OID_IF_SPEED: &str = "1.3.6.1.2.1.2.2.1.5";
pub const OID_IF_PHYS_ADDR: &str = "1.3.6.1.2.1.2.2.1.6";
pub const OID_IF_OPER_STATUS: &str = "1.3.6.1.2.1.2.2.1.8";
pub const OID_IF_IN_OCTETS: &str = "1.3.6.1.2.1.2.2.1.10";
pub const OID_IF_OUT_OCTETS: &str = "1.3.6.1.2.1.2.2.1.16";

// IP address table (for subnet discovery from routers)
pub const OID_IP_ADDR_ENTRY_ADDR: &str = "1.3.6.1.2.1.4.20.1.1";
pub const OID_IP_ADDR_ENTRY_MASK: &str = "1.3.6.1.2.1.4.20.1.3";
pub const OID_IP_ADDR_ENTRY_IF_INDEX: &str = "1.3.6.1.2.1.4.20.1.2";

// LLDP
pub const OID_LLDP_REM_SYS_NAME: &str = "1.0.8802.1.1.2.1.4.1.1.9";
pub const OID_LLDP_REM_PORT_ID: &str = "1.0.8802.1.1.2.1.4.1.1.7";
pub const OID_LLDP_REM_MAN_ADDR: &str = "1.0.8802.1.1.2.1.4.2.1.4";

// ── BER Encoding ──

fn encode_length(len: usize) -> Vec<u8> {
    if len < 128 {
        vec![len as u8]
    } else if len < 256 {
        vec![0x81, len as u8]
    } else {
        vec![0x82, (len >> 8) as u8, (len & 0xFF) as u8]
    }
}

fn encode_tlv(tag: u8, value: &[u8]) -> Vec<u8> {
    let mut out = vec![tag];
    out.extend(encode_length(value.len()));
    out.extend(value);
    out
}

fn encode_integer(val: i64) -> Vec<u8> {
    let mut bytes = Vec::new();
    if val == 0 {
        bytes.push(0);
    } else {
        let mut v = val;
        let negative = val < 0;
        let mut tmp = Vec::new();
        while v != 0 && v != -1 {
            tmp.push((v & 0xFF) as u8);
            v >>= 8;
        }
        if tmp.is_empty() {
            tmp.push(if negative { 0xFF } else { 0 });
        }
        // Add sign byte if needed
        if !negative && (tmp.last().unwrap() & 0x80) != 0 {
            tmp.push(0);
        } else if negative && (tmp.last().unwrap() & 0x80) == 0 {
            tmp.push(0xFF);
        }
        tmp.reverse();
        bytes = tmp;
    }
    encode_tlv(TAG_INTEGER, &bytes)
}

fn encode_oid(oid: &str) -> Vec<u8> {
    let parts: Vec<u32> = oid.split('.').filter_map(|s| s.parse().ok()).collect();
    if parts.len() < 2 {
        return encode_tlv(TAG_OID, &[]);
    }
    let mut bytes = vec![(parts[0] * 40 + parts[1]) as u8];
    for &part in &parts[2..] {
        if part < 128 {
            bytes.push(part as u8);
        } else {
            let mut tmp = Vec::new();
            let mut v = part;
            tmp.push((v & 0x7F) as u8);
            v >>= 7;
            while v > 0 {
                tmp.push((v & 0x7F) as u8 | 0x80);
                v >>= 7;
            }
            tmp.reverse();
            bytes.extend(tmp);
        }
    }
    encode_tlv(TAG_OID, &bytes)
}

fn encode_string(val: &[u8]) -> Vec<u8> {
    encode_tlv(TAG_OCTET_STRING, val)
}

fn encode_null() -> Vec<u8> {
    vec![TAG_NULL, 0]
}

fn encode_sequence(contents: &[u8]) -> Vec<u8> {
    encode_tlv(TAG_SEQUENCE, contents)
}

// ── BER Decoding ──

fn decode_length(data: &[u8], pos: &mut usize) -> Result<usize> {
    if *pos >= data.len() {
        bail!("unexpected end of data");
    }
    let b = data[*pos];
    *pos += 1;
    if b < 128 {
        Ok(b as usize)
    } else {
        let n = (b & 0x7F) as usize;
        let mut len = 0usize;
        for _ in 0..n {
            if *pos >= data.len() {
                bail!("unexpected end of data");
            }
            len = (len << 8) | data[*pos] as usize;
            *pos += 1;
        }
        Ok(len)
    }
}

fn decode_integer(data: &[u8]) -> i64 {
    if data.is_empty() {
        return 0;
    }
    let mut val = if data[0] & 0x80 != 0 { -1i64 } else { 0i64 };
    for &b in data {
        val = (val << 8) | b as i64;
    }
    val
}

fn decode_oid(data: &[u8]) -> String {
    if data.is_empty() {
        return String::new();
    }
    let first = data[0] / 40;
    let second = data[0] % 40;
    let mut parts = vec![first.to_string(), second.to_string()];

    let mut i = 1;
    while i < data.len() {
        let mut val = 0u32;
        loop {
            if i >= data.len() {
                break;
            }
            let b = data[i];
            i += 1;
            val = (val << 7) | (b & 0x7F) as u32;
            if b & 0x80 == 0 {
                break;
            }
        }
        parts.push(val.to_string());
    }
    parts.join(".")
}

fn decode_value(data: &[u8], pos: &mut usize) -> Result<(u8, SnmpValue)> {
    if *pos >= data.len() {
        bail!("unexpected end of data");
    }
    let tag = data[*pos];
    *pos += 1;
    let len = decode_length(data, pos)?;
    if *pos + len > data.len() {
        bail!("value length exceeds data");
    }
    let value_data = &data[*pos..*pos + len];
    *pos += len;

    let val = match tag {
        TAG_INTEGER => SnmpValue::Integer(decode_integer(value_data)),
        TAG_OCTET_STRING => SnmpValue::OctetString(value_data.to_vec()),
        TAG_NULL => SnmpValue::Null,
        TAG_OID => SnmpValue::Oid(decode_oid(value_data)),
        TAG_IP_ADDRESS if len == 4 => {
            SnmpValue::IpAddress([value_data[0], value_data[1], value_data[2], value_data[3]])
        }
        TAG_COUNTER32 => {
            let mut v = 0u32;
            for &b in value_data {
                v = (v << 8) | b as u32;
            }
            SnmpValue::Counter32(v)
        }
        TAG_GAUGE32 => {
            let mut v = 0u32;
            for &b in value_data {
                v = (v << 8) | b as u32;
            }
            SnmpValue::Gauge32(v)
        }
        TAG_TIMETICKS => {
            let mut v = 0u32;
            for &b in value_data {
                v = (v << 8) | b as u32;
            }
            SnmpValue::TimeTicks(v)
        }
        TAG_COUNTER64 => {
            let mut v = 0u64;
            for &b in value_data {
                v = (v << 8) | b as u64;
            }
            SnmpValue::Counter64(v)
        }
        TAG_NO_SUCH_OBJECT => SnmpValue::NoSuchObject,
        TAG_NO_SUCH_INSTANCE => SnmpValue::NoSuchInstance,
        TAG_END_OF_MIB_VIEW => SnmpValue::EndOfMibView,
        TAG_SEQUENCE | TAG_GET_RESPONSE | TAG_GET_REQUEST | TAG_GET_NEXT_REQUEST => {
            // Container type — return OctetString with raw bytes for further parsing
            SnmpValue::OctetString(value_data.to_vec())
        }
        _ => SnmpValue::OctetString(value_data.to_vec()),
    };
    Ok((tag, val))
}

// ── SNMP PDU building ──

fn build_pdu(tag: u8, request_id: i32, oid: &str) -> Vec<u8> {
    let req_id = encode_integer(request_id as i64);
    let error_status = encode_integer(0);
    let error_index = encode_integer(0);

    // Varbind: OID + NULL
    let mut varbind = encode_oid(oid);
    varbind.extend(encode_null());
    let varbind = encode_sequence(&varbind);
    let varbind_list = encode_sequence(&varbind);

    let mut pdu_content = Vec::new();
    pdu_content.extend(&req_id);
    pdu_content.extend(&error_status);
    pdu_content.extend(&error_index);
    pdu_content.extend(&varbind_list);

    encode_tlv(tag, &pdu_content)
}

fn build_message(version: i32, community: &str, pdu: &[u8]) -> Vec<u8> {
    let mut content = Vec::new();
    content.extend(encode_integer(version as i64)); // 0 = v1, 1 = v2c
    content.extend(encode_string(community.as_bytes()));
    content.extend(pdu);
    encode_sequence(&content)
}

/// Parse SNMP response, return vector of (OID, Value) pairs.
fn parse_response(data: &[u8]) -> Result<Vec<(String, SnmpValue)>> {
    let mut pos = 0;

    // Outer SEQUENCE
    if data[pos] != TAG_SEQUENCE {
        bail!("expected SEQUENCE, got 0x{:02x}", data[pos]);
    }
    pos += 1;
    let _msg_len = decode_length(data, &mut pos)?;

    // Version
    decode_value(data, &mut pos)?;
    // Community
    decode_value(data, &mut pos)?;

    // GetResponse PDU
    if data[pos] != TAG_GET_RESPONSE {
        bail!("expected GetResponse, got 0x{:02x}", data[pos]);
    }
    pos += 1;
    let _pdu_len = decode_length(data, &mut pos)?;

    // Request ID
    decode_value(data, &mut pos)?;
    // Error status
    let (_, err_status) = decode_value(data, &mut pos)?;
    if let SnmpValue::Integer(e) = err_status {
        if e != 0 {
            bail!("SNMP error status: {}", e);
        }
    }
    // Error index
    decode_value(data, &mut pos)?;

    // Varbind list (SEQUENCE)
    if data[pos] != TAG_SEQUENCE {
        bail!("expected varbind list SEQUENCE");
    }
    pos += 1;
    let varbind_list_len = decode_length(data, &mut pos)?;
    let varbind_end = pos + varbind_list_len;

    let mut results = Vec::new();

    while pos < varbind_end {
        // Each varbind is a SEQUENCE of (OID, value)
        if data[pos] != TAG_SEQUENCE {
            break;
        }
        pos += 1;
        let _vb_len = decode_length(data, &mut pos)?;

        // OID
        let (_, oid_val) = decode_value(data, &mut pos)?;
        let oid = match oid_val {
            SnmpValue::Oid(s) => s,
            _ => bail!("expected OID in varbind"),
        };

        // Value
        let (_, val) = decode_value(data, &mut pos)?;
        results.push((oid, val));
    }

    Ok(results)
}

// ── Public API ──

/// Send an SNMP GET request and return the value.
pub fn snmp_get(ip: &str, community: &str, oid: &str, timeout_ms: u64) -> Result<SnmpValue> {
    let req_id = rand::random::<i32>() & 0x7FFFFFFF;
    let pdu = build_pdu(TAG_GET_REQUEST, req_id, oid);
    let msg = build_message(1, community, &pdu); // v2c

    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_read_timeout(Some(Duration::from_millis(timeout_ms)))?;
    sock.send_to(&msg, format!("{}:161", ip))?;

    let mut buf = [0u8; 4096];
    let (n, _) = sock.recv_from(&mut buf)?;

    let results = parse_response(&buf[..n])?;
    if let Some((_, val)) = results.into_iter().next() {
        Ok(val)
    } else {
        bail!("empty SNMP response");
    }
}

/// Send an SNMP GET-NEXT request and return (OID, Value).
pub fn snmp_get_next(
    ip: &str,
    community: &str,
    oid: &str,
    timeout_ms: u64,
) -> Result<(String, SnmpValue)> {
    let req_id = rand::random::<i32>() & 0x7FFFFFFF;
    let pdu = build_pdu(TAG_GET_NEXT_REQUEST, req_id, oid);
    let msg = build_message(1, community, &pdu);

    let sock = UdpSocket::bind("0.0.0.0:0")?;
    sock.set_read_timeout(Some(Duration::from_millis(timeout_ms)))?;
    sock.send_to(&msg, format!("{}:161", ip))?;

    let mut buf = [0u8; 4096];
    let (n, _) = sock.recv_from(&mut buf)?;

    let results = parse_response(&buf[..n])?;
    if let Some(pair) = results.into_iter().next() {
        Ok(pair)
    } else {
        bail!("empty SNMP response");
    }
}

/// Walk an OID subtree using GET-NEXT, returning all (OID, Value) pairs.
pub fn snmp_walk(
    ip: &str,
    community: &str,
    base_oid: &str,
    timeout_ms: u64,
) -> Result<Vec<(String, SnmpValue)>> {
    let prefix = format!("{}.", base_oid);
    let mut results = Vec::new();
    let mut current_oid = base_oid.to_string();

    for _ in 0..10_000 {
        match snmp_get_next(ip, community, &current_oid, timeout_ms) {
            Ok((next_oid, val)) => {
                if val.is_end() || !next_oid.starts_with(&prefix) {
                    break;
                }
                current_oid = next_oid.clone();
                results.push((next_oid, val));
            }
            Err(_) => break,
        }
    }

    Ok(results)
}

/// Async wrappers using tokio spawn_blocking.
pub async fn async_snmp_get(
    ip: String,
    community: String,
    oid: String,
    timeout_ms: u64,
) -> Result<SnmpValue> {
    tokio::task::spawn_blocking(move || snmp_get(&ip, &community, &oid, timeout_ms)).await?
}

pub async fn async_snmp_walk(
    ip: String,
    community: String,
    base_oid: String,
    timeout_ms: u64,
) -> Result<Vec<(String, SnmpValue)>> {
    tokio::task::spawn_blocking(move || snmp_walk(&ip, &community, &base_oid, timeout_ms)).await?
}

/// Try SNMP GET of sysName to test if a host speaks SNMP.
pub async fn snmp_probe(ip: &str, community: &str, timeout_ms: u64) -> Option<String> {
    match async_snmp_get(ip.to_string(), community.to_string(), OID_SYS_NAME.to_string(), timeout_ms).await {
        Ok(val) => Some(val.as_string()),
        Err(_) => None,
    }
}

/// Gather system info via SNMP: (sysName, sysDescr, sysObjectID, sysLocation, sysUptime).
pub async fn snmp_system_info(
    ip: &str,
    community: &str,
    timeout_ms: u64,
) -> Result<(String, String, String, String, String)> {
    let ip = ip.to_string();
    let community = community.to_string();

    let name = async_snmp_get(ip.clone(), community.clone(), OID_SYS_NAME.to_string(), timeout_ms);
    let descr = async_snmp_get(ip.clone(), community.clone(), OID_SYS_DESCR.to_string(), timeout_ms);
    let obj_id = async_snmp_get(ip.clone(), community.clone(), OID_SYS_OBJECT_ID.to_string(), timeout_ms);
    let loc = async_snmp_get(ip.clone(), community.clone(), OID_SYS_LOCATION.to_string(), timeout_ms);
    let uptime = async_snmp_get(ip.clone(), community.clone(), OID_SYS_UPTIME.to_string(), timeout_ms);

    let (name, descr, obj_id, loc, uptime) = tokio::join!(name, descr, obj_id, loc, uptime);

    Ok((
        name.map(|v| v.as_string()).unwrap_or_default(),
        descr.map(|v| v.as_string()).unwrap_or_default(),
        obj_id.map(|v| v.as_string()).unwrap_or_default(),
        loc.map(|v| v.as_string()).unwrap_or_default(),
        uptime.map(|v| v.as_string()).unwrap_or_default(),
    ))
}

/// Vendor-specific CLI commands to enable SNMP on a device.
pub fn snmp_enable_instructions(vendor: &str) -> Vec<(&'static str, &'static str)> {
    match vendor.to_lowercase().as_str() {
        "mikrotik" => vec![
            ("RouterOS CLI", "/snmp set enabled=yes contact=\"admin\" location=\"rack\" trap-community=public"),
            ("RouterOS community", "/snmp community set public addresses=0.0.0.0/0 read-access=yes"),
        ],
        "cisco" => vec![
            ("IOS enable", "snmp-server community public RO\nsnmp-server location \"rack\"\nsnmp-server contact admin"),
            ("IOS save", "write memory"),
        ],
        "ubiquiti" => vec![
            ("EdgeOS/VyOS", "set service snmp community public authorization ro\ncommit; save"),
            ("UniFi Controller", "Settings > Services > SNMP > Enable SNMPv1/v2c, community: public"),
        ],
        "juniper" => vec![
            ("JunOS", "set snmp community public authorization read-only\ncommit"),
        ],
        "hpe/aruba" | "hp" | "aruba" => vec![
            ("ProCurve/ArubaOS-Switch", "snmp-server community public unrestricted\nwrite memory"),
            ("ArubaOS-CX", "snmp-server community public\nwrite memory"),
        ],
        "fortinet" => vec![
            ("FortiOS", "config system snmp community\nedit 1\nset name public\nset events cpu-high mem-low\nnext\nend"),
        ],
        "linux" | "microsoft" => vec![
            ("net-snmp (Debian/Ubuntu)", "apt install snmpd\nsed -i 's/agentaddress .*/agentaddress udp:161/' /etc/snmp/snmpd.conf\nsystemctl restart snmpd"),
            ("net-snmp (RHEL/CentOS)", "yum install net-snmp\nsystemctl enable --now snmpd"),
        ],
        _ => vec![
            ("Generic", "Consult vendor documentation to enable SNMP v2c with community string 'public' on UDP port 161."),
        ],
    }
}

/// Guess device type from sysDescr / sysObjectID.
pub fn guess_device_type(sys_descr: &str, sys_object_id: &str) -> crate::models::DeviceType {
    let desc = sys_descr.to_lowercase();
    let oid = sys_object_id;

    // MikroTik — differentiate switches, APs, and routers by model in sysDescr
    if desc.contains("mikrotik") || desc.contains("routeros") || oid.starts_with("1.3.6.1.4.1.14988") {
        // CAP/cAP/wAP models are access points
        if desc.contains("cap") || desc.contains("wap") || desc.contains("audience")
            || desc.contains("chateau") && desc.contains("wifi")
        {
            return crate::models::DeviceType::Ap;
        }
        // CSS/CRS models and switch-oriented boards (model ends with S like L009UiGS)
        if desc.contains("css") || desc.contains("crs")
            || desc.contains("switch")
        {
            return crate::models::DeviceType::Switch;
        }
        // Models with "GS" suffix are typically switch products (L009UiGS, etc.)
        let parts: Vec<&str> = sys_descr.split_whitespace().collect();
        for part in &parts {
            let p = part.to_uppercase();
            if (p.ends_with("GS") || p.ends_with("GS-5")) && p.len() > 3 {
                return crate::models::DeviceType::Switch;
            }
        }
        return crate::models::DeviceType::Router;
    }
    // Cisco
    if oid.starts_with("1.3.6.1.4.1.9") {
        if desc.contains("switch") || desc.contains("catalyst") {
            return crate::models::DeviceType::Switch;
        }
        if desc.contains("asa") || desc.contains("firewall") {
            return crate::models::DeviceType::Firewall;
        }
        if desc.contains("aironet") || desc.contains("wireless") {
            return crate::models::DeviceType::Ap;
        }
        return crate::models::DeviceType::Router;
    }
    // Ubiquiti
    if desc.contains("ubnt") || desc.contains("ubiquiti") || desc.contains("unifi") || oid.starts_with("1.3.6.1.4.1.41112") {
        if desc.contains("uap") || desc.contains("access point") {
            return crate::models::DeviceType::Ap;
        }
        if desc.contains("usw") || desc.contains("switch") {
            return crate::models::DeviceType::Switch;
        }
        return crate::models::DeviceType::Router;
    }
    // Generic detection
    if desc.contains("switch") { return crate::models::DeviceType::Switch; }
    if desc.contains("router") || desc.contains("gateway") { return crate::models::DeviceType::Router; }
    if desc.contains("firewall") { return crate::models::DeviceType::Firewall; }
    if desc.contains("printer") || desc.contains("jetdirect") { return crate::models::DeviceType::Printer; }
    if desc.contains("camera") || desc.contains("ipcam") { return crate::models::DeviceType::Camera; }
    if desc.contains("linux") || desc.contains("windows") || desc.contains("freebsd") {
        return crate::models::DeviceType::Server;
    }

    crate::models::DeviceType::Other
}

/// Extract vendor name from sysDescr.
pub fn guess_vendor(sys_descr: &str) -> Option<String> {
    let desc = sys_descr.to_lowercase();
    if desc.contains("mikrotik") || desc.contains("routeros") { return Some("MikroTik".into()); }
    if desc.contains("cisco") { return Some("Cisco".into()); }
    if desc.contains("ubnt") || desc.contains("ubiquiti") || desc.contains("unifi") { return Some("Ubiquiti".into()); }
    if desc.contains("juniper") { return Some("Juniper".into()); }
    if desc.contains("arista") { return Some("Arista".into()); }
    if desc.contains("hp ") || desc.contains("hewlett") || desc.contains("procurve") || desc.contains("aruba") { return Some("HPE/Aruba".into()); }
    if desc.contains("dell") || desc.contains("force10") { return Some("Dell".into()); }
    if desc.contains("fortinet") || desc.contains("fortigate") { return Some("Fortinet".into()); }
    if desc.contains("palo alto") { return Some("Palo Alto".into()); }
    if desc.contains("linux") { return Some("Linux".into()); }
    if desc.contains("windows") { return Some("Microsoft".into()); }
    None
}
