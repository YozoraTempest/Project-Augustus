// SPDX-License-Identifier: GPL-3.0-only

//! 将 EU4 的国家定义、地理分类和省份历史组合为固定日期的开局。

mod buildings;
mod combat;
mod countries;
mod forcelimit;
mod geography;
mod great_projects;
mod history;
mod localisation;
mod manpower;
mod military;
mod navigation;
mod terrain;

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
        let monument_garrisons = great_projects::load(&content)?;
        let mut provinces: BTreeMap<_, _> = map
            .defined_province_ids()
            .iter()
            .map(|&id| (id, Province::new(ProvinceKind::Land)))
            .collect();
        geography::apply(&content, &mut provinces)?;
        terrain::apply(&content, &map, &mut provinces)?;
        let country_history = history::apply_countries(&content, &mut countries.states)?;
        let country_history_conflicts = country_history.conflicts;
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
        // 旧档没有分国配置，统一兵种仍取法国历史，只填 MilitaryRules。各国开局不读它。
        let historical = countries::all_historical_units(&content)?;
        let (legacy_group, legacy_units) = france_legacy(&country_history.groups, &historical)?;
        let military_rules = military::load(&content, &legacy_group, legacy_units)?;
        let opening_military = combat::OpeningMilitary::load(&content)?;
        assign_opening_military(
            &opening_military,
            &country_history.groups,
            &historical,
            &provinces,
            &mut countries.states,
        )?;
        let connections = navigation::connections(&map, &provinces);
        let mut state = World::with_garrison_monuments(
            countries.states.into_values(),
            provinces,
            map.mapped_province_ids().iter().copied(),
            buildings.into_values(),
            military_rules,
            connections,
            monument_garrisons,
        )
        .map_err(|error| Error::invalid(content.path(), error.to_string()))?;
        // 1444 的国家历史没有 army 编制。规模只读取 defines 的 STARTING_ARMY_SIZE；
        // 叛军威胁没有对应状态，因此不使用 STARTING_ARMY_SIZE_REBEL_THREAT。
        state.spawn_starting_armies();
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

    /// 政治地图填色使用控制者；没有控制者时用所有者。未绘制位置使用背景色。
    /// 国家边界和选中纹理仍使用所有者。
    pub fn province_color(&self, id: u32) -> [u8; 3] {
        let Some(province) = self.state.province(id) else {
            return [15, 20, 26];
        };
        match province.kind {
            ProvinceKind::Sea => [48, 78, 108],
            ProvinceKind::Lake => [63, 98, 130],
            ProvinceKind::Wasteland => [83, 80, 72],
            ProvinceKind::Land => {
                let tag = province.controller.as_deref().or(province.owner.as_deref());
                tag.and_then(|tag| self.country_colors.get(tag).copied())
                    .unwrap_or([170, 164, 145])
            }
        }
    }
}

/// 只给旧档保留的统一兵种。新开局的国家各自使用自己的科技组和兵种组。
fn france_legacy<'a>(
    groups: &'a BTreeMap<String, history::CountryGroups>,
    historical: &'a BTreeMap<String, Vec<String>>,
) -> Result<(String, &'a [String]), Error> {
    let origin = groups.get("FRA").ok_or_else(|| {
        Error::invalid(
            Path::new("history/countries"),
            "legacy military snapshot requires FRA history",
        )
    })?;
    let group = origin
        .unit_type
        .clone()
        .or_else(|| origin.technology_group.clone())
        .ok_or_else(|| {
            Error::invalid(
                Path::new("history/countries"),
                "legacy military snapshot requires FRA unit group",
            )
        })?;
    Ok((
        group,
        historical.get("FRA").map(Vec::as_slice).unwrap_or(&[]),
    ))
}

fn assign_opening_military(
    military: &combat::OpeningMilitary,
    groups: &BTreeMap<String, history::CountryGroups>,
    historical: &BTreeMap<String, Vec<String>>,
    provinces: &BTreeMap<u32, Province>,
    countries: &mut BTreeMap<String, Country>,
) -> Result<(), Error> {
    let landed: BTreeSet<String> = provinces
        .values()
        .filter(|province| province.kind == ProvinceKind::Land)
        .filter_map(|province| province.owner.clone())
        .collect();
    for (tag, country) in countries.iter_mut() {
        let Some(origin) = groups.get(tag) else {
            if landed.contains(tag) {
                return Err(Error::invalid(
                    Path::new("history/countries"),
                    format!("{tag} has land but no technology_group"),
                ));
            }
            continue;
        };
        let Some(technology_group) = origin.technology_group.clone() else {
            if landed.contains(tag) {
                return Err(Error::invalid(
                    Path::new("history/countries"),
                    format!("{tag} has land but no technology_group"),
                ));
            }
            continue;
        };
        let unit_group = origin
            .unit_type
            .clone()
            .unwrap_or_else(|| technology_group.clone());
        let units = historical.get(tag).map(Vec::as_slice).unwrap_or(&[]);
        country.military = Some(military.resolve(tag, &technology_group, &unit_group, units)?);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeSet, time::Duration};

    use augustus_core::{
        GameDate, GameSpeed, Money, ProvinceKind, REGIMENT_STRENGTH, SavedGame, Simulation,
        UnitKind, World,
    };

    use super::OpeningWorld;

    #[test]
    #[ignore = "需要本机 EU4 目录，设置 AUGUSTUS_EU4_DIR"]
    fn vanilla_opening_places_armies_on_owned_land() {
        let dir = std::env::var("AUGUSTUS_EU4_DIR").expect("设置 AUGUSTUS_EU4_DIR");
        let world = OpeningWorld::load(dir).expect("EU4 开局");
        let state = world.state();
        let combat = &state.military_rules().combat;
        assert_eq!(combat.tactics, Money::new(5, 1));
        assert_eq!(combat.infantry.fire_damage, Money::new(25, 2));
        assert_eq!(combat.infantry.shock_damage, Money::new(2, 1));
        assert_eq!(combat.cavalry.fire_damage, Money::ZERO);
        assert_eq!(combat.cavalry.shock_damage, Money::new(8, 1));
        assert_eq!(combat.max_morale, Money::from(2));
        // 法国历史名单在科技 1 选中 western_medieval_infantry 和 chevauchee。
        assert_eq!(combat.infantry.offensive_morale, 1);
        assert_eq!(combat.infantry.defensive_morale, 1);
        assert_eq!(combat.infantry.offensive_shock, 0);
        assert_eq!(combat.cavalry.offensive_shock, 1);
        assert_eq!(combat.cavalry.defensive_shock, 1);
        assert_eq!(combat.cavalry.defensive_morale, 0);
        assert_eq!(combat.width, 15);
        let fra = state.country_military("FRA").expect("法国有军事配置");
        assert_eq!(fra.level, 3);
        assert_eq!(fra.technology_group, "western");
        assert_eq!(fra.unit_group, "western");
        assert_eq!(fra.infantry_unit, "western_medieval_infantry");
        assert_eq!(fra.cavalry_unit, "chevauchee");
        assert_eq!(fra.width, 20);
        assert_eq!(fra.max_morale, Money::new(25, 1));
        assert_eq!(fra.tactics, Money::new(5, 1));
        assert_eq!(fra.infantry.fire_damage, Money::new(35, 2));
        assert_eq!(fra.infantry.shock_damage, Money::new(5, 1));
        assert_eq!(fra.cavalry.as_ref().unwrap().shock_damage, Money::from(1));
        assert_eq!(fra.infantry.offensive_morale, 1);
        assert_eq!(fra.infantry.offensive_shock, 0);
        let lit = state.country_military("LIT").expect("立陶宛有军事配置");
        assert_eq!(lit.level, 3);
        assert_eq!(lit.technology_group, "eastern");
        assert_eq!(lit.unit_group, "eastern");
        assert_eq!(lit.infantry_unit, "bardiche_infantry");
        assert_eq!(lit.cavalry_unit, "eastern_knights");
        assert_eq!(lit.width, fra.width);
        assert_eq!(lit.max_morale, fra.max_morale);
        assert_eq!(lit.infantry.offensive_shock, 0);
        assert_eq!(lit.cavalry.as_ref().unwrap().defensive_shock, 0);
        let azt = state.country_military("AZT").expect("阿兹特克有军事配置");
        assert_eq!(azt.level, 1);
        assert_eq!(azt.technology_group, "mesoamerican");
        assert_eq!(azt.unit_group, "mesoamerican");
        assert_eq!(azt.infantry_unit, "mesoamerican_spearmen");
        assert!(azt.cavalry.is_none());
        assert_eq!(azt.width, 15);
        assert_eq!(azt.max_morale, Money::from(2));
        assert_eq!(azt.infantry.fire_damage, Money::new(35, 2));
        assert_eq!(azt.infantry.shock_damage, Money::new(3, 1));
        assert_eq!(azt.infantry.offensive_shock, 1);
        let fraction = state.military_rules().starting_army_size;
        let owners: BTreeSet<_> = state
            .armies()
            .values()
            .map(|army| army.country.clone())
            .collect();
        assert_eq!(owners.len(), state.armies().len());
        for army in state.armies().values() {
            let province = state.province(army.province).expect("驻地存在");
            assert_eq!(province.kind, ProvinceKind::Land);
            assert_eq!(province.owner.as_deref(), Some(army.country.as_str()));
            let military = state
                .country_military(&army.country)
                .expect("有军队的国家有军事配置");
            assert!(army.regiments.iter().all(|regiment| {
                regiment.kind == UnitKind::Infantry
                    && regiment.strength == REGIMENT_STRENGTH
                    && regiment.unit == military.infantry_unit
                    && regiment.morale == military.morale_milli().unwrap()
            }));
            let limit = state.country_land_force_limit(&army.country).unwrap();
            let expected = (Money::from(limit) * fraction).floor();
            assert_eq!(Money::from(army.regiments.len() as u64), expected);
        }
        for tag in state.countries().keys() {
            let limit = state.country_land_force_limit(tag).unwrap();
            let expected = (Money::from(limit) * fraction).floor();
            let has = state.armies().values().any(|army| &army.country == tag);
            assert_eq!(has, expected > Money::ZERO, "{tag}");
        }
        let mut adjacent = false;
        for army in state.armies().values() {
            let Some(neighbors) = state.land_neighbors(army.province) else {
                continue;
            };
            for province in neighbors.keys() {
                let Some(owner) = state
                    .province(*province)
                    .and_then(|province| province.owner.as_deref())
                else {
                    continue;
                };
                if owner != army.country && owners.contains(owner) {
                    adjacent = true;
                }
            }
        }
        assert!(adjacent);
        assert_eq!(
            state.country("FRA").unwrap().finances.treasury(),
            Money::ZERO
        );
        let manpower = state.country_manpower("FRA").unwrap();
        assert_eq!(manpower.available, manpower.capacity);
    }

    struct BorderFort {
        army: u32,
        country: String,
        owner: String,
        fort: u32,
        foreign_neighbors: usize,
    }

    /// 邻接、兵力够、省内没有军队的外国要塞。外国邻省更少的排在前面。
    fn border_forts(state: &World) -> Vec<BorderFort> {
        let mut found = Vec::new();
        for army in state.armies().values() {
            if army.movement.is_some() || army.is_retreating() || army.battle_id().is_some() {
                continue;
            }
            let Some(neighbors) = state.land_neighbors(army.province) else {
                continue;
            };
            let mut foreign = Vec::new();
            for &province in neighbors.keys() {
                let Some(owner) = state
                    .province(province)
                    .and_then(|province| province.owner.clone())
                else {
                    continue;
                };
                if owner != army.country {
                    foreign.push((province, owner));
                }
            }
            for (fort, owner) in &foreign {
                if state.province_fort_level(*fort) == 0 {
                    continue;
                }
                let need = state.siege_required_men(*fort).unwrap_or(u64::MAX);
                if army.strength() < need {
                    continue;
                }
                let blocked = state.armies().values().any(|other| {
                    other.province == *fort && other.strength() > 0 && !other.is_retreating()
                });
                if blocked {
                    continue;
                }
                found.push(BorderFort {
                    army: army.id(),
                    country: army.country.clone(),
                    owner: owner.clone(),
                    fort: *fort,
                    foreign_neighbors: foreign.len(),
                });
            }
        }
        found
            .sort_by_key(|candidate| (candidate.foreign_neighbors, candidate.fort, candidate.army));
        found
    }

    fn restore_opening(opening: &mut OpeningWorld, snapshot: &serde_json::Value) {
        let restored = serde_json::from_value::<SavedGame>(snapshot.clone())
            .unwrap()
            .restore(opening.state())
            .expect("开局快照可以读回");
        opening.state = restored.world;
    }

    fn clock(date: GameDate) -> Simulation {
        let mut simulation = Simulation::new(date);
        simulation.set_paused(false);
        simulation.set_speed(GameSpeed::Normal);
        simulation
    }

    /// 玩家用行军命令围城，白和平后进度清除、控制者回到所有者。
    fn player_siege(opening: &mut OpeningWorld, candidate: &BorderFort, date: GameDate) -> bool {
        let state = &mut opening.state;
        if state
            .declare_war(&candidate.country, &candidate.owner, date)
            .is_err()
        {
            return false;
        }
        if state
            .move_army(&candidate.country, candidate.army, candidate.fort, date)
            .is_err()
        {
            return false;
        }
        let mut simulation = clock(date);
        for _ in 0..40 {
            simulation.advance(state, Duration::from_secs(1));
            let started = state
                .province_siege(candidate.fort)
                .map(|progress| (progress.besieger.clone(), progress.days));
            let Some((besieger, _)) = started else {
                continue;
            };
            if besieger != candidate.country {
                return false;
            }
            let province = state.province(candidate.fort).unwrap();
            if province.owner.as_deref() != Some(candidate.owner.as_str())
                || province.controller.as_deref() != Some(candidate.owner.as_str())
            {
                return false;
            }
            if state
                .make_peace(&candidate.country, &candidate.owner, &[], date)
                .is_err()
            {
                return false;
            }
            let province = state.province(candidate.fort).unwrap();
            return province.siege.is_none()
                && province.controller.as_deref() == Some(candidate.owner.as_str())
                && province.owner.as_deref() == Some(candidate.owner.as_str());
        }
        false
    }

    /// 非玩家交战国自己走到要塞并留下围城，白和平后清除。
    fn ai_siege(opening: &mut OpeningWorld, candidate: &BorderFort, date: GameDate) -> bool {
        let state = &mut opening.state;
        if state
            .declare_war(&candidate.country, &candidate.owner, date)
            .is_err()
        {
            return false;
        }
        let mut simulation = clock(date);
        for _ in 0..40 {
            simulation.advance_for(state, Duration::from_secs(1), Some(&candidate.owner));
            let started = state
                .province_siege(candidate.fort)
                .map(|progress| progress.besieger.clone());
            let Some(besieger) = started else {
                continue;
            };
            if besieger != candidate.country {
                return false;
            }
            if state
                .make_peace(&candidate.country, &candidate.owner, &[], date)
                .is_err()
            {
                return false;
            }
            return state.province_siege(candidate.fort).is_none()
                && state
                    .province(candidate.fort)
                    .unwrap()
                    .controller
                    .as_deref()
                    == Some(candidate.owner.as_str());
        }
        false
    }

    #[test]
    #[ignore = "需要本机 EU4 目录，设置 AUGUSTUS_EU4_DIR"]
    fn opening_player_and_ai_can_besiege_a_real_fort() {
        let dir = std::env::var("AUGUSTUS_EU4_DIR").expect("设置 AUGUSTUS_EU4_DIR");
        let mut opening = OpeningWorld::load(dir).expect("EU4 开局");
        let capital = opening
            .state()
            .country("FRA")
            .and_then(|country| country.capital)
            .expect("法国有首都");
        let level = opening.state().province_fort_level(capital);
        assert!(level >= 1);
        assert_eq!(opening.state().province_garrison(capital), Some(1_000));
        assert_eq!(opening.state().province_garrison_cap(capital), 1_000);
        assert!(!opening.state().province(capital).unwrap().fort_mothballed);

        let plain = opening
            .state()
            .provinces()
            .iter()
            .find_map(|(&id, province)| {
                if province.kind != ProvinceKind::Land || province.owner.is_none() {
                    return None;
                }
                if province
                    .buildings
                    .iter()
                    .any(|building| building.contains("fort"))
                {
                    return None;
                }
                let owner = province.owner.as_deref().unwrap();
                if opening
                    .state()
                    .country(owner)
                    .and_then(|country| country.capital)
                    == Some(id)
                {
                    return None;
                }
                (opening.state().province_fort_level(id) == 0).then_some(id)
            });
        let plain = plain.expect("存在无要塞陆地");
        assert_eq!(opening.state().province_garrison(plain), None);

        let candidates = border_forts(opening.state());
        assert!(
            !candidates.is_empty(),
            "1444 没有邻接空要塞且兵力足够的军队"
        );
        let date = opening.start_date();
        let snapshot = serde_json::to_value(SavedGame::capture(
            opening.state(),
            &Simulation::new(date),
            &candidates[0].country,
        ))
        .unwrap();

        let mut player_done = false;
        let mut ai_done = false;
        for candidate in candidates.iter().take(12) {
            if !player_done && player_siege(&mut opening, candidate, date) {
                player_done = true;
            }
            restore_opening(&mut opening, &snapshot);
            if !ai_done && ai_siege(&mut opening, candidate, date) {
                ai_done = true;
            }
            restore_opening(&mut opening, &snapshot);
            if player_done && ai_done {
                break;
            }
        }
        assert!(player_done, "玩家没能从开局军队开始一次合法围城");
        assert!(ai_done, "AI 没能维持一次合法围城");
    }

    #[test]
    #[ignore = "需要本机 EU4 目录，设置 AUGUSTUS_EU4_DIR"]
    fn teutonic_opening_garrison_includes_the_monument_modifier() {
        let dir = std::env::var("AUGUSTUS_EU4_DIR").expect("设置 AUGUSTUS_EU4_DIR");
        let opening = OpeningWorld::load(dir).expect("EU4 开局");
        let state = opening.state();
        let capital = state
            .country("TEU")
            .and_then(|country| country.capital)
            .expect("条顿有首都");
        assert_eq!(state.province_garrison(capital), Some(1_150));
        assert_eq!(state.province_garrison_cap(capital), 1_150);
        assert_eq!(state.siege_required_men(capital), Some(3_000));

        let france = state
            .country("FRA")
            .and_then(|country| country.capital)
            .expect("法国有首都");
        assert_eq!(state.province_garrison(france), Some(1_000));
        assert_eq!(state.province_garrison_cap(france), 1_000);

        let castle = state.provinces().iter().find_map(|(&id, province)| {
            if id == capital || province.owner.as_deref() != Some("TEU") {
                return None;
            }
            if province.buildings.len() != 1 || !province.buildings.contains("fort_15th") {
                return None;
            }
            (state.province_fort_level(id) == 2).then_some(id)
        });
        let castle = castle.expect("条顿有非首都的 15 世纪城堡");
        assert_eq!(state.province_garrison(castle), Some(2_300));
        assert_eq!(state.province_garrison_cap(castle), 2_300);
        assert_eq!(state.siege_required_men(castle), Some(6_000));
    }
}
