//! Saber `src/core/detector/refinement.py` — SaberYOLO second-stage anti-merge split.
//!
//! Keep the control flow identical to Saber. Geometry uses DetBox AABB for stable
//! membership / assignment (shapely polygon ports were pulling in distant refs).

use super::geom::{intersection_area, point_in_polygon, DetBox};
use super::merge::TextBlock;

/// Saber `SABER_YOLO_REFINE_OVERLAP_THRESHOLD`.
const DEFAULT_OVERLAP: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct RefineDiag {
    pub refs: usize,
    pub blocks_in: usize,
    pub blocks_out: usize,
    pub splits: usize,
    pub skip_reasons: Vec<String>,
}

/// Saber `refine_detection_result_with_reference_blocks` (+ diagnostics).
pub fn refine_blocks_with_references(
    blocks: &[TextBlock],
    references: &[DetBox],
    overlap_threshold: Option<f64>,
) -> (Vec<TextBlock>, RefineDiag) {
    let mut diag = RefineDiag {
        refs: references.len(),
        blocks_in: blocks.len(),
        blocks_out: 0,
        splits: 0,
        skip_reasons: Vec::new(),
    };
    if blocks.is_empty() || references.is_empty() {
        diag.blocks_out = blocks.len();
        return (blocks.to_vec(), diag);
    }
    let thresh = overlap_threshold
        .unwrap_or(DEFAULT_OVERLAP)
        .clamp(0.0, 1.0);
    {
        let ref_desc: Vec<String> = references
            .iter()
            .map(|r| {
                format!(
                    "x={:.3} y={:.3} w={:.3} h={:.3} conf={:.2}",
                    r.x, r.y, r.w, r.h, r.conf
                )
            })
            .collect();
        crate::app_log::info(
            "ocr",
            format!("refine all_refs={} {:?}", references.len(), ref_desc),
        );
    }
    let mut refined = Vec::new();
    let mut changed = false;
    for (bi, block) in blocks.iter().enumerate() {
        match split_block_by_reference_blocks(block, references, thresh) {
            Ok(parts) => {
                crate::app_log::info(
                    "ocr",
                    format!(
                        "refine block{bi} split {} -> {} parts lines={}",
                        block.lines.len(),
                        parts.len(),
                        block.lines.len()
                    ),
                );
                diag.splits += 1;
                refined.extend(parts);
                changed = true;
            }
            Err(reason) => {
                diag.skip_reasons.push(format!("block{bi}:{reason}"));
                refined.push(block.clone());
            }
        }
    }
    if !changed {
        diag.blocks_out = blocks.len();
        return (blocks.to_vec(), diag);
    }
    // Sorting is done by caller via smart_sort (Saber sort_blocks_by_reading_order).
    diag.blocks_out = refined.len();
    (refined, diag)
}

fn split_block_by_reference_blocks(
    block: &TextBlock,
    reference_blocks: &[DetBox],
    overlap_threshold: f64,
) -> Result<Vec<TextBlock>, String> {
    let candidates = get_candidate_reference_blocks(block, reference_blocks, overlap_threshold);
    if candidates.len() <= 1 {
        return Err(format!("cand<={}", candidates.len()));
    }
    {
        let cand_desc: Vec<String> = candidates
            .iter()
            .map(|c| {
                format!(
                    "x={:.3} y={:.3} w={:.3} h={:.3} conf={:.2}",
                    c.x, c.y, c.w, c.h, c.conf
                )
            })
            .collect();
        let line_desc: Vec<String> = block
            .lines
            .iter()
            .map(|l| format!("x={:.3} y={:.3} w={:.3} h={:.3}", l.x, l.y, l.w, l.h))
            .collect();
        crate::app_log::info(
            "ocr",
            format!(
                "refine candidates={} {:?} lines={} {:?}",
                candidates.len(),
                cand_desc,
                block.lines.len(),
                line_desc
            ),
        );
    }

    let mut grouped: Vec<Vec<DetBox>> = vec![Vec::new(); candidates.len()];
    let mut unassigned = 0usize;
    let mut assign_log = Vec::new();
    for line in &block.lines {
        let Some(idx) = choose_reference_index(line, &candidates) else {
            unassigned += 1;
            assign_log.push(format!("line(x={:.3},w={:.3})->none", line.x, line.w));
            continue;
        };
        assign_log.push(format!("line(x={:.3},w={:.3})->cand{idx}", line.x, line.w));
        grouped[idx].push(*line);
    }
    crate::app_log::info("ocr", format!("refine assign {:?}", assign_log));
    // Saber aborts if any line is unassigned.
    if unassigned > 0 {
        return Err(format!(
            "unassigned={unassigned}/{} cand={}",
            block.lines.len(),
            candidates.len()
        ));
    }

    let non_empty: Vec<Vec<DetBox>> = grouped.into_iter().filter(|g| !g.is_empty()).collect();
    if non_empty.len() < 2 {
        return Err(format!("single_group cand={}", candidates.len()));
    }

    Ok(non_empty
        .into_iter()
        .map(|mut lines| {
            lines.sort_by(|a, b| {
                let (acx, acy) = a.center();
                let (bcx, bcy) = b.center();
                acy.partial_cmp(&bcy)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then(acx.partial_cmp(&bcx).unwrap_or(std::cmp::Ordering::Equal))
            });
            TextBlock { lines }
        })
        .collect())
}

/// Saber `_get_candidate_reference_blocks`.
///
/// Gate with AABB overlap first so distant false positives cannot enter, then
/// apply Saber membership: center-in-block polygon OR overlap/ref_area >= thresh.
fn get_candidate_reference_blocks(
    block: &TextBlock,
    reference_blocks: &[DetBox],
    overlap_threshold: f64,
) -> Vec<DetBox> {
    let block_poly = block.polygon();
    let ba = block.aabb();
    reference_blocks
        .iter()
        .copied()
        .filter(|reference| {
            // Hard gate: reject refs with no AABB relationship to the CTD block.
            if !aabb_contains_center(&ba, reference) && aabb_overlap_ratio(&ba, reference) < 0.05
            {
                return false;
            }
            let (cx, cy) = reference.center();
            // Saber `_point_in_block`.
            if point_in_polygon(cx, cy, &block_poly) {
                return true;
            }
            // Saber `_reference_overlaps_block` (AABB stand-in for shapely area ratio).
            aabb_overlap_ratio(&ba, reference) >= overlap_threshold
        })
        .collect()
}

fn aabb_contains_center(block: &DetBox, reference: &DetBox) -> bool {
    let cx = reference.x + reference.w * 0.5;
    let cy = reference.y + reference.h * 0.5;
    cx >= block.x
        && cx <= block.x + block.w
        && cy >= block.y
        && cy <= block.y + block.h
}

fn aabb_overlap_ratio(block: &DetBox, reference: &DetBox) -> f64 {
    let area = reference.area();
    if area <= 0.0 {
        return 0.0;
    }
    intersection_area(block, reference) / area
}

/// Saber `_choose_reference_index` with AABB-stable containment / intersection.
fn choose_reference_index(line: &DetBox, reference_blocks: &[DetBox]) -> Option<usize> {
    if reference_blocks.is_empty() {
        return None;
    }
    let lcx = line.x + line.w * 0.5;
    let lcy = line.y + line.h * 0.5;

    // 1) Refs whose AABB contains the line center → nearest center (Saber step 2,
    //    promoted: avoids broken poly-intersection assigning lines across the page).
    let containing: Vec<usize> = reference_blocks
        .iter()
        .enumerate()
        .filter(|(_, r)| lcx >= r.x && lcx <= r.x + r.w && lcy >= r.y && lcy <= r.y + r.h)
        .map(|(i, _)| i)
        .collect();
    if !containing.is_empty() {
        return containing.into_iter().min_by(|&a, &b| {
            let (ax, ay) = (
                reference_blocks[a].x + reference_blocks[a].w * 0.5,
                reference_blocks[a].y + reference_blocks[a].h * 0.5,
            );
            let (bx, by) = (
                reference_blocks[b].x + reference_blocks[b].w * 0.5,
                reference_blocks[b].y + reference_blocks[b].h * 0.5,
            );
            let da = (ax - lcx).hypot(ay - lcy);
            let db = (bx - lcx).hypot(by - lcy);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // 2) Largest AABB intersection (Saber step 1 stand-in).
    let mut best_overlap_index = None;
    let mut best_overlap_area = 0.0f64;
    for (idx, reference) in reference_blocks.iter().enumerate() {
        let overlap = intersection_area(line, reference);
        if overlap > best_overlap_area {
            best_overlap_area = overlap;
            best_overlap_index = Some(idx);
        }
    }
    if best_overlap_area > 0.0 {
        return best_overlap_index;
    }

    // 3) Nearest reference within font_size * 1.5.
    let (nearest_index, nearest_distance) = reference_blocks
        .iter()
        .enumerate()
        .map(|(idx, r)| {
            let rcx = r.x + r.w * 0.5;
            let rcy = r.y + r.h * 0.5;
            (idx, (rcx - lcx).hypot(rcy - lcy))
        })
        .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))?;
    if nearest_distance <= line.font_size() * 1.5 {
        Some(nearest_index)
    } else {
        None
    }
}
