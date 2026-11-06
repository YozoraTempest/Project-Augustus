// SPDX-License-Identifier: GPL-3.0-only

//! 将 EU4 的国家定义、地理分类和省份历史组合为固定日期的开局。

mod buildings;
mod countries;
mod geography;
mod history;
mod localisation;
mod manpower;
mod military;
mod navigation;

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use augustus_core::{Country, GameDate, Province, ProvinceKind, World};
use jomini::common::{Date, PdsDate};

use crate::{Error, content::ContentRoot, map::ProvinceMap};

pub const START_DATE: &str = "1444.11.11";

#[derive(Debug)]
pub struct OpeningWorld {
    map: ProvinceMap,
    state: World,
    // 内容中的国家颜色单独保存，核心状态不依赖任何绘图接口。
    country_colors: BTreeMap<String, [u8; 3]>,
    names: BTreeMap<String, String>,
    country_history_conflicts: BTreeSet<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct CountrySummary {
    pub provinces: usize,
    /// 有领土省份缺少发展度时，合计也保持未知。
    pub development: Option<u64>,
}

impl OpeningWorld {
    pub fn load(game_dir: impl AsRef<Path>) -> Result<Self, Error> {
        let content = ContentRoot::new(game_dir.as_ref());
        let map = ProvinceMap::load(game_dir)?;
        let mut countries = countries::load(&content)?;
        let buildings = buildings::load(&content)?;
        let mut provinces: BTreeMap<_, _> = map
            .defined_province_ids()
            .iter()
            .map(|&id| (id, Province::new(ProvinceKind::Land)))
            .collect();
        geography::apply(&content, &mut provinces)?;
        let country_history_conflicts = history::apply_countries(&content, &mut countries.states)?;
        let histories =
            history::apply_provinces(&content, &countries.colors, &mut provinces, &buildings)?;
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
        let military_rules = military::load(&content)?;
        let connections = navigation::connections(&map, &provinces);
        let state = World::new(
            countries.states.into_values(),
            provinces,
            map.mapped_province_ids().iter().copied(),
            buildings.into_values(),
            military_rules,
            connections,
        )
        .map_err(|error| Error::invalid(content.path(), error.to_string()))?;
        let mut wanted: BTreeSet<String> = state.countries().keys().cloned().collect();
        wanted.extend(
            state
                .building_definitions()
                .keys()
                .map(|id| format!("building_{id}")),
        );
        wanted.extend(
            map.defined_province_ids()
                .iter()
                .map(|id| format!("PROV{id}")),
        );
        for country in state.countries().values() {
            wanted.extend(
                country
                    .primary_culture
                    .iter()
                    .chain(country.religion.iter())
                    .cloned(),
            );
        }
        for province in state.provinces().values() {
            wanted.extend(
                province
                    .culture
                    .iter()
                    .chain(province.religion.iter())
                    .chain(province.trade_good.iter())
                    .cloned(),
            );
        }
        let names = localisation::load(&content, &wanted)?;
        Ok(Self {
            map,
            state,
            country_colors: countries.colors,
            names,
            country_history_conflicts,
        })
    }

    pub fn map(&self) -> &ProvinceMap {
        &self.map
    }

    pub fn state(&self) -> &World {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut World {
        &mut self.state
    }

    pub fn start_date(&self) -> GameDate {
        let date = Date::parse(START_DATE).expect("固定开局日期必须有效");
        GameDate::new(date.year(), date.month(), date.day()).expect("开局日期必须在游戏历法内")
    }

    pub fn display_name(&self, key: &str) -> String {
        self.names
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }

    pub fn building_name(&self, id: &str) -> String {
        self.names
            .get(&format!("building_{id}"))
            .cloned()
            .unwrap_or_else(|| id.to_owned())
    }

    pub fn province_name(&self, id: u32) -> String {
        self.names
            .get(&format!("PROV{id}"))
            .cloned()
            .or_else(|| self.map.definition_name(id).map(str::to_owned))
            .unwrap_or_else(|| format!("#{id}"))
    }

    pub fn country_color(&self, tag: &str) -> Option<[u8; 3]> {
        self.country_colors.get(tag).copied()
    }

    pub fn country_history_has_conflicts(&self, tag: &str) -> bool {
        self.country_history_conflicts.contains(tag)
    }

    /// 只汇总地图上实际存在的本国省份，排除废弃定义和仅有控制权的省份。
    pub fn country_summary(&self, tag: &str) -> Option<CountrySummary> {
        self.state.country(tag)?;
        let mut summary = CountrySummary {
            provinces: 0,
            development: Some(0),
        };
        for id in self.map.mapped_province_ids() {
            let province = &self.state.provinces()[id];
            if province.owner.as_deref() == Some(tag) {
                summary.provinces += 1;
                summary.development = summary
                    .development
                    .and_then(|total| province.development.total().map(|value| total + value));
            }
        }
        Some(summary)
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
