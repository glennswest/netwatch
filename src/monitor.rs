//! Service monitoring engine — schedules probes and records results.

use crate::config::Config;
use crate::db::Db;
use crate::models::*;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// Background monitoring loop.
pub async fn run(
    db: Arc<Db>,
    config: Arc<Config>,
    ws_tx: broadcast::Sender<String>,
) {
    // Wait for initial discovery
    tokio::time::sleep(Duration::from_secs(15)).await;

    let concurrency = config.monitoring.concurrency;
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));

    loop {
        let services = match db.list_services() {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("monitor: failed to list services: {}", e);
                tokio::time::sleep(Duration::from_secs(10)).await;
                continue;
            }
        };

        let mut handles = Vec::new();

        for svc in services {
            if !svc.enabled {
                continue;
            }

            // Skip services for non-infrastructure devices (monitor=false label)
            if let Ok(Some(device)) = db.get_device(&svc.device_id) {
                if device.labels.get("monitor").map(|v| v.as_str()) == Some("false") {
                    continue;
                }
            }

            // Check if it's time to probe this service
            if let Ok(Some(last)) = db.get_latest_probe(&svc.id) {
                if let Ok(ts) = chrono::DateTime::parse_from_rfc3339(&last.timestamp) {
                    let elapsed = chrono::Utc::now().signed_duration_since(ts);
                    if elapsed.num_seconds() < svc.interval_secs as i64 {
                        continue;
                    }
                }
            }

            let db = db.clone();
            let ws_tx = ws_tx.clone();
            let permit = semaphore.clone().acquire_owned().await;

            let handle = tokio::spawn(async move {
                let _permit = permit;
                let result = execute_probe(&svc).await;

                let probe = ProbeResult {
                    id: uuid::Uuid::new_v4().to_string(),
                    service_id: svc.id.clone(),
                    status: result.status,
                    latency_us: result.latency_us,
                    error: result.error.clone(),
                    timestamp: chrono::Utc::now().to_rfc3339(),
                };

                if let Err(e) = db.insert_probe_result(probe.clone()) {
                    tracing::error!("monitor: failed to store probe result: {}", e);
                }

                // Store latency as metric
                if let Some(latency) = result.latency_us {
                    let metric = Metric {
                        id: uuid::Uuid::new_v4().to_string(),
                        device_id: svc.device_id.clone(),
                        metric_name: format!("{}_latency_us", svc.probe_type),
                        value: latency as f64,
                        timestamp: chrono::Utc::now().to_rfc3339(),
                    };
                    let _ = db.insert_metric(metric);
                }

                // Only broadcast status changes for alerts (Down/Degraded), not routine probes
                if result.status == ProbeStatus::Down || result.status == ProbeStatus::Degraded {
                    let _ = ws_tx.send(format!(
                        r#"{{"event":"probe","service_id":"{}","device_id":"{}","status":"{}","latency_us":{}}}"#,
                        svc.id,
                        svc.device_id,
                        result.status,
                        result.latency_us.unwrap_or(0)
                    ));
                }
            });

            handles.push(handle);
        }

        // Wait for all probes in this cycle
        for handle in handles {
            let _ = handle.await;
        }

        // Sleep between scan cycles
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

struct ProbeOutput {
    status: ProbeStatus,
    latency_us: Option<i64>,
    error: Option<String>,
}

async fn execute_probe(svc: &Service) -> ProbeOutput {
    let host = svc
        .host
        .as_deref()
        .unwrap_or("127.0.0.1");
    let timeout = svc.timeout_ms as u64;

    match svc.probe_type {
        ProbeType::Icmp => probe_icmp(host, timeout).await,
        ProbeType::Tcp => probe_tcp(host, svc.port.unwrap_or(80), timeout).await,
        ProbeType::Http => {
            let default_url = format!("http://{}:{}", host, svc.port.unwrap_or(80));
            let url = svc.url.as_deref().unwrap_or(&default_url);
            probe_http(url, timeout).await
        }
        ProbeType::Https => {
            let default_url = format!("https://{}:{}", host, svc.port.unwrap_or(443));
            let url = svc.url.as_deref().unwrap_or(&default_url);
            probe_http(url, timeout).await
        }
        ProbeType::Dns => probe_dns(host, timeout).await,
        ProbeType::Snmp => probe_snmp(host, timeout).await,
    }
}

async fn probe_icmp(host: &str, timeout_ms: u64) -> ProbeOutput {
    let host = host.to_string();
    let start = Instant::now();

    let reachable = tokio::task::spawn_blocking(move || {
        crate::discovery::ping_host_sync(&host, timeout_ms)
    })
    .await
    .unwrap_or(false);

    let elapsed = start.elapsed();

    if reachable {
        ProbeOutput {
            status: ProbeStatus::Up,
            latency_us: Some(elapsed.as_micros() as i64),
            error: None,
        }
    } else {
        ProbeOutput {
            status: ProbeStatus::Down,
            latency_us: None,
            error: Some("timeout".into()),
        }
    }
}

async fn probe_tcp(host: &str, port: u16, timeout_ms: u64) -> ProbeOutput {
    let addr = format!("{}:{}", host, port);
    let start = Instant::now();

    match tokio::time::timeout(
        Duration::from_millis(timeout_ms),
        tokio::net::TcpStream::connect(&addr),
    )
    .await
    {
        Ok(Ok(_)) => ProbeOutput {
            status: ProbeStatus::Up,
            latency_us: Some(start.elapsed().as_micros() as i64),
            error: None,
        },
        Ok(Err(e)) => ProbeOutput {
            status: ProbeStatus::Down,
            latency_us: None,
            error: Some(e.to_string()),
        },
        Err(_) => ProbeOutput {
            status: ProbeStatus::Down,
            latency_us: None,
            error: Some("timeout".into()),
        },
    }
}

async fn probe_http(url: &str, timeout_ms: u64) -> ProbeOutput {
    let start = Instant::now();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .danger_accept_invalid_certs(true)
        .build();

    let client = match client {
        Ok(c) => c,
        Err(e) => {
            return ProbeOutput {
                status: ProbeStatus::Down,
                latency_us: None,
                error: Some(e.to_string()),
            }
        }
    };

    match client.get(url).send().await {
        Ok(resp) => {
            let latency = start.elapsed().as_micros() as i64;
            let status_code = resp.status().as_u16();
            if status_code < 500 {
                ProbeOutput {
                    status: ProbeStatus::Up,
                    latency_us: Some(latency),
                    error: None,
                }
            } else {
                ProbeOutput {
                    status: ProbeStatus::Degraded,
                    latency_us: Some(latency),
                    error: Some(format!("HTTP {}", status_code)),
                }
            }
        }
        Err(e) => ProbeOutput {
            status: ProbeStatus::Down,
            latency_us: None,
            error: Some(e.to_string()),
        },
    }
}

async fn probe_dns(host: &str, timeout_ms: u64) -> ProbeOutput {
    let host = host.to_string();
    let start = Instant::now();

    let result = tokio::task::spawn_blocking(move || {
        use std::net::ToSocketAddrs;
        let addr = format!("{}:53", host);
        std::net::TcpStream::connect_timeout(
            &addr.to_socket_addrs().ok()?.next()?,
            Duration::from_millis(timeout_ms),
        )
        .ok()?;
        Some(())
    })
    .await;

    match result {
        Ok(Some(())) => ProbeOutput {
            status: ProbeStatus::Up,
            latency_us: Some(start.elapsed().as_micros() as i64),
            error: None,
        },
        _ => ProbeOutput {
            status: ProbeStatus::Down,
            latency_us: None,
            error: Some("DNS unreachable".into()),
        },
    }
}

async fn probe_snmp(host: &str, timeout_ms: u64) -> ProbeOutput {
    let start = Instant::now();
    match crate::snmp::async_snmp_get(
        host.to_string(),
        "public".to_string(),
        crate::snmp::OID_SYS_UPTIME.to_string(),
        timeout_ms,
    )
    .await
    {
        Ok(_) => ProbeOutput {
            status: ProbeStatus::Up,
            latency_us: Some(start.elapsed().as_micros() as i64),
            error: None,
        },
        Err(e) => ProbeOutput {
            status: ProbeStatus::Down,
            latency_us: None,
            error: Some(e.to_string()),
        },
    }
}
