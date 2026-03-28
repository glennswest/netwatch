//! HTML page handlers using Askama templates.

use super::{AppState, HtmlTemplate};
use crate::models::*;
use askama::Template;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
};
use std::collections::HashMap;

// ── Network grouping helpers ──

fn group_devices_by_network(devices: Vec<DeviceStatus>, subnets: &[Subnet]) -> Vec<NetworkGroup> {
    let mut group_map: HashMap<String, (String, String, Vec<DeviceStatus>)> = HashMap::new();

    // Initialize from known subnets (preserves order later via sort)
    for s in subnets {
        group_map.insert(s.name.clone(), (s.name.clone(), s.cidr.clone(), Vec::new()));
    }

    for d in devices {
        let mut matched = None;
        if let Ok(addr) = d.device.ip.parse::<std::net::IpAddr>() {
            for s in subnets {
                if let Ok(net) = s.cidr.parse::<ipnetwork::IpNetwork>() {
                    if net.contains(addr) {
                        matched = Some(s.name.clone());
                        break;
                    }
                }
            }
        }
        let key = matched.unwrap_or_else(|| "Other".to_string());
        group_map
            .entry(key.clone())
            .or_insert_with(|| (key, String::new(), Vec::new()))
            .2
            .push(d);
    }

    let mut groups: Vec<NetworkGroup> = group_map
        .into_values()
        .filter(|(_, _, devs)| !devs.is_empty())
        .map(|(name, cidr, mut devices)| {
            // Sort devices by IP address numerically
            devices.sort_by(|a, b| {
                let a_ip: Option<std::net::IpAddr> = a.device.ip.parse().ok();
                let b_ip: Option<std::net::IpAddr> = b.device.ip.parse().ok();
                match (a_ip, b_ip) {
                    (Some(std::net::IpAddr::V4(a4)), Some(std::net::IpAddr::V4(b4))) => {
                        a4.octets().cmp(&b4.octets())
                    }
                    _ => a.device.ip.cmp(&b.device.ip),
                }
            });
            let up = devices.iter().filter(|d| d.status == ProbeStatus::Up).count();
            let down = devices
                .iter()
                .filter(|d| d.status == ProbeStatus::Down)
                .count();
            let svc_up: usize = devices.iter().map(|d| d.services_up).sum();
            let svc_total: usize = devices.iter().map(|d| d.services_total).sum();
            NetworkGroup {
                name,
                cidr,
                devices,
                up,
                down,
                svc_up,
                svc_total,
            }
        })
        .collect();

    groups.sort_by(|a, b| {
        if a.name == "Other" {
            std::cmp::Ordering::Greater
        } else if b.name == "Other" {
            std::cmp::Ordering::Less
        } else {
            a.name.cmp(&b.name)
        }
    });

    groups
}

pub struct ServiceNetworkGroup {
    pub name: String,
    pub cidr: String,
    pub services: Vec<ServiceRow>,
    pub up: usize,
    pub down: usize,
}

fn group_services_by_network(
    services: Vec<ServiceRow>,
    subnets: &[Subnet],
) -> Vec<ServiceNetworkGroup> {
    let mut group_map: HashMap<String, (String, String, Vec<ServiceRow>)> = HashMap::new();

    for s in subnets {
        group_map.insert(s.name.clone(), (s.name.clone(), s.cidr.clone(), Vec::new()));
    }

    for svc in services {
        let mut matched = None;
        if let Ok(addr) = svc.device_ip.parse::<std::net::IpAddr>() {
            for s in subnets {
                if let Ok(net) = s.cidr.parse::<ipnetwork::IpNetwork>() {
                    if net.contains(addr) {
                        matched = Some(s.name.clone());
                        break;
                    }
                }
            }
        }
        let key = matched.unwrap_or_else(|| "Other".to_string());
        group_map
            .entry(key.clone())
            .or_insert_with(|| (key, String::new(), Vec::new()))
            .2
            .push(svc);
    }

    let mut groups: Vec<ServiceNetworkGroup> = group_map
        .into_values()
        .filter(|(_, _, svcs)| !svcs.is_empty())
        .map(|(name, cidr, mut services)| {
            // Sort services by device IP numerically
            services.sort_by(|a, b| {
                let a_ip: Option<std::net::IpAddr> = a.device_ip.parse().ok();
                let b_ip: Option<std::net::IpAddr> = b.device_ip.parse().ok();
                match (a_ip, b_ip) {
                    (Some(std::net::IpAddr::V4(a4)), Some(std::net::IpAddr::V4(b4))) => {
                        a4.octets().cmp(&b4.octets())
                    }
                    _ => a.device_ip.cmp(&b.device_ip),
                }
            });
            let up = services
                .iter()
                .filter(|s| s.status == ProbeStatus::Up)
                .count();
            let down = services
                .iter()
                .filter(|s| s.status == ProbeStatus::Down)
                .count();
            ServiceNetworkGroup {
                name,
                cidr,
                services,
                up,
                down,
            }
        })
        .collect();

    groups.sort_by(|a, b| {
        if a.name == "Other" {
            std::cmp::Ordering::Greater
        } else if b.name == "Other" {
            std::cmp::Ordering::Less
        } else {
            a.name.cmp(&b.name)
        }
    });

    groups
}

fn build_service_rows(db: &crate::db::Db) -> Vec<ServiceRow> {
    let all_services = db.list_services().unwrap_or_default();
    let all_devices = db.list_devices().unwrap_or_default();
    let all_latest: Vec<ProbeResult> = db.get_all_latest_probes().unwrap_or_default();

    // Index devices by id
    let dev_by_id: HashMap<&str, &Device> = all_devices.iter().map(|d| (d.id.as_str(), d)).collect();
    // Index latest probes by service_id
    let probe_by_svc: HashMap<&str, &ProbeResult> =
        all_latest.iter().map(|p| (p.service_id.as_str(), p)).collect();

    let mut rows = Vec::new();
    for svc in all_services {
        let device = dev_by_id.get(svc.device_id.as_str());
        let probe = probe_by_svc.get(svc.id.as_str());
        rows.push(ServiceRow {
            device_name: device.map(|d| d.name.clone()).unwrap_or_default(),
            device_ip: device.map(|d| d.ip.clone()).unwrap_or_default(),
            status: probe
                .map(|p| p.status)
                .unwrap_or(ProbeStatus::Unknown),
            latency_us: probe.and_then(|p| p.latency_us),
            service: svc,
        });
    }
    rows
}

// ── Redirect ──

pub async fn redirect_dashboard() -> Redirect {
    Redirect::permanent("/ui/")
}

// ── Dashboard ──

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    active: String,
    version: &'static str,
    device_count: usize,
    devices_up: usize,
    devices_down: usize,
    service_count: usize,
    services_up: usize,
    services_down: usize,
    alert_count: usize,
    network_count: usize,
    networks: Vec<NetworkGroup>,
    recent_alerts: Vec<Alert>,
}

pub async fn dashboard(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let (statuses, alerts, subnets) = tokio::task::spawn_blocking(move || {
        (
            db.get_device_statuses().unwrap_or_default(),
            db.list_active_alerts().unwrap_or_default(),
            db.list_subnets().unwrap_or_default(),
        )
    })
    .await
    .unwrap();

    let devices_up = statuses
        .iter()
        .filter(|d| d.status == ProbeStatus::Up)
        .count();
    let devices_down = statuses
        .iter()
        .filter(|d| d.status == ProbeStatus::Down)
        .count();
    let svc_up: usize = statuses.iter().map(|d| d.services_up).sum();
    let svc_down: usize = statuses.iter().map(|d| d.services_down).sum();
    let svc_total: usize = statuses.iter().map(|d| d.services_total).sum();
    let device_count = statuses.len();

    let networks = group_devices_by_network(statuses, &subnets);

    HtmlTemplate(DashboardTemplate {
        active: "dashboard".into(),
        version: crate::VERSION,
        device_count,
        devices_up,
        devices_down,
        service_count: svc_total,
        services_up: svc_up,
        services_down: svc_down,
        alert_count: alerts.len(),
        network_count: networks.len(),
        networks,
        recent_alerts: alerts.into_iter().take(10).collect(),
    })
}

pub async fn dashboard_cards_partial(State(state): State<AppState>) -> impl IntoResponse {
    dashboard(State(state)).await
}

// ── Devices ──

#[derive(Template)]
#[template(path = "devices.html")]
struct DevicesTemplate {
    active: String,
    version: &'static str,
    networks: Vec<NetworkGroup>,
}

pub async fn devices(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let (statuses, subnets) = tokio::task::spawn_blocking(move || {
        (
            db.get_device_statuses().unwrap_or_default(),
            db.list_subnets().unwrap_or_default(),
        )
    })
    .await
    .unwrap();
    let networks = group_devices_by_network(statuses, &subnets);
    HtmlTemplate(DevicesTemplate {
        active: "devices".into(),
        version: crate::VERSION,
        networks,
    })
}

#[derive(Template)]
#[template(path = "devices_table.html")]
struct DevicesTableTemplate {
    networks: Vec<NetworkGroup>,
}

pub async fn devices_table_partial(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let (statuses, subnets) = tokio::task::spawn_blocking(move || {
        (
            db.get_device_statuses().unwrap_or_default(),
            db.list_subnets().unwrap_or_default(),
        )
    })
    .await
    .unwrap();
    let networks = group_devices_by_network(statuses, &subnets);
    HtmlTemplate(DevicesTableTemplate { networks })
}

// ── Device Detail ──

#[derive(Template)]
#[template(path = "device_detail.html")]
struct DeviceDetailTemplate {
    active: String,
    version: &'static str,
    device: Device,
    status: ProbeStatus,
    interfaces: Vec<NetInterface>,
    services: Vec<ServiceWithStatus>,
    recent_alerts: Vec<Alert>,
}

pub struct ServiceWithStatus {
    pub service: Service,
    pub status: ProbeStatus,
    pub latency_us: Option<i64>,
    pub last_error: Option<String>,
}

impl ServiceWithStatus {
    pub fn latency_ms(&self) -> String {
        match self.latency_us {
            Some(v) => format!("{}ms", v / 1000),
            None => "-".into(),
        }
    }
}

pub async fn device_detail(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let db = state.db.clone();
    let result = tokio::task::spawn_blocking(move || {
        let device = match db.get_device(&id) {
            Ok(Some(d)) => d,
            _ => return None,
        };

        let interfaces = db
            .list_interfaces_for_device(&id)
            .unwrap_or_default();
        let services = db
            .list_services_for_device(&id)
            .unwrap_or_default();

        let mut svc_statuses = Vec::new();
        let mut overall_down = false;
        let mut any_up = false;

        for svc in services {
            let probe = db.get_latest_probe(&svc.id).ok().flatten();
            let (status, latency, error) = match probe {
                Some(p) => (p.status, p.latency_us, p.error),
                None => (ProbeStatus::Unknown, None, None),
            };
            if status == ProbeStatus::Down {
                overall_down = true;
            }
            if status == ProbeStatus::Up {
                any_up = true;
            }
            svc_statuses.push(ServiceWithStatus {
                service: svc,
                status,
                latency_us: latency,
                last_error: error,
            });
        }

        let overall_status = if overall_down && !any_up {
            ProbeStatus::Down
        } else if overall_down {
            ProbeStatus::Degraded
        } else if any_up {
            ProbeStatus::Up
        } else {
            ProbeStatus::Unknown
        };

        let alerts = db.list_alerts(20).unwrap_or_default();
        let device_alerts: Vec<Alert> = alerts
            .into_iter()
            .filter(|a| a.device_id == id)
            .take(10)
            .collect();

        Some((device, overall_status, interfaces, svc_statuses, device_alerts))
    })
    .await
    .unwrap();

    match result {
        Some((device, status, interfaces, services, recent_alerts)) => {
            HtmlTemplate(DeviceDetailTemplate {
                active: "devices".into(),
                version: crate::VERSION,
                device,
                status,
                interfaces,
                services,
                recent_alerts,
            })
            .into_response()
        }
        None => Redirect::to("/ui/devices").into_response(),
    }
}

// ── Network Map ──

#[derive(Template)]
#[template(path = "map.html")]
struct MapTemplate {
    active: String,
    version: &'static str,
    devices_json: String,
    links_json: String,
}

pub async fn map(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let (statuses, links, positions) = tokio::task::spawn_blocking(move || {
        (
            db.get_device_statuses().unwrap_or_default(),
            db.list_links().unwrap_or_default(),
            db.list_positions().unwrap_or_default(),
        )
    })
    .await
    .unwrap();

    let mut map_devices = Vec::new();
    for ds in statuses.iter().filter(|ds| ds.status == ProbeStatus::Up || ds.status == ProbeStatus::Degraded) {
        let pos = positions
            .iter()
            .find(|p| p.device_id == ds.device.id)
            .cloned()
            .unwrap_or_else(|| {
                let (x, y) = crate::topo::place_random();
                MapPosition {
                    device_id: ds.device.id.clone(),
                    x,
                    y,
                }
            });

        map_devices.push(serde_json::json!({
            "id": ds.device.id,
            "name": ds.device.name,
            "ip": ds.device.ip,
            "type": ds.device.device_type.as_str(),
            "status": ds.status.as_str(),
            "icon_letter": ds.device.device_type.icon_letter(),
            "icon_color": ds.device.device_type.icon_color(),
            "x": pos.x,
            "y": pos.y,
            "latency_us": ds.latency_us,
            "services_up": ds.services_up,
            "services_down": ds.services_down,
        }));
    }

    let map_links: Vec<serde_json::Value> = links
        .iter()
        .map(|l| {
            serde_json::json!({
                "id": l.id,
                "source": l.source_device_id,
                "target": l.target_device_id,
                "type": l.link_type,
            })
        })
        .collect();

    HtmlTemplate(MapTemplate {
        active: "map".into(),
        version: crate::VERSION,
        devices_json: serde_json::to_string(&map_devices).unwrap_or_else(|_| "[]".into()),
        links_json: serde_json::to_string(&map_links).unwrap_or_else(|_| "[]".into()),
    })
}

// ── Services ──

#[derive(Template)]
#[template(path = "services.html")]
struct ServicesTemplate {
    active: String,
    version: &'static str,
    networks: Vec<ServiceNetworkGroup>,
}

pub struct ServiceRow {
    pub service: Service,
    pub device_name: String,
    pub device_ip: String,
    pub status: ProbeStatus,
    pub latency_us: Option<i64>,
}

impl ServiceRow {
    pub fn latency_ms(&self) -> String {
        match self.latency_us {
            Some(v) => format!("{}ms", v / 1000),
            None => "-".into(),
        }
    }
}

pub async fn services(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let (rows, subnets) = tokio::task::spawn_blocking(move || {
        (build_service_rows(&db), db.list_subnets().unwrap_or_default())
    })
    .await
    .unwrap();
    let networks = group_services_by_network(rows, &subnets);
    HtmlTemplate(ServicesTemplate {
        active: "services".into(),
        version: crate::VERSION,
        networks,
    })
}

#[derive(Template)]
#[template(path = "services_table.html")]
struct ServicesTableTemplate {
    networks: Vec<ServiceNetworkGroup>,
}

pub async fn services_table_partial(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let (rows, subnets) = tokio::task::spawn_blocking(move || {
        (build_service_rows(&db), db.list_subnets().unwrap_or_default())
    })
    .await
    .unwrap();
    let networks = group_services_by_network(rows, &subnets);
    HtmlTemplate(ServicesTableTemplate { networks })
}

// ── Alerts ──

#[derive(Template)]
#[template(path = "alerts.html")]
struct AlertsTemplate {
    active: String,
    version: &'static str,
    alerts: Vec<AlertRow>,
}

pub struct AlertRow {
    pub alert: Alert,
    pub device_name: Option<String>,
}

pub async fn alerts(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let rows = tokio::task::spawn_blocking(move || {
        let all_alerts = db.list_alerts(200).unwrap_or_default();
        let mut rows = Vec::new();
        for alert in all_alerts {
            let device_name = db
                .get_device(&alert.device_id)
                .ok()
                .flatten()
                .map(|d| d.name);
            rows.push(AlertRow { alert, device_name });
        }
        rows
    })
    .await
    .unwrap();
    HtmlTemplate(AlertsTemplate {
        active: "alerts".into(),
        version: crate::VERSION,
        alerts: rows,
    })
}

#[derive(Template)]
#[template(path = "alerts_table.html")]
struct AlertsTableTemplate {
    alerts: Vec<AlertRow>,
}

pub async fn alerts_table_partial(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let rows = tokio::task::spawn_blocking(move || {
        let all_alerts = db.list_alerts(200).unwrap_or_default();
        let mut rows = Vec::new();
        for alert in all_alerts {
            let device_name = db
                .get_device(&alert.device_id)
                .ok()
                .flatten()
                .map(|d| d.name);
            rows.push(AlertRow { alert, device_name });
        }
        rows
    })
    .await
    .unwrap();
    HtmlTemplate(AlertsTableTemplate { alerts: rows })
}

// ── Discovery ──

#[derive(Template)]
#[template(path = "discovery.html")]
struct DiscoveryTemplate {
    active: String,
    version: &'static str,
    subnets: Vec<Subnet>,
}

pub async fn discovery(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let subnets = tokio::task::spawn_blocking(move || {
        db.list_subnets().unwrap_or_default()
    })
    .await
    .unwrap();
    HtmlTemplate(DiscoveryTemplate {
        active: "discovery".into(),
        version: crate::VERSION,
        subnets,
    })
}

// ── Performance ──

#[derive(Template)]
#[template(path = "performance.html")]
struct PerformanceTemplate {
    active: String,
    version: &'static str,
    devices: Vec<Device>,
}

pub async fn performance(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let devices = tokio::task::spawn_blocking(move || {
        db.list_devices().unwrap_or_default()
    })
    .await
    .unwrap();
    HtmlTemplate(PerformanceTemplate {
        active: "performance".into(),
        version: crate::VERSION,
        devices,
    })
}

// ── Settings ──

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    active: String,
    version: &'static str,
    rules: Vec<AlertRule>,
    has_email: bool,
    has_webhook: bool,
}

pub async fn settings(State(state): State<AppState>) -> impl IntoResponse {
    let db = state.db.clone();
    let rules = tokio::task::spawn_blocking(move || {
        db.list_alert_rules().unwrap_or_default()
    })
    .await
    .unwrap();
    HtmlTemplate(SettingsTemplate {
        active: "settings".into(),
        version: crate::VERSION,
        rules,
        has_email: state.config.alerting.email.is_some(),
        has_webhook: state.config.alerting.webhook.is_some(),
    })
}
