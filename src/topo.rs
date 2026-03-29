//! Topology layout — hierarchical placement for the network map.

use crate::models::MapPosition;
use std::collections::BTreeMap;

const PADDING: f64 = 60.0;
const TIER_Y: [f64; 5] = [80.0, 250.0, 420.0, 590.0, 760.0];
const NODE_SPACING: f64 = 130.0;
const GROUP_GAP: f64 = 80.0;

/// Minimal device info needed for hierarchical placement.
pub struct DeviceInfo {
    pub id: String,
    pub device_type: String,
    pub ip: String,
}

fn tier_for_type(device_type: &str) -> usize {
    match device_type {
        "internet" => 0,
        "router" => 1,
        "switch" => 2,
        "ap" | "firewall" | "server" => 3,
        _ => 4,
    }
}

fn subnet_key(ip: &str) -> String {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() >= 3 {
        format!("{}.{}.{}", parts[0], parts[1], parts[2])
    } else {
        ip.to_string()
    }
}

/// Hierarchical layout: devices organized by network role tier and /24 subnet.
///
/// Tier 0 (y=80):  Internet
/// Tier 1 (y=250): Routers
/// Tier 2 (y=420): Switches
/// Tier 3 (y=590): APs, Firewalls, Servers
/// Tier 4 (y=760): Cameras, Phones, Printers, Other
///
/// Within each tier, devices are grouped by /24 subnet with 80px gaps between groups
/// and 130px spacing between nodes.
pub fn hierarchical_place(devices: &[DeviceInfo]) -> Vec<MapPosition> {
    if devices.is_empty() {
        return vec![];
    }

    // Assign devices to tiers
    let mut tiers: Vec<Vec<&DeviceInfo>> = vec![vec![]; 5];
    for dev in devices {
        let tier = tier_for_type(&dev.device_type);
        tiers[tier].push(dev);
    }

    let mut results = Vec::new();

    for (tier_idx, tier_devices) in tiers.iter().enumerate() {
        if tier_devices.is_empty() {
            continue;
        }

        let y = TIER_Y[tier_idx];

        // Group by /24 subnet, sorted by subnet key
        let mut subnet_groups: BTreeMap<String, Vec<&DeviceInfo>> = BTreeMap::new();
        for dev in tier_devices {
            subnet_groups.entry(subnet_key(&dev.ip)).or_default().push(dev);
        }

        // Flatten into ordered list tracking group boundaries
        let mut ordered: Vec<(&DeviceInfo, bool)> = Vec::new(); // (device, starts_new_group)
        for (i, (_key, group)) in subnet_groups.iter().enumerate() {
            for (j, dev) in group.iter().enumerate() {
                ordered.push((dev, j == 0 && i > 0));
            }
        }

        if ordered.is_empty() {
            continue;
        }

        // Calculate total width
        let mut total_width = 0.0_f64;
        for (i, &(_, new_group)) in ordered.iter().enumerate() {
            if i > 0 {
                total_width += NODE_SPACING;
                if new_group {
                    total_width += GROUP_GAP;
                }
            }
        }

        // Place centered at x=0
        let mut x = -total_width / 2.0;
        for (i, &(dev, new_group)) in ordered.iter().enumerate() {
            if i > 0 {
                x += NODE_SPACING;
                if new_group {
                    x += GROUP_GAP;
                }
            }
            results.push(MapPosition {
                device_id: dev.id.clone(),
                x: (x * 10.0).round() / 10.0,
                y,
            });
        }
    }

    // Shift all positions so minimum x = PADDING
    if let Some(min_x) = results.iter().map(|p| p.x).reduce(f64::min) {
        let shift = PADDING - min_x;
        for pos in &mut results {
            pos.x += shift;
        }
    }

    results
}

/// Place a single new device near an existing device (for when new devices are discovered).
pub fn place_near(existing: &MapPosition) -> (f64, f64) {
    let offset = 80.0;
    let angle = rand::random::<f64>() * 2.0 * std::f64::consts::PI;
    let x = existing.x + offset * angle.cos();
    let y = existing.y + offset * angle.sin();
    (x, y)
}

/// Place a device at a random position.
pub fn place_random() -> (f64, f64) {
    let x = PADDING + rand::random::<f64>() * (1200.0 - 2.0 * PADDING);
    let y = PADDING + rand::random::<f64>() * (800.0 - 2.0 * PADDING);
    (x, y)
}
