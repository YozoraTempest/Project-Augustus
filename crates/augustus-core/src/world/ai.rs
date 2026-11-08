// SPDX-License-Identifier: GPL-3.0-only

//! 非玩家国家在已经发生的战争中下达命令。
//!
//! 招募、行军和和谈都调用玩家使用的报价与执行接口，不直接改国库、人力或军队。
//! 本仓库没有核对过原版军事 AI 的目标权重，因此优先级、半士气门槛和 -25 战争分数
//! 是本项目的取舍，不是未核实的原版公式。条款能否成立只看已有的 `accepts_terms`。
//! 这里不选择宣战对象。

use std::collections::BTreeMap;

use crate::{GameDate, Money, REGIMENT_STRENGTH, UnitKind};

use super::World;

/// 从本国视角低于这个战争分数时提出白和平。-25 点，单位是千分点。
const LOSING_WARSCORE_MILLI: i64 = -25_000;

impl World {
    /// 每个新游戏日调用一次。`player` 为空、或该国不在战争中时什么都不做。
    pub(crate) fn direct_war_ai(&mut self, player: Option<&str>, date: GameDate) {
        let Some(player) = player else {
            return;
        };
        if self.country(player).is_none() {
            return;
        }
        let tags: Vec<String> = self
            .countries
            .keys()
            .filter(|tag| tag.as_str() != player)
            .cloned()
            .collect();
        for tag in tags {
            if self.wars_of(&tag).next().is_none() {
                continue;
            }
            self.offer_peace(&tag, date);
            if self.wars_of(&tag).next().is_none() {
                continue;
            }
            self.recruit_for_war(&tag, date);
            self.order_armies(&tag, date);
        }
    }

    /// 先按接受判定割让已控制的省份；分数明显不利时才提出白和平。
    fn offer_peace(&mut self, country: &str, date: GameDate) {
        let enemies: Vec<String> = self
            .wars_of(country)
            .map(|war| {
                if war.attacker() == country {
                    war.defender().to_owned()
                } else {
                    war.attacker().to_owned()
                }
            })
            .collect();
        for enemy in enemies {
            if self.war_between(country, &enemy).is_none() {
                continue;
            }
            if let Some(provinces) = self.affordable_demands(country, &enemy, date)
                && !provinces.is_empty()
            {
                let _ = self.make_peace(country, &enemy, &provinces, date);
                continue;
            }
            if self.should_offer_white_peace(country, &enemy, date) {
                let _ = self.make_peace(country, &enemy, &[], date);
            }
        }
    }

    /// 只考虑提出方已经控制的省份，按费用从低到高装进当前战争分数。
    fn affordable_demands(&self, country: &str, enemy: &str, date: GameDate) -> Option<Vec<u32>> {
        let sheet = self.peace_sheet(country, enemy, date).ok()?;
        let mut demands = sheet.provinces;
        demands.retain(|demand| demand.controlled_by_offerer);
        demands.sort_by(|left, right| {
            left.cost_milli
                .cmp(&right.cost_milli)
                .then(left.province.cmp(&right.province))
        });
        let mut chosen = Vec::new();
        let mut cost = 0_i64;
        for demand in demands {
            let next = cost.saturating_add(demand.cost_milli);
            if next <= sheet.score.total_milli {
                cost = next;
                chosen.push(demand.province);
            }
        }
        if chosen.is_empty() {
            return Some(chosen);
        }
        self.quote_peace(country, enemy, &chosen, date)
            .ok()
            .filter(|quote| quote.accepted)
            .map(|_| chosen)
    }

    fn should_offer_white_peace(&self, country: &str, enemy: &str, date: GameDate) -> bool {
        let Ok(quote) = self.quote_peace(country, enemy, &[], date) else {
            return false;
        };
        if quote.score_milli >= 0 {
            return false;
        }
        if quote.score_milli <= LOSING_WARSCORE_MILLI {
            return true;
        }
        let fielded = self
            .armies()
            .values()
            .filter(|army| army.country == country)
            .map(crate::Army::strength)
            .sum::<u64>();
        let recruiting = self
            .recruitments()
            .values()
            .flatten()
            .any(|order| order.country == country);
        fielded == 0 && !recruiting
    }

    /// 战争期间在陆军上限内最多保持一笔步兵招募。维护费已经超过税收时不再下单。
    fn recruit_for_war(&mut self, country: &str, date: GameDate) {
        let Some(limit) = self.country_land_force_limit(country) else {
            return;
        };
        let queued = self
            .recruitments()
            .values()
            .flatten()
            .filter(|order| order.country == country)
            .count() as u64;
        if queued > 0 {
            return;
        }
        let fielded = self
            .armies()
            .values()
            .filter(|army| army.country == country)
            .map(|army| army.regiments.len() as u64)
            .sum::<u64>();
        if fielded >= limit {
            return;
        }
        let Some(budget) = self.country_monthly_budget(country) else {
            return;
        };
        if budget.army_maintenance > budget.tax {
            return;
        }
        let extra = self.military_rules.annual_maintenance(UnitKind::Infantry) / Money::from(12);
        let treasury = self
            .country(country)
            .map(|state| state.finances.treasury())
            .unwrap_or(Money::ZERO);
        if budget.army_maintenance + extra > budget.tax && treasury < extra * Money::from(12) {
            return;
        }
        let mut provinces: Vec<u32> = self.active_provinces.iter().copied().collect();
        provinces.sort_unstable();
        for province in provinces {
            if self
                .quote_recruitment(country, province, UnitKind::Infantry, date)
                .is_err()
            {
                continue;
            }
            let _ = self.start_recruitment(country, province, UnitKind::Infantry, date);
            return;
        }
    }

    fn order_armies(&mut self, country: &str, date: GameDate) {
        let max_morale = self.military_rules.combat.morale_milli();
        let ids: Vec<u32> = self
            .armies()
            .iter()
            .filter(|(_, army)| army.country == country)
            .map(|(id, _)| *id)
            .collect();
        for id in ids {
            self.order_army(country, id, date, max_morale);
        }
    }

    fn order_army(&mut self, country: &str, id: u32, date: GameDate, max_morale: Option<u32>) {
        let Some(view) = self.army_view(id, max_morale) else {
            return;
        };
        if view.country != country
            || view.moving
            || view.in_battle
            || view.retreating
            || view.exiled
            || view.strength == 0
        {
            return;
        }
        let distances = self.land_travel_days(country, view.province, view.speed, false);
        let destination = if view.morale_ready {
            self.battle_destination(country, &view, &distances)
        } else {
            self.recovery_province(country, &distances)
                .filter(|province| *province != view.province)
        };
        if let Some(destination) = destination {
            let _ = self.move_army(country, id, destination, date);
        }
    }

    /// 防守优先于进攻，进攻优先于占领。已经站在目标上时留下驻扎，避免相邻省份来回走。
    fn battle_destination(
        &self,
        country: &str,
        army: &ArmyView,
        distances: &BTreeMap<u32, u32>,
    ) -> Option<u32> {
        if self.holding_occupation(country, army) {
            return None;
        }
        let mut best: Option<(u8, u32, u32)> = None;
        for (&province, &days) in distances {
            let Some(rank) = self.target_rank(country, army, province) else {
                continue;
            };
            let candidate = (rank, days, province);
            if best.is_none_or(|current| candidate < current) {
                best = Some(candidate);
            }
        }
        best.and_then(|(_, days, province)| (days > 0).then_some(province))
    }

    fn target_rank(&self, country: &str, army: &ArmyView, province: u32) -> Option<u8> {
        let state = self.province(province)?;
        let owner = state.owner.as_deref()?;
        let hostile = self.strength_at(province, |other| {
            other.country != country && self.at_war(country, &other.country)
        });
        let friendly = army.strength
            + self.strength_at(province, |other| {
                other.id() != army.id && other.country == country
            });
        let can_engage = hostile > 0 && friendly > hostile;
        if owner == country && can_engage {
            return Some(0);
        }
        if can_engage {
            return Some(1);
        }
        if hostile == 0
            && owner != country
            && self.at_war(country, owner)
            && state.controller.as_deref() != Some(country)
            && !self.province_is_fortified(province)
            && army.strength >= u64::from(REGIMENT_STRENGTH)
        {
            return Some(2);
        }
        None
    }

    /// 无要塞、兵力够、省内没有更强敌军时留在当地，让占领进度继续走。
    fn holding_occupation(&self, country: &str, army: &ArmyView) -> bool {
        let Some(state) = self.province(army.province) else {
            return false;
        };
        let Some(owner) = state.owner.as_deref() else {
            return false;
        };
        let hostile = self.strength_at(army.province, |other| {
            other.country != country && self.at_war(country, &other.country)
        });
        owner != country
            && self.at_war(country, owner)
            && state.controller.as_deref() != Some(country)
            && !self.province_is_fortified(army.province)
            && army.strength >= u64::from(REGIMENT_STRENGTH)
            && hostile < army.strength
    }

    fn recovery_province(&self, country: &str, distances: &BTreeMap<u32, u32>) -> Option<u32> {
        let mut best: Option<(u8, u32, u32)> = None;
        for (&province, &days) in distances {
            let state = self.province(province)?;
            if state.owner.as_deref() != Some(country) {
                continue;
            }
            let controlled = u8::from(state.controller.as_deref() != Some(country));
            let candidate = (controlled, days, province);
            if best.is_none_or(|current| candidate < current) {
                best = Some(candidate);
            }
        }
        best.map(|(_, _, province)| province)
    }

    fn strength_at(&self, province: u32, include: impl Fn(&crate::Army) -> bool) -> u64 {
        self.armies()
            .values()
            .filter(|army| {
                army.province == province
                    && !army.is_retreating()
                    && !army.is_exiled()
                    && army.strength() > 0
                    && include(army)
            })
            .map(crate::Army::strength)
            .sum()
    }

    fn army_view(&self, id: u32, max_morale: Option<u32>) -> Option<ArmyView> {
        let army = self.armies().get(&id)?;
        let morale_ready = max_morale.is_some_and(|max| {
            u64::from(army.average_morale(max)).saturating_mul(2) >= u64::from(max)
        });
        Some(ArmyView {
            id,
            country: army.country.clone(),
            province: army.province,
            strength: army.strength(),
            speed: army.march_speed(&self.military_rules),
            moving: army.movement.is_some(),
            in_battle: army.battle_id().is_some(),
            retreating: army.is_retreating(),
            exiled: army.is_exiled(),
            morale_ready,
        })
    }
}

struct ArmyView {
    id: u32,
    country: String,
    province: u32,
    strength: u64,
    speed: Money,
    moving: bool,
    in_battle: bool,
    retreating: bool,
    exiled: bool,
    morale_ready: bool,
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use crate::{
        Army, CombatRules, Country, Development, GameDate, GameSpeed, LandConnection,
        ManpowerRules, MilitaryRules, Money, Province, ProvinceKind, Regiment, SavedGame,
        Simulation, UnitKind,
    };

    use super::World;

    fn date() -> GameDate {
        GameDate::new(1444, 11, 11).unwrap()
    }

    fn province(owner: &str) -> Province {
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = Some(owner.to_owned());
        province.controller = Some(owner.to_owned());
        province.development = Development {
            tax: Some(3),
            production: Some(3),
            manpower: Some(3),
        };
        province
    }

    fn rules(forcelimit: bool, maintenance: bool) -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 60,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(1),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: if maintenance {
                Money::from(100)
            } else {
                Money::ZERO
            },
            monthly_reinforce: Money::ZERO,
            extra_land_reinforce_cost: Money::ZERO,
            supply_base: 6,
            base_land_forcelimit: Money::ZERO,
            development_land_forcelimit: if forcelimit { Money::ONE } else { Money::ZERO },
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
            starting_army_size: Money::ZERO,
            starting_army_size_at_war: Money::ZERO,
            manpower: ManpowerRules {
                base_capacity: 20_000,
                per_development: 0,
                recovery_months: 10,
                minimum_monthly: 1,
                infantry_cost: 1_000,
                cavalry_cost: 1_000,
            },
            combat: CombatRules::baseline(),
        }
    }

    fn link(from: u32, to: u32, distance: u32) -> LandConnection {
        LandConnection { from, to, distance }
    }

    fn sample(maintenance: bool, links: Vec<LandConnection>) -> World {
        let mut world = World::new(
            [
                Country::new("AAA"),
                Country::new("BBB"),
                Country::new("CCC"),
            ],
            BTreeMap::from([
                (1, province("AAA")),
                (2, province("BBB")),
                (3, province("CCC")),
            ]),
            [1, 2, 3],
            Vec::<crate::BuildingDefinition>::new(),
            rules(true, maintenance),
            links,
        )
        .unwrap();
        for tag in ["AAA", "BBB", "CCC"] {
            world
                .countries
                .get_mut(tag)
                .unwrap()
                .finances
                .refund(Money::from(1_000));
        }
        world
    }

    fn station(world: &mut World, id: u32, country: &str, province: u32, regiments: usize) {
        world.military.armies.insert(
            id,
            Army {
                id,
                country: country.to_owned(),
                province,
                regiments: (0..regiments)
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

    fn advance(world: &mut World, days: u32, speed: GameSpeed) -> Simulation {
        let mut simulation = Simulation::new(date());
        simulation.set_paused(false);
        simulation.set_speed(speed);
        let elapsed = Duration::from_secs(u64::from(days)) / speed.multiplier();
        assert_eq!(simulation.advance_for(world, elapsed, Some("AAA")), days);
        simulation
    }

    #[test]
    fn neighbor_orders_its_own_army_and_leaves_the_player_alone() {
        let mut world = sample(false, vec![link(1, 2, 10), link(2, 3, 1), link(3, 1, 30)]);
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 2, 2);
        station(&mut world, 3, "CCC", 3, 1);
        advance(&mut world, 1, GameSpeed::Normal);

        assert!(world.armies()[&1].movement.is_none());
        assert!(world.armies()[&3].movement.is_none());
        let movement = world.armies()[&2].movement.as_ref().unwrap();
        assert_eq!(movement.destination(), 1);
        assert!(
            movement
                .legs()
                .iter()
                .all(|leg| leg.from != 3 && leg.to != 3)
        );
        let ongoing = world.armies()[&2].movement.clone();
        advance(&mut world, 2, GameSpeed::Normal);
        assert_eq!(world.armies()[&2].province, 2);
        assert_eq!(world.armies()[&2].movement, ongoing);
    }

    #[test]
    fn closed_neutral_border_is_not_crossed_and_weak_or_tired_armies_stay() {
        let mut blocked = sample(false, vec![link(2, 3, 1), link(3, 1, 1)]);
        blocked.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut blocked, 1, "AAA", 1, 1);
        station(&mut blocked, 2, "BBB", 2, 2);
        advance(&mut blocked, 1, GameSpeed::Normal);
        assert!(blocked.armies()[&2].movement.is_none());
        assert_eq!(blocked.armies()[&2].province, 2);

        let mut weaker = sample(false, vec![link(1, 2, 1)]);
        weaker.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut weaker, 1, "AAA", 1, 3);
        station(&mut weaker, 2, "BBB", 2, 1);
        advance(&mut weaker, 1, GameSpeed::Normal);
        assert!(weaker.armies()[&2].movement.is_none());

        let mut tired = sample(false, vec![link(1, 2, 1)]);
        tired.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut tired, 1, "AAA", 1, 1);
        station(&mut tired, 2, "BBB", 2, 2);
        tired.military.armies.get_mut(&2).unwrap().regiments[0].morale = 100;
        tired.military.armies.get_mut(&2).unwrap().retreating = true;
        tired.military.armies.get_mut(&2).unwrap().retreat_shattered = Some(true);
        advance(&mut tired, 1, GameSpeed::Normal);
        assert!(tired.armies()[&2].movement.is_none());
        assert!(tired.armies()[&2].is_retreating());
    }

    #[test]
    fn low_morale_does_not_reenter_the_enemy_province() {
        let mut world = sample(false, vec![link(1, 2, 1)]);
        world.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut world, 2, "BBB", 1, 2);
        for regiment in &mut world.military.armies.get_mut(&2).unwrap().regiments {
            regiment.morale = 100;
        }
        advance(&mut world, 1, GameSpeed::Normal);
        let movement = world.armies()[&2].movement.as_ref().unwrap();
        assert_eq!(movement.destination(), 2);
        assert!(
            movement
                .legs()
                .iter()
                .all(|leg| leg.to != 1 || leg.from == 1)
        );
    }

    #[test]
    fn recruitment_uses_the_same_limits_and_is_not_repeated() {
        let mut world = sample(false, vec![link(1, 2, 1)]);
        world.declare_war("AAA", "BBB", date()).unwrap();
        let before = world.country("BBB").unwrap().finances.treasury();
        let simulation = advance(&mut world, 1, GameSpeed::Normal);
        assert_eq!(world.recruitment_queue(2).len(), 1);
        assert_eq!(world.recruitment_queue(2)[0].country, "BBB");
        assert_eq!(
            world.country("BBB").unwrap().finances.treasury(),
            before - Money::from(10)
        );
        let spent = world.country("BBB").unwrap().finances.treasury();
        world.direct_war_ai(Some("AAA"), simulation.clock().date());
        assert_eq!(world.recruitment_queue(2).len(), 1);
        assert_eq!(world.country("BBB").unwrap().finances.treasury(), spent);

        let mut paused = Simulation::new(simulation.clock().date());
        paused.set_speed(GameSpeed::Octuple);
        assert_eq!(
            paused.advance_for(&mut world, Duration::from_secs(3), Some("AAA")),
            0
        );
        assert_eq!(world.country("BBB").unwrap().finances.treasury(), spent);

        let mut broke = sample(false, vec![]);
        broke.declare_war("AAA", "BBB", date()).unwrap();
        broke.countries.get_mut("BBB").unwrap().finances = Default::default();
        advance(&mut broke, 1, GameSpeed::Normal);
        assert!(broke.recruitment_queue(2).is_empty());
        assert_eq!(
            broke.country("BBB").unwrap().finances.treasury(),
            Money::ZERO
        );

        let mut no_men = sample(false, vec![]);
        no_men.declare_war("AAA", "BBB", date()).unwrap();
        no_men
            .countries
            .get_mut("BBB")
            .unwrap()
            .manpower
            .as_mut()
            .unwrap()
            .available = 0;
        let rich = no_men.country("BBB").unwrap().finances.treasury();
        advance(&mut no_men, 1, GameSpeed::Normal);
        assert!(no_men.recruitment_queue(2).is_empty());
        assert_eq!(no_men.country("BBB").unwrap().finances.treasury(), rich);

        let mut over_budget = sample(true, vec![]);
        over_budget.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut over_budget, 2, "BBB", 2, 1);
        let rich = over_budget.country("BBB").unwrap().finances.treasury();
        advance(&mut over_budget, 1, GameSpeed::Normal);
        assert!(over_budget.recruitment_queue(2).is_empty());
        assert_eq!(
            over_budget.country("BBB").unwrap().finances.treasury(),
            rich
        );
    }

    #[test]
    fn speed_and_save_do_not_repeat_the_day() {
        let links = vec![link(1, 2, 10)];
        let mut slow = sample(false, links.clone());
        slow.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut slow, 2, "BBB", 2, 2);
        advance(&mut slow, 1, GameSpeed::Normal);

        let mut fast = sample(false, links);
        fast.declare_war("AAA", "BBB", date()).unwrap();
        station(&mut fast, 2, "BBB", 2, 2);
        advance(&mut fast, 1, GameSpeed::Octuple);

        assert_eq!(
            slow.country("BBB").unwrap().finances.treasury(),
            fast.country("BBB").unwrap().finances.treasury()
        );
        assert_eq!(slow.armies()[&2].movement, fast.armies()[&2].movement);
        assert_eq!(
            slow.recruitment_queue(2).len(),
            fast.recruitment_queue(2).len()
        );

        let saved = SavedGame::capture(
            &slow,
            &Simulation::new(date().checked_add_days(1).unwrap()),
            "AAA",
        );
        let restored = saved.restore(&slow).unwrap();
        assert_eq!(
            restored.world.country("BBB").unwrap().finances.treasury(),
            slow.country("BBB").unwrap().finances.treasury()
        );
        assert_eq!(
            restored.world.armies()[&2].movement,
            slow.armies()[&2].movement
        );
        assert_eq!(restored.world.recruitment_queue(2).len(), 1);
    }

    #[test]
    fn favorable_cession_and_losing_white_peace_follow_the_shared_rule() {
        let mut winning = sample(false, vec![link(1, 2, 1)]);
        winning.declare_war("AAA", "BBB", date()).unwrap();
        let aaa = winning.provinces.get_mut(&1).unwrap();
        aaa.controller = Some("BBB".to_owned());
        advance(&mut winning, 1, GameSpeed::Normal);
        assert!(!winning.at_war("AAA", "BBB"));
        assert_eq!(winning.province(1).unwrap().owner.as_deref(), Some("BBB"));
        assert!(winning.recruitment_queue(2).is_empty());

        let mut losing = sample(false, vec![link(1, 2, 1)]);
        losing.declare_war("AAA", "BBB", date()).unwrap();
        losing.provinces.get_mut(&2).unwrap().controller = Some("AAA".to_owned());
        station(&mut losing, 2, "BBB", 2, 2);
        advance(&mut losing, 1, GameSpeed::Normal);
        assert!(!losing.at_war("AAA", "BBB"));
        assert_eq!(losing.province(2).unwrap().owner.as_deref(), Some("BBB"));
        assert_eq!(
            losing.province(2).unwrap().controller.as_deref(),
            Some("BBB")
        );
    }
}
