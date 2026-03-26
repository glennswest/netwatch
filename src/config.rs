use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default)]
    pub discovery: DiscoveryConfig,
    #[serde(default)]
    pub monitoring: MonitoringConfig,
    #[serde(default)]
    pub alerting: AlertingConfig,
    #[serde(default)]
    pub retention: RetentionConfig,
    #[serde(default)]
    pub subnets: Vec<SubnetEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_listen")]
    pub listen: String,
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DiscoveryConfig {
    #[serde(default = "default_discovery_interval")]
    pub interval_secs: u64,
    #[serde(default = "default_community")]
    pub snmp_community: String,
    #[serde(default = "default_snmp_timeout")]
    pub snmp_timeout_ms: u64,
    #[serde(default = "default_scan_ports")]
    pub scan_ports: Vec<u16>,
    #[serde(default = "default_true")]
    pub auto_add_services: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MonitoringConfig {
    #[serde(default = "default_monitor_interval")]
    pub default_interval_secs: u32,
    #[serde(default = "default_timeout")]
    pub default_timeout_ms: u32,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AlertingConfig {
    #[serde(default = "default_cooldown")]
    pub cooldown_secs: u32,
    #[serde(default = "default_consecutive")]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub email: Option<EmailConfig>,
    #[serde(default)]
    pub webhook: Option<WebhookConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EmailConfig {
    pub smtp_host: String,
    #[serde(default = "default_smtp_port")]
    pub smtp_port: u16,
    pub from: String,
    pub to: Vec<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub tls: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
    pub headers: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RetentionConfig {
    #[serde(default = "default_probe_days")]
    pub probe_days: u32,
    #[serde(default = "default_metric_days")]
    pub metric_days: u32,
    #[serde(default = "default_alert_days")]
    pub alert_days: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SubnetEntry {
    pub name: String,
    pub cidr: String,
    #[serde(default = "default_community")]
    pub snmp_community: String,
}

// Defaults
fn default_server() -> ServerConfig {
    ServerConfig {
        listen: default_listen(),
        data_dir: default_data_dir(),
    }
}

fn default_listen() -> String { "0.0.0.0:8080".into() }
fn default_data_dir() -> PathBuf { PathBuf::from("/var/lib/netwatch") }
fn default_discovery_interval() -> u64 { 300 }
fn default_community() -> String { "public".into() }
fn default_snmp_timeout() -> u64 { 2000 }
fn default_monitor_interval() -> u32 { 60 }
fn default_timeout() -> u32 { 5000 }
fn default_concurrency() -> usize { 50 }
fn default_cooldown() -> u32 { 300 }
fn default_consecutive() -> u32 { 3 }
fn default_smtp_port() -> u16 { 587 }
fn default_probe_days() -> u32 { 30 }
fn default_metric_days() -> u32 { 90 }
fn default_alert_days() -> u32 { 180 }
fn default_true() -> bool { true }

fn default_scan_ports() -> Vec<u16> {
    vec![
        22, 23, 25, 53, 80, 110, 143, 161, 443, 445,
        993, 995, 3306, 5432, 6379, 8080, 8443,
        8291, 8728, 8729, // MikroTik
    ]
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            interval_secs: default_discovery_interval(),
            snmp_community: default_community(),
            snmp_timeout_ms: default_snmp_timeout(),
            scan_ports: default_scan_ports(),
            auto_add_services: true,
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            default_interval_secs: default_monitor_interval(),
            default_timeout_ms: default_timeout(),
            concurrency: default_concurrency(),
        }
    }
}

impl Default for AlertingConfig {
    fn default() -> Self {
        Self {
            cooldown_secs: default_cooldown(),
            consecutive_failures: default_consecutive(),
            email: None,
            webhook: None,
        }
    }
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            probe_days: default_probe_days(),
            metric_days: default_metric_days(),
            alert_days: default_alert_days(),
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        if path.exists() {
            let text = std::fs::read_to_string(path)?;
            let config: Config = toml::from_str(&text)?;
            Ok(config)
        } else {
            tracing::info!("no config file at {}, using defaults", path.display());
            Ok(Self::default())
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            server: default_server(),
            discovery: DiscoveryConfig::default(),
            monitoring: MonitoringConfig::default(),
            alerting: AlertingConfig::default(),
            retention: RetentionConfig::default(),
            subnets: Vec::new(),
        }
    }
}
