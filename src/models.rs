use serde::{Deserialize, Serialize};

// ── Device types ──

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum DeviceType {
    Router,
    Switch,
    Server,
    Firewall,
    Ap,
    Printer,
    Camera,
    Phone,
    Internet,
    Other,
}

impl DeviceType {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "router" => Self::Router,
            "switch" => Self::Switch,
            "server" => Self::Server,
            "firewall" => Self::Firewall,
            "ap" | "wireless" => Self::Ap,
            "printer" => Self::Printer,
            "camera" => Self::Camera,
            "phone" => Self::Phone,
            "internet" => Self::Internet,
            _ => Self::Other,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Router => "router",
            Self::Switch => "switch",
            Self::Server => "server",
            Self::Firewall => "firewall",
            Self::Ap => "ap",
            Self::Printer => "printer",
            Self::Camera => "camera",
            Self::Phone => "phone",
            Self::Internet => "internet",
            Self::Other => "other",
        }
    }

    pub fn icon_letter(&self) -> &'static str {
        match self {
            Self::Router => "R",
            Self::Switch => "S",
            Self::Server => "V",
            Self::Firewall => "F",
            Self::Ap => "W",
            Self::Printer => "P",
            Self::Camera => "C",
            Self::Phone => "T",
            Self::Internet => "I",
            Self::Other => "?",
        }
    }

    pub fn icon_color(&self) -> &'static str {
        match self {
            Self::Router => "#5b8af5",
            Self::Switch => "#4caf7d",
            Self::Server => "#9b6af5",
            Self::Firewall => "#e55b5b",
            Self::Ap => "#5bc0de",
            Self::Printer => "#6b7084",
            Self::Camera => "#e5a54b",
            Self::Phone => "#5baae5",
            Self::Internet => "#43b581",
            Self::Other => "#6b7084",
        }
    }

    pub fn is_infrastructure(&self) -> bool {
        matches!(self, Self::Router | Self::Switch | Self::Firewall | Self::Ap | Self::Server | Self::Internet)
    }
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Probe types ──

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProbeType {
    Icmp,
    Tcp,
    Http,
    Https,
    Dns,
    Snmp,
}

impl ProbeType {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "icmp" | "ping" => Self::Icmp,
            "tcp" => Self::Tcp,
            "http" => Self::Http,
            "https" => Self::Https,
            "dns" => Self::Dns,
            "snmp" => Self::Snmp,
            _ => Self::Tcp,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Icmp => "icmp",
            Self::Tcp => "tcp",
            Self::Http => "http",
            Self::Https => "https",
            Self::Dns => "dns",
            Self::Snmp => "snmp",
        }
    }
}

impl std::fmt::Display for ProbeType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Probe status ──

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProbeStatus {
    Up,
    Down,
    Unknown,
    Degraded,
}

impl ProbeStatus {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "up" => Self::Up,
            "down" => Self::Down,
            "degraded" => Self::Degraded,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Up => "up",
            Self::Down => "down",
            Self::Unknown => "unknown",
            Self::Degraded => "degraded",
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Up => "badge-success",
            Self::Down => "badge-danger",
            Self::Unknown => "badge-info",
            Self::Degraded => "badge-warning",
        }
    }
}

impl std::fmt::Display for ProbeStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Severity ──

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Severity {
    Info,
    Warning,
    Critical,
}

impl Severity {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "warning" | "warn" => Self::Warning,
            "critical" | "crit" => Self::Critical,
            _ => Self::Info,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }

    pub fn badge_class(&self) -> &'static str {
        match self {
            Self::Info => "badge-info",
            Self::Warning => "badge-warning",
            Self::Critical => "badge-danger",
        }
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Data models ──

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub ip: String,
    #[serde(default)]
    pub additional_ips: Vec<String>,
    pub name: String,
    pub mac: Option<String>,
    pub vendor: Option<String>,
    pub device_type: DeviceType,
    pub snmp_community: Option<String>,
    pub snmp_version: i32,
    pub sys_descr: Option<String>,
    pub sys_object_id: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub labels: std::collections::HashMap<String, String>,
    pub enabled: bool,
    pub last_seen: Option<String>,
    #[serde(default)]
    pub snmp_reachable: Option<bool>,
    #[serde(default)]
    pub snmp_last_checked: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetInterface {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub if_index: Option<i32>,
    pub ip: Option<String>,
    pub mac: Option<String>,
    pub speed_mbps: Option<i64>,
    pub status: String,
    pub if_type: Option<String>,
    pub in_octets: Option<u64>,
    pub out_octets: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Link {
    pub id: String,
    pub source_device_id: String,
    pub target_device_id: String,
    pub source_if_id: Option<String>,
    pub target_if_id: Option<String>,
    pub link_type: String,
    pub bandwidth_mbps: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub id: String,
    pub device_id: String,
    pub name: String,
    pub probe_type: ProbeType,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub interval_secs: u32,
    pub timeout_ms: u32,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub id: String,
    pub service_id: String,
    pub status: ProbeStatus,
    pub latency_us: Option<i64>,
    pub error: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub device_id: String,
    pub service_id: Option<String>,
    pub severity: Severity,
    pub message: String,
    pub acknowledged: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Subnet {
    pub id: String,
    pub name: String,
    pub cidr: String,
    pub snmp_community: String,
    pub scan_enabled: bool,
    pub last_scan: Option<String>,
    #[serde(default)]
    pub dns_servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapPosition {
    pub device_id: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub id: String,
    pub device_id: String,
    pub metric_name: String,
    pub value: f64,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: String,
    pub name: String,
    pub device_match: Option<String>,
    pub service_match: Option<String>,
    pub condition: String,
    pub threshold: f64,
    pub severity: Severity,
    pub channels: Vec<String>,
    pub cooldown_secs: u32,
    pub enabled: bool,
}

// ── View models (not stored) ──

#[derive(Debug, Clone, Serialize)]
pub struct DeviceStatus {
    pub device: Device,
    pub status: ProbeStatus,
    pub services_up: usize,
    pub services_down: usize,
    pub services_total: usize,
    pub latency_us: Option<i64>,
    pub position: Option<MapPosition>,
}

pub struct NetworkGroup {
    pub name: String,
    pub cidr: String,
    pub devices: Vec<DeviceStatus>,
    pub up: usize,
    pub down: usize,
    pub svc_up: usize,
    pub svc_total: usize,
}

impl NetworkGroup {
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }
}

impl Device {
    pub fn all_ips(&self) -> Vec<&str> {
        let mut ips = vec![self.ip.as_str()];
        for ip in &self.additional_ips {
            ips.push(ip.as_str());
        }
        ips
    }
}

impl DeviceStatus {
    pub fn latency_ms(&self) -> String {
        match self.latency_us {
            Some(v) => format!("{}ms", v / 1000),
            None => "-".into(),
        }
    }
}

impl NetInterface {
    pub fn speed_display(&self) -> String {
        match self.speed_mbps {
            Some(s) if s >= 1000 => format!("{}G", s / 1000),
            Some(s) => format!("{}M", s),
            None => "-".into(),
        }
    }
}

// ── Request types ──

#[derive(Debug, Deserialize)]
pub struct CreateDevice {
    pub name: String,
    pub ip: String,
    pub mac: Option<String>,
    pub device_type: Option<String>,
    pub snmp_community: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDevice {
    pub name: Option<String>,
    pub ip: Option<String>,
    pub additional_ips: Option<Vec<String>>,
    pub mac: Option<String>,
    pub device_type: Option<String>,
    pub snmp_community: Option<String>,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub enabled: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSubnet {
    pub name: String,
    pub cidr: String,
    pub snmp_community: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateService {
    pub device_id: String,
    pub name: String,
    pub probe_type: String,
    pub host: Option<String>,
    pub port: Option<u16>,
    pub url: Option<String>,
    pub interval_secs: Option<u32>,
    pub timeout_ms: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct CreateLink {
    pub source_device_id: String,
    pub source_if_id: Option<String>,
    pub target_device_id: String,
    pub target_if_id: Option<String>,
    pub link_type: Option<String>,
    pub bandwidth_mbps: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct PositionUpdate {
    pub device_id: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Deserialize)]
pub struct MetricQuery {
    pub device_id: Option<String>,
    pub metric_name: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<usize>,
}
