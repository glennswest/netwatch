//! Topology layout — columnar subnet placement for the network map.
//!
//! Layout structure:
//!   Internet (top center)
//!       |
//!   Routers (centered)
//!       |
//!   ┌────┬────┬────┬────┐
//!   sw9  sw10 sw11  ...    ← subnet switch headers
//!   │    │    │    │
//!   dev  dev  dev  dev     ← devices in grid rows (COLS_PER_ROW wide)
//!   dev  dev  dev  dev
//!   ...
//!
//! Each /24 subnet gets a vertical column with its switch on top
//! and devices stacked in rows below.

use crate::models::MapPosition;
use std::collections::BTreeMap;

const PADDING: f64 = 60.0;
const NODE_SPACING: f64 = 130.0;
const ROW_SPACING: f64 = 100.0;
const COL_GAP: f64 = 100.0;
const COLS_PER_ROW: usize = 4;

const INTERNET_Y: f64 = 80.0;
const ROUTER_Y: f64 = 230.0;
const SWITCH_Y: f64 = 400.0;
const DEVICES_START_Y: f64 = 550.0;

/// Minimal device info needed for hierarchical placement.
pub struct DeviceInfo {
    pub id: String,
    pub device_type: String,
    pub ip: String,
}

fn subnet_key(ip: &str) -> String {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() >= 3 {
        format!("{}.{}.{}", parts[0], parts[1], parts[2])
    } else {
        ip.to_string()
    }
}

/// Columnar layout: each /24 subnet gets a vertical column with its switch
/// on top and devices stacked in rows of [COLS_PER_ROW] below.
/// Internet and routers are centered above all columns.
pub fn hierarchical_place(devices: &[DeviceInfo]) -> Vec<MapPosition> {
    if devices.is_empty() {
        return vec![];
    }

    let mut results = Vec::new();

    // ── Categorize devices ──
    let mut internet_devs: Vec<&DeviceInfo> = Vec::new();
    let mut router_devs: Vec<&DeviceInfo> = Vec::new();
    let mut subnet_map: BTreeMap<String, Vec<&DeviceInfo>> = BTreeMap::new();

    for dev in devices {
        match dev.device_type.as_str() {
            "internet" => internet_devs.push(dev),
            "router" => router_devs.push(dev),
            _ => {
                subnet_map.entry(subnet_key(&dev.ip)).or_default().push(dev);
            }
        }
    }

    // ── Layout subnet columns ──
    // Each column: switch header on top, devices in grid rows below.
    // Columns are sized to their content width and spaced with COL_GAP.

    let mut col_x = 0.0_f64;

    for (_subnet_key, group) in &subnet_map {
        // Separate switches/APs (column header row) from other devices
        let mut headers: Vec<&DeviceInfo> = Vec::new();
        let mut regular: Vec<&DeviceInfo> = Vec::new();
        for dev in group {
            if dev.device_type == "switch" || dev.device_type == "ap" {
                headers.push(dev);
            } else {
                regular.push(dev);
            }
        }

        // Column width based on whichever row is wider: headers or device grid
        let grid_cols = regular.len().min(COLS_PER_ROW);
        let header_width = if headers.len() > 1 {
            (headers.len() as f64 - 1.0) * NODE_SPACING
        } else {
            0.0
        };
        let grid_width = if grid_cols > 1 {
            (grid_cols as f64 - 1.0) * NODE_SPACING
        } else {
            0.0
        };
        let col_width = header_width.max(grid_width);
        let center_x = col_x + col_width / 2.0;

        // Place switches/APs centered in column at SWITCH_Y
        if !headers.is_empty() {
            let hw = (headers.len() as f64 - 1.0) * NODE_SPACING;
            let start_x = center_x - hw / 2.0;
            for (i, h) in headers.iter().enumerate() {
                results.push(MapPosition {
                    device_id: h.id.clone(),
                    x: round1(start_x + i as f64 * NODE_SPACING),
                    y: SWITCH_Y,
                });
            }
        }

        // Place devices in grid rows
        for (i, dev) in regular.iter().enumerate() {
            let row = i / COLS_PER_ROW;
            let col = i % COLS_PER_ROW;
            let x = col_x + col as f64 * NODE_SPACING;
            let y = DEVICES_START_Y + row as f64 * ROW_SPACING;
            results.push(MapPosition {
                device_id: dev.id.clone(),
                x: round1(x),
                y: round1(y),
            });
        }

        col_x += col_width + COL_GAP;
    }

    // ── Center Internet and routers over columns ──
    let total_width = if col_x > COL_GAP { col_x - COL_GAP } else { 0.0 };
    let columns_center = total_width / 2.0;

    // Internet
    if !internet_devs.is_empty() {
        let w = (internet_devs.len() as f64 - 1.0) * NODE_SPACING;
        let start_x = columns_center - w / 2.0;
        for (i, dev) in internet_devs.iter().enumerate() {
            results.push(MapPosition {
                device_id: dev.id.clone(),
                x: round1(start_x + i as f64 * NODE_SPACING),
                y: INTERNET_Y,
            });
        }
    }

    // Routers
    if !router_devs.is_empty() {
        let w = (router_devs.len() as f64 - 1.0) * NODE_SPACING;
        let start_x = columns_center - w / 2.0;
        for (i, dev) in router_devs.iter().enumerate() {
            results.push(MapPosition {
                device_id: dev.id.clone(),
                x: round1(start_x + i as f64 * NODE_SPACING),
                y: ROUTER_Y,
            });
        }
    }

    // ── Shift so minimum x = PADDING ──
    if let Some(min_x) = results.iter().map(|p| p.x).reduce(f64::min) {
        let shift = PADDING - min_x;
        for pos in &mut results {
            pos.x = round1(pos.x + shift);
        }
    }

    results
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
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
