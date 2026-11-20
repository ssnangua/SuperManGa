//! Saber `src/core/detector/smart_sort.py` — reading-order sort (stddev path).
//!
//! Panel detection (Kumiko/CV) is not ported yet; we use the same stddev / simple-sort
//! logic Saber uses when `img=None` or panel detection fails. Gap thresholds 20 / 15
//! are pixel-space, so centers are scaled by `image_size`.

use super::merge::TextBlock;

/// Saber `sort_blocks_by_reading_order` / `sort_regions` (stddev branch; no panel).
pub fn sort_blocks_by_reading_order(
    blocks: Vec<TextBlock>,
    right_to_left: bool,
    image_size: Option<(u32, u32)>,
) -> Vec<TextBlock> {
    if blocks.is_empty() {
        return blocks;
    }
    let (iw, ih) = image_size.unwrap_or((1000, 1000));
    let iw = iw.max(1) as f64;
    let ih = ih.max(1) as f64;

    if blocks.len() == 1 {
        return blocks;
    }

    let centers: Vec<(f64, f64)> = blocks
        .iter()
        .map(|r| {
            let (cx, cy) = r.center();
            (cx * iw, cy * ih)
        })
        .collect();

    let xs: Vec<f64> = centers.iter().map(|c| c.0).collect();
    let ys: Vec<f64> = centers.iter().map(|c| c.1).collect();
    let is_horizontal = stddev(&xs) > stddev(&ys);

    if is_horizontal {
        // Saber: sort by x (RTL → -cx); group when |Δcx| > 20; group sort by cy.
        let mut order: Vec<usize> = (0..blocks.len()).collect();
        order.sort_by(|&a, &b| {
            let ka = if right_to_left {
                -centers[a].0
            } else {
                centers[a].0
            };
            let kb = if right_to_left {
                -centers[b].0
            } else {
                centers[b].0
            };
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        });
        group_walk(
            &blocks,
            &order,
            &centers,
            right_to_left,
            /*primary_is_x=*/ true,
            20.0,
        )
    } else {
        // Saber: sort by cy; group when |Δcy| > 15; group sort by ±cx.
        let mut order: Vec<usize> = (0..blocks.len()).collect();
        order.sort_by(|&a, &b| {
            centers[a]
                .1
                .partial_cmp(&centers[b].1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        group_walk(
            &blocks,
            &order,
            &centers,
            right_to_left,
            /*primary_is_x=*/ false,
            15.0,
        )
    }
}

fn group_walk(
    blocks: &[TextBlock],
    order: &[usize],
    centers: &[(f64, f64)],
    right_to_left: bool,
    primary_is_x: bool,
    gap: f64,
) -> Vec<TextBlock> {
    let mut sorted_all = Vec::with_capacity(blocks.len());
    let mut group: Vec<usize> = Vec::new();
    let mut prev: Option<f64> = None;

    for &idx in order {
        let primary = if primary_is_x {
            centers[idx].0
        } else {
            centers[idx].1
        };
        if let Some(p) = prev {
            if (primary - p).abs() > gap {
                flush_group(&mut sorted_all, blocks, &mut group, centers, right_to_left, primary_is_x);
            }
        }
        group.push(idx);
        prev = Some(primary);
    }
    flush_group(
        &mut sorted_all,
        blocks,
        &mut group,
        centers,
        right_to_left,
        primary_is_x,
    );
    sorted_all
}

fn flush_group(
    out: &mut Vec<TextBlock>,
    blocks: &[TextBlock],
    group: &mut Vec<usize>,
    centers: &[(f64, f64)],
    right_to_left: bool,
    primary_is_x: bool,
) {
    if group.is_empty() {
        return;
    }
    if primary_is_x {
        group.sort_by(|&a, &b| {
            centers[a]
                .1
                .partial_cmp(&centers[b].1)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    } else {
        group.sort_by(|&a, &b| {
            let ka = if right_to_left {
                -centers[a].0
            } else {
                centers[a].0
            };
            let kb = if right_to_left {
                -centers[b].0
            } else {
                centers[b].0
            };
            ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
        });
    }
    for idx in group.drain(..) {
        out.push(blocks[idx].clone());
    }
}

fn stddev(vals: &[f64]) -> f64 {
    if vals.len() <= 1 {
        return 0.0;
    }
    let n = vals.len() as f64;
    let mean = vals.iter().sum::<f64>() / n;
    let var = vals.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
    var.sqrt()
}
