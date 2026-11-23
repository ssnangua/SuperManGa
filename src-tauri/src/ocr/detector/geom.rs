//! DetBox with 4-corner quads (Saber TextLine.pts) + polygon geometry.
//!
//! Distance / intersection / covers follow shapely Polygon semantics on quads.

#[derive(Debug, Clone, Copy)]
pub struct DetBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub conf: f32,
    /// Normalized 4-corner quad (Saber TextLine.pts order).
    pub pts: [[f64; 2]; 4],
}

impl DetBox {
    pub fn xyxy(&self) -> (f64, f64, f64, f64) {
        (self.x, self.y, self.x + self.w, self.y + self.h)
    }

    pub fn area(&self) -> f64 {
        (self.w.max(0.0) * self.h.max(0.0)).max(0.0)
    }

    /// Centroid of the quad (Saber TextLine.center).
    pub fn center(&self) -> (f64, f64) {
        let cx = (self.pts[0][0] + self.pts[1][0] + self.pts[2][0] + self.pts[3][0]) * 0.25;
        let cy = (self.pts[0][1] + self.pts[1][1] + self.pts[2][1] + self.pts[3][1]) * 0.25;
        (cx, cy)
    }

    pub fn font_size(&self) -> f64 {
        self.w.min(self.h).max(0.0)
    }

    pub fn from_xyxy(x0: f64, y0: f64, x1: f64, y1: f64, conf: f32) -> Self {
        let (x0, x1) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        let (y0, y1) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        let x = x0;
        let y = y0;
        let w = (x1 - x0).max(1e-6);
        let h = (y1 - y0).max(1e-6);
        Self {
            x,
            y,
            w,
            h,
            conf,
            pts: [[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
        }
    }

    pub fn from_quad(pts: [[f64; 2]; 4], conf: f32) -> Self {
        let pts = ensure_ccw_quad(pts);
        let x0 = pts.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let y0 = pts.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let x1 = pts.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        let y1 = pts.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
        let mut b = Self::from_xyxy(x0, y0, x1, y1, conf);
        b.pts = pts;
        b
    }

    /// OBB (cx, cy, w, h, angle radians) → DetBox with oriented `pts`.
    #[allow(dead_code)]
    pub fn from_obb(cx: f64, cy: f64, w: f64, h: f64, angle: f64, conf: f32) -> Self {
        let (s, c) = angle.sin_cos();
        let hw = w * 0.5;
        let hh = h * 0.5;
        let pts = [
            [cx + c * (-hw) - s * (-hh), cy + s * (-hw) + c * (-hh)],
            [cx + c * hw - s * (-hh), cy + s * hw + c * (-hh)],
            [cx + c * hw - s * hh, cy + s * hw + c * hh],
            [cx + c * (-hw) - s * hh, cy + s * (-hw) + c * hh],
        ];
        Self::from_quad(pts, conf)
    }

    pub fn clamp01(mut self) -> Self {
        for p in &mut self.pts {
            p[0] = p[0].clamp(0.0, 1.0);
            p[1] = p[1].clamp(0.0, 1.0);
        }
        let x0 = self.pts.iter().map(|p| p[0]).fold(f64::INFINITY, f64::min);
        let y0 = self.pts.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
        let x1 = self.pts.iter().map(|p| p[0]).fold(f64::NEG_INFINITY, f64::max);
        let y1 = self.pts.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
        self.x = x0.clamp(0.0, 1.0);
        self.y = y0.clamp(0.0, 1.0);
        self.w = (x1 - self.x).max(0.01).min(1.0 - self.x);
        self.h = (y1 - self.y).max(0.01).min(1.0 - self.y);
        self
    }

    /// AABB containment (Saber `is_box_contained` on xyxy).
    pub fn contains(&self, inner: &DetBox, eps: f64) -> bool {
        let (ox1, oy1, ox2, oy2) = self.xyxy();
        let (ix1, iy1, ix2, iy2) = inner.xyxy();
        ox1 <= ix1 + eps && oy1 <= iy1 + eps && ox2 >= ix2 - eps && oy2 >= iy2 - eps
    }

    /// Point in quad (Saber polygon covers / contains).
    pub fn contains_point(&self, px: f64, py: f64) -> bool {
        point_in_ring(px, py, &self.pts)
    }

    /// Polygon area of the quad (Saber TextLine / block polygon area).
    pub fn poly_area(&self) -> f64 {
        ring_area(&self.pts).abs()
    }
}

/// AABB intersection area (Saber postprocess / aux xyxy helpers).
pub fn intersection_area(a: &DetBox, b: &DetBox) -> f64 {
    let (ax1, ay1, ax2, ay2) = a.xyxy();
    let (bx1, by1, bx2, by2) = b.xyxy();
    let ix1 = ax1.max(bx1);
    let iy1 = ay1.max(by1);
    let ix2 = ax2.min(bx2);
    let iy2 = ay2.min(by2);
    if ix1 >= ix2 || iy1 >= iy2 {
        return 0.0;
    }
    (ix2 - ix1) * (iy2 - iy1)
}

/// Overlap relative to the smaller AABB area (Saber aux rule).
pub fn overlap_ratio(a: &DetBox, b: &DetBox) -> f64 {
    let smaller = a.area().min(b.area());
    if smaller <= 0.0 {
        return 0.0;
    }
    intersection_area(a, b) / smaller
}

/// Shapely-style polygon distance on quads (0 if they touch/intersect).
pub fn poly_distance(a: &DetBox, b: &DetBox) -> f64 {
    ring_distance(&a.pts, &b.pts)
}

/// Polygon ∩ polygon area for convex quads.
pub fn poly_intersection_area(a: &DetBox, b: &DetBox) -> f64 {
    convex_intersection_area(&a.pts, &b.pts)
}

/// Polygon IoU (tests / diagnostics). SaberYOLO NMS uses letterbox xywh IoU instead.
#[allow(dead_code)]
pub fn poly_iou(a: &DetBox, b: &DetBox) -> f64 {
    let inter = poly_intersection_area(a, b);
    if inter <= 0.0 {
        return 0.0;
    }
    let uni = a.poly_area() + b.poly_area() - inter;
    if uni <= 0.0 {
        return 0.0;
    }
    (inter / uni).clamp(0.0, 1.0)
}

/// Intersection area of two convex rings (collect inside verts + edge hits → hull).
fn convex_intersection_area(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    if a.len() < 3 || b.len() < 3 {
        return 0.0;
    }
    let mut pts: Vec<[f64; 2]> = Vec::new();
    for &p in a {
        if point_in_ring(p[0], p[1], b) {
            pts.push(p);
        }
    }
    for &p in b {
        if point_in_ring(p[0], p[1], a) {
            pts.push(p);
        }
    }
    for i in 0..a.len() {
        let a0 = a[i];
        let a1 = a[(i + 1) % a.len()];
        for j in 0..b.len() {
            let b0 = b[j];
            let b1 = b[(j + 1) % b.len()];
            if let Some(hit) = segment_intersect(a0, a1, b0, b1) {
                pts.push(hit);
            }
        }
    }
    if pts.len() < 3 {
        // Fallback: Sutherland–Hodgman with forced CCW.
        let clipped = clip_convex_to_poly(a, b);
        if clipped.len() < 3 {
            return 0.0;
        }
        return ring_area_n(&clipped).abs();
    }
    let hull = convex_hull(&pts);
    if hull.len() < 3 {
        return 0.0;
    }
    ring_area_n(&hull).abs()
}

fn segment_intersect(
    a0: [f64; 2],
    a1: [f64; 2],
    b0: [f64; 2],
    b1: [f64; 2],
) -> Option<[f64; 2]> {
    let d = (a1[0] - a0[0]) * (b1[1] - b0[1]) - (a1[1] - a0[1]) * (b1[0] - b0[0]);
    if d.abs() < 1e-18 {
        return None;
    }
    let t = ((b0[0] - a0[0]) * (b1[1] - b0[1]) - (b0[1] - a0[1]) * (b1[0] - b0[0])) / d;
    let u = ((b0[0] - a0[0]) * (a1[1] - a0[1]) - (b0[1] - a0[1]) * (a1[0] - a0[0])) / d;
    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
        Some([a0[0] + t * (a1[0] - a0[0]), a0[1] + t * (a1[1] - a0[1])])
    } else {
        None
    }
}

fn ensure_ccw_quad(mut pts: [[f64; 2]; 4]) -> [[f64; 2]; 4] {
    if ring_area(&pts) < 0.0 {
        pts.swap(1, 3);
    }
    pts
}

fn ring_area(pts: &[[f64; 2]; 4]) -> f64 {
    let mut a = 0.0;
    for i in 0..4 {
        let j = (i + 1) % 4;
        a += pts[i][0] * pts[j][1] - pts[j][0] * pts[i][1];
    }
    a * 0.5
}

fn ring_area_n(pts: &[[f64; 2]]) -> f64 {
    if pts.len() < 3 {
        return 0.0;
    }
    let mut a = 0.0;
    for i in 0..pts.len() {
        let j = (i + 1) % pts.len();
        a += pts[i][0] * pts[j][1] - pts[j][0] * pts[i][1];
    }
    a * 0.5
}

fn point_in_ring(px: f64, py: f64, pts: &[[f64; 2]]) -> bool {
    let n = pts.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let yi = pts[i][1];
        let yj = pts[j][1];
        let xi = pts[i][0];
        let xj = pts[j][0];
        // Keep the sign of (yj - yi). Clamping with `.max(1e-12)` flips downward
        // edges and marks points far to the side as inside. CTD quads are skewed,
        // so that false hit zeros `poly_distance` and merges a whole text band.
        if (yi > py) != (yj > py) {
            let dy = yj - yi;
            if dy.abs() > 1e-18 {
                let x_at = (xj - xi) * (py - yi) / dy + xi;
                if px < x_at {
                    inside = !inside;
                }
            }
        }
        j = i;
    }
    inside
}

fn ring_distance(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    // Intersecting / touching → 0 (shapely Polygon.distance).
    if !clip_convex_to_poly(a, b).is_empty() || poly_intersection_n(a, b) > 1e-12 {
        return 0.0;
    }
    // Any vertex of A inside B or vice versa.
    if a.iter().any(|p| point_in_ring(p[0], p[1], b)) || b.iter().any(|p| point_in_ring(p[0], p[1], a))
    {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for i in 0..a.len() {
        let a0 = a[i];
        let a1 = a[(i + 1) % a.len()];
        for j in 0..b.len() {
            let b0 = b[j];
            let b1 = b[(j + 1) % b.len()];
            best = best.min(segment_distance(a0, a1, b0, b1));
        }
    }
    if best.is_finite() {
        best
    } else {
        0.0
    }
}

fn poly_intersection_n(a: &[[f64; 2]], b: &[[f64; 2]]) -> f64 {
    let clipped = clip_convex_to_poly(a, b);
    if clipped.len() < 3 {
        return 0.0;
    }
    ring_area_n(&clipped).abs()
}

fn segment_distance(a0: [f64; 2], a1: [f64; 2], b0: [f64; 2], b1: [f64; 2]) -> f64 {
    point_seg_dist(a0, b0, b1)
        .min(point_seg_dist(a1, b0, b1))
        .min(point_seg_dist(b0, a0, a1))
        .min(point_seg_dist(b1, a0, a1))
}

fn point_seg_dist(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let abx = b[0] - a[0];
    let aby = b[1] - a[1];
    let apx = p[0] - a[0];
    let apy = p[1] - a[1];
    let ab2 = abx * abx + aby * aby;
    let t = if ab2 <= 1e-18 {
        0.0
    } else {
        ((apx * abx + apy * aby) / ab2).clamp(0.0, 1.0)
    };
    let qx = a[0] + t * abx;
    let qy = a[1] + t * aby;
    (p[0] - qx).hypot(p[1] - qy)
}

/// Clip subject polygon by convex clip polygon (Sutherland–Hodgman).
fn clip_convex_to_poly(subject: &[[f64; 2]], clip: &[[f64; 2]]) -> Vec<[f64; 2]> {
    if subject.len() < 3 || clip.len() < 3 {
        return Vec::new();
    }
    let mut output: Vec<[f64; 2]> = subject.to_vec();
    let mut clip_ring: Vec<[f64; 2]> = clip.to_vec();
    if ring_area_n(&output) < 0.0 {
        output.reverse();
    }
    if ring_area_n(&clip_ring) < 0.0 {
        clip_ring.reverse();
    }
    for i in 0..clip_ring.len() {
        if output.is_empty() {
            break;
        }
        let c0 = clip_ring[i];
        let c1 = clip_ring[(i + 1) % clip_ring.len()];
        let input = output;
        output = Vec::new();
        for j in 0..input.len() {
            let cur = input[j];
            let prev = input[(j + input.len() - 1) % input.len()];
            let cur_in = edge_left(c0, c1, cur);
            let prev_in = edge_left(c0, c1, prev);
            if cur_in {
                if !prev_in {
                    if let Some(hit) = line_intersect(prev, cur, c0, c1) {
                        output.push(hit);
                    }
                }
                output.push(cur);
            } else if prev_in {
                if let Some(hit) = line_intersect(prev, cur, c0, c1) {
                    output.push(hit);
                }
            }
        }
    }
    // Drop near-duplicates.
    let mut cleaned = Vec::new();
    for p in output {
        if cleaned
            .last()
            .map(|q: &[f64; 2]| (q[0] - p[0]).hypot(q[1] - p[1]) > 1e-12)
            .unwrap_or(true)
        {
            cleaned.push(p);
        }
    }
    if cleaned.len() >= 2 {
        let first = cleaned[0];
        let last = *cleaned.last().unwrap();
        if (first[0] - last[0]).hypot(first[1] - last[1]) <= 1e-12 {
            cleaned.pop();
        }
    }
    cleaned
}

fn edge_left(a: [f64; 2], b: [f64; 2], p: [f64; 2]) -> bool {
    // Cross (b-a) x (p-a) >= 0 → p left of directed edge a→b (CCW clip).
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= -1e-12
}

fn line_intersect(p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], p4: [f64; 2]) -> Option<[f64; 2]> {
    let d = (p1[0] - p2[0]) * (p3[1] - p4[1]) - (p1[1] - p2[1]) * (p3[0] - p4[0]);
    if d.abs() < 1e-18 {
        return None;
    }
    let t = ((p1[0] - p3[0]) * (p3[1] - p4[1]) - (p1[1] - p3[1]) * (p3[0] - p4[0])) / d;
    Some([p1[0] + t * (p2[0] - p1[0]), p1[1] + t * (p2[1] - p1[1])])
}

/// Convex hull of points (monotone chain), returned CCW. Used for TextBlock polygon.
pub fn convex_hull(points: &[[f64; 2]]) -> Vec<[f64; 2]> {
    let mut pts: Vec<[f64; 2]> = points.to_vec();
    if pts.len() <= 1 {
        return pts;
    }
    pts.sort_by(|a, b| {
        a[0]
            .partial_cmp(&b[0])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a[1].partial_cmp(&b[1]).unwrap_or(std::cmp::Ordering::Equal))
    });
    pts.dedup_by(|a, b| (a[0] - b[0]).abs() < 1e-12 && (a[1] - b[1]).abs() < 1e-12);
    if pts.len() <= 2 {
        return pts;
    }
    let cross = |o: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
    };
    let mut lower = Vec::new();
    for p in &pts {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0.0 {
            lower.pop();
        }
        lower.push(*p);
    }
    let mut upper = Vec::new();
    for p in pts.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0.0 {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Point in arbitrary ring (Saber polygon.covers with tiny buffer approximated by exact in).
#[allow(dead_code)]
pub fn point_in_polygon(px: f64, py: f64, ring: &[[f64; 2]]) -> bool {
    point_in_ring(px, py, ring)
}

/// Intersection area of two arbitrary simple rings via clip (clip must be convex).
#[allow(dead_code)]
pub fn polygon_intersection_area(subject: &[[f64; 2]], clip_convex: &[[f64; 2]]) -> f64 {
    convex_intersection_area(subject, clip_convex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_quad_iou_is_one() {
        let a = DetBox::from_xyxy(0.1, 0.1, 0.3, 0.5, 1.0);
        let iou = poly_iou(&a, &a);
        assert!((iou - 1.0).abs() < 1e-6, "iou={iou}");
    }

    #[test]
    fn overlapping_quads_have_positive_iou() {
        let a = DetBox::from_xyxy(0.1, 0.1, 0.4, 0.4, 1.0);
        let b = DetBox::from_xyxy(0.25, 0.25, 0.55, 0.55, 1.0);
        let iou = poly_iou(&a, &b);
        assert!(iou > 0.05, "iou={iou}");
    }

    #[test]
    fn disjoint_quads_iou_zero() {
        let a = DetBox::from_xyxy(0.0, 0.0, 0.2, 0.2, 1.0);
        let b = DetBox::from_xyxy(0.5, 0.5, 0.7, 0.7, 1.0);
        assert!(poly_iou(&a, &b) < 1e-9);
    }

    #[test]
    fn skewed_vertical_lines_on_the_same_band_stay_apart() {
        // CTD minAreaRect quads: thin, slightly skewed, overlapping in y, far in x.
        let left = DetBox::from_quad(
            [
                [0.06, 0.55],
                [0.075, 0.552],
                [0.07, 0.65],
                [0.055, 0.648],
            ],
            1.0,
        );
        let right = DetBox::from_quad(
            [
                [0.60, 0.55],
                [0.615, 0.552],
                [0.61, 0.65],
                [0.595, 0.648],
            ],
            1.0,
        );
        let dist = poly_distance(&left, &right);
        assert!(
            dist > 0.4,
            "same-band skewed lines must not count as intersecting, dist={dist}"
        );
        assert!(!left.contains_point(0.01, 0.60));
        assert!(!right.contains_point(0.01, 0.60));
        assert!(left.contains_point(0.065, 0.60));
    }
}
