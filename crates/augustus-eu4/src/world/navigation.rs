// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use augustus_core::{LandConnection, Province, ProvinceKind};

use crate::map::ProvinceMap;

pub(super) fn connections(
    map: &ProvinceMap,
    provinces: &BTreeMap<u32, Province>,
) -> Vec<LandConnection> {
    let width = map.summary().width;
    map.adjacent_pairs()
        .iter()
        .filter(|(from, to)| {
            provinces[from].kind == ProvinceKind::Land && provinces[to].kind == ProvinceKind::Land
        })
        .map(|&(from, to)| {
            let [ax, ay] = map.province_anchor(from).expect("相邻省份存在于位图");
            let [bx, by] = map.province_anchor(to).expect("相邻省份存在于位图");
            let dx = ax.abs_diff(bx).min(width - ax.abs_diff(bx));
            let dy = ay.abs_diff(by);
            // 距离至少为 1。天数在下达行军时按军队最慢兵种速度向上取整。
            let distance = (f64::from(dx).hypot(f64::from(dy))).ceil() as u32;
            LandConnection {
                from,
                to,
                distance: distance.max(1),
            }
        })
        .collect()
}
