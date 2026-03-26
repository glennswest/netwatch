//! HTML page handlers using Askama templates.

use super::{AppState, HtmlTemplate};
use crate::models::*;
use askama::Template;
use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect},
};

// ── Redirect ──

pub async fn redirect_dashboard() -> Redirect {
    Redirect::permanent("/ui/")
}

// ── Dashboard ──

#[derive(Template)]
#[template(path = "dashboard.html")]
struct DashboardTemplate {
    active: String,
    device_count: usize,
    devices_up: usize,
    devices_down: usize,
    service_count: usize,
    services_up: usize,
    services_down: usize,
    alert_count: usize,
    subnet_count: usize,
    devices: Vec<DeviceStatus>,
    recent_alerts: Vec<Alert>,
}

pub async fn dashboard(State(state): State<AppState>) -> impl IntoResponse {
    let statuses = state.db.get_device_statuses().unwrap_or_default();
    let alerts = state.db.list_active_alerts().unwrap_or_default();
    let subnets = state.db.list_subnets().unwrap_or_default();

    let devices_up = statuses.iter().filter(|d| d.status == ProbeStatus::Up).count();
    let devices_down = statuses.iter().filter(|d| d.status == ProbeStatus::Down).count();
    let svc_up: usize = statuses.iter().map(|d| d.services_up).sum();
    let svc_down: usize = statuses.iter().map(|d| d.services_down).sum();
    let svc_total: usize = statuses.iter().map(|d| d.services_total).sum();

    HtmlTemplate(DashboardTemplate {
        active: "dashboard".into(),
        device_count: statuses.len(),
        devices_up,
        devices_down,
        service_count: svc_total,
        services_up: svc_up,
        services_down: svc_down,
        alert_count: alerts.len(),
        subnet_count: subnets.len(),
        devices: statuses,
        recent_alerts: alerts.into_iter().take(10).collect(),
    })
}

pub async fn dashboard_cards_partial(State(state): State<AppState>) -> impl IntoResponse {
    // Re-render just the dashboard content for HTMX refresh
    dashboard(State(state)).await
}

// ── Devices ──

#[derive(Template)]
#[template(path = "devices.html")]
struct DevicesTemplate {
    active: String,
    devices: Vec<DeviceStatus>,
}

pub async fn devices(State(state): State<AppState>) -> impl IntoResponse {
    let statuses = state.db.get_device_statuses().unwrap_or_default();
    HtmlTemplate(DevicesTemplate {
        active: "devices".into(),
        devices: statuses,
    })
}

#[derive(Template)]
#[template(path = "devices_table.html")]
struct DevicesTableTemplate {
    devices: Vec<DeviceStatus>,
}

pub async fn devices_table_partial(State(state): State<AppState>) -> impl IntoResponse {
    let statuses = state.db.get_device_statuses().unwrap_or_default();
    HtmlTemplate(DevicesTableTemplate { devices: statuses })
}

// ── Device Detail ──

#[derive(Template)]
#[template(path = "device_detail.html")]
struct DeviceDetailTemplate {
    active: String,
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
    let device = match state.db.get_device(&id) {
        Ok(Some(d)) => d,
        _ => return Redirect::to("/ui/devices").into_response(),
    };

    let interfaces = state.db.list_interfaces_for_device(&id).unwrap_or_default();
    let services = state.db.list_services_for_device(&id).unwrap_or_default();

    let mut svc_statuses = Vec::new();
    let mut overall_down = false;
    let mut any_up = false;

    for svc in services {
        let probe = state.db.get_latest_probe(&svc.id).ok().flatten();
        let (status, latency, error) = match probe {
            Some(p) => (p.status, p.latency_us, p.error),
            None => (ProbeStatus::Unknown, None, None),
        };
        if status == ProbeStatus::Down { overall_down = true; }
        if status == ProbeStatus::Up { any_up = true; }
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

    let alerts = state.db.list_alerts(20).unwrap_or_default();
    let device_alerts: Vec<Alert> = alerts
        .into_iter()
        .filter(|a| a.device_id == id)
        .take(10)
        .collect();

    HtmlTemplate(DeviceDetailTemplate {
        active: "devices".into(),
        device,
        status: overall_status,
        interfaces,
        services: svc_statuses,
        recent_alerts: device_alerts,
    }).into_response()
}

// ── Network Map ──

#[derive(Template)]
#[template(path = "map.html")]
struct MapTemplate {
    active: String,
    devices_json: String,
    links_json: String,
}

pub async fn map(State(state): State<AppState>) -> impl IntoResponse {
    let statuses = state.db.get_device_statuses().unwrap_or_default();
    let links = state.db.list_links().unwrap_or_default();
    let positions = state.db.list_positions().unwrap_or_default();

    // Build map data
    let mut map_devices = Vec::new();
    for ds in &statuses {
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
        devices_json: serde_json::to_string(&map_devices).unwrap_or_else(|_| "[]".into()),
        links_json: serde_json::to_string(&map_links).unwrap_or_else(|_| "[]".into()),
    })
}

// ── Services ──

#[derive(Template)]
#[template(path = "services.html")]
struct ServicesTemplate {
    active: String,
    services: Vec<ServiceRow>,
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
    let all_services = state.db.list_services().unwrap_or_default();
    let mut rows = Vec::new();

    for svc in all_services {
        let device = state
            .db
            .get_device(&svc.device_id)
            .ok()
            .flatten();
        let probe = state.db.get_latest_probe(&svc.id).ok().flatten();

        rows.push(ServiceRow {
            device_name: device.as_ref().map(|d| d.name.clone()).unwrap_or_default(),
            device_ip: device.as_ref().map(|d| d.ip.clone()).unwrap_or_default(),
            status: probe.as_ref().map(|p| p.status).unwrap_or(ProbeStatus::Unknown),
            latency_us: probe.as_ref().and_then(|p| p.latency_us),
            service: svc,
        });
    }

    HtmlTemplate(ServicesTemplate {
        active: "services".into(),
        services: rows,
    })
}

#[derive(Template)]
#[template(path = "services_table.html")]
struct ServicesTableTemplate {
    services: Vec<ServiceRow>,
}

pub async fn services_table_partial(State(state): State<AppState>) -> impl IntoResponse {
    let all_services = state.db.list_services().unwrap_or_default();
    let mut rows = Vec::new();
    for svc in all_services {
        let device = state.db.get_device(&svc.device_id).ok().flatten();
        let probe = state.db.get_latest_probe(&svc.id).ok().flatten();
        rows.push(ServiceRow {
            device_name: device.as_ref().map(|d| d.name.clone()).unwrap_or_default(),
            device_ip: device.as_ref().map(|d| d.ip.clone()).unwrap_or_default(),
            status: probe.as_ref().map(|p| p.status).unwrap_or(ProbeStatus::Unknown),
            latency_us: probe.as_ref().and_then(|p| p.latency_us),
            service: svc,
        });
    }
    HtmlTemplate(ServicesTableTemplate { services: rows })
}

// ── Alerts ──

#[derive(Template)]
#[template(path = "alerts.html")]
struct AlertsTemplate {
    active: String,
    alerts: Vec<AlertRow>,
}

pub struct AlertRow {
    pub alert: Alert,
    pub device_name: Option<String>,
}

pub async fn alerts(State(state): State<AppState>) -> impl IntoResponse {
    let all_alerts = state.db.list_alerts(200).unwrap_or_default();
    let mut rows = Vec::new();
    for alert in all_alerts {
        let device_name = state
            .db
            .get_device(&alert.device_id)
            .ok()
            .flatten()
            .map(|d| d.name);
        rows.push(AlertRow { alert, device_name });
    }
    HtmlTemplate(AlertsTemplate {
        active: "alerts".into(),
        alerts: rows,
    })
}

#[derive(Template)]
#[template(path = "alerts_table.html")]
struct AlertsTableTemplate {
    alerts: Vec<AlertRow>,
}

pub async fn alerts_table_partial(State(state): State<AppState>) -> impl IntoResponse {
    let all_alerts = state.db.list_alerts(200).unwrap_or_default();
    let mut rows = Vec::new();
    for alert in all_alerts {
        let device_name = state.db.get_device(&alert.device_id).ok().flatten().map(|d| d.name);
        rows.push(AlertRow { alert, device_name });
    }
    HtmlTemplate(AlertsTableTemplate { alerts: rows })
}

// ── Discovery ──

#[derive(Template)]
#[template(path = "discovery.html")]
struct DiscoveryTemplate {
    active: String,
    subnets: Vec<Subnet>,
}

pub async fn discovery(State(state): State<AppState>) -> impl IntoResponse {
    let subnets = state.db.list_subnets().unwrap_or_default();
    HtmlTemplate(DiscoveryTemplate {
        active: "discovery".into(),
        subnets,
    })
}

// ── Performance ──

#[derive(Template)]
#[template(path = "performance.html")]
struct PerformanceTemplate {
    active: String,
    devices: Vec<Device>,
}

pub async fn performance(State(state): State<AppState>) -> impl IntoResponse {
    let devices = state.db.list_devices().unwrap_or_default();
    HtmlTemplate(PerformanceTemplate {
        active: "performance".into(),
        devices,
    })
}

// ── Settings ──

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    active: String,
    rules: Vec<AlertRule>,
    has_email: bool,
    has_webhook: bool,
}

pub async fn settings(State(state): State<AppState>) -> impl IntoResponse {
    let rules = state.db.list_alert_rules().unwrap_or_default();
    HtmlTemplate(SettingsTemplate {
        active: "settings".into(),
        rules,
        has_email: state.config.alerting.email.is_some(),
        has_webhook: state.config.alerting.webhook.is_some(),
    })
}
