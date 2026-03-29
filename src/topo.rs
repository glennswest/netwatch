//! Topology layout — BFS tree placement driven by actual link data.
//!
//! Layout structure (flow-down from root):
//!   Internet (depth 0)
//!       |
//!   rose1 master hub (depth 1)
//!       |
//!   ┌────────┬────────┬────────┐
//!   rose1.gw rose1.g9 rose1.gt ...   ← virtual routers (depth 2)
//!      |        |
//!    sw1.gw   sw9.g9  [containers]   ← switches or leaf devices (depth 3)
//!      |        |
//!   devices   devices                ← end devices (depth 4+)
//!
//! Each device is positioned directly under its parent in the link tree,
//! minimizing wire lengths.

use crate::models::MapPosition;
use std::collections::{HashMap, HashSet, VecDeque};

const NODE_SPACING: f64 = 100.0;
const LAYER_SPACING: f64 = 150.0;
const Y_START: f64 = 80.0;
const PADDING: f64 = 60.0;

/// Minimal device info needed for hierarchical placement.
pub struct DeviceInfo {
    pub id: String,
    pub device_type: String,
    pub ip: String,
}

/// Link between two devices (used for tree layout).
pub struct LinkInfo {
    pub source_id: String,
    pub target_id: String,
}

/// BFS tree layout: builds a spanning tree from links and positions each
/// device centered over its children.
pub fn hierarchical_place(devices: &[DeviceInfo], links: &[LinkInfo]) -> Vec<MapPosition> {
    if devices.is_empty() {
        return vec![];
    }

    let dev_ids: HashSet<&str> = devices.iter().map(|d| d.id.as_str()).collect();
    let dev_type: HashMap<&str, &str> = devices
        .iter()
        .map(|d| (d.id.as_str(), d.device_type.as_str()))
        .collect();

    // ── Build bidirectional adjacency list (only for known devices) ──
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for link in links {
        let s = link.source_id.as_str();
        let t = link.target_id.as_str();
        if dev_ids.contains(s) && dev_ids.contains(t) {
            adj.entry(s).or_default().push(t);
            adj.entry(t).or_default().push(s);
        }
    }

    // ── Find root ──
    // Prefer "internet" device, then device with most connections
    let root = devices
        .iter()
        .find(|d| d.device_type == "internet")
        .or_else(|| {
            devices.iter().max_by_key(|d| {
                adj.get(d.id.as_str()).map(|v| v.len()).unwrap_or(0)
            })
        })
        .map(|d| d.id.as_str())
        .unwrap();

    // ── BFS to assign depth and parent ──
    let mut depth: HashMap<&str, usize> = HashMap::new();
    let mut parent: HashMap<&str, &str> = HashMap::new();
    let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut queue: VecDeque<&str> = VecDeque::new();

    depth.insert(root, 0);
    queue.push_back(root);

    while let Some(node) = queue.pop_front() {
        let d = depth[node];
        if let Some(neighbors) = adj.get(node) {
            // Sort neighbors for deterministic layout: routers first, then
            // switches, then alphabetical by type then id
            let mut sorted_neighbors: Vec<&str> = neighbors.clone();
            sorted_neighbors.sort_by(|a, b| {
                let ta = dev_type.get(a).copied().unwrap_or("zzz");
                let tb = dev_type.get(b).copied().unwrap_or("zzz");
                let type_order = |t: &str| -> u8 {
                    match t {
                        "router" => 0,
                        "switch" => 1,
                        "ap" => 2,
                        _ => 3,
                    }
                };
                type_order(ta)
                    .cmp(&type_order(tb))
                    .then_with(|| a.cmp(b))
            });

            for &neighbor in &sorted_neighbors {
                if !depth.contains_key(neighbor) {
                    depth.insert(neighbor, d + 1);
                    parent.insert(neighbor, node);
                    children.entry(node).or_default().push(neighbor);
                    queue.push_back(neighbor);
                }
            }
        }
    }

    // ── Collect orphans (devices not reached by BFS) ──
    let orphans: Vec<&str> = devices
        .iter()
        .filter(|d| !depth.contains_key(d.id.as_str()))
        .map(|d| d.id.as_str())
        .collect();

    // ── Compute subtree widths bottom-up ──
    // Width = sum of children widths, minimum 1 slot
    let max_depth = depth.values().copied().max().unwrap_or(0);
    let mut width: HashMap<&str, f64> = HashMap::new();

    // Process from deepest to shallowest
    for d in (0..=max_depth).rev() {
        for (&node, &node_depth) in &depth {
            if node_depth != d {
                continue;
            }
            let child_width: f64 = children
                .get(node)
                .map(|kids| kids.iter().map(|k| width.get(k).copied().unwrap_or(1.0)).sum())
                .unwrap_or(0.0);
            width.insert(node, child_width.max(1.0));
        }
    }

    // ── Assign X positions top-down ──
    let mut x_pos: HashMap<&str, f64> = HashMap::new();
    let total_width = width.get(root).copied().unwrap_or(1.0);
    let root_x = total_width * NODE_SPACING / 2.0;
    x_pos.insert(root, root_x);

    for d in 0..=max_depth {
        for (&node, &node_depth) in &depth {
            if node_depth != d {
                continue;
            }
            if let Some(kids) = children.get(node) {
                let node_x = x_pos[node];
                let node_w = width[node];
                let mut cursor = node_x - (node_w * NODE_SPACING) / 2.0;
                for &kid in kids {
                    let kid_w = width.get(kid).copied().unwrap_or(1.0);
                    let kid_x = cursor + (kid_w * NODE_SPACING) / 2.0;
                    x_pos.insert(kid, kid_x);
                    cursor += kid_w * NODE_SPACING;
                }
            }
        }
    }

    // ── Build results ──
    let mut results: Vec<MapPosition> = Vec::with_capacity(devices.len());

    for (&node, &d) in &depth {
        let x = x_pos.get(node).copied().unwrap_or(0.0);
        let y = Y_START + d as f64 * LAYER_SPACING;
        results.push(MapPosition {
            device_id: node.to_string(),
            x: round1(x),
            y: round1(y),
        });
    }

    // ── Place orphans in a row at the bottom ──
    if !orphans.is_empty() {
        let orphan_y = Y_START + (max_depth as f64 + 2.0) * LAYER_SPACING;
        let orphan_total = orphans.len() as f64;
        let orphan_start = (total_width * NODE_SPACING / 2.0)
            - ((orphan_total - 1.0) * NODE_SPACING / 2.0);
        for (i, orphan) in orphans.iter().enumerate() {
            results.push(MapPosition {
                device_id: orphan.to_string(),
                x: round1(orphan_start + i as f64 * NODE_SPACING),
                y: round1(orphan_y),
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
