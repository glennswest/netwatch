use crate::models::*;
use anyhow::Result;
use redb::{Database, ReadableTable, ReadableTableMetadata, TableDefinition};
use std::path::Path;
use std::sync::Arc;

// Table definitions: key = id (string), value = JSON bytes
const DEVICES: TableDefinition<&str, &[u8]> = TableDefinition::new("devices");
const INTERFACES: TableDefinition<&str, &[u8]> = TableDefinition::new("interfaces");
const LINKS: TableDefinition<&str, &[u8]> = TableDefinition::new("links");
const SERVICES: TableDefinition<&str, &[u8]> = TableDefinition::new("services");
const PROBES: TableDefinition<&str, &[u8]> = TableDefinition::new("probes");
const ALERTS: TableDefinition<&str, &[u8]> = TableDefinition::new("alerts");
const SUBNETS: TableDefinition<&str, &[u8]> = TableDefinition::new("subnets");
const POSITIONS: TableDefinition<&str, &[u8]> = TableDefinition::new("positions");
const METRICS: TableDefinition<&str, &[u8]> = TableDefinition::new("metrics");
const ALERT_RULES: TableDefinition<&str, &[u8]> = TableDefinition::new("alert_rules");
const LATEST_PROBES: TableDefinition<&str, &[u8]> = TableDefinition::new("latest_probes");

pub struct Db {
    inner: Database,
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let db = Database::create(path)?;
        // Ensure all tables exist
        let write = db.begin_write()?;
        write.open_table(DEVICES)?;
        write.open_table(INTERFACES)?;
        write.open_table(LINKS)?;
        write.open_table(SERVICES)?;
        write.open_table(PROBES)?;
        write.open_table(ALERTS)?;
        write.open_table(SUBNETS)?;
        write.open_table(POSITIONS)?;
        write.open_table(METRICS)?;
        write.open_table(ALERT_RULES)?;
        write.open_table(LATEST_PROBES)?;
        write.commit()?;
        Ok(Self { inner: db })
    }

    // ── Generic helpers ──

    fn get_all<T: serde::de::DeserializeOwned>(&self, table: TableDefinition<&str, &[u8]>) -> Result<Vec<T>> {
        let read = self.inner.begin_read()?;
        let tbl = read.open_table(table)?;
        let mut items = Vec::new();
        for entry in tbl.iter()? {
            let (_, val) = entry?;
            items.push(serde_json::from_slice(val.value())?);
        }
        Ok(items)
    }

    fn get_one<T: serde::de::DeserializeOwned>(&self, table: TableDefinition<&str, &[u8]>, id: &str) -> Result<Option<T>> {
        let read = self.inner.begin_read()?;
        let tbl = read.open_table(table)?;
        match tbl.get(id)? {
            Some(val) => Ok(Some(serde_json::from_slice(val.value())?)),
            None => Ok(None),
        }
    }

    fn put<T: serde::Serialize>(&self, table: TableDefinition<&str, &[u8]>, id: &str, item: &T) -> Result<()> {
        let json = serde_json::to_vec(item)?;
        let write = self.inner.begin_write()?;
        {
            let mut tbl = write.open_table(table)?;
            tbl.insert(id, json.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    fn del(&self, table: TableDefinition<&str, &[u8]>, id: &str) -> Result<()> {
        let write = self.inner.begin_write()?;
        {
            let mut tbl = write.open_table(table)?;
            tbl.remove(id)?;
        }
        write.commit()?;
        Ok(())
    }

    fn clear_table(&self, table: TableDefinition<&str, &[u8]>) -> Result<usize> {
        let write = self.inner.begin_write()?;
        let count;
        {
            let mut tbl = write.open_table(table)?;
            count = tbl.len()? as usize;
            tbl.retain(|_, _| false)?;
        }
        write.commit()?;
        Ok(count)
    }

    /// Wipe all tables — full database reset.
    pub fn reset_all(&self) -> Result<usize> {
        let mut total = 0;
        total += self.clear_table(DEVICES)?;
        total += self.clear_table(INTERFACES)?;
        total += self.clear_table(LINKS)?;
        total += self.clear_table(SERVICES)?;
        total += self.clear_table(PROBES)?;
        total += self.clear_table(ALERTS)?;
        total += self.clear_table(SUBNETS)?;
        total += self.clear_table(POSITIONS)?;
        total += self.clear_table(METRICS)?;
        total += self.clear_table(ALERT_RULES)?;
        total += self.clear_table(LATEST_PROBES)?;
        Ok(total)
    }

    // ── Devices ──

    pub fn list_devices(&self) -> Result<Vec<Device>> {
        self.get_all(DEVICES)
    }

    pub fn get_device(&self, id: &str) -> Result<Option<Device>> {
        self.get_one(DEVICES, id)
    }

    pub fn get_device_by_ip(&self, ip: &str) -> Result<Option<Device>> {
        let devices = self.list_devices()?;
        Ok(devices.into_iter().find(|d| d.ip == ip))
    }

    pub fn insert_device(&self, device: Device) -> Result<()> {
        self.put(DEVICES, &device.id, &device)
    }

    pub fn update_device(&self, _old: Device, new: Device) -> Result<()> {
        self.put(DEVICES, &new.id, &new)
    }

    pub fn delete_device(&self, id: &str) -> Result<()> {
        self.del(DEVICES, id)
    }

    // ── Interfaces ──

    pub fn list_interfaces_for_device(&self, device_id: &str) -> Result<Vec<NetInterface>> {
        let all: Vec<NetInterface> = self.get_all(INTERFACES)?;
        Ok(all.into_iter().filter(|i| i.device_id == device_id).collect())
    }

    pub fn get_interface(&self, id: &str) -> Result<Option<NetInterface>> {
        self.get_one(INTERFACES, id)
    }

    pub fn upsert_interface(&self, iface: NetInterface) -> Result<()> {
        self.put(INTERFACES, &iface.id, &iface)
    }

    pub fn delete_interfaces_for_device(&self, device_id: &str) -> Result<()> {
        let ifaces = self.list_interfaces_for_device(device_id)?;
        if !ifaces.is_empty() {
            let write = self.inner.begin_write()?;
            {
                let mut tbl = write.open_table(INTERFACES)?;
                for iface in &ifaces {
                    tbl.remove(iface.id.as_str())?;
                }
            }
            write.commit()?;
        }
        Ok(())
    }

    // ── Links ──

    pub fn list_links(&self) -> Result<Vec<Link>> {
        self.get_all(LINKS)
    }

    pub fn insert_link(&self, link: Link) -> Result<()> {
        self.put(LINKS, &link.id, &link)
    }

    pub fn delete_link(&self, id: &str) -> Result<()> {
        self.del(LINKS, id)
    }

    pub fn delete_links_for_device(&self, device_id: &str) -> Result<()> {
        let links = self.list_links()?;
        let to_remove: Vec<&Link> = links
            .iter()
            .filter(|l| l.source_device_id == device_id || l.target_device_id == device_id)
            .collect();
        if !to_remove.is_empty() {
            let write = self.inner.begin_write()?;
            {
                let mut tbl = write.open_table(LINKS)?;
                for link in to_remove {
                    tbl.remove(link.id.as_str())?;
                }
            }
            write.commit()?;
        }
        Ok(())
    }

    // ── Services ──

    pub fn list_services(&self) -> Result<Vec<Service>> {
        self.get_all(SERVICES)
    }

    pub fn list_services_for_device(&self, device_id: &str) -> Result<Vec<Service>> {
        let all = self.list_services()?;
        Ok(all.into_iter().filter(|s| s.device_id == device_id).collect())
    }

    pub fn get_service(&self, id: &str) -> Result<Option<Service>> {
        self.get_one(SERVICES, id)
    }

    pub fn insert_service(&self, service: Service) -> Result<()> {
        self.put(SERVICES, &service.id, &service)
    }

    pub fn delete_service(&self, id: &str) -> Result<()> {
        self.del(SERVICES, id)
    }

    pub fn delete_services_for_device(&self, device_id: &str) -> Result<()> {
        let svcs = self.list_services_for_device(device_id)?;
        if !svcs.is_empty() {
            let write = self.inner.begin_write()?;
            {
                let mut tbl = write.open_table(SERVICES)?;
                for svc in &svcs {
                    tbl.remove(svc.id.as_str())?;
                }
            }
            write.commit()?;
        }
        Ok(())
    }

    // ── Probe Results ──

    pub fn insert_probe_result(&self, result: ProbeResult) -> Result<()> {
        let json = serde_json::to_vec(&result)?;
        let write = self.inner.begin_write()?;
        {
            let mut probes = write.open_table(PROBES)?;
            probes.insert(result.id.as_str(), json.as_slice())?;
            let mut latest = write.open_table(LATEST_PROBES)?;
            latest.insert(result.service_id.as_str(), json.as_slice())?;
        }
        write.commit()?;
        Ok(())
    }

    pub fn list_probe_results(&self, service_id: &str, limit: usize) -> Result<Vec<ProbeResult>> {
        let all: Vec<ProbeResult> = self.get_all(PROBES)?;
        let mut filtered: Vec<ProbeResult> = all
            .into_iter()
            .filter(|p| p.service_id == service_id)
            .collect();
        filtered.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        filtered.truncate(limit);
        Ok(filtered)
    }

    pub fn get_latest_probe(&self, service_id: &str) -> Result<Option<ProbeResult>> {
        self.get_one(LATEST_PROBES, service_id)
    }

    pub fn get_all_latest_probes(&self) -> Result<Vec<ProbeResult>> {
        self.get_all(LATEST_PROBES)
    }

    // ── Alerts ──

    pub fn insert_alert(&self, alert: Alert) -> Result<()> {
        self.put(ALERTS, &alert.id, &alert)
    }

    pub fn list_alerts(&self, limit: usize) -> Result<Vec<Alert>> {
        let mut all: Vec<Alert> = self.get_all(ALERTS)?;
        all.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        all.truncate(limit);
        Ok(all)
    }

    pub fn list_active_alerts(&self) -> Result<Vec<Alert>> {
        let all: Vec<Alert> = self.get_all(ALERTS)?;
        let mut active: Vec<Alert> = all.into_iter().filter(|a| !a.acknowledged).collect();
        active.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(active)
    }

    pub fn acknowledge_alert(&self, id: &str) -> Result<()> {
        if let Some(mut alert) = self.get_one::<Alert>(ALERTS, id)? {
            alert.acknowledged = true;
            self.put(ALERTS, id, &alert)?;
        }
        Ok(())
    }

    pub fn count_active_alerts(&self) -> Result<usize> {
        Ok(self.list_active_alerts()?.len())
    }

    pub fn clear_all_alerts(&self) -> Result<usize> {
        let alerts: Vec<Alert> = self.get_all(ALERTS)?;
        let count = alerts.len();
        let write = self.inner.begin_write()?;
        {
            let mut table = write.open_table(ALERTS)?;
            for alert in &alerts {
                table.remove(alert.id.as_str())?;
            }
        }
        write.commit()?;
        Ok(count)
    }

    // ── Subnets ──

    pub fn list_subnets(&self) -> Result<Vec<Subnet>> {
        self.get_all(SUBNETS)
    }

    pub fn get_subnet(&self, id: &str) -> Result<Option<Subnet>> {
        self.get_one(SUBNETS, id)
    }

    pub fn get_subnet_by_cidr(&self, cidr: &str) -> Result<Option<Subnet>> {
        let all = self.list_subnets()?;
        Ok(all.into_iter().find(|s| s.cidr == cidr))
    }

    pub fn insert_subnet(&self, subnet: Subnet) -> Result<()> {
        self.put(SUBNETS, &subnet.id, &subnet)
    }

    pub fn upsert_subnet_by_cidr(&self, cidr: &str, name: &str, community: &str) -> Result<()> {
        if self.get_subnet_by_cidr(cidr)?.is_none() {
            let subnet = Subnet {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.to_string(),
                cidr: cidr.to_string(),
                snmp_community: community.to_string(),
                scan_enabled: true,
                last_scan: None,
            };
            self.insert_subnet(subnet)?;
        }
        Ok(())
    }

    pub fn update_subnet_last_scan(&self, id: &str, time: &str) -> Result<()> {
        if let Some(mut subnet) = self.get_one::<Subnet>(SUBNETS, id)? {
            subnet.last_scan = Some(time.to_string());
            self.put(SUBNETS, id, &subnet)?;
        }
        Ok(())
    }

    pub fn delete_subnet(&self, id: &str) -> Result<()> {
        self.del(SUBNETS, id)
    }

    // ── Map Positions ──

    pub fn list_positions(&self) -> Result<Vec<MapPosition>> {
        self.get_all(POSITIONS)
    }

    pub fn get_position(&self, device_id: &str) -> Result<Option<MapPosition>> {
        self.get_one(POSITIONS, device_id)
    }

    pub fn upsert_position(&self, pos: MapPosition) -> Result<()> {
        self.put(POSITIONS, &pos.device_id, &pos)
    }

    pub fn delete_position(&self, device_id: &str) -> Result<()> {
        self.del(POSITIONS, device_id)
    }

    // ── Metrics ──

    pub fn insert_metric(&self, metric: Metric) -> Result<()> {
        self.put(METRICS, &metric.id, &metric)
    }

    pub fn list_metrics(&self, device_id: &str, metric_name: &str, limit: usize) -> Result<Vec<Metric>> {
        let all: Vec<Metric> = self.get_all(METRICS)?;
        let mut filtered: Vec<Metric> = all
            .into_iter()
            .filter(|m| m.device_id == device_id && m.metric_name == metric_name)
            .collect();
        filtered.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        filtered.truncate(limit);
        filtered.reverse(); // oldest first for charts
        Ok(filtered)
    }

    // ── Alert Rules ──

    pub fn list_alert_rules(&self) -> Result<Vec<AlertRule>> {
        self.get_all(ALERT_RULES)
    }

    pub fn insert_alert_rule(&self, rule: AlertRule) -> Result<()> {
        self.put(ALERT_RULES, &rule.id, &rule)
    }

    pub fn delete_alert_rule(&self, id: &str) -> Result<()> {
        self.del(ALERT_RULES, id)
    }

    // ── Device status (composite) ──

    pub fn get_device_statuses(&self) -> Result<Vec<DeviceStatus>> {
        let devices = self.list_devices()?;
        let all_services = self.list_services()?;
        let all_latest: Vec<ProbeResult> = self.get_all(LATEST_PROBES)?;
        let all_positions = self.list_positions()?;

        // Index services by device_id
        let mut svc_by_device: std::collections::HashMap<String, Vec<&Service>> =
            std::collections::HashMap::new();
        for svc in &all_services {
            svc_by_device.entry(svc.device_id.clone()).or_default().push(svc);
        }

        // Index latest probes by service_id
        let mut probe_by_svc: std::collections::HashMap<String, &ProbeResult> =
            std::collections::HashMap::new();
        for probe in &all_latest {
            probe_by_svc.insert(probe.service_id.clone(), probe);
        }

        // Index positions by device_id
        let mut pos_by_device: std::collections::HashMap<String, &MapPosition> =
            std::collections::HashMap::new();
        for pos in &all_positions {
            pos_by_device.insert(pos.device_id.clone(), pos);
        }

        let mut statuses = Vec::with_capacity(devices.len());

        for device in devices {
            let services = svc_by_device.get(&device.id);
            let svc_count = services.map_or(0, |v| v.len());
            let mut up = 0usize;
            let mut down = 0usize;
            let mut latency = None;

            if let Some(svcs) = services {
                for svc in svcs {
                    if let Some(probe) = probe_by_svc.get(&svc.id) {
                        match probe.status {
                            ProbeStatus::Up => up += 1,
                            ProbeStatus::Down => down += 1,
                            _ => {}
                        }
                        if svc.probe_type == ProbeType::Icmp {
                            latency = probe.latency_us;
                        }
                    }
                }
            }

            let status = if svc_count == 0 {
                ProbeStatus::Unknown
            } else if down > 0 && up == 0 {
                ProbeStatus::Down
            } else if down > 0 {
                ProbeStatus::Degraded
            } else if up > 0 {
                ProbeStatus::Up
            } else {
                ProbeStatus::Unknown
            };

            let position = pos_by_device.get(&device.id).cloned().cloned();
            statuses.push(DeviceStatus {
                device,
                status,
                services_up: up,
                services_down: down,
                services_total: svc_count,
                latency_us: latency,
                position,
            });
        }
        Ok(statuses)
    }

    // ── Cascade delete ──

    pub fn delete_device_cascade(&self, id: &str) -> Result<()> {
        self.delete_services_for_device(id)?;
        self.delete_interfaces_for_device(id)?;
        self.delete_links_for_device(id)?;
        self.delete_position(id)?;
        self.delete_device(id)?;
        Ok(())
    }

    // ── Retention ──

    pub fn cleanup_before(&self, before: &str) -> Result<(usize, usize, usize)> {
        let mut probes_removed = 0;
        let mut metrics_removed = 0;
        let mut alerts_removed = 0;

        // Probes
        let old_probes: Vec<String> = self.get_all::<ProbeResult>(PROBES)?
            .into_iter()
            .filter(|p| p.timestamp.as_str() < before)
            .map(|p| p.id)
            .collect();
        if !old_probes.is_empty() {
            let write = self.inner.begin_write()?;
            {
                let mut tbl = write.open_table(PROBES)?;
                for id in &old_probes {
                    tbl.remove(id.as_str())?;
                    probes_removed += 1;
                }
            }
            write.commit()?;
        }

        // Metrics
        let old_metrics: Vec<String> = self.get_all::<Metric>(METRICS)?
            .into_iter()
            .filter(|m| m.timestamp.as_str() < before)
            .map(|m| m.id)
            .collect();
        if !old_metrics.is_empty() {
            let write = self.inner.begin_write()?;
            {
                let mut tbl = write.open_table(METRICS)?;
                for id in &old_metrics {
                    tbl.remove(id.as_str())?;
                    metrics_removed += 1;
                }
            }
            write.commit()?;
        }

        // Old acknowledged alerts
        let old_alerts: Vec<String> = self.get_all::<Alert>(ALERTS)?
            .into_iter()
            .filter(|a| a.acknowledged && a.created_at.as_str() < before)
            .map(|a| a.id)
            .collect();
        if !old_alerts.is_empty() {
            let write = self.inner.begin_write()?;
            {
                let mut tbl = write.open_table(ALERTS)?;
                for id in &old_alerts {
                    tbl.remove(id.as_str())?;
                    alerts_removed += 1;
                }
            }
            write.commit()?;
        }

        Ok((probes_removed, metrics_removed, alerts_removed))
    }
}

/// Background retention loop.
pub async fn retention_loop(db: Arc<Db>, config: Arc<crate::config::Config>) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
    loop {
        interval.tick().await;
        let before = chrono::Utc::now()
            - chrono::Duration::days(config.retention.probe_days as i64);
        let before_str = before.to_rfc3339();
        match db.cleanup_before(&before_str) {
            Ok((p, m, a)) => {
                if p + m + a > 0 {
                    tracing::info!("retention: removed {} probes, {} metrics, {} alerts", p, m, a);
                }
            }
            Err(e) => tracing::error!("retention error: {}", e),
        }
    }
}
