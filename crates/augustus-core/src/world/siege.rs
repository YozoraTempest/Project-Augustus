// SPDX-License-Identifier: GPL-3.0-only

//! 要塞围城与驻军。
//!
//! 等级取首都 `capital_city.fort_level = 1` 和建筑 `fort_level` 的较大值，只用来区分有没有要塞。
//! 这个较大值不参与所需兵力。驻军上限是 `GARRISON_SIZE`（1000），defines 没有按等级相乘。
//! 能推进围城需要 `SIEGE_FORCE_NEEDED_MULTIPLIER` 倍的当前驻军。不足时暂停，不退已有天数。
//! 驻军少于 `SIEGE_GARRISON_SURRENDER` 时同样要满三倍才结束。驻军为 0 时三倍仍是 0，敌军停驻当天结束。
//! 围攻军全部离开、撤退或继续行军时清除围城。交战当天暂停。
//! 换一个交战方时进度作废，与无要塞占领使用同一条项目规则。
//! 阶段长度为 `DAYS_PER_SIEGE_PHASE`。骰面 14 和分档 4/11/13/15 没有反编译出处，检定表不产生结果，不改变控制者。
//! 炮兵、封锁、将领围城和破城保持 0。`SIEGE_WIN` 和四档加成留在接口上。
//! 疾病按 `SIEGE_DISEASE_IMPACT` 乘团的最大兵力 `REGIMENT_STRENGTH` 扣除。没有自然 1 的出处时，日常推进不调用它。
//! 停用会立刻清空驻军，但不取消攻守反转。
//! 城市修正 `garrison_growth = 0.05` 在启用且由所有者控制时每月恢复，并消耗所有者人力。
//! `manpower.garrison_growth = 0.01` 的注释只写人力增加，没有算法，不接入。
//! 围城只改变控制者。状态上限仍用 `SIEGE_MEMORY` 与 `SIEGE_MEMORY_SCALED`，取整方式未证实。

use std::collections::BTreeMap;

use crate::{
    BuildingDefinition, Country, GameDate, OCCUPATION_PHASE_DAYS, Province, ProvinceKind,
    REGIMENT_STRENGTH, SiegeProgress, SiegeRoll, WorldError,
};

use super::World;

/// `GARRISON_SIZE`。
const GARRISON_SIZE: u32 = 1_000;
/// `SIEGE_FORCE_NEEDED_MULTIPLIER`。
const SIEGE_FORCE_NEEDED_MULTIPLIER: u64 = 3;
/// `SIEGE_GARRISON_SURRENDER`。
const SIEGE_GARRISON_SURRENDER: u32 = 100;
/// `SIEGE_WIN`。
const SIEGE_WIN: u32 = 20;
/// `SIEGE_DISEASE_IMPACT`，以千分比保存。
const SIEGE_DISEASE_IMPACT_PERMILLE: u64 = 50;
/// `SIEGE_MEMORY`。
const SIEGE_MEMORY: u32 = 11;
/// `SIEGE_MEMORY_SCALED`，以千分比保存。
const SIEGE_MEMORY_SCALED_PERMILLE: u32 = 500;
/// `SIEGE_BONUS_SUPPLIES_SHORTAGE`。
const SIEGE_BONUS_SUPPLIES_SHORTAGE: u32 = 1;
/// `SIEGE_BONUS_FOOD_SHORTAGE`。
const SIEGE_BONUS_FOOD_SHORTAGE: u32 = 2;
/// `SIEGE_BONUS_WATER_SHORTAGE`。
const SIEGE_BONUS_WATER_SHORTAGE: u32 = 3;
/// `SIEGE_BONUS_DEFENDERS_DESERT`。
const SIEGE_BONUS_DEFENDERS_DESERT: u32 = 2;
/// `MAX_BREACH`。本阶段破城始终为 0，常量只封住以后的加成。
const MAX_BREACH: u32 = 3;
/// 城市静态修正 `garrison_growth`。
const CITY_GARRISON_GROWTH_PERMILLE: u64 = 50;

struct SiegeSide {
    lead: String,
    includes_owner: bool,
}

struct PhaseOutcome {
    result: SiegeRoll,
    status_bonus: u32,
    disease: bool,
    surrender: bool,
}

pub(super) fn fort_level_of(
    province: &Province,
    id: u32,
    countries: &BTreeMap<String, Country>,
    buildings: &BTreeMap<String, BuildingDefinition>,
) -> u32 {
    if province.kind != ProvinceKind::Land {
        return 0;
    }
    let mut level = 0;
    if let Some(owner) = province.owner.as_deref()
        && countries.get(owner).and_then(|country| country.capital) == Some(id)
    {
        level = 1;
    }
    for building in &province.buildings {
        if let Some(definition) = buildings.get(building) {
            level = level.max(definition.fort_level);
        }
    }
    level
}

/// 有要塞时为 `GARRISON_SIZE`。等级只决定有没有要塞，不放大上限。
pub(super) fn garrison_cap(level: u32) -> u32 {
    if level == 0 { 0 } else { GARRISON_SIZE }
}

/// `SIEGE_MEMORY + trunc(SIEGE_MEMORY_SCALED * level)`。取整方式未证实，检定未接入时结算不读取它。
pub(super) fn max_siege_status(level: u32) -> u32 {
    SIEGE_MEMORY.saturating_add(level.saturating_mul(SIEGE_MEMORY_SCALED_PERMILLE) / 1_000)
}

/// 炮兵、封锁和将领都还没有输入。破城输入目前为 0，上限是 `MAX_BREACH`。
fn reserved_siege_bonus() -> u32 {
    let artillery = 0;
    let blockade = 0;
    let leader = 0;
    artillery + blockade + leader + breach_bonus(0)
}

fn breach_bonus(breach: u32) -> u32 {
    breach.min(MAX_BREACH)
}

/// 骰面和分档没有出处，所以不产生结果。defines 里的阈值和加成留在接口上。
fn phase_table_outcome() -> Option<PhaseOutcome> {
    let _interface = (
        SIEGE_WIN,
        SIEGE_BONUS_SUPPLIES_SHORTAGE,
        SIEGE_BONUS_FOOD_SHORTAGE,
        SIEGE_BONUS_WATER_SHORTAGE,
        SIEGE_BONUS_DEFENDERS_DESERT,
        reserved_siege_bonus(),
    );
    None
}

/// 疾病扣除的是最大兵力的百分之五，不是当前兵力。不足这一数额时扣到 0。
fn disease_loss(strength: u32) -> u32 {
    let loss = u64::from(REGIMENT_STRENGTH) * SIEGE_DISEASE_IMPACT_PERMILLE / 1_000;
    let loss = u32::try_from(loss).unwrap_or(strength);
    strength.min(loss)
}

impl World {
    pub fn province_fort_level(&self, id: u32) -> u32 {
        let Some(province) = self.province(id) else {
            return 0;
        };
        fort_level_of(province, id, &self.countries, &self.buildings)
    }

    pub fn province_garrison(&self, id: u32) -> Option<u32> {
        self.province(id).and_then(|province| province.garrison)
    }

    pub fn province_garrison_cap(&self, id: u32) -> u32 {
        garrison_cap(self.province_fort_level(id))
    }

    /// 当前驻军对应的最低围城兵力。没有要塞时为空。
    pub fn siege_required_men(&self, id: u32) -> Option<u64> {
        if self.province_fort_level(id) == 0 {
            return None;
        }
        let garrison = self.province_garrison(id).unwrap_or(0);
        Some(u64::from(garrison) * SIEGE_FORCE_NEEDED_MULTIPLIER)
    }

    pub fn province_siege(&self, id: u32) -> Option<&SiegeProgress> {
        self.province(id)
            .and_then(|province| province.siege.as_ref())
    }

    /// 停在省内、与控制者交战的兵力合计。
    pub fn besieging_men(&self, id: u32) -> u64 {
        let Some(controller) = self
            .province(id)
            .and_then(|province| province.controller.clone())
        else {
            return 0;
        };
        self.hostile_stopped_men(id, &controller).values().sum()
    }

    /// 停用立刻清空驻军。重新启用后靠每月恢复，不立即补满。
    pub fn set_fort_mothballed(&mut self, id: u32, mothballed: bool) -> Result<(), WorldError> {
        if self.province_fort_level(id) == 0 {
            return Err(WorldError::InvalidSiege { province: id });
        }
        let province = self.provinces.get_mut(&id).expect("要塞省份存在");
        province.fort_mothballed = mothballed;
        if mothballed {
            province.garrison = Some(0);
        }
        Ok(())
    }

    pub(super) fn ensure_garrisons(&mut self) {
        let ids: Vec<u32> = self.active_provinces.iter().copied().collect();
        for id in ids {
            let level = self.province_fort_level(id);
            if level == 0 {
                continue;
            }
            let province = self.provinces.get_mut(&id).expect("要塞省份存在");
            if province.garrison.is_some() {
                continue;
            }
            let cap = garrison_cap(level);
            province.garrison = Some(if province.fort_mothballed { 0 } else { cap });
        }
    }

    /// 每月 1 日，由所有者控制的启用要塞按城市 `garrison_growth = 0.05` 恢复，并从所有者人力扣人。
    /// `manpower.garrison_growth = 0.01` 未接入。
    pub(crate) fn reinforce_garrisons(&mut self) {
        let ids: Vec<u32> = self.active_provinces.iter().copied().collect();
        for id in ids {
            let level = self.province_fort_level(id);
            if level == 0 {
                continue;
            }
            let province = &self.provinces[&id];
            if province.fort_mothballed {
                continue;
            }
            let Some(owner) = province.owner.clone() else {
                continue;
            };
            if province.controller.as_deref() != Some(owner.as_str()) {
                continue;
            }
            let cap = garrison_cap(level);
            let current = province.garrison.unwrap_or(0);
            if current >= cap {
                continue;
            }
            let want = (u64::from(cap) * CITY_GARRISON_GROWTH_PERMILLE / 1_000)
                .min(u64::from(cap - current));
            if want == 0 {
                continue;
            }
            let available = self
                .countries
                .get(&owner)
                .and_then(|country| country.manpower.as_ref())
                .map(|pool| pool.available)
                .unwrap_or(0);
            let spend = want.min(available);
            if spend == 0 {
                continue;
            }
            self.countries
                .get_mut(&owner)
                .expect("要塞所有者存在")
                .manpower
                .as_mut()
                .expect("国家人力已初始化")
                .available -= spend;
            let garrison = self
                .provinces
                .get_mut(&id)
                .expect("要塞省份存在")
                .garrison
                .get_or_insert(0);
            *garrison = garrison.saturating_add(u32::try_from(spend).unwrap_or(u32::MAX));
        }
    }

    pub(crate) fn tick_sieges(&mut self, date: GameDate) {
        let provinces: Vec<u32> = self.active_provinces.iter().copied().collect();
        for province in provinces {
            self.tick_siege(province, date);
        }
    }

    fn tick_siege(&mut self, id: u32, date: GameDate) {
        if self.province_fort_level(id) == 0 {
            self.clear_siege(id);
            return;
        }
        if self.battle_in_province(id).is_some() {
            return;
        }
        let Some(controller) = self.provinces[&id].controller.clone() else {
            self.clear_siege(id);
            return;
        };
        let owner = self.provinces[&id].owner.clone().expect("要塞省份有所有者");
        let men = self.hostile_stopped_men(id, &controller);
        if men.is_empty() {
            self.clear_siege(id);
            return;
        }
        let garrison = self.provinces[&id].garrison.unwrap_or(0);
        let required = u64::from(garrison) * SIEGE_FORCE_NEEDED_MULTIPLIER;
        if garrison < SIEGE_GARRISON_SURRENDER {
            if let Some(side) = self.leading_side(&men, &owner, required) {
                self.finish_siege(id, &side, date);
            }
            return;
        }
        let recorded = self.provinces[&id].siege.clone();
        let Some(side) = self.leading_side(&men, &owner, required) else {
            let keep = recorded.as_ref().is_some_and(|progress| {
                men.iter()
                    .any(|(tag, count)| *count > 0 && self.same_war_side(tag, &progress.besieger))
            });
            if !keep {
                self.clear_siege(id);
            }
            return;
        };
        let continued = recorded
            .as_ref()
            .is_some_and(|progress| self.same_war_side(&progress.besieger, &side.lead));
        let previous = if continued {
            recorded.expect("同一交战方沿用围城")
        } else {
            SiegeProgress {
                besieger: side.lead.clone(),
                days: 0,
                status: 0,
                last_roll: None,
                last_result: None,
            }
        };
        let days = previous.days + 1;
        if days >= OCCUPATION_PHASE_DAYS {
            self.resolve_siege_phase(id, &side, previous.status, date);
            return;
        }
        self.provinces.get_mut(&id).expect("要塞省份存在").siege = Some(SiegeProgress {
            besieger: side.lead,
            days,
            status: previous.status,
            last_roll: previous.last_roll,
            last_result: previous.last_result,
        });
    }

    fn resolve_siege_phase(&mut self, id: u32, side: &SiegeSide, status: u32, date: GameDate) {
        let Some(outcome) = phase_table_outcome() else {
            if self.besieging_men(id) == 0 {
                self.clear_siege(id);
                return;
            }
            self.provinces.get_mut(&id).expect("要塞省份存在").siege = Some(SiegeProgress {
                besieger: side.lead.clone(),
                days: 0,
                status,
                last_roll: None,
                last_result: None,
            });
            return;
        };
        if outcome.surrender {
            self.finish_siege(id, side, date);
            return;
        }
        if outcome.disease {
            self.apply_siege_disease(id, &side.lead);
        }
        let next_status = status
            .saturating_add(outcome.status_bonus)
            .min(max_siege_status(self.province_fort_level(id)));
        if self.besieging_men(id) == 0 {
            self.clear_siege(id);
            return;
        }
        self.provinces.get_mut(&id).expect("要塞省份存在").siege = Some(SiegeProgress {
            besieger: side.lead.clone(),
            days: 0,
            status: next_status,
            last_roll: None,
            last_result: Some(outcome.result),
        });
    }

    fn apply_siege_disease(&mut self, province: u32, besieger: &str) {
        let ids: Vec<u32> = self
            .military
            .armies
            .iter()
            .filter(|(_, army)| {
                army.province == province
                    && army.movement.is_none()
                    && !army.is_retreating()
                    && !army.is_exiled()
                    && army.battle_id().is_none()
                    && self.same_war_side(&army.country, besieger)
            })
            .map(|(id, _)| *id)
            .collect();
        let mut empty = Vec::new();
        for id in ids {
            let army = self.military.armies.get_mut(&id).expect("围攻军队存在");
            army.regiments.retain_mut(|regiment| {
                regiment.strength = regiment
                    .strength
                    .saturating_sub(disease_loss(regiment.strength));
                regiment.strength > 0
            });
            if army.regiments.is_empty() {
                empty.push(id);
            }
        }
        for id in empty {
            self.military.armies.remove(&id);
        }
    }

    fn leading_side(
        &self,
        men: &BTreeMap<String, u64>,
        owner: &str,
        minimum: u64,
    ) -> Option<SiegeSide> {
        let tags: Vec<&str> = men.keys().map(String::as_str).collect();
        let mut grouped = vec![false; tags.len()];
        let mut best: Option<(u64, String, bool)> = None;
        for (index, tag) in tags.iter().enumerate() {
            if grouped[index] {
                continue;
            }
            let mut members = vec![*tag];
            grouped[index] = true;
            for (other_index, other) in tags.iter().enumerate().skip(index + 1) {
                if grouped[other_index] {
                    continue;
                }
                if self.same_war_side(tag, other) {
                    grouped[other_index] = true;
                    members.push(*other);
                }
            }
            let total: u64 = members.iter().map(|member| men[*member]).sum();
            if total < minimum {
                continue;
            }
            let lead = members
                .iter()
                .copied()
                .max_by(|left, right| men[*left].cmp(&men[*right]).then_with(|| right.cmp(left)))
                .expect("一侧至少有一个国家");
            let includes_owner = members
                .iter()
                .any(|member| *member == owner || self.same_war_side(member, owner));
            let replace = best.as_ref().is_none_or(|(current_men, current_lead, _)| {
                total > *current_men || (total == *current_men && lead < current_lead.as_str())
            });
            if replace {
                best = Some((total, lead.to_owned(), includes_owner));
            }
        }
        best.map(|(_, lead, includes_owner)| SiegeSide {
            lead,
            includes_owner,
        })
    }

    fn finish_siege(&mut self, id: u32, side: &SiegeSide, date: GameDate) {
        let owner = self.provinces[&id].owner.clone().expect("要塞省份有所有者");
        let next = if side.includes_owner {
            owner
        } else {
            side.lead.clone()
        };
        self.clear_siege(id);
        if self.provinces[&id].controller.as_deref() == Some(next.as_str()) {
            return;
        }
        self.provinces
            .get_mut(&id)
            .expect("要塞省份存在")
            .controller = Some(next.clone());
        self.control_changed = true;
        self.cancel_uncontrolled_recruitment(id, &next, date);
    }

    fn clear_siege(&mut self, id: u32) {
        if let Some(province) = self.provinces.get_mut(&id) {
            province.siege = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use crate::{
        Army, BuildingDefinition, Country, Development, GameDate, GameSpeed, LandConnection,
        ManpowerRules, MilitaryRules, Money, Province, ProvinceKind, Regiment, SavedGame,
        SiegeProgress, Simulation, UnitKind, World, WorldError,
    };

    use super::disease_loss;

    fn date() -> GameDate {
        GameDate::new(1444, 11, 11).unwrap()
    }

    fn rules() -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 60,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(1),
            recruitment_full_refund_progress: Money::ONE,
            land_maintenance_factor: Money::ZERO,
            monthly_reinforce: Money::ZERO,
            extra_land_reinforce_cost: Money::ZERO,
            supply_base: 6,
            base_land_forcelimit: Money::ZERO,
            development_land_forcelimit: Money::ONE,
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
            starting_army_size: Money::ZERO,
            starting_army_size_at_war: Money::ZERO,
            manpower: ManpowerRules {
                base_capacity: 1_000,
                per_development: 1_000,
                recovery_months: 10,
                minimum_monthly: 1,
                infantry_cost: 1_000,
                cavalry_cost: 1_000,
            },
            combat: crate::CombatRules::baseline(),
        }
    }

    fn land(owner: &str, tax: u32) -> Province {
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = Some(owner.to_owned());
        province.controller = Some(owner.to_owned());
        province.development = Development {
            tax: Some(tax),
            production: Some(1),
            manpower: Some(1),
        };
        province
    }

    fn fort_building() -> BuildingDefinition {
        BuildingDefinition {
            id: "fort_15th".to_owned(),
            cost: Money::from(200),
            months: 30,
            tax_modifier: Money::ZERO,
            land_forcelimit: Money::ZERO,
            local_regiment_cost: Money::ZERO,
            regiment_recruit_speed: Money::ZERO,
            fort_level: 2,
            replaces: None,
            constructible: false,
        }
    }

    fn world_of(
        countries: Vec<Country>,
        provinces: Vec<(u32, Province)>,
        buildings: Vec<BuildingDefinition>,
        links: Vec<LandConnection>,
    ) -> World {
        let ids: Vec<_> = provinces.iter().map(|(id, _)| *id).collect();
        World::new(
            countries,
            provinces.into_iter().collect(),
            ids,
            buildings,
            rules(),
            links,
        )
        .unwrap()
    }

    fn capital_world() -> World {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        world_of(
            vec![aaa, Country::new("BBB"), Country::new("CCC")],
            vec![(1, land("AAA", 12))],
            vec![],
            vec![],
        )
    }

    fn station(world: &mut World, id: u32, country: &str, province: u32, men: u32) {
        world.military.armies.insert(
            id,
            Army {
                id,
                country: country.to_owned(),
                province,
                regiments: vec![Regiment::fresh(UnitKind::Infantry, men)],
                movement: None,
                battle_id: None,
                retreating: false,
                retreat_shattered: Some(false),
                exiled: false,
                retreat_previous: None,
                retreat_return: None,
            },
        );
        world.military.next_army_id = world.military.next_army_id.max(id + 1);
    }

    fn station_regiments(world: &mut World, id: u32, country: &str, province: u32, count: usize) {
        world.military.armies.insert(
            id,
            Army {
                id,
                country: country.to_owned(),
                province,
                regiments: (0..count)
                    .map(|_| Regiment::fresh(UnitKind::Infantry, 1_000))
                    .collect(),
                movement: None,
                battle_id: None,
                retreating: false,
                retreat_shattered: Some(false),
                exiled: false,
                retreat_previous: None,
                retreat_return: None,
            },
        );
        world.military.next_army_id = world.military.next_army_id.max(id + 1);
    }

    fn tick(world: &mut World, days: u32) {
        let mut simulation = Simulation::new(date());
        let advanced = simulation.advance_days(world, days, None);
        assert_eq!(advanced.days, days);
    }

    #[test]
    fn disease_takes_a_share_of_maximum_strength() {
        assert_eq!(disease_loss(1_000), 50);
        assert_eq!(disease_loss(400), 50);
        assert_eq!(disease_loss(40), 40);

        let mut world = capital_world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 400);
        world
            .military
            .armies
            .get_mut(&1)
            .unwrap()
            .regiments
            .push(Regiment::fresh(UnitKind::Infantry, 40));
        world.apply_siege_disease(1, "BBB");
        let army = &world.armies()[&1];
        assert_eq!(army.regiments.len(), 1);
        assert_eq!(army.regiments[0].strength, 350);
    }

    #[test]
    fn unconnected_phase_table_does_not_change_control() {
        assert!(super::phase_table_outcome().is_none());
        let mut world = capital_world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station_regiments(&mut world, 1, "BBB", 1, 4);
        tick(&mut world, 60);
        let progress = world.province_siege(1).unwrap();
        assert_eq!(progress.days, 0);
        assert_eq!(progress.status, 0);
        assert!(progress.last_roll.is_none());
        assert!(progress.last_result.is_none());
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert_eq!(world.province(1).unwrap().owner.as_deref(), Some("AAA"));
        assert_eq!(world.armies()[&1].regiments[0].strength, 1_000);
    }

    #[test]
    fn capital_and_fort_building_use_distinct_garrisons() {
        let world = capital_world();
        assert_eq!(world.province_fort_level(1), 1);
        assert_eq!(world.province_garrison(1), Some(1_000));
        assert_eq!(world.siege_required_men(1), Some(3_000));

        let mut province = land("AAA", 3);
        province.buildings.insert("fort_15th".to_owned());
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let stacked = world_of(
            vec![aaa, Country::new("BBB")],
            vec![(1, province)],
            vec![fort_building()],
            vec![],
        );
        assert_eq!(stacked.province_fort_level(1), 2);
        assert_eq!(stacked.province_garrison(1), Some(1_000));
        assert_eq!(stacked.province_garrison_cap(1), 1_000);
        assert_eq!(stacked.siege_required_men(1), Some(3_000));

        let open = world_of(
            vec![Country::new("AAA"), Country::new("BBB")],
            vec![(1, land("AAA", 3))],
            vec![],
            vec![],
        );
        assert_eq!(open.province_fort_level(1), 0);
        assert_eq!(open.province_garrison(1), None);
        assert_eq!(open.siege_required_men(1), None);
    }

    #[test]
    fn short_force_pauses_and_replenishing_keeps_days() {
        let mut world = capital_world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 2_999);
        tick(&mut world, 10);
        assert!(world.province_siege(1).is_none());
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );

        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 3_000;
        tick(&mut world, 4);
        let progress = world.province_siege(1).unwrap().clone();
        assert_eq!(progress.besieger, "BBB");
        assert_eq!(progress.days, 4);
        assert_eq!(progress.status, 0);

        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 1_000;
        tick(&mut world, 3);
        assert_eq!(world.province_siege(1).unwrap().days, 4);
        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 3_000;
        tick(&mut world, 1);
        assert_eq!(world.province_siege(1).unwrap().days, 5);
    }

    #[test]
    fn leaving_retreating_and_changing_side_drop_progress() {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut world = world_of(
            vec![aaa, Country::new("BBB"), Country::new("CCC")],
            vec![(1, land("AAA", 4)), (2, land("BBB", 4))],
            vec![],
            vec![LandConnection {
                from: 1,
                to: 2,
                distance: 1,
            }],
        );
        world.declare_war("BBB", "AAA", date()).unwrap();
        world.declare_war("CCC", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 3_000);
        tick(&mut world, 6);
        assert_eq!(world.province_siege(1).unwrap().days, 6);

        // 每次 tick 都从开局日重开时钟，一天路程用开局日下单，下一天抵达。
        world.move_army("BBB", 1, 2, date()).unwrap();
        tick(&mut world, 1);
        assert!(world.province_siege(1).is_none());
        assert_eq!(world.armies()[&1].province, 2);

        world.move_army("BBB", 1, 1, date()).unwrap();
        tick(&mut world, 1);
        assert_eq!(world.province_siege(1).unwrap().days, 1);
        world.military.armies.get_mut(&1).unwrap().retreating = true;
        tick(&mut world, 1);
        assert!(world.province_siege(1).is_none());
        assert_eq!(world.armies()[&1].province, 1);

        world.military.armies.get_mut(&1).unwrap().retreating = false;
        tick(&mut world, 3);
        assert_eq!(world.province_siege(1).unwrap().days, 3);
        assert_eq!(world.province_siege(1).unwrap().status, 0);
        world.military.armies.remove(&1);
        station(&mut world, 2, "CCC", 1, 4_000);
        tick(&mut world, 1);
        let progress = world.province_siege(1).unwrap();
        assert_eq!(progress.besieger, "CCC");
        assert_eq!(progress.days, 1);
        assert_eq!(progress.status, 0);
    }

    #[test]
    fn battle_pauses_the_phase_and_relief_arrives_as_defender() {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut world = world_of(
            vec![aaa, Country::new("BBB")],
            vec![(1, land("AAA", 4)), (2, land("BBB", 4))],
            vec![],
            vec![LandConnection {
                from: 1,
                to: 2,
                distance: 1,
            }],
        );
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 3_000);
        // 一千对三千会在抵达当天被击溃，战斗结束后面的围城不再暂停。
        station(&mut world, 2, "AAA", 2, 3_000);
        tick(&mut world, 2);
        assert_eq!(world.province_siege(1).unwrap().days, 2);
        world.move_army("AAA", 2, 1, date()).unwrap();
        tick(&mut world, 1);
        let battle = world.battle_in_province(1).unwrap();
        assert_eq!(battle.side_armies(0), &[2]);
        assert_eq!(battle.side_armies(1), &[1]);
        assert_eq!(world.province_siege(1).unwrap().days, 2);
    }

    #[test]
    fn mothballed_fort_still_makes_the_besieger_the_attacker() {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut world = world_of(
            vec![aaa, Country::new("BBB")],
            vec![(1, land("AAA", 4)), (2, land("BBB", 4))],
            vec![],
            vec![LandConnection {
                from: 1,
                to: 2,
                distance: 1,
            }],
        );
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 3_000);
        station(&mut world, 2, "AAA", 2, 3_000);
        tick(&mut world, 2);
        world.set_fort_mothballed(1, true).unwrap();
        assert_eq!(world.province_garrison(1), Some(0));
        world.move_army("AAA", 2, 1, date()).unwrap();
        tick(&mut world, 1);
        let battle = world.battle_in_province(1).unwrap();
        assert_eq!(battle.side_armies(0), &[2]);
        assert_eq!(battle.side_armies(1), &[1]);
        assert_eq!(world.province_siege(1).unwrap().days, 2);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
    }

    #[test]
    fn garrison_under_one_hundred_still_needs_triple_men() {
        let mut world = capital_world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        world.provinces.get_mut(&1).unwrap().garrison = Some(99);
        station(&mut world, 1, "BBB", 1, 296);
        tick(&mut world, 1);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert!(world.province_siege(1).is_none());

        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 297;
        tick(&mut world, 1);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("BBB")
        );
        assert_eq!(world.province(1).unwrap().owner.as_deref(), Some("AAA"));
        assert!(world.province_siege(1).is_none());
    }

    #[test]
    fn mothballed_fort_falls_the_day_an_enemy_stops() {
        let mut world = capital_world();
        world.set_fort_mothballed(1, true).unwrap();
        assert_eq!(world.province_garrison(1), Some(0));
        assert_eq!(world.siege_required_men(1), Some(0));
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 1);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("BBB")
        );
        assert_eq!(world.province(1).unwrap().owner.as_deref(), Some("AAA"));
        assert!(world.province_siege(1).is_none());
        let war = world.war_between("BBB", "AAA").unwrap().id();
        let score = world.war_score("BBB", war).unwrap();
        assert!(score.occupation_milli > 0);
    }

    #[test]
    fn low_garrison_capture_changes_controller_only_and_peace_clears_progress() {
        let mut world = capital_world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        world.provinces.get_mut(&1).unwrap().garrison = Some(50);
        station_regiments(&mut world, 1, "BBB", 1, 1);
        tick(&mut world, 1);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("BBB")
        );
        assert_eq!(world.province(1).unwrap().owner.as_deref(), Some("AAA"));
        assert!(world.province_siege(1).is_none());
        let war = world.war_between("BBB", "AAA").unwrap().id();
        assert!(world.war_score("BBB", war).unwrap().occupation_milli > 0);

        let mut again = capital_world();
        again.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut again, 1, "BBB", 1, 3_000);
        tick(&mut again, 5);
        assert!(again.province_siege(1).is_some());
        again
            .make_peace("BBB", "AAA", &[], date().checked_add_days(5).unwrap())
            .unwrap();
        assert!(again.province_siege(1).is_none());
        assert_eq!(
            again.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
    }

    #[test]
    fn pause_speed_and_reload_keep_the_same_phase() {
        let prepare = |men: u32| {
            let mut world = capital_world();
            world.declare_war("BBB", "AAA", date()).unwrap();
            station(&mut world, 1, "BBB", 1, men);
            world
        };
        let mut paused = Simulation::new(date());
        let mut world = prepare(3_000);
        paused.set_paused(true);
        assert_eq!(paused.advance(&mut world, Duration::from_secs(30)), 0);
        assert!(world.province_siege(1).is_none());

        let mut normal = Simulation::new(date());
        normal.set_paused(false);
        normal.set_speed(GameSpeed::Normal);
        let mut fast = Simulation::new(date());
        fast.set_paused(false);
        fast.set_speed(GameSpeed::Octuple);
        let mut slow_world = prepare(3_000);
        let mut fast_world = prepare(3_000);
        assert_eq!(normal.advance(&mut slow_world, Duration::from_secs(8)), 8);
        assert_eq!(fast.advance(&mut fast_world, Duration::from_secs(1)), 8);
        assert_eq!(slow_world.province_siege(1), fast_world.province_siege(1));

        // 存档拒绝单团超过 1000 人。四团满编仍高于三倍驻军，且不超过补给上限。
        let mut left = prepare(3_000);
        let mut right = prepare(3_000);
        station_regiments(&mut left, 1, "BBB", 1, 4);
        station_regiments(&mut right, 1, "BBB", 1, 4);
        tick(&mut left, 30);
        tick(&mut right, 30);
        let rolled = left.province_siege(1).unwrap().clone();
        assert_eq!(rolled.days, 0);
        assert_eq!(rolled.status, 0);
        assert!(rolled.last_roll.is_none());
        assert_eq!(left.province(1).unwrap().controller.as_deref(), Some("AAA"));
        let saved = SavedGame::capture(
            &left,
            &Simulation::new(date().checked_add_days(30).unwrap()),
            "AAA",
        );
        let restored = serde_json::from_value::<SavedGame>(serde_json::to_value(&saved).unwrap())
            .unwrap()
            .restore(&left)
            .unwrap();
        let mut loaded = restored.world;
        let mut continued = right;
        tick(&mut loaded, 30);
        tick(&mut continued, 30);
        assert_eq!(loaded.province_siege(1), continued.province_siege(1));
        assert_eq!(
            loaded.province(1).unwrap().controller,
            continued.province(1).unwrap().controller
        );
    }

    #[test]
    fn old_saves_gain_a_full_garrison_and_reject_broken_sieges() {
        let world = capital_world();
        let saved = SavedGame::capture(&world, &Simulation::new(date()), "AAA");
        let mut legacy = serde_json::to_value(&saved).unwrap();
        let province = legacy["provinces"]["1"].as_object_mut().unwrap();
        province.remove("garrison");
        province.remove("fort_mothballed");
        province.remove("siege");
        let loaded = serde_json::from_value::<SavedGame>(legacy)
            .unwrap()
            .restore(&world)
            .unwrap();
        assert_eq!(loaded.world.province_garrison(1), Some(1_000));
        assert!(loaded.world.province_siege(1).is_none());

        let mut broken = land("AAA", 1);
        broken.siege = Some(SiegeProgress {
            besieger: "BBB".to_owned(),
            days: 30,
            status: 0,
            last_roll: None,
            last_result: None,
        });
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let error = World::new(
            [aaa, Country::new("BBB")],
            BTreeMap::from([(1, broken)]),
            [1],
            [],
            rules(),
            Vec::<LandConnection>::new(),
        );
        assert!(matches!(
            error,
            Err(WorldError::InvalidSiege { province: 1 })
        ));
    }

    #[test]
    fn garrison_regrows_from_manpower_only_while_enabled() {
        let mut world = capital_world();
        world.provinces.get_mut(&1).unwrap().garrison = Some(900);
        let before = world.country_manpower("AAA").unwrap().available;
        tick(&mut world, 20);
        assert_eq!(world.province_garrison(1), Some(950));
        assert_eq!(
            world.country_manpower("AAA").unwrap().available,
            before - 50
        );

        world.set_fort_mothballed(1, true).unwrap();
        world.set_fort_mothballed(1, false).unwrap();
        assert_eq!(world.province_garrison(1), Some(0));
        let capacity = world.country_manpower("AAA").unwrap().capacity;
        world
            .countries
            .get_mut("AAA")
            .unwrap()
            .manpower
            .as_mut()
            .unwrap()
            .available = capacity;
        let before = world.country_manpower("AAA").unwrap().available;
        tick(&mut world, 31);
        assert_eq!(world.province_garrison(1), Some(50));
        assert_eq!(
            world.country_manpower("AAA").unwrap().available,
            before - 50
        );
    }

    #[test]
    fn ai_holds_a_legal_siege_and_ignores_one_it_cannot_feed() {
        let mut world = capital_world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 3_000);
        let decision = world.direct_war_ai(Some("AAA"), date());
        let army = decision
            .iter()
            .find(|decision| decision.country == "BBB")
            .unwrap()
            .armies
            .first()
            .unwrap();
        assert_eq!(army.action, "stay");
        assert_eq!(army.reason, "正在围城，留在当地");

        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 2_000;
        let decision = world.direct_war_ai(Some("AAA"), date());
        let army = decision
            .iter()
            .find(|decision| decision.country == "BBB")
            .unwrap()
            .armies
            .first()
            .unwrap();
        assert_ne!(army.reason, "正在围城，留在当地");
    }

    #[test]
    fn ai_marches_onto_an_enemy_fort_it_can_besiege() {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut world = world_of(
            vec![aaa, Country::new("BBB")],
            vec![(1, land("AAA", 4)), (2, land("BBB", 4))],
            vec![],
            vec![LandConnection {
                from: 1,
                to: 2,
                distance: 1,
            }],
        );
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 2, 3_000);
        let decision = world.direct_war_ai(Some("AAA"), date());
        let army = &decision[0].armies[0];
        assert_eq!(army.action, "move");
        assert_eq!(army.destination, Some(1));
    }

    #[test]
    fn unfortified_occupation_still_finishes_without_a_siege() {
        let mut world = world_of(
            vec![Country::new("AAA"), Country::new("BBB")],
            vec![(1, land("AAA", 4))],
            vec![],
            vec![],
        );
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 30);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("BBB")
        );
        assert!(world.province_siege(1).is_none());
        assert!(world.province(1).unwrap().occupation.is_none());
    }
}
