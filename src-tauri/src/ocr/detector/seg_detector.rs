//! Saber / CTD `SegDetectorRepresenter` (db_utils.py).
//!
//! Contour → minAreaRect → unclip → score → map to dest size.
//! Port of MashiroSaber03/Saber-Translator `src/interfaces/ctd/utils/db_utils.py`.
//!
//! Unclip: for a minAreaRect quad, `distance = area * ratio / perimeter` then expand
//! both sides by `distance` in the rect frame (equivalent to pyclipper offset on a
//! rectangle; avoids a C++ clipper dependency in the Tauri build).

use super::geom::DetBox;

const MAX_CANDIDATES: usize = 1000;

/// Run SegDetectorRepresenter; boxes normalized to `dest_w`×`dest_h` (0–1).
pub fn boxes_from_bitmap(
    map: &[f32],
    mh: usize,
    mw: usize,
    thresh: f32,
    unclip_ratio: f64,
    dest_w: u32,
    dest_h: u32,
) -> Vec<DetBox> {
    if mh == 0 || mw == 0 || map.len() < mh * mw || dest_w == 0 || dest_h == 0 {
        return Vec::new();
    }
    let binary: Vec<u8> = map[..mh * mw]
        .iter()
        .map(|&v| if v > thresh { 255u8 } else { 0 })
        .collect();

    let contours = find_contours(&binary, mh, mw);
    let mut boxes = Vec::new();
    for contour in contours.into_iter().take(MAX_CANDIDATES) {
        if contour.len() < 3 {
            continue;
        }
        let Some((points, short_side)) = get_mini_box(&contour) else {
            continue;
        };
        if short_side < 2.0 {
            continue;
        }
        let Some(expanded) = unclip_rect_quad(&points, unclip_ratio) else {
            continue;
        };
        let Some((box_pts, _)) = get_mini_box(&expanded) else {
            continue;
        };
        let score = box_score(map, mh, mw, &contour);
        let mut norm = [[0.0f64; 2]; 4];
        let mut xs = [0.0f64; 4];
        let mut ys = [0.0f64; 4];
        for i in 0..4 {
            let x = (box_pts[i][0] / mw as f64 * dest_w as f64)
                .round()
                .clamp(0.0, dest_w as f64);
            let y = (box_pts[i][1] / mh as f64 * dest_h as f64)
                .round()
                .clamp(0.0, dest_h as f64);
            xs[i] = x;
            ys[i] = y;
            norm[i] = [
                (x / dest_w as f64).clamp(0.0, 1.0),
                (y / dest_h as f64).clamp(0.0, 1.0),
            ];
        }
        let x0 = xs.iter().cloned().fold(f64::INFINITY, f64::min) / dest_w as f64;
        let y0 = ys.iter().cloned().fold(f64::INFINITY, f64::min) / dest_h as f64;
        let x1 = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max) / dest_w as f64;
        let y1 = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max) / dest_h as f64;
        if x1 - x0 < 1e-6 || y1 - y0 < 1e-6 {
            continue;
        }
        boxes.push(DetBox::from_quad(norm, score));
    }
    boxes.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    boxes
}

/// Dest = bitmap size (normalized to the map itself).
#[allow(dead_code)]
pub fn boxes_from_db_bitmap(
    map: &[f32],
    mh: usize,
    mw: usize,
    thresh: f32,
    unclip_ratio: f64,
) -> Vec<DetBox> {
    boxes_from_bitmap(map, mh, mw, thresh, unclip_ratio, mw as u32, mh as u32)
}

fn find_contours(binary: &[u8], h: usize, w: usize) -> Vec<Vec<[f64; 2]>> {
    let mut visited = vec![false; h * w];
    let mut contours = Vec::new();
    let dirs = [(0isize, 1), (1, 0), (0, -1), (-1, 0)];
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if visited[i] || binary[i] == 0 {
                continue;
            }
            let mut stack = vec![(x, y)];
            visited[i] = true;
            let mut pixels = Vec::new();
            while let Some((cx, cy)) = stack.pop() {
                pixels.push([cx as f64, cy as f64]);
                for (dx, dy) in dirs {
                    let nx = cx as isize + dx;
                    let ny = cy as isize + dy;
                    if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                        continue;
                    }
                    let ni = ny as usize * w + nx as usize;
                    if visited[ni] || binary[ni] == 0 {
                        continue;
                    }
                    visited[ni] = true;
                    stack.push((nx as usize, ny as usize));
                }
            }
            if pixels.len() < 4 {
                continue;
            }
            let contour = boundary_points(&pixels, binary, h, w);
            contours.push(if contour.len() >= 3 { contour } else { pixels });
        }
    }
    contours
}

fn boundary_points(
    pixels: &[[f64; 2]],
    binary: &[u8],
    h: usize,
    w: usize,
) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    let dirs = [
        (0isize, 1),
        (1, 0),
        (0, -1),
        (-1, 0),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    for p in pixels {
        let x = p[0] as usize;
        let y = p[1] as usize;
        let mut edge = false;
        for (dx, dy) in dirs {
            let nx = x as isize + dx;
            let ny = y as isize + dy;
            if nx < 0 || ny < 0 || nx >= w as isize || ny >= h as isize {
                edge = true;
                break;
            }
            if binary[ny as usize * w + nx as usize] == 0 {
                edge = true;
                break;
            }
        }
        if edge {
            out.push(*p);
        }
    }
    out
}

fn get_mini_box(contour: &[[f64; 2]]) -> Option<([[f64; 2]; 4], f64)> {
    if contour.len() < 3 {
        return None;
    }
    let hull = convex_hull(contour);
    if hull.len() < 3 {
        return None;
    }
    let (center, size, angle) = rotating_calipers_min_area_rect(&hull)?;
    let (w, h) = size;
    let short_side = w.min(h);
    let pts = box_points(center, w, h, angle);
    let mut indexed: Vec<(usize, [f64; 2])> = pts.into_iter().enumerate().collect();
    indexed.sort_by(|a, b| a.1[0].partial_cmp(&b.1[0]).unwrap_or(std::cmp::Ordering::Equal));
    let (first, fourth) = if indexed[1].1[1] > indexed[0].1[1] {
        (0, 1)
    } else {
        (1, 0)
    };
    let (second, third) = if indexed[3].1[1] > indexed[2].1[1] {
        (2, 3)
    } else {
        (3, 2)
    };
    Some((
        [
            indexed[first].1,
            indexed[second].1,
            indexed[third].1,
            indexed[fourth].1,
        ],
        short_side,
    ))
}

fn box_points(center: [f64; 2], w: f64, h: f64, angle_rad: f64) -> [[f64; 2]; 4] {
    let (s, c) = angle_rad.sin_cos();
    let hw = w * 0.5;
    let hh = h * 0.5;
    let corners = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
    let mut out = [[0.0; 2]; 4];
    for (i, p) in corners.iter().enumerate() {
        out[i][0] = center[0] + c * p[0] - s * p[1];
        out[i][1] = center[1] + s * p[0] + c * p[1];
    }
    out
}

fn convex_hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut pts: Vec<[f64; 2]> = points.to_vec();
    pts.sort_by(|a, b| {
        a[0].partial_cmp(&b[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
    });
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9);
    if pts.len() <= 2 {
        return pts;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0
        {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0
        {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn rotating_calipers_min_area_rect(hull: &[[f64; 2]]) -> Option<([f64; 2], (f64, f64), f64)> {
    let n = hull.len();
    if n < 3 {
        return None;
    }
    let mut best_area = f64::INFINITY;
    let mut best = None;
    for i in 0..n {
        let a = hull[i];
        let b = hull[(i + 1) % n];
        let edge = [b[0] - a[0], b[1] - a[1]];
        let elen = (edge[0] * edge[0] + edge[1] * edge[1]).sqrt();
        if elen < 1e-12 {
            continue;
        }
        let ux = edge[0] / elen;
        let uy = edge[1] / elen;
        let vx = -uy;
        let vy = ux;
        let mut min_u = f64::INFINITY;
        let mut max_u = f64::NEG_INFINITY;
        let mut min_v = f64::INFINITY;
        let mut max_v = f64::NEG_INFINITY;
        for p in hull {
            let du = (p[0] - a[0]) * ux + (p[1] - a[1]) * uy;
            let dv = (p[0] - a[0]) * vx + (p[1] - a[1]) * vy;
            min_u = min_u.min(du);
            max_u = max_u.max(du);
            min_v = min_v.min(dv);
            max_v = max_v.max(dv);
        }
        let width = max_u - min_u;
        let height = max_v - min_v;
        let area = width * height;
        if area < best_area {
            best_area = area;
            let cu = (min_u + max_u) * 0.5;
            let cv = (min_v + max_v) * 0.5;
            let cx = a[0] + ux * cu + vx * cv;
            let cy = a[1] + uy * cu + vy * cv;
            let angle = uy.atan2(ux);
            best = Some(([cx, cy], (width, height), angle));
        }
    }
    best
}

/// Saber unclip on a minAreaRect quad: expand by `area*ratio/perimeter` each side.
fn unclip_rect_quad(box_pts: &[[f64; 2]; 4], unclip_ratio: f64) -> Option<Vec<[f64; 2]>> {
    if unclip_ratio <= 0.0 {
        return Some(box_pts.to_vec());
    }
    let area = polygon_area(box_pts).abs();
    let peri = polygon_perimeter(box_pts);
    if peri <= 0.0 || area <= 0.0 {
        return None;
    }
    let distance = area * unclip_ratio / peri;
    // Re-fit rect, expand w/h by 2*distance (rect offset equivalent).
    let (center, (w, h), angle) = rotating_calipers_min_area_rect(box_pts)?;
    let pts = box_points(center, w + 2.0 * distance, h + 2.0 * distance, angle);
    Some(pts.to_vec())
}

fn polygon_area(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    if n < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        a += pts[i][0] * pts[j][1] - pts[j][0] * pts[i][1];
    }
    a * 0.5
}

fn polygon_perimeter(pts: &[[f64; 2]]) -> f64 {
    let n = pts.len();
    if n < 2 {
        return 0.0;
    }
    let mut p = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        p += (pts[i][0] - pts[j][0]).hypot(pts[i][1] - pts[j][1]);
    }
    p
}

fn box_score(map: &[f32], mh: usize, mw: usize, contour: &[[f64; 2]]) -> f32 {
    if contour.is_empty() {
        return 0.0;
    }
    let mut xmin = f64::INFINITY;
    let mut xmax = f64::NEG_INFINITY;
    let mut ymin = f64::INFINITY;
    let mut ymax = f64::NEG_INFINITY;
    for p in contour {
        xmin = xmin.min(p[0]);
        xmax = xmax.max(p[0]);
        ymin = ymin.min(p[1]);
        ymax = ymax.max(p[1]);
    }
    let xmin = xmin.floor().clamp(0.0, (mw - 1) as f64) as usize;
    let xmax = xmax.ceil().clamp(0.0, (mw - 1) as f64) as usize;
    let ymin = ymin.floor().clamp(0.0, (mh - 1) as f64) as usize;
    let ymax = ymax.ceil().clamp(0.0, (mh - 1) as f64) as usize;
    let bw = xmax - xmin + 1;
    let bh = ymax - ymin + 1;
    let mut mask = vec![0u8; bw * bh];
    let poly: Vec<[f64; 2]> = contour
        .iter()
        .map(|p| [p[0] - xmin as f64, p[1] - ymin as f64])
        .collect();
    fill_poly(&mut mask, bw, bh, &poly);
    let mut sum = 0.0f64;
    let mut count = 0usize;
    for y in 0..bh {
        for x in 0..bw {
            if mask[y * bw + x] == 0 {
                continue;
            }
            sum += f64::from(map[(ymin + y) * mw + (xmin + x)]);
            count += 1;
        }
    }
    if count == 0 {
        return 0.0;
    }
    (sum / count as f64).clamp(0.0, 1.0) as f32
}

fn fill_poly(mask: &mut [u8], w: usize, h: usize, poly: &[[f64; 2]]) {
    if poly.len() < 3 {
        return;
    }
    for y in 0..h {
        let mut nodes = Vec::new();
        let mut j = poly.len() - 1;
        for i in 0..poly.len() {
            let yi = poly[i][1];
            let yj = poly[j][1];
            if (yi < y as f64 && yj >= y as f64) || (yj < y as f64 && yi >= y as f64) {
                let t = (y as f64 - yi) / (yj - yi).max(1e-12);
                nodes.push(poly[i][0] + t * (poly[j][0] - poly[i][0]));
            }
            j = i;
        }
        nodes.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mut k = 0;
        while k + 1 < nodes.len() {
            let x0 = nodes[k].ceil().max(0.0) as usize;
            let x1 = nodes[k + 1].floor().min((w.saturating_sub(1)) as f64) as usize;
            if x0 <= x1 {
                for x in x0..=x1 {
                    mask[y * w + x] = 1;
                }
            }
            k += 2;
        }
    }
}
