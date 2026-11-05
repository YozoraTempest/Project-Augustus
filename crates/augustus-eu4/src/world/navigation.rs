// SPDX-License-Identifier: GPL-3.0-only

use std::{collections::BTreeMap, path::Path};

use augustus_core::{LandConnection, Money, Province, ProvinceKind};

use crate::{Error, map::ProvinceMap};

pub(super) fn connections(
    map: &ProvinceMap,
    provinces: &BTreeMap<u32, Province>,
    speed: Money,
    source: &Path,
) -> Result<Vec<LandConnection>, Error> {
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
            // 首版基础行军时间使用地图距离 / 步兵速度，向上取整到完整游戏日。
            let distance = (f64::from(dx).hypot(f64::from(dy))).ceil() as u32;
            let days =
                u32::try_from((Money::from(distance.max(1)) / speed).ceil()).map_err(|_| {
                    Error::invalid(source, "land movement time is outside supported range")
                })?;
            Ok(LandConnection { from, to, days })
        })
        .collect()
}
