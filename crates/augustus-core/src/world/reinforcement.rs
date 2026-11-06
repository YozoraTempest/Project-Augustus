// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use crate::{GameDate, Money, REGIMENT_STRENGTH, UnitKind};

use super::World;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ArmyReinforcement {
    men: u64,
    cost: Money,
    full: bool,
    short_manpower: bool,
    short_treasury: bool,
}

impl ArmyReinforcement {
    fn label(&self) -> String {
        if self.full {
            return "下月补员：已满编".to_owned();
        }
        let mut text = format!("下月补员：+{} 人，{:.3} 金币", self.men, self.cost);
        let mut reasons = Vec::new();
        if self.short_manpower {
            reasons.push("人力不足");
        }
        if self.short_treasury {
            reasons.push("金币不足");
        }
        if !reasons.is_empty() {
            text.push('（');
            text.push_str(&reasons.join("、"));
            text.push('）');
        }
        text
    }
}

#[derive(Debug)]
struct CountryReinforcement {
    men: u64,
    cost: Money,
    grants: Vec<(u32, usize, u32)>,
    armies: BTreeMap<u32, ArmyReinforcement>,
}

struct RegimentState {
    index: usize,
    kind: UnitKind,
    strength: u32,
}

struct ArmyState {
    id: u32,
    province: u32,
    regiments: Vec<RegimentState>,
}

impl World {
    /// 下一次月初结算军费、恢复人力、抵达并损耗之后，这支军队会得到的补员。
    pub fn army_reinforcement_text(&self, army_id: u32, date: GameDate) -> Option<String> {
        let tag = self.armies().get(&army_id)?.country.clone();
        let targets = self.army_states(&tag, Some(date));
        let plan = self.reinforcement_plan(
            &tag,
            self.treasury_before_reinforcement(&tag),
            self.manpower_before_reinforcement(&tag),
            &targets,
        );
        plan.armies.get(&army_id).map(ArmyReinforcement::label)
    }

    /// 使用当前国库和人力给各国不满编团补员。调用方须先完成月初军费、人力恢复和损耗。
    pub(crate) fn reinforce_armies(&mut self) {
        let tags: Vec<_> = self.countries.keys().cloned().collect();
        for tag in tags {
            let treasury = self.countries[&tag].finances.treasury();
            let manpower = self.countries[&tag]
                .manpower
                .as_ref()
                .expect("国家人力已初始化")
                .available;
            let targets = self.army_states(&tag, None);
            let plan = self.reinforcement_plan(&tag, treasury, manpower, &targets);
            for (army_id, index, men) in plan.grants {
                self.military
                    .armies
                    .get_mut(&army_id)
                    .expect("补员军队存在")
                    .regiments[index]
                    .strength += men;
            }
            let country = self.countries.get_mut(&tag).expect("补员国家存在");
            country.finances.spend(plan.cost);
            country
                .manpower
                .as_mut()
                .expect("国家人力已初始化")
                .available -= plan.men;
        }
    }

    /// 下一次补员看到的国库：先扣除当月税收与维护，允许余额为负。
    fn treasury_before_reinforcement(&self, tag: &str) -> Money {
        let annual_tax = self
            .active_provinces
            .iter()
            .filter(|id| self.provinces[id].owner.as_deref() == Some(tag))
            .map(|id| self.province_annual_tax(*id).expect("有效领土具备基础税收"))
            .sum();
        let upkeep = self.country_army_cost(tag, None);
        self.countries[tag].finances.treasury_after_next_settlement(
            annual_tax,
            upkeep.base_annual,
            upkeep.excess_annual,
        )
    }

    /// 本月恢复的人力可以当月补进部队。超过上限的储备不再增长。
    fn manpower_before_reinforcement(&self, tag: &str) -> u64 {
        let summary = self.country_manpower(tag).expect("国家人力已初始化");
        summary.available + summary.monthly_recovery
    }

    fn army_states(&self, tag: &str, date: Option<GameDate>) -> Vec<ArmyState> {
        let projected = date.and_then(|date| self.strengths_after_next_attrition(date));
        self.military
            .armies
            .values()
            .filter(|army| army.country == tag)
            .map(|army| {
                if let Some((province, strengths)) = projected
                    .as_ref()
                    .and_then(|projected| projected.get(&army.id()))
                {
                    ArmyState {
                        id: army.id(),
                        province: *province,
                        regiments: army
                            .regiments
                            .iter()
                            .enumerate()
                            .zip(strengths)
                            .map(|((index, regiment), strength)| RegimentState {
                                index,
                                kind: regiment.kind,
                                strength: *strength,
                            })
                            .collect(),
                    }
                } else {
                    ArmyState {
                        id: army.id(),
                        province: army.province,
                        regiments: army
                            .regiments
                            .iter()
                            .enumerate()
                            .map(|(index, regiment)| RegimentState {
                                index,
                                kind: regiment.kind,
                                strength: regiment.strength,
                            })
                            .collect(),
                    }
                }
            })
            .collect()
    }

    fn reinforcement_plan(
        &self,
        tag: &str,
        mut treasury: Money,
        mut manpower: u64,
        targets: &[ArmyState],
    ) -> CountryReinforcement {
        let mut grants = Vec::new();
        let mut armies = BTreeMap::new();
        let mut total_men = 0_u64;
        let mut total_cost = Money::ZERO;
        let mut blocked: Option<ReinforcementBlock> = None;
        for army in targets {
            let rate =
                self.monthly_reinforcement_men(self.reinforcement_location(army.province, tag));
            let mut report = ArmyReinforcement {
                men: 0,
                cost: Money::ZERO,
                full: army.regiments.iter().any(|regiment| regiment.strength > 0)
                    && army.regiments.iter().all(|regiment| {
                        regiment.strength == 0 || regiment.strength >= REGIMENT_STRENGTH
                    }),
                short_manpower: false,
                short_treasury: false,
            };
            for regiment in &army.regiments {
                if regiment.strength == 0 || regiment.strength >= REGIMENT_STRENGTH {
                    continue;
                }
                let wanted = u64::from(REGIMENT_STRENGTH - regiment.strength).min(rate);
                if wanted == 0 {
                    continue;
                }
                let cost_of = |men| self.military_rules.reinforcement_cost(regiment.kind, men);
                if let Some(reason) = blocked {
                    match reason {
                        ReinforcementBlock::Treasury => report.short_treasury = true,
                        ReinforcementBlock::Manpower => report.short_manpower = true,
                    }
                    if manpower < wanted {
                        report.short_manpower = true;
                    }
                    if affordable_men(cost_of, wanted, treasury) < wanted {
                        report.short_treasury = true;
                    }
                    continue;
                }
                let by_manpower = wanted.min(manpower);
                if by_manpower == 0 {
                    blocked = Some(ReinforcementBlock::Manpower);
                    report.short_manpower = true;
                    if affordable_men(cost_of, wanted, treasury) < wanted {
                        report.short_treasury = true;
                    }
                    continue;
                }
                let affordable = affordable_men(cost_of, by_manpower, treasury);
                if affordable == 0 {
                    blocked = Some(ReinforcementBlock::Treasury);
                    report.short_treasury = true;
                    if by_manpower < wanted {
                        report.short_manpower = true;
                    }
                    continue;
                }
                let cost = cost_of(affordable);
                treasury -= cost;
                manpower -= affordable;
                total_men += affordable;
                total_cost += cost;
                report.men += affordable;
                report.cost += cost;
                grants.push((
                    army.id,
                    regiment.index,
                    u32::try_from(affordable).expect("补员人数在团规模内"),
                ));
                if affordable < by_manpower {
                    report.short_treasury = true;
                }
                if by_manpower < wanted {
                    report.short_manpower = true;
                }
            }
            armies.insert(army.id, report);
        }
        CountryReinforcement {
            men: total_men,
            cost: total_cost,
            grants,
            armies,
        }
    }

    /// 维护费滑条尚未接入，维护系数固定为 100%。人数向下取整。
    fn monthly_reinforcement_men(&self, location: Money) -> u64 {
        let amount =
            Money::from(REGIMENT_STRENGTH) * self.military_rules.monthly_reinforce * location;
        let whole = amount.floor();
        if whole <= Money::ZERO {
            return 0;
        }
        u64::try_from(whole).unwrap_or(u64::MAX)
    }

    /// 本国且控制为 100%，无主陆地为 10%，外国或被占领为 50%。
    fn reinforcement_location(&self, province_id: u32, country: &str) -> Money {
        let province = self.province(province_id).expect("军队所在省份存在");
        if province.owner.as_deref() == Some(country)
            && province.controller.as_deref() == Some(country)
        {
            Money::ONE
        } else if province.owner.is_none() {
            Money::new(1, 1)
        } else {
            Money::new(5, 1)
        }
    }
}

#[derive(Clone, Copy)]
enum ReinforcementBlock {
    Manpower,
    Treasury,
}

fn affordable_men(cost_of: impl Fn(u64) -> Money, men: u64, treasury: Money) -> u64 {
    if men == 0 || treasury < Money::ZERO {
        return 0;
    }
    let mut low = 0_u64;
    let mut high = men;
    while low < high {
        let mid = low + (high - low).div_ceil(2);
        if cost_of(mid) <= treasury {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use crate::{
        Army, ArmyMovement, Country, Development, GameDate, ManpowerRules, MilitaryRules, Money,
        MovementLeg, Province, ProvinceKind, REGIMENT_STRENGTH, Regiment, SavedGame, Simulation,
        UnitKind, World,
    };

    fn rules() -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 30,
            cavalry_days: 30,
            infantry_speed: Money::ONE,
            cavalry_speed: Money::ONE,
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: Money::new(25, 2),
            monthly_reinforce: Money::new(1, 1),
            extra_land_reinforce_cost: Money::from(2),
            supply_base: 100,
            base_land_forcelimit: Money::from(10),
            development_land_forcelimit: Money::ZERO,
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::new(),
            manpower: ManpowerRules {
                base_capacity: 0,
                per_development: 1_000,
                recovery_months: 120,
                minimum_monthly: 100,
                infantry_cost: 1_000,
                cavalry_cost: 1_000,
            },
        }
    }

    fn land(owner: Option<&str>, controller: Option<&str>, manpower: u32) -> Province {
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = owner.map(str::to_owned);
        province.controller = controller.map(str::to_owned);
        province.development = Development {
            tax: Some(0),
            production: Some(0),
            manpower: Some(manpower),
        };
        province
    }

    fn world() -> World {
        World::new(
            [Country::new("AAA"), Country::new("BBB")],
            BTreeMap::from([
                (1, land(Some("AAA"), Some("AAA"), 10)),
                (2, land(Some("BBB"), Some("BBB"), 1)),
                (3, land(None, None, 0)),
                (4, land(Some("AAA"), Some("BBB"), 0)),
            ]),
            [1, 2, 3, 4],
            [],
            rules(),
            [],
        )
        .unwrap()
    }

    fn fund(world: &mut World, tag: &str, gold: Money) {
        world.countries.get_mut(tag).unwrap().finances.refund(gold);
    }

    fn set_manpower(world: &mut World, tag: &str, available: u64) {
        world
            .countries
            .get_mut(tag)
            .unwrap()
            .manpower
            .as_mut()
            .unwrap()
            .available = available;
    }

    fn add_army(world: &mut World, country: &str, province: u32, regiments: Vec<Regiment>) -> u32 {
        let id = world.military.next_army_id;
        world.military.next_army_id += 1;
        world.military.armies.insert(
            id,
            Army {
                id,
                country: country.to_owned(),
                province,
                regiments,
                movement: None,
            },
        );
        id
    }

    fn regiment(kind: UnitKind, strength: u32) -> Regiment {
        Regiment { kind, strength }
    }

    fn strength(world: &World, army: u32, index: usize) -> u32 {
        world.armies().get(&army).unwrap().regiments[index].strength
    }

    fn reinforcement_text(world: &World, army: u32) -> String {
        world
            .army_reinforcement_text(army, GameDate::new(1444, 11, 30).unwrap())
            .unwrap()
    }

    #[test]
    fn owned_infantry_reinforces_one_hundred_without_changing_maintenance() {
        let mut world = world();
        fund(&mut world, "AAA", Money::from(10));
        set_manpower(&mut world, "AAA", 10_050);
        let army = add_army(
            &mut world,
            "AAA",
            1,
            vec![regiment(UnitKind::Infantry, 500)],
        );
        let budget = world.country_monthly_budget("AAA").unwrap();
        assert_eq!(
            reinforcement_text(&world, army),
            "下月补员：+100 人，0.500 金币"
        );

        world.reinforce_armies();

        assert_eq!(strength(&world, army, 0), 600);
        assert_eq!(world.country_manpower("AAA").unwrap().available, 9_950);
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(10) - Money::new(5, 1)
        );
        assert_eq!(world.country_monthly_budget("AAA").unwrap(), budget);
        assert_eq!(
            reinforcement_text(&world, army),
            "下月补员：+100 人，0.500 金币"
        );
    }

    #[test]
    fn location_sets_the_monthly_cap() {
        let mut world = world();
        fund(&mut world, "AAA", Money::from(10));
        let owned = add_army(
            &mut world,
            "AAA",
            1,
            vec![regiment(UnitKind::Infantry, 800)],
        );
        let foreign = add_army(
            &mut world,
            "AAA",
            2,
            vec![regiment(UnitKind::Infantry, 800)],
        );
        let unowned = add_army(
            &mut world,
            "AAA",
            3,
            vec![regiment(UnitKind::Infantry, 800)],
        );
        let occupied = add_army(
            &mut world,
            "AAA",
            4,
            vec![regiment(UnitKind::Infantry, 800)],
        );

        world.reinforce_armies();

        assert_eq!(strength(&world, owned, 0), 900);
        assert_eq!(strength(&world, foreign, 0), 850);
        assert_eq!(strength(&world, unowned, 0), 810);
        assert_eq!(strength(&world, occupied, 0), 850);
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(10) - Money::new(105, 2)
        );
    }

    #[test]
    fn gap_cavalry_and_full_regiments_follow_the_cost_rule() {
        let mut world = world();
        fund(&mut world, "AAA", Money::from(10));
        let army = add_army(
            &mut world,
            "AAA",
            1,
            vec![
                regiment(UnitKind::Infantry, 980),
                regiment(UnitKind::Cavalry, 999),
                regiment(UnitKind::Infantry, REGIMENT_STRENGTH),
            ],
        );

        world.reinforce_armies();

        assert_eq!(strength(&world, army, 0), 1_000);
        assert_eq!(strength(&world, army, 1), 1_000);
        assert_eq!(strength(&world, army, 2), 1_000);
        // 步兵 20 人 0.100，骑兵 1 人 0.012，满编团不扣费。
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(10) - Money::new(112, 3)
        );
        assert_eq!(
            world.country_manpower("AAA").unwrap().available,
            10_000 - 21
        );
        assert_eq!(reinforcement_text(&world, army), "下月补员：已满编");
    }

    #[test]
    fn scarce_manpower_follows_army_and_regiment_order_including_surplus() {
        let mut world = world();
        fund(&mut world, "AAA", Money::from(10));
        fund(&mut world, "BBB", Money::from(10));
        // 恢复量压到 0，预览和立即补员使用同一份人力。
        world.military_rules.manpower.minimum_monthly = 0;
        world.military_rules.manpower.recovery_months = u32::MAX;
        set_manpower(&mut world, "AAA", 150);
        let first = add_army(
            &mut world,
            "AAA",
            1,
            vec![
                regiment(UnitKind::Infantry, 900),
                regiment(UnitKind::Infantry, 900),
            ],
        );
        let second = add_army(
            &mut world,
            "AAA",
            1,
            vec![regiment(UnitKind::Infantry, 900)],
        );
        let other = add_army(
            &mut world,
            "BBB",
            2,
            vec![regiment(UnitKind::Infantry, 900)],
        );

        assert_eq!(
            reinforcement_text(&world, first),
            "下月补员：+150 人，0.750 金币（人力不足）"
        );
        assert_eq!(
            reinforcement_text(&world, second),
            "下月补员：+0 人，0.000 金币（人力不足）"
        );

        world.reinforce_armies();

        assert_eq!(strength(&world, first, 0), 1_000);
        assert_eq!(strength(&world, first, 1), 950);
        assert_eq!(strength(&world, second, 0), 900);
        assert_eq!(strength(&world, other, 0), 1_000);
        assert_eq!(world.country_manpower("AAA").unwrap().available, 0);
        assert_eq!(world.country_manpower("BBB").unwrap().available, 900);
    }

    #[test]
    fn unaffordable_regiment_blocks_later_cheaper_regiments() {
        let mut world = world();
        fund(&mut world, "AAA", Money::new(1, 2));
        let blocked = add_army(
            &mut world,
            "AAA",
            1,
            vec![
                regiment(UnitKind::Cavalry, 900),
                regiment(UnitKind::Infantry, 900),
            ],
        );
        world.reinforce_armies();
        assert_eq!(strength(&world, blocked, 0), 900);
        assert_eq!(strength(&world, blocked, 1), 900);
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::new(1, 2)
        );
    }

    #[test]
    fn partial_payment_continues_with_a_later_cheaper_regiment() {
        let mut world = world();
        fund(&mut world, "AAA", Money::new(2, 2));
        let partial = add_army(
            &mut world,
            "AAA",
            1,
            vec![
                regiment(UnitKind::Cavalry, 900),
                regiment(UnitKind::Infantry, 900),
            ],
        );
        // 国库 0.020：骑兵 1 人花费 0.012，剩余还能补 1 名步兵。
        world.reinforce_armies();
        assert_eq!(strength(&world, partial, 0), 901);
        assert_eq!(strength(&world, partial, 1), 901);
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::new(3, 3)
        );

        world
            .countries
            .get_mut("AAA")
            .unwrap()
            .finances
            .spend(Money::from(1));
        world.reinforce_armies();
        assert_eq!(strength(&world, partial, 0), 901);
        assert!(world.country("AAA").unwrap().finances.treasury() < Money::ZERO);
    }

    #[test]
    fn month_tick_settles_and_recovers_before_reinforcing_once() {
        let mut world = world();
        fund(&mut world, "AAA", Money::new(6, 1));
        set_manpower(&mut world, "AAA", 50);
        let army = add_army(
            &mut world,
            "AAA",
            1,
            vec![regiment(UnitKind::Infantry, 900)],
        );
        assert_eq!(
            reinforcement_text(&world, army),
            "下月补员：+78 人，0.390 金币（金币不足）"
        );

        let start = GameDate::new(1444, 11, 30).unwrap();
        let mut simulation = Simulation::new(start);
        simulation.set_paused(false);
        assert_eq!(simulation.advance(&mut world, Duration::from_secs(1)), 1);

        assert_eq!(strength(&world, army, 0), 978);
        assert_eq!(world.country_manpower("AAA").unwrap().available, 72);
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::new(2, 3)
        );
        let maintenance = world
            .country("AAA")
            .unwrap()
            .finances
            .last_settlement()
            .unwrap();
        assert_eq!(maintenance.army_maintenance, Money::new(208, 3));
        assert_eq!(maintenance.excess_maintenance, Money::ZERO);

        let saved = SavedGame::capture(&world, &simulation, "AAA");
        let restored = saved.restore(&world).unwrap();
        assert_eq!(strength(&restored.world, army, 0), 978);
        assert_eq!(
            restored.world.country("AAA").unwrap().finances.treasury(),
            Money::new(2, 3)
        );
        assert!(restored.simulation.clock().is_paused());
    }

    #[test]
    fn reinforcement_on_arrival_day_uses_the_destination_province() {
        let mut world = world();
        fund(&mut world, "AAA", Money::from(10));
        let army = add_army(
            &mut world,
            "AAA",
            1,
            vec![regiment(UnitKind::Infantry, 900)],
        );
        let depart = GameDate::new(1444, 11, 30).unwrap();
        world.military.armies.get_mut(&army).unwrap().movement = Some(ArmyMovement {
            legs: vec![MovementLeg {
                from: 1,
                to: 2,
                started_on: depart,
                arrives_on: GameDate::new(1444, 12, 1).unwrap(),
            }],
            leg_index: 0,
        });
        // 补给和补员都在抵达之后，外国省份每月最多 50 人。
        assert_eq!(
            reinforcement_text(&world, army),
            "下月补员：+50 人，0.250 金币"
        );
        let mut simulation = Simulation::new(depart);
        simulation.set_paused(false);
        simulation.advance(&mut world, Duration::from_secs(1));

        let arrived = world.armies().get(&army).unwrap();
        assert_eq!(arrived.province, 2);
        assert!(arrived.movement.is_none());
        assert_eq!(arrived.regiments[0].strength, 950);
    }

    #[test]
    fn attrition_losses_are_refilled_in_the_same_tick() {
        let mut world = world();
        world.military_rules.supply_base = 1;
        fund(&mut world, "AAA", Money::from(10));
        let army = add_army(
            &mut world,
            "AAA",
            1,
            vec![
                regiment(UnitKind::Infantry, 1_000),
                regiment(UnitKind::Infantry, 1_000),
            ],
        );
        assert_eq!(
            reinforcement_text(&world, army),
            "下月补员：+100 人，0.500 金币"
        );

        let start = GameDate::new(1444, 11, 30).unwrap();
        let mut simulation = Simulation::new(start);
        simulation.set_paused(false);
        simulation.advance(&mut world, Duration::from_secs(1));

        assert_eq!(strength(&world, army, 0), 1_000);
        assert_eq!(strength(&world, army, 1), 1_000);
        assert_eq!(world.country_manpower("AAA").unwrap().available, 9_900);
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(10) - Money::new(416, 3) - Money::new(5, 1)
        );
    }

    #[test]
    fn over_force_limit_does_not_change_reinforcement_cost_or_maintenance() {
        let mut world = world();
        world.military_rules.base_land_forcelimit = Money::ONE;
        fund(&mut world, "AAA", Money::from(10));
        let army = add_army(
            &mut world,
            "AAA",
            1,
            vec![
                regiment(UnitKind::Infantry, 1_000),
                regiment(UnitKind::Infantry, 900),
            ],
        );
        let budget = world.country_monthly_budget("AAA").unwrap();
        assert!(budget.excess_army_maintenance > Money::ZERO);

        world.reinforce_armies();

        assert_eq!(strength(&world, army, 1), 1_000);
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(10) - Money::new(5, 1)
        );
        assert_eq!(world.country_monthly_budget("AAA").unwrap(), budget);
    }
}
