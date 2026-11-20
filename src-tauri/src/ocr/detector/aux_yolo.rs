//! Conservative merge of primary boxes with auxiliary YSGYolo (Saber aux_yolo rules).

use super::geom::{overlap_ratio, DetBox};

const DEFAULT_OVERLAP: f64 = 0.1;

pub fn merge_aux_yolo_boxes(
    main_lines: &[DetBox],
    aux_lines: &[DetBox],
    overlap_threshold: Option<f64>,
) -> Vec<DetBox> {
    let thresh = overlap_threshold.unwrap_or(DEFAULT_OVERLAP).clamp(0.0, 1.0);
    if main_lines.is_empty() {
        return aux_lines.to_vec();
    }
    if aux_lines.is_empty() {
        return main_lines.to_vec();
    }

    let mut main_indices_to_remove = std::collections::HashSet::new();
    let mut aux_lines_to_add: Vec<DetBox> = Vec::new();

    for aux in aux_lines {
        let aux_area = aux.area();
        if aux_area <= 0.0 {
            continue;
        }
        let mut contained_main = Vec::new();
        let mut max_overlap_others = 0.0f64;
        for (main_idx, main) in main_lines.iter().enumerate() {
            let ratio = overlap_ratio(aux, main);
            if ratio <= 0.0 {
                continue;
            }
            if aux.contains(main, 0.002) {
                contained_main.push(main_idx);
            } else {
                max_overlap_others = max_overlap_others.max(ratio);
            }
        }
        if !contained_main.is_empty() {
            let contained_area: f64 = contained_main.iter().map(|&i| main_lines[i].area()).sum();
            if contained_area > 0.0
                && aux_area >= contained_area * 2.0
                && max_overlap_others < thresh
            {
                main_indices_to_remove.extend(contained_main);
                aux_lines_to_add.push(*aux);
            }
            continue;
        }
        if max_overlap_others < thresh {
            aux_lines_to_add.push(*aux);
        }
    }

    let mut merged: Vec<DetBox> = main_lines
        .iter()
        .enumerate()
        .filter(|(i, _)| !main_indices_to_remove.contains(i))
        .map(|(_, b)| *b)
        .collect();
    merged.extend(aux_lines_to_add);

    let mut unique = Vec::new();
    for box_ in merged {
        let key = (
            (box_.x * 10_000.0).round() as i64,
            (box_.y * 10_000.0).round() as i64,
            (box_.w * 10_000.0).round() as i64,
            (box_.h * 10_000.0).round() as i64,
        );
        if unique
            .iter()
            .any(|(k, _): &(_, DetBox)| *k == key)
        {
            continue;
        }
        unique.push((key, box_));
    }
    unique.into_iter().map(|(_, b)| b).collect()
}
