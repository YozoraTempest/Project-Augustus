// SPDX-License-Identifier: GPL-3.0-only

//! 省份补给与月初损耗。同一省份的全部军队共用一个上限，按团数而不是当前兵力计算。

use std::collections::BTreeMap;

use crate::{Army, Development, GameDate, ProvinceKind};

use super::World;

/// 下一次月初结算时，这支军队所在省份的补给与预计减员。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupplyForecast {
    pub province: u32,
    pub on: GameDate,
    /// 结算日该省的补给上限。
    pub supply_limit: u32,
    /// 结算日预计留在该省的团数，包含其他军队。
    pub regiment_count: u32,
    /// 这支军队自己预计减少的人数。
    pub loss: u32,
}

impl World {
    /// 预览严格晚于今天的下一个 1 日。当天若已是 1 日，则看下个月。
    pub fn supply_forecast(&self, army_id: u32, date: GameDate) -> Option<SupplyForecast> {
        let on = next_supply_date(date)?;
        let army = self.armies().get(&army_id)?;
        let province = province_at(army, on);
        let supply_limit = self.land_supply_limit(province)?;
        let mut regiment_count = 0u32;
        for other in self.armies().values() {
            if province_at(other, on) == province {
                regiment_count = regiment_count.saturating_add(regiment_count_of(other));
            }
        }
        Some(SupplyForecast {
            province,
            on,
            supply_limit,
            regiment_count,
            loss: army_loss(army, regiment_count, supply_limit),
        })
    }

    /// 只在进入 1 日时调用。先完成当天抵达，再按抵达后的省份结算；不从人力池扣人。
    pub(crate) fn settle_attrition(&mut self, date: GameDate) {
        if date.day() != 1 {
            return;
        }
        let mut groups: BTreeMap<u32, (u32, Vec<u32>)> = BTreeMap::new();
        for (&id, army) in &self.military.armies {
            let entry = groups.entry(army.province).or_default();
            entry.0 = entry.0.saturating_add(regiment_count_of(army));
            entry.1.push(id);
        }
        let mut empty = Vec::new();
        for (province, (occupation, ids)) in groups {
            let Some(limit) = self.land_supply_limit(province) else {
                continue;
            };
            if occupation <= limit {
                continue;
            }
            for id in ids {
                let army = self.military.armies.get_mut(&id).expect("待结算的军队仍在");
                army.regiments.retain_mut(|regiment| {
                    let loss = regiment_loss(regiment.strength, occupation, limit);
                    regiment.strength = regiment.strength.saturating_sub(loss);
                    regiment.strength > 0
                });
                if army.regiments.is_empty() {
                    empty.push(id);
                }
            }
        }
        for id in empty {
            self.military.armies.remove(&id);
        }
    }

    fn land_supply_limit(&self, province: u32) -> Option<u32> {
        let province = self.province(province)?;
        (province.kind == ProvinceKind::Land).then(|| {
            supply_limit_value(
                self.military_rules.supply_base,
                province.terrain_supply,
                development_sum(province.development),
            )
        })
    }
}

/// 上限 = floor((基础 + 地形补给) × (1.25 + 0.02 × 发展度))。
fn supply_limit_value(base: u32, terrain: u32, development: u32) -> u32 {
    let weight = u128::from(base) + u128::from(terrain);
    let factor = 125 + u128::from(development) * 2;
    u32::try_from(weight.saturating_mul(factor) / 100).unwrap_or(u32::MAX)
}

/// 未填写的发展度不记入，也不改写成 0。
fn development_sum(development: Development) -> u32 {
    let total = u64::from(development.tax.unwrap_or(0))
        + u64::from(development.production.unwrap_or(0))
        + u64::from(development.manpower.unwrap_or(0));
    u32::try_from(total).unwrap_or(u32::MAX)
}

fn next_supply_date(date: GameDate) -> Option<GameDate> {
    let (year, month) = if date.month() == 12 {
        (date.year().checked_add(1)?, 1)
    } else {
        (date.year(), date.month() + 1)
    };
    GameDate::new(year, month, 1)
}

/// 与行军完成使用同一规则：抵达日当天已经算在目标省份。
fn province_at(army: &Army, date: GameDate) -> u32 {
    let mut province = army.province;
    let Some(movement) = &army.movement else {
        return province;
    };
    for leg in &movement.legs()[movement.leg_index()..] {
        if leg.arrives_on > date {
            break;
        }
        province = leg.to;
    }
    province
}

fn regiment_count_of(army: &Army) -> u32 {
    u32::try_from(army.regiments.len()).unwrap_or(u32::MAX)
}

fn army_loss(army: &Army, occupation: u32, limit: u32) -> u32 {
    army.regiments.iter().fold(0u32, |total, regiment| {
        total.saturating_add(regiment_loss(regiment.strength, occupation, limit))
    })
}

/// 超出上限时百分比为 min(5, 超出团数 × 10 / 上限)，每人损失按这个比例向下取整。
fn regiment_loss(strength: u32, occupation: u32, limit: u32) -> u32 {
    if occupation <= limit {
        return 0;
    }
    let strength = u128::from(strength);
    let excess = u128::from(occupation - limit);
    let limit = u128::from(limit);
    let loss = if limit == 0 || excess.saturating_mul(2) >= limit {
        strength * 5 / 100
    } else {
        strength * excess * 10 / (limit * 100)
    };
    u32::try_from(loss).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use crate::{
        Army, ArmyMovement, Country, Development, GameDate, ManpowerRules, MilitaryRules, Money,
        MovementLeg, Province, ProvinceKind, Regiment, SavedGame, Simulation, UnitKind, World,
        WorldError,
    };

    use super::supply_limit_value;

    fn date(year: i16, month: u8, day: u8) -> GameDate {
        GameDate::new(year, month, day).expect("日期有效")
    }

    fn rules(supply_base: u32, infantry_days: u32) -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(1),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: Money::ZERO,
            supply_base,
            base_land_forcelimit: Money::ZERO,
            development_land_forcelimit: Money::ZERO,
            land_forcelimit_extra_cost_factor: Money::ZERO,
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
            manpower: ManpowerRules {
                base_capacity: 20_000,
                per_development: 0,
                recovery_months: 10,
                minimum_monthly: 100,
                infantry_cost: 1_000,
                cavalry_cost: 1_000,
            },
        }
    }

    fn land(terrain: u32, tax: u32, production: Option<u32>, manpower: u32) -> Province {
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = Some("AAA".to_owned());
        province.controller = Some("AAA".to_owned());
        province.development = Development {
            tax: Some(tax),
            production,
            manpower: Some(manpower),
        };
        province.terrain_supply = terrain;
        province
    }

    fn sample(supply_base: u32, provinces: Vec<(u32, Province)>) -> World {
        let ids: Vec<_> = provinces.iter().map(|(id, _)| *id).collect();
        World::new(
            [Country::new("AAA")],
            provinces.into_iter().collect(),
            ids,
            Vec::<crate::BuildingDefinition>::new(),
            rules(supply_base, 20),
            Vec::<crate::LandConnection>::new(),
        )
        .expect("世界有效")
    }

    fn station(world: &mut World, id: u32, province: u32, strengths: &[u32]) {
        world.military.armies.insert(
            id,
            Army {
                id,
                country: "AAA".to_owned(),
                province,
                regiments: strengths
                    .iter()
                    .map(|&strength| Regiment {
                        kind: UnitKind::Infantry,
                        strength,
                    })
                    .collect(),
                movement: None,
            },
        );
        world.military.next_army_id = world.military.next_army_id.max(id + 1);
    }

    fn strengths(world: &World, id: u32) -> Vec<u32> {
        world.armies()[&id]
            .regiments
            .iter()
            .map(|regiment| regiment.strength)
            .collect()
    }

    fn advance(world: &mut World, start: GameDate, days: u32) -> Simulation {
        let mut simulation = Simulation::new(start);
        simulation.set_paused(false);
        simulation.advance(world, Duration::from_secs(u64::from(days)));
        simulation
    }

    #[test]
    fn supply_limit_uses_terrain_and_partial_development() {
        assert_eq!(supply_limit_value(6, 10, 15), 24);
        assert_eq!(supply_limit_value(6, 0, 3), 7);
    }

    #[test]
    fn zero_supply_base_is_rejected() {
        let result = World::new(
            [Country::new("AAA")],
            [(1, land(0, 1, Some(1), 1))].into(),
            [1],
            Vec::<crate::BuildingDefinition>::new(),
            rules(0, 20),
            Vec::<crate::LandConnection>::new(),
        );
        assert!(matches!(result, Err(WorldError::InvalidMilitaryRules)));
    }

    #[test]
    fn cap_leaves_nineteen_and_drops_twenty() {
        let mut world = sample(2, vec![(1, land(0, 0, Some(0), 0))]);
        station(&mut world, 1, 1, &[19, 20, 1_000, 1_000]);
        let manpower = world.country_manpower("AAA").expect("人力").available;
        world.settle_attrition(date(1444, 12, 1));
        assert_eq!(strengths(&world, 1), vec![19, 19, 950, 950]);
        assert_eq!(
            world.country_manpower("AAA").expect("人力").available,
            manpower
        );
        assert_eq!(
            world
                .supply_forecast(1, date(1444, 12, 1))
                .expect("预览")
                .on,
            date(1445, 1, 1)
        );
    }

    #[test]
    fn armies_share_one_pool_and_small_regiments_can_lose_nothing() {
        let mut world = sample(1, vec![(1, land(0, 0, Some(0), 0))]);
        station(&mut world, 1, 1, &[1_000]);
        world.settle_attrition(date(1444, 12, 1));
        assert_eq!(strengths(&world, 1), vec![1_000]);

        station(&mut world, 2, 1, &[500]);
        world.settle_attrition(date(1445, 1, 1));
        assert_eq!(strengths(&world, 1), vec![950]);
        assert_eq!(strengths(&world, 2), vec![475]);

        let mut wide = sample(4, vec![(1, land(0, 50, Some(0), 0))]);
        let mut men = vec![1_000; 11];
        men.push(10);
        station(&mut wide, 1, 1, &men);
        let forecast = wide.supply_forecast(1, date(1444, 11, 11)).expect("预览");
        assert_eq!(forecast.supply_limit, 9);
        assert_eq!(forecast.regiment_count, 12);
        assert_eq!(forecast.loss, 33 * 11);
        wide.settle_attrition(date(1444, 12, 1));
        let settled = strengths(&wide, 1);
        assert_eq!(&settled[..11], &[967; 11]);
        assert_eq!(settled[11], 10);
    }

    #[test]
    fn arrival_on_the_first_is_counted_at_the_destination() {
        let mut world = sample(
            2,
            vec![(1, land(1, 0, Some(0), 0)), (2, land(10, 0, Some(0), 0))],
        );
        station(&mut world, 1, 1, &[1_000, 1_000]);
        station(&mut world, 2, 1, &[1_000, 1_000]);
        station(&mut world, 3, 2, &[1_000; 14]);
        let arrives = date(1444, 12, 1);
        world
            .military
            .armies
            .get_mut(&2)
            .expect("行军军队")
            .movement = Some(ArmyMovement {
            legs: vec![MovementLeg {
                from: 1,
                to: 2,
                started_on: date(1444, 11, 11),
                arrives_on: arrives,
            }],
            leg_index: 0,
        });

        let stayer = world.supply_forecast(1, date(1444, 11, 11)).expect("预览");
        let mover = world.supply_forecast(2, date(1444, 11, 11)).expect("预览");
        assert_eq!(stayer.province, 1);
        assert_eq!(stayer.regiment_count, 2);
        assert_eq!(stayer.loss, 0);
        assert_eq!(mover.on, arrives);
        assert_eq!(mover.province, 2);
        assert_eq!(mover.supply_limit, 15);
        assert_eq!(mover.regiment_count, 16);
        assert_eq!(mover.loss, 12);

        advance(&mut world, date(1444, 11, 11), 20);
        assert_eq!(world.armies()[&2].province, 2);
        assert_eq!(strengths(&world, 1), vec![1_000, 1_000]);
        assert_eq!(strengths(&world, 2), vec![994, 994]);
        assert_eq!(strengths(&world, 3), vec![994; 14]);
    }

    #[test]
    fn recruitment_finishing_on_the_first_is_not_included() {
        let mut world = sample(1, vec![(1, land(0, 0, Some(0), 0))]);
        station(&mut world, 1, 1, &[1_000]);
        world
            .countries
            .get_mut("AAA")
            .expect("国家")
            .finances
            .refund(Money::from(100));
        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, date(1444, 11, 11))
            .expect("招募");
        let before = world.country_manpower("AAA").expect("人力");
        advance(&mut world, date(1444, 11, 11), 20);
        assert_eq!(strengths(&world, 1), vec![1_000, 1_000]);
        let after = world.country_manpower("AAA").expect("人力");
        assert_eq!(after.available, before.available + before.monthly_recovery);
    }

    #[test]
    fn provinces_crossed_between_settlements_are_not_charged() {
        let mut world = sample(
            1,
            vec![
                (1, land(20, 0, Some(0), 0)),
                (2, land(0, 0, Some(0), 0)),
                (3, land(20, 0, Some(0), 0)),
            ],
        );
        station(&mut world, 1, 1, &[1_000; 4]);
        world.military.armies.get_mut(&1).expect("军队").movement = Some(ArmyMovement {
            legs: vec![
                MovementLeg {
                    from: 1,
                    to: 2,
                    started_on: date(1444, 11, 11),
                    arrives_on: date(1444, 11, 20),
                },
                MovementLeg {
                    from: 2,
                    to: 3,
                    started_on: date(1444, 11, 20),
                    arrives_on: date(1444, 11, 28),
                },
            ],
            leg_index: 0,
        });
        let forecast = world.supply_forecast(1, date(1444, 11, 15)).expect("预览");
        assert_eq!(forecast.province, 3);
        assert_eq!(forecast.loss, 0);
        advance(&mut world, date(1444, 11, 11), 20);
        assert_eq!(world.armies()[&1].province, 3);
        assert!(world.armies()[&1].movement.is_none());
        assert_eq!(strengths(&world, 1), vec![1_000; 4]);
    }

    #[test]
    fn save_roundtrip_does_not_repeat_the_same_day() {
        let mut world = sample(2, vec![(1, land(0, 0, Some(0), 0))]);
        station(&mut world, 1, 1, &[1_000; 4]);
        let start = date(1444, 11, 11);
        let simulation = advance(&mut world, start, 20);
        assert_eq!(simulation.clock().date(), date(1444, 12, 1));
        assert_eq!(strengths(&world, 1), vec![950; 4]);

        let saved = SavedGame::capture(&world, &simulation, "AAA");
        let mut value = serde_json::to_value(&saved).expect("可序列化");
        assert!(value["provinces"]["1"].get("terrain_supply").is_none());
        value["provinces"]["1"]["terrain_supply"] = serde_json::json!(99);
        let mut restored = serde_json::from_value::<SavedGame>(value)
            .expect("旧字段可忽略")
            .restore(&world)
            .expect("可恢复");
        assert_eq!(restored.world.province(1).expect("省份").terrain_supply, 0);
        assert_eq!(strengths(&restored.world, 1), vec![950; 4]);

        restored.simulation.set_paused(false);
        restored
            .simulation
            .advance(&mut restored.world, Duration::from_secs(10));
        assert_eq!(strengths(&restored.world, 1), vec![950; 4]);
        restored
            .simulation
            .advance(&mut restored.world, Duration::from_secs(21));
        assert_eq!(restored.simulation.clock().date(), date(1445, 1, 1));
        assert_eq!(strengths(&restored.world, 1), vec![903; 4]);
    }
}
