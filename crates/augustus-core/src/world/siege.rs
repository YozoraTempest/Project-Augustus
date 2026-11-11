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
//! 推进围城所需的是满员团数，不是当前驻军人数的三倍。
//! 真实等级大于 0 且当前驻军为 0 时要 1 个满员团。否则团数是
//! 等级 × `SIEGE_FORCE_NEEDED_MULTIPLIER` × (1000 + 国家 `garrison_size` 千分比) / 1000，
//! 向零截断，再乘 `GARRISON_SIZE`。省份 `local_garrison_size` 不接。不足时暂停，不退已有天数。
//! 驻军少于 `SIEGE_GARRISON_SURRENDER` 时同样要达到这个人数才当天结束。
//! 围攻军全部离开、撤退或继续行军时清除围城。交战当天暂停。
//! 换一个交战方时进度作废，与无要塞占领使用同一条项目规则。破墙次数一并清掉。
//! 阶段天数在防御、围城能力和战术差都是 0 时等于 `DAYS_PER_SIEGE_PHASE`。这些修正还没有来源，调用处传 0。
//! 骰子 14 面，见 1.37.5 `eu4.exe` `0x14023FCE0`。
//! 顺序是投降、疾病、破墙，然后才按 `< 5`、`< SIEGE_WIN - 8`、`< SIEGE_WIN - 6`、`< SIEGE_WIN - 4`、`< SIEGE_WIN` 分档。
//! 达到 `SIEGE_WIN` 投降。自然 1 造成疾病，除非修正后已经达到投降。
//! 破墙分 = 骰子 + 过时要塞项 / 2 + 炮兵加成 / 2。两项都再向零截断。次数未满 `MAX_BREACH` 且总点数未到投降才破墙。
//! 过时要塞项在攻方为空或要塞停用时为 0。国家 `fort_level` 还没有来源，调用处传 0，所以这项目前是 0。
//! 炮兵加成按当前兵力计算。本项没有炮兵单位，兵力合计传 0，加成是 0。封锁和将领保持 0。
//! 破墙：次数 +1，状态 +2，驻军先降到整千人再乘 50。不改控制者，不改要塞等级。旧档位不扣驻军。
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
/// `MAX_BREACH`。达到这次数后不再破墙，已有次数仍计入总点数。
const MAX_BREACH: u32 = 3;
/// 破墙分门槛。结算里的字面量，不跟骰面绑在一起。
const BREACH_SCORE: i64 = 14;
/// 破墙状态加成。千分槽里的字面量 2000，显示值是 2。
const BREACH_STATUS_BONUS: u32 = 2;
/// 破墙对每个整千人扣掉的人数。先做 `人数 / 1000`，再乘这个数。
const BREACH_GARRISON_PERMILLE: u64 = 50;
/// 炮兵可用等级的缺省底数。修正为 0 时上限就是它。
const ARTILLERY_LEVEL_CAP: i64 = 5;
/// `artillery_level_modifier` 缺省因子。修正为 0 时因子是 1000。
const ARTILLERY_LEVEL_FACTOR: i64 = 1_000;
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

fn breach_bonus(breach: u32) -> u32 {
    breach.min(MAX_BREACH)
}

pub(super) fn breach_count_allowed(breaches: u32) -> bool {
    breaches <= MAX_BREACH
}

/// 过时要塞项。攻方侧为空，或要塞停用时为 0。
/// 否则是 max(0, (攻击方 `fort_level` − max(省份要塞, 2)) / 2)，向零截断。
/// 破墙分还会把这一项再除以 2。国家 `fort_level` 还没有来源，结算传入 0。
fn obsolete_fort_term(
    has_attacker: bool,
    mothballed: bool,
    attacker_fort: i32,
    province_fort: i32,
) -> i32 {
    if !has_attacker || mothballed {
        return 0;
    }
    let gap = i64::from(attacker_fort) - i64::from(province_fort.max(2));
    i32::try_from((gap / 2).max(0)).unwrap_or(i32::MAX)
}

/// 炮兵加成。`strength` 是当前炮兵兵力千分比之和，满编一团是 1000。
/// `cap` 与 `factor` 用原始千分比。定义对象为空时二进制原样使用，虚函数体还没接。
/// `per` 为 0 时比值改用 `cap * 1000`。比值大于 0 且整除结果是 0 时，加成抬到 1。这一步在取较小值之后。
fn artillery_bonus(fort_level: i32, strength: i64, cap: i64, factor: i64) -> i64 {
    let per = i64::from(fort_level) / 2 + 1;
    let scaled = strength * factor / 1_000;
    let ratio = if per == 0 { cap * 1_000 } else { scaled / per };
    let mut bonus = cap.min(ratio / 1_000);
    if ratio > 0 && bonus == 0 {
        bonus = 1;
    }
    bonus
}

/// 本项没有炮兵单位。兵力合计是 0，缺省上限和因子给出的加成也是 0。
fn active_artillery_bonus(fort_level: u32) -> i32 {
    let fort = i32::try_from(fort_level).unwrap_or(i32::MAX);
    let bonus = artillery_bonus(fort, 0, ARTILLERY_LEVEL_CAP, ARTILLERY_LEVEL_FACTOR);
    i32::try_from(bonus).unwrap_or(0)
}

/// 战术差。(攻方 − 守方) × 250 / 1000，再钳到 [-500, 500]。
/// 两侧基础值的来源还没命名。双方基础相同且修正为 0 时这一项是 0。
fn tactics_difference(attacker: i32, defender: i32) -> i32 {
    let term = (i64::from(attacker) - i64::from(defender)) * 250 / 1_000;
    i32::try_from(term.clamp(-500, 500)).unwrap_or(0)
}

/// 阶段天数。百分比 = max(100, 1000 + 防御 + 本地防御 − 围城能力 − 战术差)。
/// 天数 = `DAYS_PER_SIEGE_PHASE` × 百分比 / 1000。百分比下限 100 时是 3 天。
fn phase_length_days(
    defensiveness: i32,
    local_defensiveness: i32,
    siege_ability: i32,
    tactics: i32,
) -> u32 {
    let percent = 1_000 + i64::from(defensiveness) + i64::from(local_defensiveness)
        - i64::from(siege_ability)
        - i64::from(tactics);
    let percent = percent.max(100);
    let days = i64::from(OCCUPATION_PHASE_DAYS) * percent / 1_000;
    u32::try_from(days).unwrap_or(u32::MAX)
}

/// 防御、本地防御、围城能力和战术差都还没有来源，阶段长度保持 30。
fn active_phase_days() -> u32 {
    phase_length_days(0, 0, 0, tactics_difference(0, 0))
}

/// 修正后的点数决定结果。分档相对 `SIEGE_WIN`，不把 4/11/13/15 写成常数。
/// 封锁和将领保持 0，不进总点数。炮兵用完整加成，过时要塞项用完整项；破墙分只用二者的一半。
fn phase_outcome(
    roll: u32,
    status: u32,
    fort_level: u32,
    breaches: u32,
    obsolete: i32,
    artillery: i32,
) -> PhaseOutcome {
    let total = i64::from(roll)
        + i64::from(status)
        + i64::from(breach_bonus(breaches))
        + i64::from(artillery)
        + i64::from(obsolete)
        - i64::from(fort_level);
    if total >= i64::from(SIEGE_WIN) {
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
    let score = i64::from(roll) + i64::from(obsolete) / 2 + i64::from(artillery) / 2;
    if score >= BREACH_SCORE && breaches < MAX_BREACH {
        return PhaseOutcome {
            result: SiegeRoll::Breach,
            status_bonus: BREACH_STATUS_BONUS,
            disease: false,
            surrender: false,
        };
    }
    let total = u32::try_from(total).unwrap_or(0);
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

/// 破墙扣减。先把人数降到整千人，再乘 50。不足一千人不扣。
fn breach_garrison_loss(men: u32) -> u32 {
    let thousands = u64::from(men) / 1_000;
    let loss = thousands * BREACH_GARRISON_PERMILLE;
    u32::try_from(loss).unwrap_or(men)
}

fn next_status(status: u32, bonus: u32, fort_level: u32) -> u32 {
    status
        .saturating_add(bonus)
        .min(max_siege_status(fort_level))
}

/// 所需满员团数换成人数。驻军为 0 时是 1 个团。否则先按等级截断团数，再乘 `GARRISON_SIZE`。
fn required_men(level: u32, garrison: u32, permille: i32) -> u64 {
    let regiments = if garrison == 0 {
        1
    } else {
        let count = i128::from(level)
            * i128::from(SIEGE_FORCE_NEEDED_MULTIPLIER)
            * (1_000 + i128::from(permille))
            / 1_000;
        if count <= 0 { 0 } else { count }
    };
    u64::try_from(regiments)
        .unwrap_or(u64::MAX)
        .saturating_mul(u64::from(GARRISON_SIZE))
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

    /// 推进围城所需的人数。没有要塞时为空。
    /// 驻军为 0 时是 1 个满员团。否则按要塞等级换算团数，不按当前人数的三倍。
    pub fn siege_required_men(&self, id: u32) -> Option<u64> {
        let level = self.province_fort_level(id);
        if level == 0 {
            return None;
        }
        let garrison = self.province_garrison(id).unwrap_or(0);
        Some(required_men(
            level,
            garrison,
            province_garrison_permille(self, id),
        ))
    }

    /// 当前阶段长度。修正来源还没接，结果是 30。
    pub fn siege_phase_days(&self) -> u32 {
        active_phase_days()
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
        let required = self.siege_required_men(id).expect("要塞省份有所需兵力");
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
                breaches: 0,
                last_roll: None,
                last_result: None,
            }
        };
        let days = previous.days + 1;
        if days >= active_phase_days() {
            self.resolve_siege_phase(id, &side, previous.status, previous.breaches, date);
            return;
        }
        self.provinces.get_mut(&id).expect("要塞省份存在").siege = Some(SiegeProgress {
            besieger: side.lead,
            days,
            status: previous.status,
            breaches: previous.breaches,
            last_roll: previous.last_roll,
            last_result: previous.last_result,
        });
    }

    fn resolve_siege_phase(
        &mut self,
        id: u32,
        side: &SiegeSide,
        status: u32,
        breaches: u32,
        date: GameDate,
    ) {
        let fort_level = self.province_fort_level(id);
        let mothballed = self
            .province(id)
            .is_some_and(|province| province.fort_mothballed);
        let province_fort = i32::try_from(fort_level).unwrap_or(i32::MAX);
        // 能进结算说明攻方侧非空。国家 fort_level 仍传 0。
        let obsolete = obsolete_fort_term(true, mothballed, 0, province_fort);
        let artillery = active_artillery_bonus(fort_level);
        let roll = self.military.siege_random.roll_to(SIEGE_DICE_SIDES);
        let outcome = phase_outcome(roll, status, fort_level, breaches, obsolete, artillery);
        if outcome.surrender {
            self.finish_siege(id, side, date);
            return;
        }
        if outcome.disease {
            self.apply_siege_disease(id, &side.lead);
        }
        let next_breaches = if outcome.result == SiegeRoll::Breach {
            let cap = self.province_garrison_cap(id);
            let province = self.provinces.get_mut(&id).expect("要塞省份存在");
            let men = province.garrison.unwrap_or(0);
            let left = men.saturating_sub(breach_garrison_loss(men)).min(cap);
            province.garrison = Some(left);
            breaches.saturating_add(1).min(MAX_BREACH)
        } else {
            breaches
        };
        let next_status = next_status(status, outcome.status_bonus, fort_level);
        if self.besieging_men(id) == 0 {
            self.clear_siege(id);
            return;
        }
        self.provinces.get_mut(&id).expect("要塞省份存在").siege = Some(SiegeProgress {
            besieger: side.lead.clone(),
            days: 0,
            status: next_status,
            breaches: next_breaches,
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

    use super::{
        SIEGE_WIN, artillery_bonus, breach_garrison_loss, disease_loss, garrison_cap,
        max_siege_status, next_status, obsolete_fort_term, phase_length_days, phase_outcome,
        required_men, tactics_difference,
    };

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

        let outcome = |roll, status, fort| phase_outcome(roll, status, fort, 0, 0, 0);
        let quiet = outcome(4, 0, 0);
        assert_eq!(quiet.result, SiegeRoll::StatusQuo);
        assert_eq!(quiet.status_bonus, 0);
        let supplies = outcome(5, 0, 0);
        assert_eq!(supplies.result, SiegeRoll::SuppliesShortage);
        assert_eq!(supplies.status_bonus, 1);
        let still_supplies = outcome(SIEGE_WIN - 9, 0, 0);
        assert_eq!(still_supplies.result, SiegeRoll::SuppliesShortage);
        let food = outcome(SIEGE_WIN - 8, 0, 0);
        assert_eq!(food.result, SiegeRoll::FoodShortage);
        assert_eq!(food.status_bonus, 2);
        // 骰子 14 先被破墙拿走。缺水带用 13 加上状态，凑到 `SIEGE_WIN - 6`。
        let water = outcome(SIEGE_WIN - 7, 1, 0);
        assert_eq!(water.result, SiegeRoll::WaterShortage);
        assert_eq!(water.status_bonus, 3);
        let desert = outcome(SIEGE_WIN - 7, 3, 0);
        assert_eq!(desert.result, SiegeRoll::DefendersDesert);
        assert_eq!(desert.status_bonus, 2);
        assert!(!desert.surrender);
        let last_desert = outcome(SIEGE_WIN - 7, 6, 0);
        assert_eq!(last_desert.result, SiegeRoll::DefendersDesert);
        let win = outcome(SIEGE_WIN, 0, 0);
        assert!(win.surrender);
        assert_eq!(win.result, SiegeRoll::Surrender);

        let disease = outcome(1, 0, 1);
        assert_eq!(disease.result, SiegeRoll::Disease);
        assert!(disease.disease);
        assert!(!disease.surrender);
        let disease_loses_to_surrender = outcome(1, SIEGE_WIN, 1);
        assert!(disease_loses_to_surrender.surrender);
        assert!(!disease_loses_to_surrender.disease);

        // 1 级要塞把 14 点的总点数压成 13，但破墙分仍是 14。
        let pressed = outcome(14, 0, 1);
        assert_eq!(pressed.result, SiegeRoll::Breach);
        assert_eq!(pressed.status_bonus, 2);
        assert!(!pressed.surrender);
    }

    #[test]
    fn breach_score_orders_surrender_disease_then_breach() {
        let breach = phase_outcome(14, 0, 1, 0, 0, 0);
        assert_eq!(breach.result, SiegeRoll::Breach);
        assert_eq!(breach.status_bonus, 2);

        let capped = phase_outcome(14, 0, 1, 3, 0, 0);
        assert_eq!(capped.result, SiegeRoll::DefendersDesert);
        assert!(!capped.surrender);

        let surrendered = phase_outcome(14, 10, 0, 0, 0, 0);
        assert!(surrendered.surrender);
        assert_eq!(surrendered.result, SiegeRoll::Surrender);

        // 过时项 26 使破墙分达到 14，但总点数被要塞等级压到 20 以下。自然 1 仍是疾病。
        let disease = phase_outcome(1, 0, 10, 0, 26, 0);
        assert_eq!(disease.result, SiegeRoll::Disease);
        assert!(disease.disease);
        assert!(!disease.surrender);

        // 炮兵加成为 1 时，进破墙分的一半仍是 0。13 不破墙，完整加成把总点数推进缺水。
        let half = phase_outcome(13, 0, 0, 0, 0, 1);
        assert_eq!(half.result, SiegeRoll::WaterShortage);

        // 完整项 3 再除以 2 是 1，骰子 13 的破墙分是 14。
        let helped = phase_outcome(13, 0, 8, 0, 3, 0);
        assert_eq!(helped.result, SiegeRoll::Breach);
        assert_eq!(next_status(10, helped.status_bonus, 1), 11);
        assert_eq!(breach_garrison_loss(1_000), 50);
        assert_eq!(breach_garrison_loss(1_150), 50);
        assert_eq!(breach_garrison_loss(1_500), 50);
        assert_eq!(breach_garrison_loss(19), 0);
    }

    #[test]
    fn obsolete_fort_and_artillery_follow_the_binary_rounding() {
        assert_eq!(obsolete_fort_term(false, false, 8, 2), 0);
        assert_eq!(obsolete_fort_term(true, true, 8, 2), 0);
        assert_eq!(obsolete_fort_term(true, false, 8, 2), 3);
        assert_eq!(obsolete_fort_term(true, false, 2, 2), 0);
        assert_eq!(obsolete_fort_term(true, false, 3, 1), 0);
        assert_eq!(obsolete_fort_term(true, false, 0, 4), 0);

        assert_eq!(artillery_bonus(2, 0, 5, 1_000), 0);
        assert_eq!(artillery_bonus(2, 1_000, 5, 1_000), 1);
        assert_eq!(artillery_bonus(2, 4_000, 5, 1_000), 2);
        assert_eq!(artillery_bonus(1, 1_000, 5, 1_000), 1);
        assert_eq!(artillery_bonus(2, 1_000, 0, 1_000), 1);
        assert_eq!(artillery_bonus(0, 0, 5, 1_000), 0);
    }

    #[test]
    fn phase_length_floors_at_three_days_and_required_men_truncate() {
        assert_eq!(tactics_difference(0, 0), 0);
        assert_eq!(tactics_difference(4_000, 0), 500);
        assert_eq!(tactics_difference(0, 4_000), -500);
        assert_eq!(phase_length_days(0, 0, 0, 0), 30);
        assert_eq!(phase_length_days(1_000, 0, 0, 0), 60);
        assert_eq!(phase_length_days(0, 0, 2_000, 0), 3);
        assert_eq!(phase_length_days(0, 0, 0, 500), 15);

        assert_eq!(required_men(1, 1_000, 0), 3_000);
        assert_eq!(required_men(1, 1_150, 150), 3_000);
        assert_eq!(required_men(2, 2_300, 150), 6_000);
        assert_eq!(required_men(1, 0, 0), 1_000);
        assert_eq!(required_men(1, 1_000, 334), 4_000);
        assert_eq!(required_men(20, 20_000, 0), 60_000);
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
    fn three_breaches_cut_garrison_without_changing_controller() {
        let mut fort = land("AAA", 4);
        fort.buildings.insert("fort_high".to_owned());
        let mut owner = Country::new("AAA");
        owner.capital = Some(2);
        let building = BuildingDefinition {
            id: "fort_high".to_owned(),
            cost: Money::from(200),
            months: 30,
            tax_modifier: Money::ZERO,
            land_forcelimit: Money::ZERO,
            local_regiment_cost: Money::ZERO,
            regiment_recruit_speed: Money::ZERO,
            fort_level: 20,
            replaces: None,
            constructible: false,
        };
        let mut world = world_of(
            vec![owner, Country::new("BBB")],
            vec![(1, fort), (2, land("AAA", 3))],
            vec![building],
            vec![],
        );
        assert_eq!(world.province_fort_level(1), 20);
        assert_eq!(world.province_garrison(1), Some(20_000));
        assert_eq!(world.siege_required_men(1), Some(60_000));
        assert_eq!(world.siege_phase_days(), 30);
        world.declare_war("BBB", "AAA", date()).unwrap();
        // 单团不计补给损耗。疾病每次扣 50，几百个阶段仍高于所需兵力。
        station(&mut world, 1, "BBB", 1, 200_000);

        let mut breaches = 0u32;
        let mut status = 0u32;
        let mut garrison = 20_000u32;
        for _ in 0..800 {
            for _ in 0..30 {
                world.tick_siege(1, date());
            }
            let progress = world.province_siege(1).expect("围城还在");
            assert_eq!(
                world.province(1).unwrap().controller.as_deref(),
                Some("AAA")
            );
            assert_eq!(world.province(1).unwrap().owner.as_deref(), Some("AAA"));
            if progress.breaches != breaches {
                assert_eq!(progress.breaches, breaches + 1);
                assert_eq!(progress.last_result, Some(SiegeRoll::Breach));
                garrison -= garrison / 1_000 * 50;
                assert_eq!(world.province_garrison(1), Some(garrison));
                assert_eq!(
                    progress.status,
                    status.saturating_add(2).min(max_siege_status(20))
                );
                breaches = progress.breaches;
            }
            status = progress.status;
            if breaches == 3 {
                break;
            }
        }
        assert_eq!(breaches, 3);
        assert_eq!(garrison, 17_150);
        let frozen = garrison;
        for _ in 0..40 {
            for _ in 0..30 {
                world.tick_siege(1, date());
            }
        }
        let progress = world.province_siege(1).expect("满三次后围城继续");
        assert_eq!(progress.breaches, 3);
        assert_eq!(world.province_garrison(1), Some(frozen));
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
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
        assert_eq!(stacked.siege_required_men(1), Some(6_000));

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
        assert_eq!(world.siege_required_men(1), Some(3_000));
        assert_eq!(world.province_garrison(2), Some(2_300));
        assert_eq!(world.province_garrison_cap(2), 2_300);
        assert_eq!(world.siege_required_men(2), Some(6_000));
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
        assert_eq!(world.siege_required_men(1), Some(3_000));
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
        assert_eq!(world.siege_required_men(1), Some(1_000));
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
        assert_eq!(world.siege_required_men(1), Some(3_000));
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
    fn garrison_under_one_hundred_needs_the_fort_level_requirement() {
        let mut world = capital_world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        world.provinces.get_mut(&1).unwrap().garrison = Some(99);
        // 1 级、修正 0 仍要 3 个满员团，不是 99 的三倍。
        station(&mut world, 1, "BBB", 1, 2_999);
        tick(&mut world, 1);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert!(world.province_siege(1).is_none());

        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 3_000;
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
        assert_eq!(world.siege_required_men(1), Some(1_000));
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
        station_regiments(&mut world, 1, "BBB", 1, 3);
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

        let mut legacy_siege = serde_json::to_value(&saved).unwrap();
        legacy_siege["provinces"]["1"]["siege"] = serde_json::json!({
            "besieger": "BBB",
            "days": 4,
            "status": 2
        });
        let loaded = serde_json::from_value::<SavedGame>(legacy_siege)
            .unwrap()
            .restore(&world)
            .unwrap();
        let progress = loaded.world.province_siege(1).unwrap();
        assert_eq!(progress.breaches, 0);
        assert_eq!(progress.days, 4);
        assert_eq!(progress.status, 2);

        let mut broken = land("AAA", 1);
        broken.siege = Some(SiegeProgress {
            besieger: "BBB".to_owned(),
            days: 30,
            status: 0,
            breaches: 0,
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

        let mut over = land("AAA", 1);
        over.siege = Some(SiegeProgress {
            besieger: "BBB".to_owned(),
            days: 1,
            status: 0,
            breaches: 4,
            last_roll: None,
            last_result: None,
        });
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let error = World::new(
            [aaa, Country::new("BBB")],
            BTreeMap::from([(1, over)]),
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
            breaches: 0,
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
        assert_eq!(army.reason, "围攻省份 1");

        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 2_000;
        let decision = world.direct_war_ai(Some("AAA"), date());
        let army = decision
            .iter()
            .find(|decision| decision.country == "BBB")
            .unwrap()
            .armies
            .first()
            .unwrap();
        assert_ne!(army.reason, "围攻省份 1");
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
