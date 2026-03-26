//! REST API handlers.

use super::AppState;
use crate::models::*;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

// ── Devices ──

pub async fn list_devices(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.get_device_statuses() {
        Ok(statuses) => Json(statuses).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn get_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_device(&id) {
        Ok(Some(device)) => Json(device).into_response(),
        Ok(None) => StatusCode::NOT_FOUND.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create_device(
    State(state): State<AppState>,
    Json(req): Json<CreateDevice>,
) -> impl IntoResponse {
    let now = chrono::Utc::now().to_rfc3339();
    let device = Device {
        id: uuid::Uuid::new_v4().to_string(),
        ip: req.ip,
        name: req.name,
        mac: req.mac,
        vendor: None,
        device_type: req
            .device_type
            .as_deref()
            .map(DeviceType::parse)
            .unwrap_or(DeviceType::Other),
        snmp_community: req.snmp_community,
        snmp_version: 2,
        sys_descr: None,
        sys_object_id: None,
        location: req.location,
        notes: req.notes,
        enabled: true,
        last_seen: None,
        created_at: now.clone(),
        updated_at: now,
    };

    // Auto-place on map
    let (x, y) = crate::topo::place_random();
    let pos = MapPosition {
        device_id: device.id.clone(),
        x,
        y,
    };

    match state.db.insert_device(device.clone()) {
        Ok(()) => {
            let _ = state.db.upsert_position(pos);
            (StatusCode::CREATED, Json(device)).into_response()
        }
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn update_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateDevice>,
) -> impl IntoResponse {
    let old = match state.db.get_device(&id) {
        Ok(Some(d)) => d,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let mut new = old.clone();
    if let Some(name) = req.name { new.name = name; }
    if let Some(ip) = req.ip { new.ip = ip; }
    if let Some(mac) = req.mac { new.mac = Some(mac); }
    if let Some(dt) = req.device_type { new.device_type = DeviceType::parse(&dt); }
    if let Some(c) = req.snmp_community { new.snmp_community = Some(c); }
    if let Some(l) = req.location { new.location = Some(l); }
    if let Some(n) = req.notes { new.notes = Some(n); }
    if let Some(e) = req.enabled { new.enabled = e; }
    new.updated_at = chrono::Utc::now().to_rfc3339();

    match state.db.update_device(old, new.clone()) {
        Ok(()) => Json(new).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_device(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_device_cascade(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ── Interfaces ──

pub async fn list_interfaces(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.list_interfaces_for_device(&id) {
        Ok(ifaces) => Json(ifaces).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ── Links ──

pub async fn list_links(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.list_links() {
        Ok(links) => Json(links).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create_link(
    State(state): State<AppState>,
    Json(req): Json<CreateLink>,
) -> impl IntoResponse {
    let link = Link {
        id: uuid::Uuid::new_v4().to_string(),
        source_device_id: req.source_device_id,
        target_device_id: req.target_device_id,
        source_if_id: req.source_if_id,
        target_if_id: req.target_if_id,
        link_type: req.link_type.unwrap_or_else(|| "ethernet".into()),
        bandwidth_mbps: req.bandwidth_mbps,
    };
    match state.db.insert_link(link.clone()) {
        Ok(()) => (StatusCode::CREATED, Json(link)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_link(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_link(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ── Services ──

pub async fn list_services(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.list_services() {
        Ok(services) => Json(services).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn list_device_services(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.list_services_for_device(&id) {
        Ok(services) => Json(services).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create_service(
    State(state): State<AppState>,
    Json(req): Json<CreateService>,
) -> impl IntoResponse {
    let device = match state.db.get_device(&req.device_id) {
        Ok(Some(d)) => d,
        Ok(None) => return (StatusCode::NOT_FOUND, "device not found").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let svc = Service {
        id: uuid::Uuid::new_v4().to_string(),
        device_id: req.device_id,
        name: req.name,
        probe_type: ProbeType::parse(&req.probe_type),
        host: req.host.or(Some(device.ip)),
        port: req.port,
        url: req.url,
        interval_secs: req.interval_secs.unwrap_or(60),
        timeout_ms: req.timeout_ms.unwrap_or(5000),
        enabled: true,
    };

    match state.db.insert_service(svc.clone()) {
        Ok(()) => (StatusCode::CREATED, Json(svc)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_service(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_service(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn list_probes(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100usize);
    match state.db.list_probe_results(&id, limit) {
        Ok(probes) => Json(probes).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ── Alerts ──

pub async fn list_alerts(
    State(state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(100usize);
    match state.db.list_alerts(limit) {
        Ok(alerts) => Json(alerts).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn acknowledge_alert(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.acknowledge_alert(&id) {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_alert(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Reuse acknowledge — we don't hard-delete alerts via API normally
    match state.db.acknowledge_alert(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

// ── Subnets ──

pub async fn list_subnets(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.list_subnets() {
        Ok(subnets) => Json(subnets).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn create_subnet(
    State(state): State<AppState>,
    Json(req): Json<CreateSubnet>,
) -> impl IntoResponse {
    // Validate CIDR
    if req.cidr.parse::<ipnetwork::IpNetwork>().is_err() {
        return (StatusCode::BAD_REQUEST, "invalid CIDR").into_response();
    }

    let subnet = Subnet {
        id: uuid::Uuid::new_v4().to_string(),
        name: req.name,
        cidr: req.cidr,
        snmp_community: req.snmp_community.unwrap_or_else(|| "public".into()),
        scan_enabled: true,
        last_scan: None,
    };

    match state.db.insert_subnet(subnet.clone()) {
        Ok(()) => (StatusCode::CREATED, Json(subnet)).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn delete_subnet(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_subnet(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn trigger_scan(
    State(state): State<AppState>,
    Json(body): Json<serde_json::Value>,
) -> impl IntoResponse {
    let subnet_id = body
        .get("subnet_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let subnet = match state.db.get_subnet(subnet_id) {
        Ok(Some(s)) => s,
        Ok(None) => return (StatusCode::NOT_FOUND, "subnet not found").into_response(),
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };

    let db = state.db.clone();
    let config = state.config.clone();
    let ws_tx = state.ws_tx.clone();

    tokio::spawn(async move {
        match crate::discovery::scan_single(&db, &config, &subnet, &ws_tx).await {
            Ok(found) => tracing::info!("manual scan of {} found {} hosts", subnet.cidr, found),
            Err(e) => tracing::error!("manual scan error: {}", e),
        }
    });

    (StatusCode::ACCEPTED, "scan started").into_response()
}

// ── Map ──

pub async fn list_positions(State(state): State<AppState>) -> impl IntoResponse {
    match state.db.list_positions() {
        Ok(positions) => Json(positions).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn update_position(
    State(state): State<AppState>,
    Json(req): Json<PositionUpdate>,
) -> impl IntoResponse {
    let pos = MapPosition {
        device_id: req.device_id,
        x: req.x,
        y: req.y,
    };
    match state.db.upsert_position(pos) {
        Ok(()) => StatusCode::OK.into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn auto_layout(State(state): State<AppState>) -> impl IntoResponse {
    let devices = match state.db.list_devices() {
        Ok(d) => d,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let links = match state.db.list_links() {
        Ok(l) => l,
        Err(e) => return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    };
    let positions = state.db.list_positions().unwrap_or_default();

    let device_ids: Vec<String> = devices.iter().map(|d| d.id.clone()).collect();
    let link_pairs: Vec<(String, String)> = links
        .iter()
        .map(|l| (l.source_device_id.clone(), l.target_device_id.clone()))
        .collect();

    let new_positions = crate::topo::auto_layout(&device_ids, &positions, &link_pairs, 100);

    for pos in &new_positions {
        let _ = state.db.upsert_position(pos.clone());
    }

    Json(new_positions).into_response()
}

// ── Metrics ──

pub async fn list_device_metrics(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let metric = params
        .get("metric")
        .map(|s| s.as_str())
        .unwrap_or("icmp_latency_us");
    let limit = params
        .get("limit")
        .and_then(|v| v.parse().ok())
        .unwrap_or(200usize);
    match state.db.list_metrics(&id, metric, limit) {
        Ok(metrics) => Json(metrics).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

pub async fn query_metrics(
    State(state): State<AppState>,
    Query(q): Query<MetricQuery>,
) -> impl IntoResponse {
    let device_id = q.device_id.as_deref().unwrap_or("");
    let metric_name = q.metric_name.as_deref().unwrap_or("icmp_latency_us");
    let limit = q.limit.unwrap_or(200);
    match state.db.list_metrics(device_id, metric_name, limit) {
        Ok(metrics) => Json(metrics).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}
