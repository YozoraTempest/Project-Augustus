// SPDX-License-Identifier: GPL-3.0-only

//! 要塞围城与驻军。
//!
//! `FORT_LEVEL` 取首都 `capital_city.fort_level = 1` 和建筑 `fort_level` 的较大值。
//! 围城惩罚和状态上限用这个较大值。驻军条数是另一条规则：首都的 1 与各建筑 `fort_level` 相加。
//! 人数 = 条数 × `GARRISON_SIZE` × (1000 + 国家 `garrison_size` 千分比) / 1000，向零截断。
//! 千分比只来自开局奇观的 `country_modifiers`，按奇观所在省份的当前所有者相加。
//! 同一修正名的多处来源在引擎写入时相加。1444 只有马里恩堡一处实测，加法按这条写入规则实现。
//! 理念、政体改革、特权、事件和省份 `local_garrison_size` 不接：内容侧没有这些来源，1444 也不生效。
//! 奇观易主后上限按新所有者重算，已经在营的人数不立刻下调，只在读档时截到当时上限。
//! 能推进围城需要 `SIEGE_FORCE_NEEDED_MULTIPLIER` 倍的当前驻军。不足时暂停，不退已有天数。
//! 驻军少于 `SIEGE_GARRISON_SURRENDER` 时同样要满三倍才结束。驻军为 0 时三倍仍是 0，敌军停驻当天结束。
//! 围攻军全部离开、撤退或继续行军时清除围城。交战当天暂停。
//! 换一个交战方时进度作废，与无要塞占领使用同一条项目规则。
//! 阶段长度为 `DAYS_PER_SIEGE_PHASE`。骰子 14 面，见 1.37.5 `eu4.exe` `0x14023FCE0`。
//! 修正后的点数按 `< 5`、`< SIEGE_WIN - 8`、`< SIEGE_WIN - 6`、`< SIEGE_WIN - 4`、`< SIEGE_WIN` 分档。
//! 达到 `SIEGE_WIN` 投降。自然 1 造成疾病，除非修正后已经达到投降。
//! 炮兵、封锁、将领保持 0。破墙结果留在接口，本项不实现。
//! 疾病按 `SIEGE_DISEASE_IMPACT` 乘团的最大兵力 `REGIMENT_STRENGTH` 扣除。
//! 停用会立刻清空驻军，但不取消攻守反转。
//! 月度恢复以驻军上限为基数：上限 ×（城市 50‰ + 基础人力每点 10‰）/ 1000，向零截断后夹到上限。
//! 满员增量为 0，不扣人力池。围城进行中本月不恢复。
//! 启用且控制者是所有者才恢复，这是产品规则。
//! `expanded_infrastructure`、叛乱者倍率和国家 `global_garrison_growth` 没有数据来源，不接入。
//! 围城只改变控制者。状态上限是 `SIEGE_MEMORY + trunc(等级 * SIEGE_MEMORY_SCALED)`，向零截断。

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
/// 城市静态修正 `garrison_growth`，脚本 0.05，按千分位加一次。
const CITY_GARRISON_GROWTH_PERMILLE: u64 = 50;
/// 基础人力发展度每点的 `garrison_growth` 千分位。
const MANPOWER_GARRISON_GROWTH_PERMILLE: u64 = 10;
/// 围城骰面数。1.37.5 在 `0x14023FCE0` 对 14 取余后加 1。
const SIEGE_DICE_SIDES: u32 = 14;

struct SiegeSide {
    lead: String,
    includes_owner: bool,
}

/// 围城等级和驻军条数分开算。等级取较大值，条数相加。
pub(super) struct FortSources {
    pub level: u32,
    pub regiments: u32,
}

struct PhaseOutcome {
    result: SiegeRoll,
    status_bonus: u32,
    disease: bool,
    surrender: bool,
}

pub(super) fn fort_sources(
    province: &Province,
    id: u32,
    countries: &BTreeMap<String, Country>,
    buildings: &BTreeMap<String, BuildingDefinition>,
) -> FortSources {
    if province.kind != ProvinceKind::Land {
        return FortSources {
            level: 0,
            regiments: 0,
        };
    }
    let mut level = 0;
    let mut regiments: u32 = 0;
    if let Some(owner) = province.owner.as_deref()
        && countries.get(owner).and_then(|country| country.capital) == Some(id)
    {
        level = 1;
        regiments = 1;
    }
    for building in &province.buildings {
        if let Some(definition) = buildings.get(building) {
            level = level.max(definition.fort_level);
            regiments = regiments.saturating_add(definition.fort_level);
        }
    }
    FortSources { level, regiments }
}

/// 人数 = 条数 × `GARRISON_SIZE` × (1000 + 千分比) / 1000，向零截断。
/// 千分比为负且乘积小于 0 时人数为 0。
pub(super) fn garrison_cap(regiments: u32, permille: i32) -> u32 {
    let men =
        i128::from(GARRISON_SIZE) * i128::from(regiments) * (1_000 + i128::from(permille)) / 1_000;
    if men <= 0 {
        0
    } else {
        u32::try_from(men).unwrap_or(u32::MAX)
    }
}

/// 同一所有者名下的奇观千分比相加。奇观省份没有所有者时不计入。
pub(super) fn garrison_permille_by_owner(
    provinces: &BTreeMap<u32, Province>,
    monuments: &BTreeMap<u32, i32>,
) -> BTreeMap<String, i32> {
    let mut totals: BTreeMap<String, i32> = BTreeMap::new();
    for (province_id, permille) in monuments {
        let Some(owner) = provinces
            .get(province_id)
            .and_then(|province| province.owner.clone())
        else {
            continue;
        };
        let total = totals.entry(owner).or_insert(0);
        *total = total.saturating_add(*permille);
    }
    totals
}

fn province_garrison_permille(world: &World, id: u32) -> i32 {
    let Some(owner) = world
        .province(id)
        .and_then(|province| province.owner.as_deref())
    else {
        return 0;
    };
    garrison_permille_by_owner(&world.provinces, &world.monument_garrison_permille)
        .get(owner)
        .copied()
        .unwrap_or(0)
}

/// `SIEGE_MEMORY + trunc(SIEGE_MEMORY_SCALED * level)`。向零截断，1 级是 11。
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

/// 修正后的点数决定结果。分档相对 `SIEGE_WIN`，不把 4/11/13/15 写成常数。
/// 破墙在原版里会抢走一部分高点，本项不实现，这些点数仍走下面的分档。
fn phase_outcome(roll: u32, status: u32, fort_level: u32) -> PhaseOutcome {
    let total = roll
        .saturating_add(status)
        .saturating_add(reserved_siege_bonus())
        .saturating_sub(fort_level);
    if total >= SIEGE_WIN {
        return PhaseOutcome {
            result: SiegeRoll::Surrender,
            status_bonus: 0,
            disease: false,
            surrender: true,
        };
    }
    if roll == 1 {
        return PhaseOutcome {
            result: SiegeRoll::Disease,
            status_bonus: 0,
            disease: true,
            surrender: false,
        };
    }
    let (result, status_bonus) = if total < 5 {
        (SiegeRoll::StatusQuo, 0)
    } else if total < SIEGE_WIN.saturating_sub(8) {
        (SiegeRoll::SuppliesShortage, SIEGE_BONUS_SUPPLIES_SHORTAGE)
    } else if total < SIEGE_WIN.saturating_sub(6) {
        (SiegeRoll::FoodShortage, SIEGE_BONUS_FOOD_SHORTAGE)
    } else if total < SIEGE_WIN.saturating_sub(4) {
        (SiegeRoll::WaterShortage, SIEGE_BONUS_WATER_SHORTAGE)
    } else {
        (SiegeRoll::DefendersDesert, SIEGE_BONUS_DEFENDERS_DESERT)
    };
    PhaseOutcome {
        result,
        status_bonus,
        disease: false,
        surrender: false,
    }
}

/// 疾病扣除的是最大兵力的百分之五，不是当前兵力。不足这一数额时扣到 0。
fn disease_loss(strength: u32) -> u32 {
    let loss = u64::from(REGIMENT_STRENGTH) * SIEGE_DISEASE_IMPACT_PERMILLE / 1_000;
    let loss = u32::try_from(loss).unwrap_or(strength);
    strength.min(loss)
}

impl World {
    fn fort_sources_at(&self, id: u32) -> FortSources {
        let Some(province) = self.province(id) else {
            return FortSources {
                level: 0,
                regiments: 0,
            };
        };
        fort_sources(province, id, &self.countries, &self.buildings)
    }

    pub fn province_fort_level(&self, id: u32) -> u32 {
        self.fort_sources_at(id).level
    }

    pub fn province_garrison(&self, id: u32) -> Option<u32> {
        self.province(id).and_then(|province| province.garrison)
    }

    pub fn province_garrison_cap(&self, id: u32) -> u32 {
        garrison_cap(
            self.fort_sources_at(id).regiments,
            province_garrison_permille(self, id),
        )
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
            let sources = self.fort_sources_at(id);
            if sources.level == 0 {
                continue;
            }
            let permille = province_garrison_permille(self, id);
            let province = self.provinces.get_mut(&id).expect("要塞省份存在");
            if province.garrison.is_some() {
                continue;
            }
            let cap = garrison_cap(sources.regiments, permille);
            province.garrison = Some(if province.fort_mothballed { 0 } else { cap });
        }
    }

    /// 每月 1 日恢复驻军。增量 = 上限 × 千分位 / 1000，向零截断，再夹到上限。
    /// 千分位是城市 `garrison_growth` 50，加上基础人力发展度每点 10。人力字段缺失时不加该项。
    /// 基数是上限，满员增量为 0，不从人力池扣人。`siege` 存在时本月不恢复。
    /// 控制者是所有者且未停用才恢复。这是产品规则，不是已证实的原版对应关系。
    /// 未接入：`expanded_infrastructure` 没有扩建次数，叛乱者控制没有状态，
    /// 国家 `global_garrison_growth` 没有修正来源。
    pub(crate) fn reinforce_garrisons(&mut self) {
        let ids: Vec<u32> = self.active_provinces.iter().copied().collect();
        for id in ids {
            let sources = self.fort_sources_at(id);
            if sources.level == 0 {
                continue;
            }
            let size_permille = province_garrison_permille(self, id);
            let province = &self.provinces[&id];
            if province.fort_mothballed || province.siege.is_some() {
                continue;
            }
            if province.owner.is_none() || province.controller != province.owner {
                continue;
            }
            let mut modifier = CITY_GARRISON_GROWTH_PERMILLE;
            if let Some(manpower) = province.development.manpower {
                modifier += u64::from(manpower) * MANPOWER_GARRISON_GROWTH_PERMILLE;
            }
            let cap = garrison_cap(sources.regiments, size_permille);
            let current = province.garrison.unwrap_or(0);
            let room = u64::from(cap.saturating_sub(current));
            // 人力发展度是 u32，与上限相乘后可能超过 u64。
            let gain = (u128::from(cap) * u128::from(modifier) / 1_000).min(u128::from(room));
            if gain == 0 {
                continue;
            }
            let garrison = self
                .provinces
                .get_mut(&id)
                .expect("要塞省份存在")
                .garrison
                .get_or_insert(0);
            *garrison = garrison.saturating_add(u32::try_from(gain).unwrap_or(u32::MAX));
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
        let fort_level = self.province_fort_level(id);
        let roll = self.military.siege_random.roll_to(SIEGE_DICE_SIDES);
        let outcome = phase_outcome(roll, status, fort_level);
        if outcome.surrender {
            self.finish_siege(id, side, date);
            return;
        }
        if outcome.disease {
            self.apply_siege_disease(id, &side.lead);
        }
        let next_status = status
            .saturating_add(outcome.status_bonus)
            .min(max_siege_status(fort_level));
        if self.besieging_men(id) == 0 {
            self.clear_siege(id);
            return;
        }
        self.provinces.get_mut(&id).expect("要塞省份存在").siege = Some(SiegeProgress {
            besieger: side.lead.clone(),
            days: 0,
            status: next_status,
            last_roll: Some(u8::try_from(roll).unwrap_or(u8::MAX)),
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
        SiegeProgress, SiegeRoll, Simulation, UnitKind, World, WorldError,
    };

    use super::{SIEGE_WIN, disease_loss, garrison_cap, max_siege_status, phase_outcome};

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

    fn world_with_monuments(
        countries: Vec<Country>,
        provinces: Vec<(u32, Province)>,
        buildings: Vec<BuildingDefinition>,
        monuments: BTreeMap<u32, i32>,
    ) -> World {
        let ids: Vec<_> = provinces.iter().map(|(id, _)| *id).collect();
        World::with_garrison_monuments(
            countries,
            provinces.into_iter().collect(),
            ids,
            buildings,
            rules(),
            Vec::<LandConnection>::new(),
            monuments,
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

    fn owned_capital(manpower: u32, with_fort: bool) -> World {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut province = land("AAA", 4);
        province.development.manpower = Some(manpower);
        let mut buildings = Vec::new();
        if with_fort {
            province.buildings.insert("fort_15th".to_owned());
            buildings.push(fort_building());
        }
        world_of(
            vec![aaa, Country::new("BBB")],
            vec![(1, province)],
            buildings,
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
    fn phase_table_uses_win_relative_bands_and_natural_one_disease() {
        assert_eq!(max_siege_status(1), 11);
        assert_eq!(max_siege_status(2), 12);
        assert_eq!(max_siege_status(3), 12);

        let quiet = phase_outcome(4, 0, 0);
        assert_eq!(quiet.result, SiegeRoll::StatusQuo);
        assert_eq!(quiet.status_bonus, 0);
        let supplies = phase_outcome(5, 0, 0);
        assert_eq!(supplies.result, SiegeRoll::SuppliesShortage);
        assert_eq!(supplies.status_bonus, 1);
        let still_supplies = phase_outcome(SIEGE_WIN - 9, 0, 0);
        assert_eq!(still_supplies.result, SiegeRoll::SuppliesShortage);
        let food = phase_outcome(SIEGE_WIN - 8, 0, 0);
        assert_eq!(food.result, SiegeRoll::FoodShortage);
        assert_eq!(food.status_bonus, 2);
        let water = phase_outcome(SIEGE_WIN - 6, 0, 0);
        assert_eq!(water.result, SiegeRoll::WaterShortage);
        assert_eq!(water.status_bonus, 3);
        let desert = phase_outcome(SIEGE_WIN - 4, 0, 0);
        assert_eq!(desert.result, SiegeRoll::DefendersDesert);
        assert_eq!(desert.status_bonus, 2);
        assert!(!desert.surrender);
        let last_desert = phase_outcome(SIEGE_WIN - 1, 0, 0);
        assert_eq!(last_desert.result, SiegeRoll::DefendersDesert);
        let win = phase_outcome(SIEGE_WIN, 0, 0);
        assert!(win.surrender);
        assert_eq!(win.result, SiegeRoll::Surrender);

        let disease = phase_outcome(1, 0, 1);
        assert_eq!(disease.result, SiegeRoll::Disease);
        assert!(disease.disease);
        assert!(!disease.surrender);
        let disease_loses_to_surrender = phase_outcome(1, SIEGE_WIN, 1);
        assert!(disease_loses_to_surrender.surrender);
        assert!(!disease_loses_to_surrender.disease);

        // 1 级要塞把 14 点压成 13，落在食物短缺，不是写死的 13。
        let pressed = phase_outcome(14, 0, 1);
        assert_eq!(pressed.result, SiegeRoll::FoodShortage);
    }

    #[test]
    fn full_garrison_siege_changes_controller_only() {
        let mut world = capital_world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        // 单团避免按团数计算的补给损耗。疾病只扣满编的 5%。
        station(&mut world, 1, "BBB", 1, 20_000);
        let mut captured = false;
        for _ in 0..200 {
            tick(&mut world, 30);
            if world.province(1).unwrap().controller.as_deref() == Some("BBB") {
                captured = true;
                break;
            }
        }
        assert!(captured);
        assert_eq!(world.province(1).unwrap().owner.as_deref(), Some("AAA"));
        assert!(world.province_siege(1).is_none());
        let war = world.war_between("BBB", "AAA").unwrap().id();
        assert!(world.war_score("BBB", war).unwrap().occupation_milli > 0);
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
        assert_eq!(stacked.province_garrison(1), Some(3_000));
        assert_eq!(stacked.province_garrison_cap(1), 3_000);
        assert_eq!(stacked.siege_required_men(1), Some(9_000));

        let mut fort = land("AAA", 3);
        fort.buildings.insert("fort_15th".to_owned());
        let mut owner = Country::new("AAA");
        owner.capital = Some(2);
        let building_only = world_of(
            vec![owner, Country::new("BBB")],
            vec![(1, fort), (2, land("AAA", 3))],
            vec![fort_building()],
            vec![],
        );
        assert_eq!(building_only.province_fort_level(1), 2);
        assert_eq!(building_only.province_garrison(1), Some(2_000));
        assert_eq!(building_only.province_garrison_cap(1), 2_000);
        assert_eq!(building_only.siege_required_men(1), Some(6_000));
        assert_eq!(building_only.province_garrison(2), Some(1_000));

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
    fn garrison_cap_uses_thousandths_and_truncates_toward_zero() {
        assert_eq!(garrison_cap(1, 0), 1_000);
        assert_eq!(garrison_cap(1, 150), 1_150);
        assert_eq!(garrison_cap(2, 150), 2_300);
        assert_eq!(garrison_cap(1, -1), 999);
        assert_eq!(garrison_cap(1, -1_000), 0);
        assert_eq!(garrison_cap(1, -1_001), 0);
        assert_eq!(garrison_cap(0, 150), 0);
        assert_eq!(garrison_cap(u32::MAX, i32::MAX), u32::MAX);
    }

    #[test]
    fn monument_country_garrison_scales_every_owned_fort() {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut bbb = Country::new("BBB");
        bbb.capital = Some(3);
        let mut fort = land("AAA", 3);
        fort.buildings.insert("fort_15th".to_owned());
        let mut other_fort = land("BBB", 3);
        other_fort.buildings.insert("fort_15th".to_owned());
        let world = world_with_monuments(
            vec![aaa, bbb],
            vec![
                (1, land("AAA", 4)),
                (2, fort),
                (3, land("BBB", 4)),
                (4, other_fort),
                (10, land("AAA", 1)),
            ],
            vec![fort_building()],
            BTreeMap::from([(10, 150)]),
        );
        assert_eq!(world.province_garrison(1), Some(1_150));
        assert_eq!(world.province_garrison_cap(1), 1_150);
        assert_eq!(world.siege_required_men(1), Some(3_450));
        assert_eq!(world.province_garrison(2), Some(2_300));
        assert_eq!(world.province_garrison_cap(2), 2_300);
        assert_eq!(world.siege_required_men(2), Some(6_900));
        assert_eq!(world.province_garrison(3), Some(1_000));
        assert_eq!(world.siege_required_men(3), Some(3_000));
        assert_eq!(world.province_garrison(4), Some(2_000));
        assert_eq!(world.siege_required_men(4), Some(6_000));
        assert_eq!(world.province_garrison(10), None);
    }

    #[test]
    fn monuments_of_one_owner_add_and_an_ownerless_one_does_not() {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut empty = Province::new(ProvinceKind::Land);
        empty.development.tax = Some(1);
        let world = world_with_monuments(
            vec![aaa],
            vec![
                (1, land("AAA", 4)),
                (10, land("AAA", 1)),
                (11, land("AAA", 1)),
                (12, empty),
            ],
            vec![],
            BTreeMap::from([(10, 150), (11, 100), (12, 80), (99, 500)]),
        );
        assert_eq!(world.province_garrison(1), Some(1_250));
        assert_eq!(world.province_garrison_cap(1), 1_250);
        assert_eq!(world.siege_required_men(1), Some(3_750));
    }

    #[test]
    fn garrison_above_the_modified_cap_is_clamped_when_the_world_is_built() {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut capital = land("AAA", 4);
        capital.garrison = Some(5_000);
        let mut bare = land("AAA", 1);
        bare.garrison = Some(10);
        let content = world_with_monuments(
            vec![aaa],
            vec![(1, capital), (10, land("AAA", 1))],
            vec![],
            BTreeMap::from([(10, 150)]),
        );
        assert_eq!(content.province_garrison(1), Some(1_150));

        let error = World::with_garrison_monuments(
            [Country::new("AAA")],
            BTreeMap::from([(1, bare)]),
            [1],
            [],
            rules(),
            Vec::<LandConnection>::new(),
            BTreeMap::from([(10, 150)]),
        );
        assert!(matches!(
            error,
            Err(WorldError::InvalidSiege { province: 1 })
        ));

        let saved = SavedGame::capture(&content, &Simulation::new(date()), "AAA");
        let mut over = serde_json::to_value(&saved).unwrap();
        over["provinces"]["1"]["garrison"] = serde_json::json!(5_000);
        let loaded = serde_json::from_value::<SavedGame>(over)
            .unwrap()
            .restore(&content)
            .unwrap();
        assert_eq!(loaded.world.province_garrison(1), Some(1_150));

        let mut missing = serde_json::to_value(&saved).unwrap();
        missing["provinces"]["1"]
            .as_object_mut()
            .unwrap()
            .remove("garrison");
        let loaded = serde_json::from_value::<SavedGame>(missing)
            .unwrap()
            .restore(&content)
            .unwrap();
        assert_eq!(loaded.world.province_garrison(1), Some(1_150));
    }

    #[test]
    fn negative_modifier_floors_men_at_zero() {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let world = world_with_monuments(
            vec![aaa],
            vec![(1, land("AAA", 4)), (10, land("AAA", 1))],
            vec![],
            BTreeMap::from([(10, -1_500)]),
        );
        assert_eq!(world.province_garrison(1), Some(0));
        assert_eq!(world.province_garrison_cap(1), 0);
        assert_eq!(world.siege_required_men(1), Some(0));
    }

    #[test]
    fn garrison_bonus_follows_the_monument_owner_without_shrinking_men() {
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut bbb = Country::new("BBB");
        bbb.capital = Some(2);
        let mut world = world_with_monuments(
            vec![aaa, bbb],
            vec![
                (1, land("AAA", 4)),
                (2, land("BBB", 4)),
                (10, land("AAA", 1)),
            ],
            vec![],
            BTreeMap::from([(10, 150)]),
        );
        assert_eq!(world.province_garrison_cap(1), 1_150);
        assert_eq!(world.province_garrison_cap(2), 1_000);

        world.provinces.get_mut(&10).unwrap().owner = Some("BBB".to_owned());
        assert_eq!(world.province_garrison_cap(1), 1_000);
        assert_eq!(world.province_garrison(1), Some(1_150));
        assert_eq!(world.siege_required_men(1), Some(3_450));
        assert_eq!(world.province_garrison_cap(2), 1_150);
        assert_eq!(world.province_garrison(2), Some(1_000));
        assert_eq!(world.siege_required_men(2), Some(3_000));
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
        assert!(
            rolled
                .last_roll
                .is_some_and(|roll| (1..=14).contains(&roll))
        );
        assert!(rolled.last_result.is_some());
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
    fn garrison_regrows_from_cap_without_spending_manpower() {
        // 上限 1000、基础人力 3：城市 50‰ + 人力 30‰，每月 +80。
        let mut small = owned_capital(3, false);
        assert_eq!(small.province_garrison_cap(1), 1_000);
        small.provinces.get_mut(&1).unwrap().garrison = Some(200);
        let before = small.country_manpower("AAA").unwrap().available;
        tick(&mut small, 20);
        assert_eq!(small.province_garrison(1), Some(280));
        assert_eq!(small.country_manpower("AAA").unwrap().available, before);

        small.provinces.get_mut(&1).unwrap().garrison = Some(960);
        tick(&mut small, 20);
        assert_eq!(small.province_garrison(1), Some(1_000));
        assert_eq!(small.country_manpower("AAA").unwrap().available, before);

        small.provinces.get_mut(&1).unwrap().garrison = Some(1_000);
        tick(&mut small, 20);
        assert_eq!(small.province_garrison(1), Some(1_000));
        assert_eq!(small.country_manpower("AAA").unwrap().available, before);

        // 上限 3000、基础人力 6：50‰ + 60‰，每月 +330。维也纳样本。
        let mut vienna = owned_capital(6, true);
        assert_eq!(vienna.province_garrison_cap(1), 3_000);
        vienna.provinces.get_mut(&1).unwrap().garrison = Some(1_000);
        let before = vienna.country_manpower("AAA").unwrap().available;
        tick(&mut vienna, 20);
        assert_eq!(vienna.province_garrison(1), Some(1_330));
        assert_eq!(vienna.country_manpower("AAA").unwrap().available, before);

        vienna.provinces.get_mut(&1).unwrap().garrison = Some(3_000);
        tick(&mut vienna, 20);
        assert_eq!(vienna.province_garrison(1), Some(3_000));
        assert_eq!(vienna.country_manpower("AAA").unwrap().available, before);

        // 11 月 30 日置围城，下一天月初仍不恢复。随后的围城结算可能清掉进度。
        let mut besieged = owned_capital(3, false);
        besieged.provinces.get_mut(&1).unwrap().garrison = Some(200);
        besieged.provinces.get_mut(&1).unwrap().siege = Some(SiegeProgress {
            besieger: "BBB".to_owned(),
            days: 1,
            status: 0,
            last_roll: None,
            last_result: None,
        });
        let before = besieged.country_manpower("AAA").unwrap().available;
        let mut simulation = Simulation::new(GameDate::new(1444, 11, 30).unwrap());
        assert_eq!(simulation.advance_days(&mut besieged, 1, None).days, 1);
        assert_eq!(besieged.province_garrison(1), Some(200));
        assert_eq!(besieged.country_manpower("AAA").unwrap().available, before);

        // 停用、控制者不是所有者：产品规则，不恢复。
        let mut mothballed = owned_capital(3, false);
        mothballed.set_fort_mothballed(1, true).unwrap();
        tick(&mut mothballed, 20);
        assert_eq!(mothballed.province_garrison(1), Some(0));

        let mut occupied = owned_capital(3, false);
        occupied.provinces.get_mut(&1).unwrap().garrison = Some(200);
        occupied.provinces.get_mut(&1).unwrap().controller = Some("BBB".to_owned());
        let before = occupied.country_manpower("AAA").unwrap().available;
        tick(&mut occupied, 20);
        assert_eq!(occupied.province_garrison(1), Some(200));
        assert_eq!(occupied.country_manpower("AAA").unwrap().available, before);

        // 人力字段缺失时只留城市 50‰。人力池为空也不扣、也不挡住恢复。
        let mut missing = owned_capital(3, false);
        missing.provinces.get_mut(&1).unwrap().development.manpower = None;
        missing.provinces.get_mut(&1).unwrap().garrison = Some(200);
        missing
            .countries
            .get_mut("AAA")
            .unwrap()
            .manpower
            .as_mut()
            .unwrap()
            .available = 0;
        missing.reinforce_garrisons();
        assert_eq!(missing.province_garrison(1), Some(250));
        assert_eq!(
            missing.countries["AAA"]
                .manpower
                .as_ref()
                .unwrap()
                .available,
            0
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
