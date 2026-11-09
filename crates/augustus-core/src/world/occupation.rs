// SPDX-License-Identifier: GPL-3.0-only

//! 无要塞省份的占领。
//!
//! 已核对 1.37：堡垒等级为 0 时需要 1000 人，围城阶段为 30 天，第一次阶段结算因驻军为 0 而直接成功。
//! 人数按整数千人比较，999 人不足，1000 人足够。兵力不足只暂停，不回退已有天数。
//! 敌对的驻军全部离开、撤退或继续行军时，进度清除。这与围城对象被销毁后天数不再保留一致。
//! 围城能力、封锁、火炮、领袖围城、疾病和攻城骰子保持中性，不另造修正。
//! 换一个交战方时进度作废，避免一国完成另一国已经累计的天数。这是本项目的规则。
//! 占领只改变控制者。所有者，或与所有者同侧的国家，收复时把控制权交还所有者。

use std::collections::BTreeMap;

use crate::{GameDate, OCCUPATION_PHASE_DAYS, OccupationProgress, ProvinceKind, REGIMENT_STRENGTH};

use super::World;

struct OccupyingSide {
    lead: String,
    includes_owner: bool,
}

impl World {
    /// 首都静态修正 `capital_city.fort_level = 1`，要塞建筑的 `fort_level` 大于 0。
    /// 二者只要有一个成立，就不按无要塞省份推进。具体围城等级和叠堡规则不在本项。
    pub fn province_is_fortified(&self, id: u32) -> bool {
        self.occupation_fort_level(id) > 0
    }

    pub(crate) fn tick_occupations(&mut self, date: GameDate) {
        let provinces: Vec<u32> = self.active_provinces.iter().copied().collect();
        for province in provinces {
            self.tick_province_occupation(province, date);
        }
    }

    fn occupation_fort_level(&self, id: u32) -> u32 {
        self.province_fort_level(id)
    }

    fn tick_province_occupation(&mut self, id: u32, date: GameDate) {
        if !self.can_occupy_without_fort(id) {
            self.clear_occupation(id);
            return;
        }
        // 交战当天暂停。战斗结束后若胜方仍留下足够驻军，同一天的结算可以继续推进。
        if self.battle_in_province(id).is_some() {
            return;
        }
        let Some(controller) = self.provinces[&id].controller.clone() else {
            self.clear_occupation(id);
            return;
        };
        let men = self.hostile_stopped_men(id, &controller);
        if men.is_empty() {
            self.clear_occupation(id);
            return;
        }
        let owner = self.provinces[&id]
            .owner
            .clone()
            .expect("可占领省份有所有者");
        let leading = self.leading_occupier(&men, &owner);
        let recorded = self.provinces[&id].occupation.clone();
        let Some(side) = leading else {
            let keep = recorded.as_ref().is_some_and(|progress| {
                men.iter()
                    .any(|(tag, count)| *count > 0 && self.same_war_side(tag, &progress.occupier))
            });
            if !keep {
                self.clear_occupation(id);
            }
            return;
        };
        let continued = recorded
            .as_ref()
            .is_some_and(|progress| self.same_war_side(&progress.occupier, &side.lead));
        let days = if continued {
            recorded.expect("同一交战方沿用已有进度").days
        } else {
            0
        } + 1;
        if days >= OCCUPATION_PHASE_DAYS {
            self.finish_occupation(id, &side, date);
            return;
        }
        self.provinces.get_mut(&id).expect("省份存在").occupation = Some(OccupationProgress {
            occupier: side.lead,
            days,
        });
    }

    fn can_occupy_without_fort(&self, id: u32) -> bool {
        let Some(province) = self.province(id) else {
            return false;
        };
        province.kind == ProvinceKind::Land
            && province.owner.is_some()
            && province.controller.is_some()
            && self.occupation_fort_level(id) == 0
    }

    /// 停下、不在撤退、不在战斗中，且与当前控制者交战的军队。人数按国家合计。
    pub(super) fn hostile_stopped_men(
        &self,
        province: u32,
        controller: &str,
    ) -> BTreeMap<String, u64> {
        let mut men = BTreeMap::new();
        for army in self.military.armies.values() {
            if army.province != province
                || army.movement.is_some()
                || army.is_retreating()
                || army.is_exiled()
                || army.battle_id().is_some()
                || army.strength() == 0
                || !self.at_war(&army.country, controller)
            {
                continue;
            }
            *men.entry(army.country.clone()).or_default() += army.strength();
        }
        men
    }

    /// 至少 1000 人的一侧里，人数最多者推进；人数相同则标签较小的国家作为记录方。
    fn leading_occupier(&self, men: &BTreeMap<String, u64>, owner: &str) -> Option<OccupyingSide> {
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
            // 原版比较的是人数除以 1000。无要塞所需为 1 个千人，因此少于 1000 人不足。
            if total < u64::from(REGIMENT_STRENGTH) {
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
        best.map(|(_, lead, includes_owner)| OccupyingSide {
            lead,
            includes_owner,
        })
    }

    fn finish_occupation(&mut self, id: u32, side: &OccupyingSide, date: GameDate) {
        let owner = self.provinces[&id]
            .owner
            .clone()
            .expect("可占领省份有所有者");
        let next = if side.includes_owner {
            owner
        } else {
            side.lead.clone()
        };
        self.provinces.get_mut(&id).expect("省份存在").occupation = None;
        if self.provinces[&id].controller.as_deref() == Some(next.as_str()) {
            return;
        }
        self.provinces.get_mut(&id).expect("省份存在").controller = Some(next.clone());
        self.control_changed = true;
        self.cancel_uncontrolled_recruitment(id, &next, date);
    }

    fn clear_occupation(&mut self, id: u32) {
        if let Some(province) = self.provinces.get_mut(&id) {
            province.occupation = None;
        }
    }

    /// 控制者改变后，取消该省里不属于新控制者、且尚未交付的招募。退款沿用原有规则，只退一次。
    pub(crate) fn cancel_uncontrolled_recruitment(
        &mut self,
        province: u32,
        controller: &str,
        date: GameDate,
    ) {
        let orders: Vec<(String, u32)> = self
            .recruitment_queue(province)
            .iter()
            .filter(|order| order.country != controller)
            .map(|order| (order.country.clone(), order.id()))
            .collect();
        for (country, order_id) in orders.into_iter().rev() {
            self.cancel_recruitment(&country, province, order_id, date)
                .expect("失去控制时取消尚未交付的招募");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::Duration;

    use crate::{
        Army, ArmyMovement, Battle, BuildingDefinition, CombatRules, Country, Development,
        GameDate, LandConnection, ManpowerRules, MilitaryRules, Money, MovementLeg,
        OccupationProgress, Province, ProvinceKind, Regiment, SavedGame, Simulation, UnitKind,
        World, WorldError,
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
            combat: CombatRules::baseline(),
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

    fn world_with(provinces: Vec<(u32, Province)>, buildings: Vec<BuildingDefinition>) -> World {
        let aaa = Country::new("AAA");
        let bbb = Country::new("BBB");
        let ccc = Country::new("CCC");
        let ids: Vec<_> = provinces.iter().map(|(id, _)| *id).collect();
        World::new(
            [aaa, bbb, ccc],
            provinces.into_iter().collect(),
            ids,
            buildings,
            rules(),
            Vec::<LandConnection>::new(),
        )
        .unwrap()
    }

    fn world() -> World {
        world_with(vec![(1, land("AAA", 12))], vec![])
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

    fn tick(world: &mut World, days: u32) {
        let mut current = date();
        for _ in 0..days {
            current = current.checked_add_days(1).unwrap();
            world.tick_occupations(current);
        }
    }

    fn temple() -> BuildingDefinition {
        BuildingDefinition {
            id: "temple".to_owned(),
            cost: Money::from(100),
            months: 12,
            tax_modifier: Money::ZERO,
            land_forcelimit: Money::ZERO,
            local_regiment_cost: Money::ZERO,
            regiment_recruit_speed: Money::ZERO,
            fort_level: 0,
            replaces: None,
            constructible: true,
        }
    }

    #[test]
    fn one_thousand_men_advance_and_short_garrisons_do_not() {
        let mut world = world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 999);
        tick(&mut world, 1);
        assert!(world.province(1).unwrap().occupation.is_none());

        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 500;
        station(&mut world, 2, "BBB", 1, 500);
        tick(&mut world, 1);
        let progress = world.province(1).unwrap().occupation.clone().unwrap();
        assert_eq!(progress.days, 1);
        assert_eq!(progress.occupier, "BBB");
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert_eq!(world.province(1).unwrap().owner.as_deref(), Some("AAA"));
    }

    #[test]
    fn thirty_days_change_controller_tax_and_keep_owner_limits() {
        let mut world = world();
        let owner_limit = world.country_land_force_limit("AAA");
        let owner_manpower = world.country_manpower("AAA").unwrap().capacity;
        let occupier_manpower = world.country_manpower("BBB").unwrap().capacity;
        assert_eq!(world.country_monthly_budget("AAA").unwrap().tax, Money::ONE);
        assert_eq!(
            world.country_monthly_budget("BBB").unwrap().tax,
            Money::ZERO
        );
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 29);
        assert_eq!(
            world.province(1).unwrap().occupation.as_ref().unwrap().days,
            29
        );
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert!(!world.take_control_changed());

        tick(&mut world, 1);
        let province = world.province(1).unwrap();
        assert_eq!(province.owner.as_deref(), Some("AAA"));
        assert_eq!(province.controller.as_deref(), Some("BBB"));
        assert!(province.occupation.is_none());
        assert!(world.take_control_changed());
        assert!(!world.take_control_changed());
        assert_eq!(
            world.country_monthly_budget("AAA").unwrap().tax,
            Money::ZERO
        );
        assert_eq!(world.country_monthly_budget("BBB").unwrap().tax, Money::ONE);
        assert_eq!(world.country_land_force_limit("AAA"), owner_limit);
        assert_eq!(world.country_land_force_limit("BBB"), Some(0));
        assert_eq!(
            world.country_manpower("AAA").unwrap().capacity,
            owner_manpower
        );
        assert_eq!(
            world.country_manpower("BBB").unwrap().capacity,
            occupier_manpower
        );

        world.settle_monthly_finances(GameDate::new(1444, 12, 1).unwrap());
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::ZERO
        );
        assert_eq!(
            world.country("BBB").unwrap().finances.treasury(),
            Money::ONE
        );
    }

    #[test]
    fn leaving_clears_progress_and_too_few_men_pause_it() {
        let mut world = world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 5);
        assert_eq!(
            world.province(1).unwrap().occupation.as_ref().unwrap().days,
            5
        );

        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 999;
        tick(&mut world, 3);
        assert_eq!(
            world.province(1).unwrap().occupation.as_ref().unwrap().days,
            5
        );

        world.military.armies.get_mut(&1).unwrap().retreating = true;
        tick(&mut world, 1);
        assert!(world.province(1).unwrap().occupation.is_none());

        world.military.armies.get_mut(&1).unwrap().retreating = false;
        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 1_000;
        world.military.armies.get_mut(&1).unwrap().movement = Some(ArmyMovement {
            legs: vec![MovementLeg {
                from: 1,
                to: 1,
                started_on: date(),
                arrives_on: date().checked_add_days(2).unwrap(),
            }],
            leg_index: 0,
        });
        tick(&mut world, 1);
        assert!(world.province(1).unwrap().occupation.is_none());
    }

    #[test]
    fn battle_pauses_and_does_not_finish() {
        let mut world = world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 29);
        world.military.battles.insert(1, Battle::new(1, 1, date()));
        world.military.next_battle_id = 2;
        tick(&mut world, 1);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert_eq!(
            world.province(1).unwrap().occupation.as_ref().unwrap().days,
            29
        );
    }

    #[test]
    fn owner_recaptures_without_changing_owner() {
        let mut world = world();
        world.provinces.get_mut(&1).unwrap().controller = Some("BBB".to_owned());
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 1, "AAA", 1, 1_000);
        tick(&mut world, 30);
        let province = world.province(1).unwrap();
        assert_eq!(province.owner.as_deref(), Some("AAA"));
        assert_eq!(province.controller.as_deref(), Some("AAA"));
        assert!(province.occupation.is_none());
    }

    #[test]
    fn another_side_resets_days_and_ties_use_the_smaller_tag() {
        let mut world = world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        world.declare_war("CCC", "AAA", date()).unwrap();
        station(&mut world, 1, "CCC", 1, 1_000);
        station(&mut world, 2, "BBB", 1, 1_000);
        tick(&mut world, 1);
        assert_eq!(
            world
                .province(1)
                .unwrap()
                .occupation
                .as_ref()
                .unwrap()
                .occupier,
            "BBB"
        );

        world.military.armies.get_mut(&2).unwrap().regiments[0].strength = 0;
        world.military.armies.get_mut(&1).unwrap().regiments[0].strength = 2_000;
        tick(&mut world, 1);
        let progress = world.province(1).unwrap().occupation.clone().unwrap();
        assert_eq!(progress.occupier, "CCC");
        assert_eq!(progress.days, 1);
    }

    #[test]
    fn capitals_and_fort_buildings_are_not_unfortified() {
        let capital = land("AAA", 1);
        let mut aaa = Country::new("AAA");
        aaa.capital = Some(1);
        let mut world = World::new(
            [aaa, Country::new("BBB"), Country::new("CCC")],
            BTreeMap::from([(1, capital)]),
            [1],
            [],
            rules(),
            Vec::<LandConnection>::new(),
        )
        .unwrap();
        assert!(world.province_is_fortified(1));
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 30);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert!(world.province(1).unwrap().occupation.is_none());

        let mut fortified = land("AAA", 1);
        fortified.buildings.insert("fort_15th".to_owned());
        let mut world = world_with(
            vec![(1, fortified)],
            vec![BuildingDefinition {
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
            }],
        );
        assert!(world.province_is_fortified(1));
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 5);
        assert!(world.province(1).unwrap().occupation.is_none());
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
    }

    #[test]
    fn lost_control_cancels_recruitment_once_and_recapture_keeps_it() {
        let mut world = world();
        world
            .countries
            .get_mut("AAA")
            .unwrap()
            .finances
            .refund(Money::from(100));
        let before = world.country_manpower("AAA").unwrap().available;
        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, date())
            .unwrap();
        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, date())
            .unwrap();
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(80)
        );
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 30);
        assert!(world.recruitment_queue(1).is_empty());
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(100)
        );
        assert_eq!(world.country_manpower("AAA").unwrap().available, before);
        tick(&mut world, 1);
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(100)
        );

        world.provinces.get_mut(&1).unwrap().controller = Some("AAA".to_owned());
        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, date())
            .unwrap();
        world.provinces.get_mut(&1).unwrap().controller = Some("BBB".to_owned());
        world.provinces.get_mut(&1).unwrap().occupation = None;
        let spent = world.country("AAA").unwrap().finances.treasury();
        station(&mut world, 2, "AAA", 1, 1_000);
        world.military.armies.remove(&1);
        tick(&mut world, 30);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert_eq!(world.recruitment_queue(1).len(), 1);
        assert_eq!(world.country("AAA").unwrap().finances.treasury(), spent);
    }

    #[test]
    fn paid_construction_continues_without_another_charge_or_refund() {
        let mut world = world_with(vec![(1, land("AAA", 1))], vec![temple()]);
        world
            .countries
            .get_mut("AAA")
            .unwrap()
            .finances
            .refund(Money::from(100));
        world
            .start_construction("AAA", 1, "temple", date())
            .unwrap();
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::ZERO
        );
        let completes = world
            .province(1)
            .unwrap()
            .construction
            .clone()
            .unwrap()
            .completes_on;
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 30);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("BBB")
        );
        assert_eq!(
            world
                .province(1)
                .unwrap()
                .construction
                .as_ref()
                .map(|project| project.building.as_str()),
            Some("temple")
        );
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::ZERO
        );
        world.complete_constructions(completes.add_days(-1));
        assert!(world.province(1).unwrap().construction.is_some());
        world.complete_constructions(completes);
        assert!(world.province(1).unwrap().buildings.contains("temple"));
        assert!(world.province(1).unwrap().construction.is_none());
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::ZERO
        );
    }

    #[test]
    fn due_recruitment_is_delivered_before_occupation_can_refund_it() {
        let mut rules = rules();
        rules.infantry_days = 1;
        let mut world = World::new(
            [
                Country::new("AAA"),
                Country::new("BBB"),
                Country::new("CCC"),
            ],
            BTreeMap::from([(1, land("AAA", 1))]),
            [1],
            [],
            rules,
            Vec::<LandConnection>::new(),
        )
        .unwrap();
        world
            .countries
            .get_mut("AAA")
            .unwrap()
            .finances
            .refund(Money::from(10));
        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, date())
            .unwrap();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 7, "BBB", 1, 1_000);
        for _ in 0..29 {
            world.tick_occupations(date().checked_add_days(1).unwrap());
        }
        assert_eq!(
            world.province(1).unwrap().occupation.as_ref().unwrap().days,
            29
        );
        let mut simulation = Simulation::new(date());
        simulation.set_paused(false);
        assert_eq!(simulation.advance(&mut world, Duration::from_secs(1)), 1);
        assert!(world.recruitment_queue(1).is_empty());
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::ZERO
        );
        assert!(world.armies().values().any(|army| army.country == "AAA"));
        assert!(
            world.battle_in_province(1).is_some()
                || world.province(1).unwrap().controller.as_deref() == Some("BBB")
        );
    }

    #[test]
    fn saves_keep_progress_and_old_provinces_load() {
        let mut world = world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 4);
        let saved = SavedGame::capture(&world, &Simulation::new(date()), "AAA");
        let restored = serde_json::from_value::<SavedGame>(serde_json::to_value(&saved).unwrap())
            .unwrap()
            .restore(&world)
            .unwrap();
        assert_eq!(
            restored.world.province(1).unwrap().occupation,
            Some(OccupationProgress {
                occupier: "BBB".to_owned(),
                days: 4,
            })
        );
        assert_eq!(
            restored.world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );

        let mut legacy = serde_json::to_value(&saved).unwrap();
        legacy["provinces"]["1"]
            .as_object_mut()
            .unwrap()
            .remove("occupation");
        let loaded = serde_json::from_value::<SavedGame>(legacy)
            .unwrap()
            .restore(&world)
            .unwrap();
        assert!(loaded.world.province(1).unwrap().occupation.is_none());

        let mut broken = land("AAA", 1);
        broken.occupation = Some(OccupationProgress {
            occupier: "BBB".to_owned(),
            days: 0,
        });
        let error = World::new(
            [Country::new("AAA"), Country::new("BBB")],
            BTreeMap::from([(1, broken)]),
            [1],
            [],
            rules(),
            Vec::<LandConnection>::new(),
        );
        assert!(matches!(
            error,
            Err(WorldError::InvalidOccupation { province: 1 })
        ));
    }

    #[test]
    fn peaceful_troops_do_not_occupy() {
        let mut world = world();
        station(&mut world, 1, "BBB", 1, 1_000);
        tick(&mut world, 30);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert!(world.province(1).unwrap().occupation.is_none());
    }

    #[test]
    fn simulation_hook_finishes_on_the_thirtieth_day() {
        let mut world = world();
        world.declare_war("BBB", "AAA", date()).unwrap();
        station(&mut world, 1, "BBB", 1, 1_000);
        let mut simulation = Simulation::new(date());
        simulation.set_paused(false);
        assert_eq!(simulation.advance(&mut world, Duration::from_secs(29)), 29);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("AAA")
        );
        assert_eq!(simulation.advance(&mut world, Duration::from_secs(1)), 1);
        assert_eq!(
            world.province(1).unwrap().controller.as_deref(),
            Some("BBB")
        );
        assert!(world.province(1).unwrap().owner.as_deref() == Some("AAA"));
        assert!(world.province(1).unwrap().occupation.is_none());
    }
}
