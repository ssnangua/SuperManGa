//! Saber `textline_merge.merge_textlines` / `geometry.can_merge_textlines`.
//! Uses `poly_distance` (shapely Polygon.distance semantics on quads).

use std::collections::HashMap;

use super::geom::{poly_distance, DetBox};
use super::merge::TextBlock;

const ASPECT_RATIO_TOL: f64 = 1.3;
const FONT_SIZE_RATIO_TOL: f64 = 2.0;
const CHAR_GAP_TOLERANCE: f64 = 1.0;
const CHAR_GAP_TOLERANCE2: f64 = 3.0;
const DISCARD_CONNECTION_GAP: f64 = 2.0;
const SPLIT_GAMMA: f64 = 0.5;
const SPLIT_SIGMA: f64 = 2.0;

/// Merge DBNet/CTD text lines into Saber-style TextBlocks (keep constituent lines).
pub fn merge_textlines(lines: &[DetBox]) -> Vec<TextBlock> {
    if lines.is_empty() {
        return Vec::new();
    }
    if lines.len() == 1 {
        return vec![TextBlock::from_line(lines[0])];
    }

    let n = lines.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut rank = vec![0u8; n];

    for i in 0..n {
        for j in (i + 1)..n {
            if can_merge_textlines(&lines[i], &lines[j]) {
                unite(&mut parent, &mut rank, i, j);
            }
        }
    }

    let mut components: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        let r = find(&mut parent, i);
        components[r].push(i);
    }

    let mut blocks = Vec::new();
    for indices in components.into_iter().filter(|c| !c.is_empty()) {
        for part in split_text_region(lines, &indices) {
            let mut ls: Vec<DetBox> = part.into_iter().map(|i| lines[i]).collect();
            ls.sort_by(|a, b| {
                a.y.partial_cmp(&b.y)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
            });
            if !ls.is_empty() {
                blocks.push(TextBlock { lines: ls });
            }
        }
    }

    blocks.sort_by(|a, b| {
        a.aabb()
            .y
            .partial_cmp(&b.aabb().y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(
                a.aabb()
                    .x
                    .partial_cmp(&b.aabb().x)
                    .unwrap_or(std::cmp::Ordering::Equal),
            )
    });
    blocks
}

fn find(parent: &mut [usize], i: usize) -> usize {
    let mut x = i;
    while parent[x] != x {
        parent[x] = parent[parent[x]];
        x = parent[x];
    }
    x
}

fn unite(parent: &mut [usize], rank: &mut [u8], a: usize, b: usize) {
    let mut ra = find(parent, a);
    let mut rb = find(parent, b);
    if ra == rb {
        return;
    }
    if rank[ra] < rank[rb] {
        std::mem::swap(&mut ra, &mut rb);
    }
    parent[rb] = ra;
    if rank[ra] == rank[rb] {
        rank[ra] += 1;
    }
}

fn can_merge_textlines(a: &DetBox, b: &DetBox) -> bool {
    let fs_a = font_size(a);
    let fs_b = font_size(b);
    let char_size = fs_a.min(fs_b);
    if char_size <= 0.0 {
        return false;
    }
    if fs_a.max(fs_b) / char_size > FONT_SIZE_RATIO_TOL {
        return false;
    }

    let ar_a = aspect_ratio(a);
    let ar_b = aspect_ratio(b);
    if ar_a > ASPECT_RATIO_TOL && ar_b < 1.0 / ASPECT_RATIO_TOL {
        return false;
    }
    if ar_b > ASPECT_RATIO_TOL && ar_a < 1.0 / ASPECT_RATIO_TOL {
        return false;
    }

    // Saber: `a.polygon.distance(b.polygon)`.
    let dist = poly_distance(a, b);
    if dist > DISCARD_CONNECTION_GAP * char_size {
        return false;
    }

    let angle_diff = (line_angle(a) - line_angle(b)).abs();
    if angle_diff > 15.0 * std::f64::consts::PI / 180.0 {
        return false;
    }

    if dist < char_size * CHAR_GAP_TOLERANCE {
        return true;
    }

    // Saber: `a.poly_distance(b)` (mid-edge).
    mid_edge_distance(a, b) <= char_size * CHAR_GAP_TOLERANCE2
}

fn font_size(b: &DetBox) -> f64 {
    b.w.min(b.h).max(0.0)
}

fn aspect_ratio(b: &DetBox) -> f64 {
    if b.h <= 1e-12 {
        return 1.0;
    }
    b.w / b.h
}

fn line_angle(b: &DetBox) -> f64 {
    if b.h > b.w {
        std::f64::consts::FRAC_PI_2
    } else {
        0.0
    }
}

fn mid_edge_distance(a: &DetBox, b: &DetBox) -> f64 {
    let (ax1, ay1, ax2, ay2) = a.xyxy();
    let (bx1, by1, bx2, by2) = b.xyxy();
    if line_angle(a) < 0.25 && line_angle(b) < 0.25 {
        let a_top = ((ax1 + ax2) * 0.5, ay1);
        let a_bot = ((ax1 + ax2) * 0.5, ay2);
        let b_top = ((bx1 + bx2) * 0.5, by1);
        let b_bot = ((bx1 + bx2) * 0.5, by2);
        [
            hypot(a_top, b_top),
            hypot(a_top, b_bot),
            hypot(a_bot, b_top),
            hypot(a_bot, b_bot),
        ]
        .into_iter()
        .fold(f64::INFINITY, f64::min)
    } else {
        let a_l = (ax1, (ay1 + ay2) * 0.5);
        let a_r = (ax2, (ay1 + ay2) * 0.5);
        let b_l = (bx1, (by1 + by2) * 0.5);
        let b_r = (bx2, (by1 + by2) * 0.5);
        [
            hypot(a_l, b_l),
            hypot(a_l, b_r),
            hypot(a_r, b_l),
            hypot(a_r, b_r),
        ]
        .into_iter()
        .fold(f64::INFINITY, f64::min)
    }
}

fn hypot(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn split_text_region(lines: &[DetBox], indices: &[usize]) -> Vec<Vec<usize>> {
    if indices.len() <= 1 {
        return vec![indices.to_vec()];
    }
    if indices.len() == 2 {
        let a = &lines[indices[0]];
        let b = &lines[indices[1]];
        let fs = font_size(a).max(font_size(b));
        // Saber: line1.distance_to(line2) → polygon distance.
        let dist = poly_distance(a, b);
        let angle_ok = (line_angle(a) - line_angle(b)).abs() < 0.2 * std::f64::consts::PI;
        if dist < (1.0 + SPLIT_GAMMA) * fs && angle_ok {
            return vec![indices.to_vec()];
        }
        return vec![vec![indices[0]], vec![indices[1]]];
    }

    let mut edges: Vec<(usize, usize, f64)> = Vec::new();
    for i in 0..indices.len() {
        for j in (i + 1)..indices.len() {
            let u = indices[i];
            let v = indices[j];
            edges.push((u, v, poly_distance(&lines[u], &lines[v])));
        }
    }
    edges.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));

    let max_id = lines.len();
    let mut parent: Vec<usize> = (0..max_id).collect();
    let mut rank = vec![0u8; max_id];
    let mut mst: Vec<(usize, usize, f64)> = Vec::new();
    for (u, v, w) in edges {
        let ru = find(&mut parent, u);
        let rv = find(&mut parent, v);
        if ru == rv {
            continue;
        }
        unite(&mut parent, &mut rank, u, v);
        mst.push((u, v, w));
        if mst.len() + 1 >= indices.len() {
            break;
        }
    }
    mst.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    if mst.is_empty() {
        return vec![indices.to_vec()];
    }

    let distances: Vec<f64> = mst.iter().map(|e| e.2).collect();
    let fontsize =
        indices.iter().map(|&i| font_size(&lines[i])).sum::<f64>() / indices.len() as f64;
    let mean = distances.iter().sum::<f64>() / distances.len() as f64;
    let var = distances.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / distances.len() as f64;
    let std = var.sqrt();
    // Normalized-space stand-in for Saber's `max(0.3 * fontsize + 5, 5)` (pixels).
    let std_threshold = (0.3 * fontsize + 1e-3).max(1e-3);
    let max_dist = distances[0];
    let (u0, v0, _) = mst[0];
    let (c0x, c0y) = lines[u0].center();
    let (c1x, c1y) = lines[v0].center();
    let centroid_align = (c0x - c1x).abs().min((c0y - c1y).abs());
    // Saber: max_poly_distance == 0 and max_centroid_alignment < 5 (pixels).
    // Centers/distances here are normalized; 5px ≈ 5/min(side) — callers use page-norm boxes.
    let overlapping = poly_distance(&lines[u0], &lines[v0]) <= 1e-9;
    let keep_together = (max_dist <= mean + std * SPLIT_SIGMA
        || max_dist <= fontsize * (1.0 + SPLIT_GAMMA))
        && (std < std_threshold || (overlapping && centroid_align < 0.005));

    if keep_together {
        return vec![indices.to_vec()];
    }

    let mut parent2: Vec<usize> = (0..max_id).collect();
    let mut rank2 = vec![0u8; max_id];
    for &(u, v, _) in mst.iter().skip(1) {
        unite(&mut parent2, &mut rank2, u, v);
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for &i in indices {
        let r = find(&mut parent2, i);
        groups.entry(r).or_default().push(i);
    }
    let mut out = Vec::new();
    for group in groups.into_values() {
        out.extend(split_text_region(lines, &group));
    }
    out
}
