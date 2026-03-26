use crate::models::*;
use anyhow::Result;
use native_db::*;
use std::path::Path;
use std::sync::Arc;

/// Wrapper around native_db providing all storage operations.
pub struct Db {
    inner: Database<'static>,
}

// Safety: we ensure Models lives as long as the database via Box::leak
unsafe impl Send for Db {}
unsafe impl Sync for Db {}

fn define_models() -> &'static Models {
    let mut models = Models::new();
    models.define::<Device>().expect("define Device");
    models.define::<NetInterface>().expect("define NetInterface");
    models.define::<Link>().expect("define Link");
    models.define::<Service>().expect("define Service");
    models.define::<ProbeResult>().expect("define ProbeResult");
    models.define::<Alert>().expect("define Alert");
    models.define::<Subnet>().expect("define Subnet");
    models.define::<MapPosition>().expect("define MapPosition");
    models.define::<Metric>().expect("define Metric");
    models.define::<AlertRule>().expect("define AlertRule");
    Box::leak(Box::new(models))
}

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let models = define_models();
        let inner = Builder::new().create(models, path)?;
        Ok(Self { inner })
    }

    // ── Devices ──

    pub fn list_devices(&self) -> Result<Vec<Device>> {
        let r = self.inner.r_transaction()?;
        let items: Vec<Device> = r.scan().primary()?.all()?.try_collect()?;
        Ok(items)
    }

    pub fn get_device(&self, id: &str) -> Result<Option<Device>> {
        let r = self.inner.r_transaction()?;
        let item = r.get().primary(id.to_string())?;
        Ok(item)
    }

    pub fn get_device_by_ip(&self, ip: &str) -> Result<Option<Device>> {
        let r = self.inner.r_transaction()?;
        let item = r.get().secondary(DeviceKey::ip, ip.to_string())?;
        Ok(item)
    }

    pub fn insert_device(&self, device: Device) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        rw.insert(device)?;
        rw.commit()?;
        Ok(())
    }

    pub fn update_device(&self, old: Device, new: Device) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        rw.update(old, new)?;
        rw.commit()?;
        Ok(())
    }

    pub fn delete_device(&self, id: &str) -> Result<()> {
        if let Some(device) = self.get_device(id)? {
            let rw = self.inner.rw_transaction()?;
            rw.remove(device)?;
            rw.commit()?;
        }
        Ok(())
    }

    // ── Interfaces ──

    pub fn list_interfaces_for_device(&self, device_id: &str) -> Result<Vec<NetInterface>> {
        let r = self.inner.r_transaction()?;
        let all: Vec<NetInterface> = r.scan().primary()?.all()?.try_collect()?;
        Ok(all.into_iter().filter(|i| i.device_id == device_id).collect())
    }

    pub fn upsert_interface(&self, iface: NetInterface) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        if let Ok(Some(old)) = {
            let r2 = self.inner.r_transaction()?;
            let v: Result<Option<NetInterface>, _> = r2.get().primary(iface.id.clone());
            v
        } {
            rw.update(old, iface)?;
        } else {
            rw.insert(iface)?;
        }
        rw.commit()?;
        Ok(())
    }

    pub fn get_interface(&self, id: &str) -> Result<Option<NetInterface>> {
        let r = self.inner.r_transaction()?;
        Ok(r.get().primary(id.to_string())?)
    }

    pub fn delete_interfaces_for_device(&self, device_id: &str) -> Result<()> {
        let ifaces = self.list_interfaces_for_device(device_id)?;
        if !ifaces.is_empty() {
            let rw = self.inner.rw_transaction()?;
            for iface in ifaces {
                rw.remove(iface)?;
            }
            rw.commit()?;
        }
        Ok(())
    }

    // ── Links ──

    pub fn list_links(&self) -> Result<Vec<Link>> {
        let r = self.inner.r_transaction()?;
        let items: Vec<Link> = r.scan().primary()?.all()?.try_collect()?;
        Ok(items)
    }

    pub fn insert_link(&self, link: Link) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        rw.insert(link)?;
        rw.commit()?;
        Ok(())
    }

    pub fn delete_link(&self, id: &str) -> Result<()> {
        if let Some(link) = {
            let r = self.inner.r_transaction()?;
            let v: Option<Link> = r.get().primary(id.to_string())?;
            v
        } {
            let rw = self.inner.rw_transaction()?;
            rw.remove(link)?;
            rw.commit()?;
        }
        Ok(())
    }

    pub fn delete_links_for_device(&self, device_id: &str) -> Result<()> {
        let links = self.list_links()?;
        let to_remove: Vec<Link> = links
            .into_iter()
            .filter(|l| l.source_device_id == device_id || l.target_device_id == device_id)
            .collect();
        if !to_remove.is_empty() {
            let rw = self.inner.rw_transaction()?;
            for link in to_remove {
                rw.remove(link)?;
            }
            rw.commit()?;
        }
        Ok(())
    }

    // ── Services ──

    pub fn list_services(&self) -> Result<Vec<Service>> {
        let r = self.inner.r_transaction()?;
        let items: Vec<Service> = r.scan().primary()?.all()?.try_collect()?;
        Ok(items)
    }

    pub fn list_services_for_device(&self, device_id: &str) -> Result<Vec<Service>> {
        let all = self.list_services()?;
        Ok(all.into_iter().filter(|s| s.device_id == device_id).collect())
    }

    pub fn get_service(&self, id: &str) -> Result<Option<Service>> {
        let r = self.inner.r_transaction()?;
        Ok(r.get().primary(id.to_string())?)
    }

    pub fn insert_service(&self, service: Service) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        rw.insert(service)?;
        rw.commit()?;
        Ok(())
    }

    pub fn delete_service(&self, id: &str) -> Result<()> {
        if let Some(svc) = self.get_service(id)? {
            let rw = self.inner.rw_transaction()?;
            rw.remove(svc)?;
            rw.commit()?;
        }
        Ok(())
    }

    pub fn delete_services_for_device(&self, device_id: &str) -> Result<()> {
        let svcs = self.list_services_for_device(device_id)?;
        if !svcs.is_empty() {
            let rw = self.inner.rw_transaction()?;
            for svc in svcs {
                rw.remove(svc)?;
            }
            rw.commit()?;
        }
        Ok(())
    }

    // ── Probe Results ──

    pub fn insert_probe_result(&self, result: ProbeResult) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        rw.insert(result)?;
        rw.commit()?;
        Ok(())
    }

    pub fn list_probe_results(&self, service_id: &str, limit: usize) -> Result<Vec<ProbeResult>> {
        let r = self.inner.r_transaction()?;
        let all: Vec<ProbeResult> = r.scan().primary()?.all()?.try_collect()?;
        let mut filtered: Vec<ProbeResult> = all
            .into_iter()
            .filter(|p| p.service_id == service_id)
            .collect();
        filtered.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        filtered.truncate(limit);
        Ok(filtered)
    }

    pub fn get_latest_probe(&self, service_id: &str) -> Result<Option<ProbeResult>> {
        let results = self.list_probe_results(service_id, 1)?;
        Ok(results.into_iter().next())
    }

    pub fn get_latest_probe_for_device(&self, device_id: &str) -> Result<Option<ProbeResult>> {
        let services = self.list_services_for_device(device_id)?;
        // Find the ICMP service or first service
        let icmp_svc = services.iter().find(|s| s.probe_type == ProbeType::Icmp);
        let svc = icmp_svc.or(services.first());
        if let Some(svc) = svc {
            self.get_latest_probe(&svc.id)
        } else {
            Ok(None)
        }
    }

    // ── Alerts ──

    pub fn insert_alert(&self, alert: Alert) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        rw.insert(alert)?;
        rw.commit()?;
        Ok(())
    }

    pub fn list_alerts(&self, limit: usize) -> Result<Vec<Alert>> {
        let r = self.inner.r_transaction()?;
        let all: Vec<Alert> = r.scan().primary()?.all()?.try_collect()?;
        let mut sorted = all;
        sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        sorted.truncate(limit);
        Ok(sorted)
    }

    pub fn list_active_alerts(&self) -> Result<Vec<Alert>> {
        let r = self.inner.r_transaction()?;
        let all: Vec<Alert> = r.scan().primary()?.all()?.try_collect()?;
        let mut active: Vec<Alert> = all.into_iter().filter(|a| !a.acknowledged).collect();
        active.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(active)
    }

    pub fn acknowledge_alert(&self, id: &str) -> Result<()> {
        let r = self.inner.r_transaction()?;
        if let Some(old) = r.get().primary::<Alert>(id.to_string())? {
            drop(r);
            let mut new = old.clone();
            new.acknowledged = true;
            let rw = self.inner.rw_transaction()?;
            rw.update(old, new)?;
            rw.commit()?;
        }
        Ok(())
    }

    pub fn count_active_alerts(&self) -> Result<usize> {
        Ok(self.list_active_alerts()?.len())
    }

    // ── Subnets ──

    pub fn list_subnets(&self) -> Result<Vec<Subnet>> {
        let r = self.inner.r_transaction()?;
        let items: Vec<Subnet> = r.scan().primary()?.all()?.try_collect()?;
        Ok(items)
    }

    pub fn get_subnet(&self, id: &str) -> Result<Option<Subnet>> {
        let r = self.inner.r_transaction()?;
        Ok(r.get().primary(id.to_string())?)
    }

    pub fn get_subnet_by_cidr(&self, cidr: &str) -> Result<Option<Subnet>> {
        let r = self.inner.r_transaction()?;
        Ok(r.get().secondary(SubnetKey::cidr, cidr.to_string())?)
    }

    pub fn insert_subnet(&self, subnet: Subnet) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        rw.insert(subnet)?;
        rw.commit()?;
        Ok(())
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
        let r = self.inner.r_transaction()?;
        if let Some(old) = r.get().primary::<Subnet>(id.to_string())? {
            drop(r);
            let mut new = old.clone();
            new.last_scan = Some(time.to_string());
            let rw = self.inner.rw_transaction()?;
            rw.update(old, new)?;
            rw.commit()?;
        }
        Ok(())
    }

    pub fn delete_subnet(&self, id: &str) -> Result<()> {
        if let Some(subnet) = self.get_subnet(id)? {
            let rw = self.inner.rw_transaction()?;
            rw.remove(subnet)?;
            rw.commit()?;
        }
        Ok(())
    }

    // ── Map Positions ──

    pub fn list_positions(&self) -> Result<Vec<MapPosition>> {
        let r = self.inner.r_transaction()?;
        let items: Vec<MapPosition> = r.scan().primary()?.all()?.try_collect()?;
        Ok(items)
    }

    pub fn get_position(&self, device_id: &str) -> Result<Option<MapPosition>> {
        let r = self.inner.r_transaction()?;
        Ok(r.get().primary(device_id.to_string())?)
    }

    pub fn upsert_position(&self, pos: MapPosition) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        if let Ok(Some(old)) = {
            let r = self.inner.r_transaction()?;
            r.get().primary::<MapPosition>(pos.device_id.clone())
        } {
            rw.update(old, pos)?;
        } else {
            rw.insert(pos)?;
        }
        rw.commit()?;
        Ok(())
    }

    pub fn delete_position(&self, device_id: &str) -> Result<()> {
        if let Some(pos) = self.get_position(device_id)? {
            let rw = self.inner.rw_transaction()?;
            rw.remove(pos)?;
            rw.commit()?;
        }
        Ok(())
    }

    // ── Metrics ──

    pub fn insert_metric(&self, metric: Metric) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        rw.insert(metric)?;
        rw.commit()?;
        Ok(())
    }

    pub fn list_metrics(
        &self,
        device_id: &str,
        metric_name: &str,
        limit: usize,
    ) -> Result<Vec<Metric>> {
        let r = self.inner.r_transaction()?;
        let all: Vec<Metric> = r.scan().primary()?.all()?.try_collect()?;
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
        let r = self.inner.r_transaction()?;
        let items: Vec<AlertRule> = r.scan().primary()?.all()?.try_collect()?;
        Ok(items)
    }

    pub fn insert_alert_rule(&self, rule: AlertRule) -> Result<()> {
        let rw = self.inner.rw_transaction()?;
        rw.insert(rule)?;
        rw.commit()?;
        Ok(())
    }

    pub fn delete_alert_rule(&self, id: &str) -> Result<()> {
        let r = self.inner.r_transaction()?;
        if let Some(rule) = r.get().primary::<AlertRule>(id.to_string())? {
            drop(r);
            let rw = self.inner.rw_transaction()?;
            rw.remove(rule)?;
            rw.commit()?;
        }
        Ok(())
    }

    // ── Device status (composite query) ──

    pub fn get_device_statuses(&self) -> Result<Vec<DeviceStatus>> {
        let devices = self.list_devices()?;
        let mut statuses = Vec::with_capacity(devices.len());

        for device in devices {
            let services = self.list_services_for_device(&device.id)?;
            let mut up = 0usize;
            let mut down = 0usize;
            let mut latency = None;

            for svc in &services {
                if let Some(probe) = self.get_latest_probe(&svc.id)? {
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

            let status = if services.is_empty() {
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

            let position = self.get_position(&device.id)?;

            statuses.push(DeviceStatus {
                device,
                status,
                services_up: up,
                services_down: down,
                services_total: services.len(),
                latency_us: latency,
                position,
            });
        }

        Ok(statuses)
    }

    // ── Bulk delete (for cascading device delete) ──

    pub fn delete_device_cascade(&self, id: &str) -> Result<()> {
        self.delete_services_for_device(id)?;
        self.delete_interfaces_for_device(id)?;
        self.delete_links_for_device(id)?;
        self.delete_position(id)?;
        // Delete probe results for this device's services
        // (already handled since services are deleted)
        self.delete_device(id)?;
        Ok(())
    }

    // ── Retention cleanup ──

    pub fn cleanup_before(&self, before: &str) -> Result<(usize, usize, usize)> {
        let mut probes_removed = 0usize;
        let mut metrics_removed = 0usize;
        let mut alerts_removed = 0usize;

        // Probe results
        {
            let r = self.inner.r_transaction()?;
            let all: Vec<ProbeResult> = r.scan().primary()?.all()?.try_collect()?;
            let old: Vec<ProbeResult> = all
                .into_iter()
                .filter(|p| p.timestamp.as_str() < before)
                .collect();
            drop(r);
            if !old.is_empty() {
                let rw = self.inner.rw_transaction()?;
                for item in old {
                    rw.remove(item)?;
                    probes_removed += 1;
                }
                rw.commit()?;
            }
        }

        // Metrics
        {
            let r = self.inner.r_transaction()?;
            let all: Vec<Metric> = r.scan().primary()?.all()?.try_collect()?;
            let old: Vec<Metric> = all
                .into_iter()
                .filter(|m| m.timestamp.as_str() < before)
                .collect();
            drop(r);
            if !old.is_empty() {
                let rw = self.inner.rw_transaction()?;
                for item in old {
                    rw.remove(item)?;
                    metrics_removed += 1;
                }
                rw.commit()?;
            }
        }

        // Old acknowledged alerts
        {
            let r = self.inner.r_transaction()?;
            let all: Vec<Alert> = r.scan().primary()?.all()?.try_collect()?;
            let old: Vec<Alert> = all
                .into_iter()
                .filter(|a| a.acknowledged && a.created_at.as_str() < before)
                .collect();
            drop(r);
            if !old.is_empty() {
                let rw = self.inner.rw_transaction()?;
                for item in old {
                    rw.remove(item)?;
                    alerts_removed += 1;
                }
                rw.commit()?;
            }
        }

        Ok((probes_removed, metrics_removed, alerts_removed))
    }
}

/// Background retention loop — cleans old probe results, metrics, alerts.
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
                    tracing::info!(
                        "retention cleanup: {} probes, {} metrics, {} alerts removed",
                        p, m, a
                    );
                }
            }
            Err(e) => tracing::error!("retention cleanup error: {}", e),
        }
    }
}
