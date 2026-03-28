//! Alert engine — evaluates probe results, generates alerts, sends notifications.

use crate::config::Config;
use crate::db::Db;
use crate::models::*;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;

/// Background alert evaluation loop.
pub async fn run(
    db: Arc<Db>,
    config: Arc<Config>,
    ws_tx: broadcast::Sender<String>,
) {
    // Track consecutive failures per service
    let mut failure_counts: HashMap<String, u32> = HashMap::new();
    // Track cooldowns: service_id -> last alert time
    let mut cooldowns: HashMap<String, chrono::DateTime<chrono::Utc>> = HashMap::new();

    let consecutive_threshold = config.alerting.consecutive_failures;
    let cooldown_secs = config.alerting.cooldown_secs as i64;

    let mut interval = tokio::time::interval(Duration::from_secs(15));

    loop {
        interval.tick().await;

        let services = match db.list_services() {
            Ok(s) => s,
            Err(_) => continue,
        };

        for svc in &services {
            if !svc.enabled {
                continue;
            }

            // Skip non-infrastructure devices
            if let Ok(Some(device)) = db.get_device(&svc.device_id) {
                if device.labels.get("monitor").map(|v| v.as_str()) == Some("false") {
                    continue;
                }
            }

            let latest = match db.get_latest_probe(&svc.id) {
                Ok(Some(p)) => p,
                _ => continue,
            };

            match latest.status {
                ProbeStatus::Down => {
                    let count = failure_counts
                        .entry(svc.id.clone())
                        .or_insert(0);
                    *count += 1;

                    if *count >= consecutive_threshold {
                        // Check cooldown
                        let now = chrono::Utc::now();
                        if let Some(last) = cooldowns.get(&svc.id) {
                            if now.signed_duration_since(*last).num_seconds() < cooldown_secs {
                                continue;
                            }
                        }

                        // Get device info for alert message
                        let device_name = db
                            .get_device(&svc.device_id)
                            .ok()
                            .flatten()
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| svc.device_id.clone());

                        let message = format!(
                            "{} on {} is DOWN (failed {} consecutive probes){}",
                            svc.name,
                            device_name,
                            count,
                            latest
                                .error
                                .as_deref()
                                .map(|e| format!(" — {}", e))
                                .unwrap_or_default()
                        );

                        let alert = Alert {
                            id: uuid::Uuid::new_v4().to_string(),
                            device_id: svc.device_id.clone(),
                            service_id: Some(svc.id.clone()),
                            severity: Severity::Critical,
                            message: message.clone(),
                            acknowledged: false,
                            created_at: now.to_rfc3339(),
                        };

                        if let Err(e) = db.insert_alert(alert.clone()) {
                            tracing::error!("alert: failed to store alert: {}", e);
                            continue;
                        }

                        cooldowns.insert(svc.id.clone(), now);
                        tracing::warn!("ALERT: {}", message);

                        // Send notifications
                        send_notifications(&config, &message).await;

                        // Broadcast via WebSocket
                        let _ = ws_tx.send(format!(
                            r#"{{"event":"alert","severity":"critical","message":"{}","device_id":"{}"}}"#,
                            message.replace('"', "\\\""),
                            svc.device_id
                        ));
                    }
                }
                ProbeStatus::Up => {
                    // Check if recovering from failure
                    if let Some(count) = failure_counts.remove(&svc.id) {
                        if count >= consecutive_threshold {
                            let device_name = db
                                .get_device(&svc.device_id)
                                .ok()
                                .flatten()
                                .map(|d| d.name.clone())
                                .unwrap_or_else(|| svc.device_id.clone());

                            let message = format!(
                                "{} on {} is UP (recovered after {} failures)",
                                svc.name, device_name, count
                            );

                            let alert = Alert {
                                id: uuid::Uuid::new_v4().to_string(),
                                device_id: svc.device_id.clone(),
                                service_id: Some(svc.id.clone()),
                                severity: Severity::Info,
                                message: message.clone(),
                                acknowledged: false,
                                created_at: chrono::Utc::now().to_rfc3339(),
                            };
                            let _ = db.insert_alert(alert);

                            tracing::info!("RECOVERY: {}", message);
                            send_notifications(&config, &message).await;

                            let _ = ws_tx.send(format!(
                                r#"{{"event":"alert","severity":"info","message":"{}","device_id":"{}"}}"#,
                                message.replace('"', "\\\""),
                                svc.device_id
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
    }
}

async fn send_notifications(config: &Config, message: &str) {
    // Email
    if let Some(ref email_cfg) = config.alerting.email {
        if let Err(e) = send_email(email_cfg, message).await {
            tracing::error!("alert email failed: {}", e);
        }
    }

    // Webhook
    if let Some(ref webhook_cfg) = config.alerting.webhook {
        if let Err(e) = send_webhook(webhook_cfg, message).await {
            tracing::error!("alert webhook failed: {}", e);
        }
    }
}

async fn send_email(cfg: &crate::config::EmailConfig, message: &str) -> Result<()> {
    use lettre::message::header::ContentType;
    use lettre::transport::smtp::authentication::Credentials;
    use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};

    let mut email_builder = Message::builder()
        .from(cfg.from.parse()?)
        .subject(format!("Netwatch Alert: {}", &message[..message.len().min(60)]));

    for to in &cfg.to {
        email_builder = email_builder.to(to.parse()?);
    }

    let email = email_builder
        .header(ContentType::TEXT_PLAIN)
        .body(message.to_string())?;

    let mut transport_builder = if cfg.tls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.smtp_host)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&cfg.smtp_host)
    };

    transport_builder = transport_builder.port(cfg.smtp_port);

    if let (Some(user), Some(pass)) = (&cfg.username, &cfg.password) {
        transport_builder =
            transport_builder.credentials(Credentials::new(user.clone(), pass.clone()));
    }

    let transport = transport_builder.build();
    transport.send(email).await?;

    tracing::info!("alert email sent to {:?}", cfg.to);
    Ok(())
}

async fn send_webhook(cfg: &crate::config::WebhookConfig, message: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let payload = serde_json::json!({
        "text": message,
        "source": "netwatch",
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });

    let mut req = client.post(&cfg.url).json(&payload);

    if let Some(ref headers) = cfg.headers {
        for (key, value) in headers {
            req = req.header(key, value);
        }
    }

    req.send().await?;
    tracing::info!("alert webhook sent to {}", cfg.url);
    Ok(())
}
