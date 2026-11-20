//! Text-line merge that preserves constituent lines (Saber TextBlock style).

use super::geom::{convex_hull, intersection_area, DetBox};

/// A merged text block that still knows its original lines.
#[derive(Debug, Clone)]
pub struct TextBlock {
    pub lines: Vec<DetBox>,
}

impl TextBlock {
    pub fn from_line(line: DetBox) -> Self {
        Self { lines: vec![line] }
    }

    pub fn aabb(&self) -> DetBox {
        let mut iter = self.lines.iter();
        let Some(first) = iter.next() else {
            return DetBox::from_xyxy(0.0, 0.0, 0.01, 0.01, 0.0);
        };
        let mut cur = *first;
        for line in iter {
            cur = union(&cur, line);
        }
        cur.clamp01()
    }

    /// Saber `TextBlock.polygon` stand-in: convex hull of all line quads.
    pub fn polygon(&self) -> Vec<[f64; 2]> {
        let mut pts = Vec::with_capacity(self.lines.len() * 4);
        for line in &self.lines {
            pts.extend_from_slice(&line.pts);
        }
        let hull = convex_hull(&pts);
        if hull.len() >= 3 {
            hull
        } else {
            self.aabb().pts.to_vec()
        }
    }

    /// Saber `TextBlock.center` (polygon centroid).
    pub fn center(&self) -> (f64, f64) {
        let poly = self.polygon();
        if poly.is_empty() {
            return self.aabb().center();
        }
        let n = poly.len() as f64;
        let cx = poly.iter().map(|p| p[0]).sum::<f64>() / n;
        let cy = poly.iter().map(|p| p[1]).sum::<f64>() / n;
        (cx, cy)
    }

    /// Saber `analyze_direction_from_textlines` — majority vote on long-edge angle.
    /// Falls back to AABB taller-than-wide when there are no lines.
    pub fn is_vertical(&self) -> bool {
        if self.lines.is_empty() {
            let b = self.aabb();
            return b.h > b.w;
        }
        let mut v = 0usize;
        let mut h = 0usize;
        for line in &self.lines {
            if line_direction_is_vertical(line) {
                v += 1;
            } else {
                h += 1;
            }
        }
        v >= h
    }
}

/// Longest-edge angle → direction (Saber `angle_to_direction` / `calculate_polygon_angle`).
fn line_direction_is_vertical(line: &DetBox) -> bool {
    let pts = &line.pts;
    let mut best_len = -1.0f64;
    let mut best_angle = 0.0f64;
    for i in 0..4 {
        let p1 = pts[i];
        let p2 = pts[(i + 1) % 4];
        let dx = p2[0] - p1[0];
        let dy = p2[1] - p1[1];
        let len = (dx * dx + dy * dy).sqrt();
        if len > best_len {
            best_len = len;
            best_angle = dy.atan2(dx).to_degrees();
        }
    }
    let mut angle = best_angle % 180.0;
    if angle > 90.0 {
        angle -= 180.0;
    }
    if angle < -90.0 {
        angle += 180.0;
    }
    // |angle| <= 45 → horizontal writing; else vertical column.
    angle.abs() > 45.0
}

/// Merge nearby boxes. `allow_horizontal=false` = Ballons page-level (no side-by-side
/// columns). `true` = coalesce same-row DBNet fragments inside one bubble for OCR.
pub fn merge_nearby_blocks(
    boxes: &[DetBox],
    iou_thresh: f64,
    gap_frac: f64,
) -> Vec<TextBlock> {
    merge_nearby_blocks_ex(boxes, iou_thresh, gap_frac, false)
}

pub fn merge_nearby_blocks_ex(
    boxes: &[DetBox],
    iou_thresh: f64,
    gap_frac: f64,
    allow_horizontal: bool,
) -> Vec<TextBlock> {
    if boxes.is_empty() {
        return Vec::new();
    }
    let mut items: Vec<DetBox> = boxes.to_vec();
    items.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    let mut used = vec![false; items.len()];
    let mut out = Vec::new();
    for i in 0..items.len() {
        if used[i] {
            continue;
        }
        let mut lines = vec![items[i]];
        used[i] = true;
        let mut changed = true;
        while changed {
            changed = false;
            for j in 0..items.len() {
                if used[j] {
                    continue;
                }
                let other = items[j];
                if lines
                    .iter()
                    .any(|line| should_merge(line, &other, iou_thresh, gap_frac, allow_horizontal))
                {
                    lines.push(other);
                    used[j] = true;
                    changed = true;
                }
            }
        }
        lines.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
        out.push(TextBlock { lines });
    }
    out.sort_by(|a, b| {
        a.aabb()
            .y
            .partial_cmp(&b.aabb().y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    out
}

/// Inside one YOLO/bubble hit: glue **same-row** word fragments only.
/// Must NOT vertically merge rows — that yields a multi-line AABB which CTC
/// (height=48) crushes into garbage like "RTWIU" / "I".
#[allow(dead_code)]
pub fn coalesce_ocr_lines(fragments: &[DetBox]) -> Vec<DetBox> {
    if fragments.is_empty() {
        return Vec::new();
    }
    // allow_horizontal + gap_frac=0 disables vertical stack merge in should_merge
    // when we pass a dedicated mode — use same_row_only helper.
    coalesce_same_row_fragments(fragments)
}

fn coalesce_same_row_fragments(fragments: &[DetBox]) -> Vec<DetBox> {
    if fragments.is_empty() {
        return Vec::new();
    }
    let mut items = fragments.to_vec();
    items.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut used = vec![false; items.len()];
    let mut out = Vec::new();
    for i in 0..items.len() {
        if used[i] {
            continue;
        }
        let mut cur = items[i];
        used[i] = true;
        let mut changed = true;
        while changed {
            changed = false;
            for j in 0..items.len() {
                if used[j] {
                    continue;
                }
                if same_row_fragments(&cur, &items[j]) {
                    cur = union(&cur, &items[j]);
                    used[j] = true;
                    changed = true;
                }
            }
        }
        out.push(cur.clamp01());
    }
    out.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
    out
}

fn same_row_fragments(a: &DetBox, b: &DetBox) -> bool {
    let (ax1, ay1, ax2, ay2) = a.xyxy();
    let (bx1, by1, bx2, by2) = b.xyxy();
    let ref_h = a.h.min(b.h).max(1e-6);
    let y_overlap = (ay2.min(by2) - ay1.max(by1)).max(0.0);
    if y_overlap / ref_h < 0.45 {
        return false;
    }
    let gap_x = if ax2 < bx1 {
        bx1 - ax2
    } else if bx2 < ax1 {
        ax1 - bx2
    } else {
        0.0
    };
    // Same line: overlapping or small gap vs line height (not vs width).
    gap_x <= ref_h * 1.2
}

#[allow(dead_code)]
pub fn merge_nearby(boxes: &[DetBox], iou_thresh: f64, gap_frac: f64) -> Vec<DetBox> {
    merge_nearby_blocks(boxes, iou_thresh, gap_frac)
        .into_iter()
        .map(|b| b.aabb())
        .collect()
}

fn should_merge(
    a: &DetBox,
    b: &DetBox,
    iou_thresh: f64,
    gap_frac: f64,
    allow_horizontal: bool,
) -> bool {
    let (ax1, ay1, ax2, ay2) = a.xyxy();
    let (bx1, by1, bx2, by2) = b.xyxy();
    let gap_x = if ax2 < bx1 {
        bx1 - ax2
    } else if bx2 < ax1 {
        ax1 - bx2
    } else {
        0.0
    };
    let gap_y = if ay2 < by1 {
        by1 - ay2
    } else if by2 < ay1 {
        ay1 - by2
    } else {
        0.0
    };
    let ref_h = a.h.min(b.h).max(1e-6);
    let ref_w = a.w.min(b.w).max(1e-6);
    let x_overlap = (ax2.min(bx2) - ax1.max(bx1)).max(0.0);
    let y_overlap = (ay2.min(by2) - ay1.max(by1)).max(0.0);
    let x_ratio = x_overlap / ref_w;
    let y_ratio = y_overlap / ref_h;

    // Page-level: never glue separate columns (top EN | ES headers).
    if !allow_horizontal && gap_x > 1e-4 {
        return false;
    }

    let inter = intersection_area(a, b);
    let union_a = a.area() + b.area() - inter;
    if union_a > 0.0 && inter / union_a >= iou_thresh {
        return true;
    }

    // Vertical stack (same column).
    if x_ratio >= 0.35 && gap_y <= gap_frac.max(0.5) * ref_h {
        return true;
    }

    // Same-row fragments of one sentence (bubble-internal only).
    if allow_horizontal && y_ratio >= 0.4 && gap_x <= gap_frac.max(0.6) * ref_h.max(ref_w * 0.15) {
        return true;
    }
    false
}

/// Split a block whose lines sit in separate horizontal columns (e.g. dual bubbles).
#[allow(dead_code)]
pub fn split_side_by_side_columns(block: TextBlock) -> Vec<TextBlock> {
    if block.lines.len() < 2 {
        return vec![block];
    }
    let mut lines = block.lines;
    lines.sort_by(|a, b| {
        a.x
            .partial_cmp(&b.x)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal))
    });
    let mut clusters: Vec<Vec<DetBox>> = vec![vec![lines[0]]];
    for line in lines.into_iter().skip(1) {
        let prev = clusters.last().unwrap().last().unwrap();
        let (_, _, px1, _) = prev.xyxy();
        let (lx0, _, _, _) = line.xyxy();
        let gap = lx0 - px1;
        let ref_w = prev.w.min(line.w).max(1e-6);
        if gap > (0.02_f64).max(0.25 * ref_w) {
            clusters.push(vec![line]);
        } else {
            clusters.last_mut().unwrap().push(line);
        }
    }
    if clusters.len() < 2 {
        return vec![TextBlock {
            lines: clusters.into_iter().next().unwrap_or_default(),
        }];
    }
    clusters
        .into_iter()
        .map(|mut ls| {
            ls.sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(std::cmp::Ordering::Equal));
            TextBlock { lines: ls }
        })
        .collect()
}

fn union(a: &DetBox, b: &DetBox) -> DetBox {
    let (ax1, ay1, ax2, ay2) = a.xyxy();
    let (bx1, by1, bx2, by2) = b.xyxy();
    DetBox::from_xyxy(
        ax1.min(bx1),
        ay1.min(by1),
        ax2.max(bx2),
        ay2.max(by2),
        a.conf.max(b.conf),
    )
}
