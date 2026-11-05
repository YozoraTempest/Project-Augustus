// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, HashMap};

/// 标记选取最靠近省份像素重心的实际像素，凹形省份和群岛也不会落在海上。
pub(super) fn calculate(pixels: &[u32], width: u32) -> BTreeMap<u32, [u32; 2]> {
    let mut sums: HashMap<u32, [u64; 3]> = HashMap::new();
    runs(pixels, width, |id, y, start, end| {
        let entry = sums.entry(id).or_default();
        let count = u64::from(end - start);
        entry[0] += u64::from(start + end - 1) * count / 2;
        entry[1] += u64::from(y) * count;
        entry[2] += count;
    });
    let centers: HashMap<_, _> = sums
        .into_iter()
        .map(|(id, [x, y, count])| (id, [x as f64 / count as f64, y as f64 / count as f64]))
        .collect();
    let mut closest: HashMap<u32, (f64, [u32; 2])> = HashMap::new();
    runs(pixels, width, |id, y, start, end| {
        let [cx, cy] = centers[&id];
        let x = (cx.round() as u32).clamp(start, end - 1);
        let distance = (f64::from(x) - cx).powi(2) + (f64::from(y) - cy).powi(2);
        let current = closest.entry(id).or_insert((f64::INFINITY, [x, y]));
        if distance < current.0 {
            *current = (distance, [x, y]);
        }
    });
    closest
        .into_iter()
        .map(|(id, (_, point))| (id, point))
        .collect()
}

fn runs(pixels: &[u32], width: u32, mut visit: impl FnMut(u32, u32, u32, u32)) {
    // 按一行内的连续色块汇总，避免每个像素都查询一次省份索引。
    for (y, row) in pixels.chunks_exact(width as usize).enumerate() {
        let mut start = 0;
        while start < row.len() {
            let id = row[start];
            let mut end = start + 1;
            while end < row.len() && row[end] == id {
                end += 1;
            }
            if id != 0 {
                visit(id, y as u32, start as u32, end as u32);
            }
            start = end;
        }
    }
}
