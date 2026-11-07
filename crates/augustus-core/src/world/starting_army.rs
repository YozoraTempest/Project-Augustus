// SPDX-License-Identifier: GPL-3.0-only

//! 新开局的陆军。读档不走这里，军事状态整份来自存档。

use crate::{Army, Money, ProvinceKind, REGIMENT_STRENGTH, Regiment, UnitKind};

use super::World;

impl World {
    /// 为有合法驻地的国家各生成一支满编陆军。
    ///
    /// 团数是已取整陆军上限乘开局比例后再向下取整。和平与交战使用不同比例。
    /// 不扣国库，也不扣人力；随后的维护、补给、损耗、拆分合并和补员与招募部队相同。
    /// 国家按标签顺序分配编号。相同的开局状态会得到相同编制。
    pub fn spawn_starting_armies(&mut self) {
        let tags: Vec<String> = self.countries.keys().cloned().collect();
        for tag in tags {
            let fraction = if self.wars_of(&tag).next().is_some() {
                self.military_rules.starting_army_size_at_war
            } else {
                self.military_rules.starting_army_size
            };
            let count = self
                .country_land_force_limit(&tag)
                .map(|limit| starting_regiment_count(limit, fraction))
                .unwrap_or(0);
            let Some(province) = self.starting_station(&tag) else {
                continue;
            };
            if count == 0 || self.military.next_army_id == u32::MAX {
                continue;
            }
            let id = self.military.next_army_id;
            self.military.next_army_id += 1;
            // 骑兵比例、火炮和叛军威胁都还没有可核对的开局输入，正规团全部使用步兵。
            let morale = self
                .military_rules
                .combat
                .morale_milli()
                .expect("陆战规则已检查");
            let regiments = std::iter::repeat_with(|| Regiment {
                kind: UnitKind::Infantry,
                strength: REGIMENT_STRENGTH,
                morale,
            })
            .take(usize::try_from(count).expect("开局团数在支持范围内"))
            .collect();
            self.military.armies.insert(
                id,
                Army {
                    id,
                    country: tag,
                    province,
                    regiments,
                    movement: None,
                    battle_id: None,
                    retreating: false,
                },
            );
        }
    }

    /// 首都若是本国有效陆地就驻在首都，否则用编号最小的本国有效陆地。
    fn starting_station(&self, tag: &str) -> Option<u32> {
        let owned_land = |id: u32| {
            self.active_provinces.contains(&id)
                && self.provinces.get(&id).is_some_and(|province| {
                    province.kind == ProvinceKind::Land && province.owner.as_deref() == Some(tag)
                })
        };
        self.countries
            .get(tag)?
            .capital
            .filter(|&id| owned_land(id))
            .or_else(|| {
                self.active_provinces
                    .iter()
                    .copied()
                    .find(|&id| owned_land(id))
            })
    }
}

/// 比例小于等于零、或乘积不足一团时不生成空军队。
fn starting_regiment_count(limit: u64, fraction: Money) -> u64 {
    if fraction <= Money::ZERO || limit == 0 {
        return 0;
    }
    let count = (Money::from(limit) * fraction).floor();
    if count <= Money::ZERO {
        return 0;
    }
    u64::try_from(count).expect("开局团数在支持范围内")
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use crate::{
        ArmyOrganizationError, BattleOutcome, CombatRules, Country, Development, GameDate,
        LandConnection, ManpowerRules, MilitaryRules, Money, Province, ProvinceKind,
        REGIMENT_STRENGTH, SavedGame, Simulation, UnitKind, World, WorldError,
    };

    fn date() -> GameDate {
        GameDate::new(1444, 11, 11).unwrap()
    }

    fn rules(peace: i64, at_war: i64) -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 60,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(2),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: Money::from(1),
            monthly_reinforce: Money::new(1, 1),
            extra_land_reinforce_cost: Money::from(2),
            supply_base: 6,
            base_land_forcelimit: Money::from(8),
            development_land_forcelimit: Money::ZERO,
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
            starting_army_size: Money::new(peace, 2),
            starting_army_size_at_war: Money::new(at_war, 2),
            manpower: ManpowerRules {
                base_capacity: 10_000,
                per_development: 0,
                recovery_months: 10,
                minimum_monthly: 100,
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

    fn sample() -> World {
        let mut austria = Country::new("HAB");
        austria.capital = Some(1);
        let mut france = Country::new("FRA");
        france.capital = Some(9);
        let mut empty = Country::new("AAA");
        empty.capital = Some(1);
        let sea = Province::new(ProvinceKind::Sea);
        let wasteland = Province::new(ProvinceKind::Wasteland);
        World::new(
            [austria, france, empty],
            [
                (1, land("HAB", 1)),
                (2, land("FRA", 1)),
                (3, land("FRA", 1)),
                (8, wasteland),
                (9, sea),
            ]
            .into(),
            [1, 2, 3, 8, 9],
            [],
            rules(75, 100),
            [
                LandConnection {
                    from: 1,
                    to: 2,
                    distance: 10,
                },
                LandConnection {
                    from: 2,
                    to: 3,
                    distance: 10,
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn neighboring_countries_get_stable_armies_without_a_recruitment_charge() {
        let mut world = sample();
        let manpower = world.country_manpower("FRA").unwrap().available;
        let treasury = world.country("FRA").unwrap().finances.treasury();
        world.spawn_starting_armies();
        let again = {
            let mut second = sample();
            second.spawn_starting_armies();
            second
                .armies()
                .values()
                .map(|army| {
                    (
                        army.id(),
                        army.country.clone(),
                        army.province,
                        army.regiments.len(),
                    )
                })
                .collect::<Vec<_>>()
        };
        let armies: Vec<_> = world
            .armies()
            .values()
            .map(|army| {
                (
                    army.id(),
                    army.country.clone(),
                    army.province,
                    army.regiments.len(),
                )
            })
            .collect();
        assert_eq!(armies, again);
        assert_eq!(
            armies,
            vec![(1, "FRA".to_owned(), 2, 6), (2, "HAB".to_owned(), 1, 6),]
        );
        assert!(world.armies().values().all(|army| {
            army.movement.is_none()
                && army.regiments.iter().all(|regiment| {
                    regiment.kind == UnitKind::Infantry && regiment.strength == REGIMENT_STRENGTH
                })
        }));
        assert_eq!(world.country("FRA").unwrap().finances.treasury(), treasury);
        assert_eq!(world.country_manpower("FRA").unwrap().available, manpower);
        assert!(world.armies().values().all(|army| army.country != "AAA"));
    }

    #[test]
    fn wartime_fraction_uses_owned_land_and_only_the_owner_can_command() {
        let mut world = sample();
        world.declare_war("HAB", "FRA", date()).unwrap();
        world.spawn_starting_armies();
        let france_id = world
            .armies()
            .values()
            .find(|army| army.country == "FRA")
            .unwrap()
            .id();
        assert_eq!(world.armies()[&france_id].province, 2);
        assert_eq!(world.armies()[&france_id].regiments.len(), 8);
        assert!(world.land_neighbors(1).unwrap().contains_key(&2));
        assert_eq!(
            world.split_army("HAB", france_id, 1),
            Err(ArmyOrganizationError::NotPlayerArmy)
        );
        let detached = world.split_army("FRA", france_id, 1).unwrap();
        assert_eq!(world.armies()[&detached].regiments.len(), 1);
        assert_eq!(world.armies()[&france_id].regiments.len(), 7);
    }

    #[test]
    fn starting_armies_take_attrition_maintenance_and_round_trip_in_saves() {
        let mut world = sample();
        world.declare_war("HAB", "FRA", date()).unwrap();
        world.spawn_starting_armies();
        let id = world
            .armies()
            .values()
            .find(|army| army.country == "FRA")
            .unwrap()
            .id();
        let before = world.armies()[&id].strength();
        let saved = SavedGame::capture(&world, &Simulation::new(date()), "FRA");
        let mut simulation = Simulation::new(date());
        simulation.set_paused(false);
        simulation.advance(&mut world, Duration::from_secs(20));
        assert!(world.armies()[&id].strength() < before);
        assert!(world.country("FRA").unwrap().finances.treasury() < Money::ZERO);
        let restored = saved.restore(&sample()).unwrap();
        assert_eq!(restored.world.armies()[&id].strength(), before);
        assert_eq!(restored.world.armies().len(), 2);
        assert_eq!(
            restored.world.country("FRA").unwrap().finances.treasury(),
            Money::ZERO
        );

        let mut bare = sample();
        let empty = SavedGame::capture(&bare, &Simulation::new(date()), "FRA");
        bare.spawn_starting_armies();
        let loaded = empty.restore(&bare).unwrap();
        assert!(loaded.world.armies().is_empty());
    }

    #[test]
    fn starting_armies_fight_on_contact_then_retreat_and_reload() {
        let mut world = sample();
        let date = date();
        world.declare_war("HAB", "FRA", date).unwrap();
        world.spawn_starting_armies();
        let france_id = world
            .armies()
            .values()
            .find(|army| army.country == "FRA")
            .unwrap()
            .id();
        let austria_id = world
            .armies()
            .values()
            .find(|army| army.country == "HAB")
            .unwrap()
            .id();
        let max = world.military_rules().combat.morale_milli().unwrap();
        assert!(world.armies().values().all(|army| {
            army.battle_id().is_none()
                && !army.is_retreating()
                && army.regiments.iter().all(|regiment| regiment.morale == max)
        }));
        world.resolve_battles(date);
        assert!(world.battle_in_province(1).is_none());
        assert!(world.battle_in_province(2).is_none());

        let quoted = world.quote_movement("FRA", france_id, 1, date).unwrap();
        let travel = date.days_until(quoted.legs().last().unwrap().arrives_on);
        assert!(travel > 1);
        let travel = u64::try_from(travel).unwrap();
        let before = world.armies()[&france_id].strength();
        world.move_army("FRA", france_id, 1, date).unwrap();
        let mut simulation = Simulation::new(date);
        simulation.set_paused(false);
        assert_eq!(
            simulation.advance(&mut world, Duration::from_secs(travel - 1)),
            u32::try_from(travel - 1).unwrap()
        );
        assert_eq!(world.armies()[&france_id].province, 2);
        assert!(world.battle_in_province(1).is_none());
        assert_eq!(simulation.advance(&mut world, Duration::from_secs(1)), 1);
        assert_eq!(world.armies()[&france_id].province, 1);
        assert!(world.armies()[&france_id].battle_id().is_some());
        assert!(world.battle_in_province(1).is_some());
        let damaged = world.armies()[&france_id].strength();
        assert!(damaged < before);
        assert!(world.armies()[&austria_id].strength() < before);

        let battle_date = simulation.clock().date();
        let saved = SavedGame::capture(&world, &simulation, "FRA");
        let restored = saved.restore(&sample()).unwrap();
        assert_eq!(restored.world.armies()[&france_id].strength(), damaged);
        assert!(restored.world.battle_in_province(1).is_some());
        let mut continued = restored.world;
        continued.resolve_battles(battle_date);
        assert_eq!(continued.armies()[&france_id].strength(), damaged);

        world
            .countries
            .get_mut("FRA")
            .unwrap()
            .manpower
            .as_mut()
            .unwrap()
            .available = 0;
        world.reinforce_armies();
        assert_eq!(world.armies()[&france_id].strength(), damaged);

        for regiment in &mut world.military.armies.get_mut(&france_id).unwrap().regiments {
            regiment.morale = 1;
        }
        let next = battle_date.checked_add_days(1).unwrap();
        world.resolve_battles(next);
        assert!(matches!(
            world.battle_report(1).unwrap().outcome,
            BattleOutcome::Retreat { loser: 0 }
        ));
        let loser = &world.armies()[&france_id];
        assert!(loser.is_retreating());
        assert!(loser.battle_id().is_none());
        assert_eq!(loser.province, 1);
        assert_eq!(loser.movement.as_ref().unwrap().destination(), 3);
        assert!(world.armies()[&austria_id].battle_id().is_none());
        world.resolve_battles(next);
        assert!(world.battle_in_province(1).is_none());
        assert!(world.armies()[&france_id].is_retreating());
    }

    #[test]
    fn negative_starting_fraction_is_rejected() {
        let mut military = rules(75, 100);
        military.starting_army_size = Money::new(-1, 2);
        let result = World::new(
            [Country::new("FRA")],
            [(1, land("FRA", 1))].into(),
            [1],
            [],
            military,
            [],
        );
        assert!(matches!(result, Err(WorldError::InvalidMilitaryRules)));
    }

    #[test]
    fn fraction_below_one_regiment_creates_no_army() {
        let mut military = rules(50, 100);
        military.base_land_forcelimit = Money::from(1);
        let mut world = World::new(
            [Country::new("FRA")],
            [(1, land("FRA", 1))].into(),
            [1],
            [],
            military,
            [],
        )
        .unwrap();
        world.spawn_starting_armies();
        assert!(world.armies().is_empty());
        assert_eq!(
            world.country("FRA").unwrap().finances.treasury(),
            Money::ZERO
        );
    }
}
