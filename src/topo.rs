//! Topology layout — force-directed auto-placement for the network map.

use crate::models::MapPosition;

const REPULSION: f64 = 5000.0;
const ATTRACTION: f64 = 0.01;
const DAMPING: f64 = 0.9;
const MIN_DISTANCE: f64 = 50.0;
const CANVAS_W: f64 = 1200.0;
const CANVAS_H: f64 = 800.0;
const PADDING: f64 = 60.0;

/// Auto-layout devices using force-directed algorithm.
/// Takes current positions (if any) and links, returns updated positions.
pub fn auto_layout(
    device_ids: &[String],
    existing_positions: &[MapPosition],
    links: &[(String, String)], // (source_id, target_id)
    iterations: usize,
) -> Vec<MapPosition> {
    let n = device_ids.len();
    if n == 0 {
        return vec![];
    }

    // Initialize positions
    let mut xs: Vec<f64> = Vec::with_capacity(n);
    let mut ys: Vec<f64> = Vec::with_capacity(n);

    for (i, id) in device_ids.iter().enumerate() {
        if let Some(pos) = existing_positions.iter().find(|p| p.device_id == *id) {
            xs.push(pos.x);
            ys.push(pos.y);
        } else {
            // Place in a circle initially
            let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
            let r = (CANVAS_W.min(CANVAS_H) / 2.0) - PADDING;
            xs.push(CANVAS_W / 2.0 + r * angle.cos());
            ys.push(CANVAS_H / 2.0 + r * angle.sin());
        }
    }

    let mut vx = vec![0.0f64; n];
    let mut vy = vec![0.0f64; n];

    // Build link index map
    let id_to_idx: std::collections::HashMap<&str, usize> = device_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();

    for _ in 0..iterations {
        let mut fx = vec![0.0f64; n];
        let mut fy = vec![0.0f64; n];

        // Repulsion between all pairs
        for i in 0..n {
            for j in (i + 1)..n {
                let dx = xs[i] - xs[j];
                let dy = ys[i] - ys[j];
                let dist = (dx * dx + dy * dy).sqrt().max(MIN_DISTANCE);
                let force = REPULSION / (dist * dist);
                let fx_comp = force * dx / dist;
                let fy_comp = force * dy / dist;
                fx[i] += fx_comp;
                fy[i] += fy_comp;
                fx[j] -= fx_comp;
                fy[j] -= fy_comp;
            }
        }

        // Attraction along links
        for (src, tgt) in links {
            if let (Some(&si), Some(&ti)) = (id_to_idx.get(src.as_str()), id_to_idx.get(tgt.as_str())) {
                let dx = xs[si] - xs[ti];
                let dy = ys[si] - ys[ti];
                let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                let force = ATTRACTION * dist;
                let fx_comp = force * dx / dist;
                let fy_comp = force * dy / dist;
                fx[si] -= fx_comp;
                fy[si] -= fy_comp;
                fx[ti] += fx_comp;
                fy[ti] += fy_comp;
            }
        }

        // Center gravity (pull toward center)
        for i in 0..n {
            let dx = xs[i] - CANVAS_W / 2.0;
            let dy = ys[i] - CANVAS_H / 2.0;
            fx[i] -= dx * 0.001;
            fy[i] -= dy * 0.001;
        }

        // Apply forces
        for i in 0..n {
            vx[i] = (vx[i] + fx[i]) * DAMPING;
            vy[i] = (vy[i] + fy[i]) * DAMPING;

            // Clamp velocity
            let speed = (vx[i] * vx[i] + vy[i] * vy[i]).sqrt();
            if speed > 50.0 {
                vx[i] = vx[i] / speed * 50.0;
                vy[i] = vy[i] / speed * 50.0;
            }

            xs[i] += vx[i];
            ys[i] += vy[i];

            // Keep within bounds
            xs[i] = xs[i].clamp(PADDING, CANVAS_W - PADDING);
            ys[i] = ys[i].clamp(PADDING, CANVAS_H - PADDING);
        }
    }

    device_ids
        .iter()
        .enumerate()
        .map(|(i, id)| MapPosition {
            device_id: id.clone(),
            x: (xs[i] * 10.0).round() / 10.0,
            y: (ys[i] * 10.0).round() / 10.0,
        })
        .collect()
}

/// Place a single new device near an existing device (for when new devices are discovered).
pub fn place_near(existing: &MapPosition) -> (f64, f64) {
    let offset = 80.0;
    let angle = rand::random::<f64>() * 2.0 * std::f64::consts::PI;
    let x = (existing.x + offset * angle.cos()).clamp(PADDING, CANVAS_W - PADDING);
    let y = (existing.y + offset * angle.sin()).clamp(PADDING, CANVAS_H - PADDING);
    (x, y)
}

/// Place a device at a random position.
pub fn place_random() -> (f64, f64) {
    let x = PADDING + rand::random::<f64>() * (CANVAS_W - 2.0 * PADDING);
    let y = PADDING + rand::random::<f64>() * (CANVAS_H - 2.0 * PADDING);
    (x, y)
}
