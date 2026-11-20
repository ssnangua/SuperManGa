//! Saber `src/core/detector/postprocess.py`.

use super::geom::intersection_area;
use super::merge::TextBlock;
use super::smart_sort::sort_blocks_by_reading_order;

/// Saber `postprocess_blocks` default overlap for merge_overlapping_blocks.
const DEFAULT_OVERLAP: f64 = 0.7;

/// Saber `postprocess_blocks` with `sort_method='smart'`.
pub fn postprocess_blocks(
    blocks: Vec<TextBlock>,
    right_to_left: bool,
    image_size: Option<(u32, u32)>,
) -> Vec<TextBlock> {
    if blocks.is_empty() {
        return blocks;
    }
    let blocks = remove_contained_blocks(blocks);
    let blocks = merge_overlapping_blocks(blocks, DEFAULT_OVERLAP);
    sort_blocks_by_reading_order(blocks, right_to_left, image_size)
}

/// Re-sort only (after refine). Avoids re-running contain/overlap merge.
pub fn sort_blocks_reading_order(
    blocks: Vec<TextBlock>,
    right_to_left: bool,
    image_size: Option<(u32, u32)>,
) -> Vec<TextBlock> {
    sort_blocks_by_reading_order(blocks, right_to_left, image_size)
}

fn remove_contained_blocks(blocks: Vec<TextBlock>) -> Vec<TextBlock> {
    if blocks.len() <= 1 {
        return blocks;
    }
    let mut to_remove = vec![false; blocks.len()];
    for i in 0..blocks.len() {
        if to_remove[i] {
            continue;
        }
        let bi = blocks[i].aabb();
        for j in 0..blocks.len() {
            if i == j || to_remove[j] {
                continue;
            }
            let bj = blocks[j].aabb();
            // i contained in j -> remove i (Saber is_box_contained on xyxy)
            if bj.contains(&bi, 0.0) {
                to_remove[i] = true;
                break;
            }
        }
    }
    blocks
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !to_remove[*i])
        .map(|(_, b)| b)
        .collect()
}

fn merge_overlapping_blocks(mut blocks: Vec<TextBlock>, overlap_threshold: f64) -> Vec<TextBlock> {
    if blocks.len() <= 1 {
        return blocks;
    }
    loop {
        let mut merge_pair: Option<(usize, usize)> = None;
        'outer: for i in 0..blocks.len() {
            for j in (i + 1)..blocks.len() {
                let a = blocks[i].aabb();
                let b = blocks[j].aabb();
                let smaller = a.area().min(b.area());
                if smaller <= 0.0 {
                    continue;
                }
                if intersection_area(&a, &b) / smaller > overlap_threshold {
                    merge_pair = Some((i, j));
                    break 'outer;
                }
            }
        }
        let Some((i, j)) = merge_pair else {
            break;
        };
        let mut lines = blocks[i].lines.clone();
        lines.extend(blocks[j].lines.iter().copied());
        blocks[i] = TextBlock { lines };
        blocks.remove(j);
    }
    blocks
}
