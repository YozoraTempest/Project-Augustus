// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeSet;

use crate::{GameDate, SaveError, War, WarError, WarState};

use super::World;

impl World {
    pub(crate) fn clone_wars(&self) -> WarState {
        self.war_state.clone()
    }

    pub fn at_war(&self, left: &str, right: &str) -> bool {
        left != right && self.war_between(left, right).is_some()
    }

    pub fn war_between(&self, left: &str, right: &str) -> Option<&War> {
        self.war_state
            .active
            .values()
            .find(|war| war.binds(left, right))
    }

    pub fn wars_of<'a>(&'a self, country: &'a str) -> impl Iterator<Item = &'a War> + 'a {
        self.war_state
            .active
            .values()
            .filter(move |war| war.involves(country))
    }

    /// 拒绝时不修改已有战争、编号或开始日期。
    pub fn declaration_error(&self, attacker: &str, defender: &str) -> Result<(), WarError> {
        if self.country(attacker).is_none() || self.country(defender).is_none() {
            return Err(WarError::UnknownCountry);
        }
        if attacker == defender {
            return Err(WarError::SelfTarget);
        }
        if self.at_war(attacker, defender) {
            return Err(WarError::AlreadyAtWar);
        }
        if self.war_state.next_id == u32::MAX {
            return Err(WarError::WarLimit);
        }
        Ok(())
    }

    pub fn declare_war(
        &mut self,
        attacker: &str,
        defender: &str,
        started_on: GameDate,
    ) -> Result<u32, WarError> {
        self.declaration_error(attacker, defender)?;
        Ok(self.war_state.insert(attacker, defender, started_on))
    }

    pub(crate) fn restore_wars(&mut self, wars: WarState, date: GameDate) -> Result<(), SaveError> {
        let mut pairs = BTreeSet::new();
        for (&id, war) in &wars.active {
            if id == 0
                || id != war.id()
                || war.attacker() == war.defender()
                || war.started_on() > date
                || self.country(war.attacker()).is_none()
                || self.country(war.defender()).is_none()
            {
                return Err(SaveError::InvalidWar);
            }
            let mut pair = [war.attacker(), war.defender()];
            pair.sort_unstable();
            if !pairs.insert((pair[0].to_owned(), pair[1].to_owned())) {
                return Err(SaveError::InvalidWar);
            }
        }
        if wars.next_id == 0
            || wars
                .active
                .keys()
                .next_back()
                .is_some_and(|id| *id >= wars.next_id)
        {
            return Err(SaveError::InvalidWar);
        }
        self.war_state = wars;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, time::Duration};

    use serde_json::json;

    use crate::{
        Army, ArmyMovement, ArmyOrganizationError, CombatRules, Country, Development, GameDate,
        LandConnection, ManpowerRules, MilitaryRules, Money, MovementError, MovementLeg, Province,
        ProvinceKind, RecruitmentError, Regiment, SaveError, SavedGame, Simulation, UnitKind,
        WarError, WarStatus,
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

    fn rules() -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 60,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(1),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: Money::ZERO,
            monthly_reinforce: Money::ZERO,
            extra_land_reinforce_cost: Money::ZERO,
            supply_base: 6,
            base_land_forcelimit: Money::ZERO,
            development_land_forcelimit: Money::ZERO,
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
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

    fn sample() -> World {
        World::new(
            [
                Country::new("AAA"),
                Country::new("BBB"),
                Country::new("CCC"),
            ],
            BTreeMap::from([
                (1, province("AAA")),
                (5, province("AAA")),
                (2, province("BBB")),
                (4, province("BBB")),
                (3, province("CCC")),
            ]),
            [1, 2, 3, 4, 5],
            Vec::<crate::BuildingDefinition>::new(),
            rules(),
            [
                LandConnection {
                    from: 1,
                    to: 5,
                    distance: 1,
                },
                LandConnection {
                    from: 1,
                    to: 2,
                    distance: 1,
                },
                LandConnection {
                    from: 2,
                    to: 4,
                    distance: 1,
                },
                LandConnection {
                    from: 1,
                    to: 3,
                    distance: 1,
                },
                LandConnection {
                    from: 3,
                    to: 4,
                    distance: 1,
                },
            ],
        )
        .unwrap()
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
            },
        );
        world.military.next_army_id = world.military.next_army_id.max(id + 1);
    }

    fn facts(world: &World, country: &str) -> Vec<(u32, String, String, GameDate, WarStatus)> {
        world
            .wars_of(country)
            .map(|war| {
                (
                    war.id(),
                    war.attacker().to_owned(),
                    war.defender().to_owned(),
                    war.started_on(),
                    war.status(),
                )
            })
            .collect()
    }

    #[test]
    fn declaration_matches_on_both_sides_and_rejects_invalid_targets() {
        let mut world = sample();
        let date = date();
        let id = world.declare_war("AAA", "BBB", date).unwrap();
        assert_eq!(id, 1);
        let expected = vec![(
            1,
            "AAA".to_owned(),
            "BBB".to_owned(),
            date,
            WarStatus::Ongoing,
        )];
        assert_eq!(facts(&world, "AAA"), expected);
        assert_eq!(facts(&world, "BBB"), expected);
        assert!(world.at_war("AAA", "BBB"));
        assert!(world.at_war("BBB", "AAA"));
        assert!(!world.at_war("AAA", "CCC"));
        assert!(!world.at_war("AAA", "AAA"));

        let unchanged = facts(&world, "AAA");
        assert_eq!(
            world.declare_war("AAA", "BBB", date.checked_add_days(3).unwrap()),
            Err(WarError::AlreadyAtWar)
        );
        assert_eq!(
            world.declare_war("BBB", "AAA", date),
            Err(WarError::AlreadyAtWar)
        );
        assert_eq!(
            world.declare_war("AAA", "AAA", date),
            Err(WarError::SelfTarget)
        );
        assert_eq!(
            world.declare_war("AAA", "ZZZ", date),
            Err(WarError::UnknownCountry)
        );
        assert_eq!(
            world.declare_war("ZZZ", "ZZZ", date),
            Err(WarError::UnknownCountry)
        );
        assert_eq!(facts(&world, "AAA"), unchanged);
        assert_eq!(world.war_state.next_id, 2);

        let second = world.declare_war("AAA", "CCC", date).unwrap();
        assert_eq!(second, 2);
        assert_eq!(facts(&world, "BBB"), unchanged);
        assert!(world.at_war("AAA", "CCC"));
    }

    #[test]
    fn march_enters_enemy_territory_without_crossing_neutrals() {
        let mut world = sample();
        let date = date();
        station(&mut world, 1, "AAA", 1, 1);
        station(&mut world, 2, "BBB", 2, 1);
        let own = world.quote_movement("AAA", 1, 5, date).unwrap();
        assert_eq!(
            own.legs().iter().map(|leg| leg.to).collect::<Vec<_>>(),
            vec![5]
        );
        assert_eq!(
            world.quote_movement("AAA", 1, 2, date),
            Err(MovementError::NoAccess)
        );
        assert_eq!(
            world.quote_movement("BBB", 2, 1, date),
            Err(MovementError::NoAccess)
        );
        assert!(!world.can_enter("AAA", 2));
        assert!(!world.can_enter("AAA", 3));

        world.declare_war("AAA", "BBB", date).unwrap();
        assert!(world.can_enter("AAA", 2));
        assert!(world.can_enter("AAA", 4));
        assert!(world.can_enter("BBB", 1));
        assert!(!world.can_enter("AAA", 3));
        assert!(!world.can_enter("BBB", 3));
        world.provinces.get_mut(&1).unwrap().controller = Some("BBB".to_owned());
        assert!(!world.can_enter("AAA", 1));
        assert!(world.can_enter("BBB", 1));
        world.provinces.get_mut(&1).unwrap().controller = Some("AAA".to_owned());

        let movement = world.quote_movement("AAA", 1, 4, date).unwrap();
        assert_eq!(
            std::iter::once(movement.legs()[0].from)
                .chain(movement.legs().iter().map(|leg| leg.to))
                .collect::<Vec<_>>(),
            vec![1, 2, 4]
        );
        assert_eq!(
            world.quote_movement("AAA", 1, 3, date),
            Err(MovementError::NoAccess)
        );
        world.quote_movement("BBB", 2, 1, date).unwrap();

        world.move_army("AAA", 1, 4, date).unwrap();
        let mut simulation = Simulation::new(date);
        simulation.set_paused(false);
        assert_eq!(simulation.advance(&mut world, Duration::from_secs(2)), 2);
        // 途经省份有敌军时停下接战，不再穿过该省继续前往原目的地。
        assert_eq!(world.armies()[&1].province, 2);
        assert!(world.armies()[&1].movement.is_none());
        assert!(world.armies()[&1].battle_id().is_some());
        assert!(world.battle_in_province(2).is_some());
    }

    #[test]
    fn advance_stops_at_a_closed_border() {
        let mut world = sample();
        let date = date();
        station(&mut world, 1, "AAA", 1, 1);
        world.declare_war("AAA", "BBB", date).unwrap();
        world.military.armies.get_mut(&1).unwrap().movement = Some(ArmyMovement {
            legs: vec![
                MovementLeg {
                    from: 1,
                    to: 2,
                    started_on: date,
                    arrives_on: date,
                },
                MovementLeg {
                    from: 2,
                    to: 3,
                    started_on: date,
                    arrives_on: date,
                },
            ],
            leg_index: 0,
        });
        world.complete_movements(date);
        let army = &world.armies()[&1];
        assert_eq!(army.province, 2);
        assert!(army.movement.is_none());
    }

    #[test]
    fn save_keeps_wars_and_old_files_still_load() {
        let mut world = sample();
        let date = date();
        station(&mut world, 1, "AAA", 1, 1);
        world.move_army("AAA", 1, 5, date).unwrap();
        let peaceful = SavedGame::capture(&world, &Simulation::new(date), "AAA");
        let mut legacy = serde_json::to_value(&peaceful).unwrap();
        assert!(legacy.as_object_mut().unwrap().remove("wars").is_some());
        let restored = serde_json::from_value::<SavedGame>(legacy)
            .unwrap()
            .restore(&world)
            .unwrap();
        assert!(!restored.world.at_war("AAA", "BBB"));
        assert_eq!(
            restored.world.armies()[&1]
                .movement
                .as_ref()
                .unwrap()
                .destination(),
            5
        );

        // 旧档检查时军队正在前往本国省份。敌境行军从驻扎状态重新下达。
        let army = world.military.armies.get_mut(&1).unwrap();
        army.province = 1;
        army.movement = None;
        world.declare_war("AAA", "BBB", date).unwrap();
        world.move_army("AAA", 1, 4, date).unwrap();
        let saved = SavedGame::capture(&world, &Simulation::new(date), "AAA");
        let restored = serde_json::from_value::<SavedGame>(serde_json::to_value(&saved).unwrap())
            .unwrap()
            .restore(&world)
            .unwrap();
        assert_eq!(facts(&restored.world, "AAA"), facts(&world, "AAA"));
        assert_eq!(
            restored.world.armies()[&1].movement,
            world.armies()[&1].movement
        );
        let crate::RestoredGame {
            world: mut restored_world,
            simulation: mut restored_simulation,
            ..
        } = restored;
        restored_simulation.set_paused(false);
        assert_eq!(
            restored_simulation.advance(&mut restored_world, Duration::from_secs(2)),
            2
        );
        assert_eq!(restored_world.armies()[&1].province, 4);
        assert!(restored_world.at_war("BBB", "AAA"));

        let mut stripped = serde_json::to_value(&saved).unwrap();
        stripped.as_object_mut().unwrap().remove("wars");
        let stripped_result = serde_json::from_value::<SavedGame>(stripped)
            .unwrap()
            .restore(&world);
        let Err(error) = stripped_result else {
            panic!("失去战争关系后，进入敌境的行军不能恢复");
        };
        assert!(matches!(error, SaveError::InvalidMilitary));

        let mut duplicated = serde_json::to_value(&saved).unwrap();
        let war = duplicated["wars"]["active"]["1"].clone();
        duplicated["wars"]["active"]["2"] = json!({
            "id": 2,
            "attacker": war["defender"],
            "defender": war["attacker"],
            "started_on": war["started_on"],
            "status": "ongoing",
        });
        duplicated["wars"]["next_id"] = json!(3);
        let duplicated_result = serde_json::from_value::<SavedGame>(duplicated)
            .unwrap()
            .restore(&world);
        let Err(error) = duplicated_result else {
            panic!("同一对国家不能有两场战争");
        };
        assert!(matches!(error, SaveError::InvalidWar));
    }

    #[test]
    fn foreign_armies_and_recruitment_cannot_be_commanded() {
        let mut world = sample();
        let date = date();
        station(&mut world, 2, "BBB", 2, 2);
        world
            .countries
            .get_mut("BBB")
            .unwrap()
            .finances
            .refund(Money::from(100));
        world
            .start_recruitment("BBB", 2, UnitKind::Infantry, date)
            .unwrap();
        let order = world.recruitment(2).unwrap().id();
        assert_eq!(
            world.cancel_recruitment("AAA", 2, order, date),
            Err(RecruitmentError::NotPlayerRecruitment)
        );
        assert_eq!(world.recruitment_queue(2).len(), 1);
        assert_eq!(
            world.split_army("AAA", 2, 1),
            Err(ArmyOrganizationError::NotPlayerArmy)
        );
        assert_eq!(
            world.disband_army("AAA", 2),
            Err(ArmyOrganizationError::NotPlayerArmy)
        );
        assert_eq!(
            world.move_army("AAA", 2, 4, date),
            Err(MovementError::NotPlayerArmy)
        );
        assert_eq!(world.armies()[&2].regiments.len(), 2);
        assert_eq!(world.armies()[&2].province, 2);
        assert!(world.armies().contains_key(&2));
    }
}
