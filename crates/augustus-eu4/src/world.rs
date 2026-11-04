// SPDX-License-Identifier: GPL-3.0-only

//! 将 EU4 的国家定义、地理分类和省份历史组合为固定日期的开局。

mod countries;
mod geography;
mod history;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use augustus_core::{Country, Province, ProvinceKind, World};

use crate::{Error, content::ContentRoot, map::ProvinceMap};

pub const START_DATE: &str = "1444.11.11";

#[derive(Debug)]
pub struct OpeningWorld {
    map: ProvinceMap,
    state: World,
    // 内容中的国家颜色单独保存，核心状态不依赖任何绘图接口。
    country_colors: BTreeMap<String, [u8; 3]>,
}

impl OpeningWorld {
    pub fn load(game_dir: impl AsRef<Path>) -> Result<Self, Error> {
        let content = ContentRoot::new(game_dir.as_ref());
        let map = ProvinceMap::load(game_dir)?;
        let country_colors = countries::load(&content)?;
        let mut provinces: BTreeMap<_, _> = map
            .defined_province_ids()
            .iter()
            .map(|&id| {
                (
                    id,
                    Province {
                        kind: ProvinceKind::Land,
                        owner: None,
                    },
                )
            })
            .collect();
        geography::apply(&content, &mut provinces)?;
        let histories = history::apply(&content, &country_colors, &mut provinces)?;
        // 已绘制的陆地必须有历史文件；缺失内容不能被误当成未殖民省份。
        for id in map.mapped_province_ids() {
            if provinces
                .get(id)
                .is_some_and(|province| province.kind == ProvinceKind::Land)
                && !histories.contains(id)
            {
                return Err(Error::invalid(
                    content.path().join("history/provinces"),
                    format!("mapped land province {id} has no history"),
                ));
            }
        }
        let countries = country_colors.keys().cloned().map(Country::new);
        let state = World::new(countries, provinces)
            .map_err(|error| Error::invalid(content.path(), error.to_string()))?;
        Ok(Self {
            map,
            state,
            country_colors,
        })
    }

    pub fn map(&self) -> &ProvinceMap {
        &self.map
    }

    pub fn state(&self) -> &World {
        &self.state
    }

    /// 只统计开局持有省份的国家；注册但尚未出现的国家不计入此数。
    pub fn landed_country_count(&self) -> usize {
        self.map
            .mapped_province_ids()
            .iter()
            .filter_map(|id| self.state.province_owner(*id))
            .map(Country::tag)
            .collect::<BTreeSet<_>>()
            .len()
    }

    /// 政治地图与省份查询使用同一个所有者状态，未绘制位置使用背景色。
    pub fn province_color(&self, id: u32) -> [u8; 3] {
        let Some(province) = self.state.province(id) else {
            return [15, 20, 26];
        };
        match province.kind {
            ProvinceKind::Sea => [48, 78, 108],
            ProvinceKind::Lake => [63, 98, 130],
            ProvinceKind::Wasteland => [83, 80, 72],
            ProvinceKind::Land => self
                .state
                .province_owner(id)
                .map_or([170, 164, 145], |country| {
                    self.country_colors[country.tag()]
                }),
        }
    }
}
