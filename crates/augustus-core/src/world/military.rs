// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Army, GameDate, MilitaryRules, MilitaryState, ProvinceKind, REGIMENT_STRENGTH, Recruitment,
    RecruitmentError, RecruitmentQuote, Regiment, SaveError, UnitKind,
};

use super::World;

impl World {
    pub fn military_rules(&self) -> &MilitaryRules {
        &self.military_rules
    }

    pub fn military(&self) -> &MilitaryState {
        &self.military
    }

    pub fn armies(&self) -> &BTreeMap<u32, Army> {
        &self.military.armies
    }

    pub fn recruitments(&self) -> &BTreeMap<u32, Recruitment> {
        &self.military.recruitments
    }

    pub fn recruitment(&self, province: u32) -> Option<&Recruitment> {
        self.military.recruitments.get(&province)
    }

    pub fn quote_recruitment(
        &self,
        country: &str,
        province: u32,
        kind: UnitKind,
        date: GameDate,
    ) -> Result<RecruitmentQuote, RecruitmentError> {
        let country = self
            .country(country)
            .ok_or(RecruitmentError::UnknownCountry)?;
        let state = self
            .province(province)
            .filter(|state| {
                self.active_provinces.contains(&province) && state.kind == ProvinceKind::Land
            })
            .ok_or(RecruitmentError::InvalidProvince)?;
        if state.owner.as_deref() != Some(country.tag())
            || state.controller.as_deref() != Some(country.tag())
        {
            return Err(RecruitmentError::NotOwnedAndControlled);
        }
        if self.recruitment(province).is_some() {
            return Err(RecruitmentError::AlreadyRecruiting);
        }
        if self.military.next_army_id == u32::MAX {
            return Err(RecruitmentError::ArmyLimit);
        }
        let cost = self.military_rules.cost(kind);
        let available = country.finances.treasury();
        if available < cost {
            return Err(RecruitmentError::InsufficientFunds { cost, available });
        }
        let manpower = self.military_rules.manpower_cost(kind);
        let available = country
            .manpower
            .as_ref()
            .expect("国家人力已初始化")
            .available;
        if available < manpower {
            return Err(RecruitmentError::InsufficientManpower {
                cost: manpower,
                available,
            });
        }
        Ok(RecruitmentQuote {
            cost,
            manpower,
            completes_on: date
                .checked_add_days(self.military_rules.recruitment_days(kind))
                .ok_or(RecruitmentError::DateOutOfRange)?,
        })
    }

    pub fn start_recruitment(
        &mut self,
        country: &str,
        province: u32,
        kind: UnitKind,
        date: GameDate,
    ) -> Result<(), RecruitmentError> {
        let quote = self.quote_recruitment(country, province, kind, date)?;
        let country_state = self.countries.get_mut(country).expect("招募国家已注册");
        // 金币、人力和军队编号都检查通过后，一次性扣款并创建招募项目。
        country_state.finances.spend(quote.cost);
        country_state
            .manpower
            .as_mut()
            .expect("国家人力已初始化")
            .available -= quote.manpower;
        let army_id = self.military.next_army_id;
        self.military.next_army_id += 1;
        self.military.recruitments.insert(
            province,
            Recruitment {
                army_id,
                country: country.to_owned(),
                kind,
                started_on: date,
                completes_on: quote.completes_on,
                paid_cost: Some(quote.cost),
                paid_manpower: Some(quote.manpower),
            },
        );
        Ok(())
    }

    pub(crate) fn complete_recruitments(&mut self, date: GameDate) {
        let ready: Vec<_> = self
            .military
            .recruitments
            .iter()
            .filter(|(_, recruitment)| recruitment.completes_on <= date)
            .map(|(&province, _)| province)
            .collect();
        for province in ready {
            let recruitment = self
                .military
                .recruitments
                .remove(&province)
                .expect("招募已到期");
            let regiment = Regiment {
                kind: recruitment.kind,
                strength: REGIMENT_STRENGTH,
            };
            // 同国同省份的后续部队加入驻军，保留原军队编号。
            if let Some(army) = self.military.armies.values_mut().find(|army| {
                army.country == recruitment.country
                    && army.province == province
                    && army.movement.is_none()
            }) {
                army.regiments.push(regiment);
            } else {
                self.military.armies.insert(
                    recruitment.army_id,
                    Army {
                        id: recruitment.army_id,
                        country: recruitment.country,
                        province,
                        regiments: vec![regiment],
                        movement: None,
                    },
                );
            }
        }
    }

    pub(crate) fn restore_military(
        &mut self,
        mut military: MilitaryState,
        date: GameDate,
    ) -> Result<(), SaveError> {
        let valid_province = |id| {
            self.active_provinces.contains(&id)
                && self
                    .province(id)
                    .is_some_and(|province| province.kind == ProvinceKind::Land)
        };
        let mut ids = BTreeSet::new();
        for (&id, army) in &military.armies {
            if id == 0
                || id != army.id()
                || !ids.insert(id)
                || self.country(&army.country).is_none()
                || !valid_province(army.province)
                || army.regiments.is_empty()
                || army
                    .regiments
                    .iter()
                    .any(|regiment| regiment.strength == 0 || regiment.strength > REGIMENT_STRENGTH)
            {
                return Err(SaveError::InvalidMilitary);
            }
            if let Some(movement) = &army.movement
                && !self.valid_movement(movement, army.province, &army.country, date)
            {
                return Err(SaveError::InvalidMilitary);
            }
        }
        for (&province, project) in &mut military.recruitments {
            // 第 1 至 6 版未保存实付金额；按该兵种的当前规则补齐。纯步兵存档仍是步兵费用。
            let cost = project
                .paid_cost
                .get_or_insert(self.military_rules.cost(project.kind));
            project
                .paid_manpower
                .get_or_insert(self.military_rules.manpower_cost(project.kind));
            if project.army_id == 0
                || !ids.insert(project.army_id)
                || !valid_province(province)
                || self.country(&project.country).is_none()
                || project.started_on > date
                || project.completes_on <= date
                || *cost < crate::Money::ZERO
            {
                return Err(SaveError::InvalidMilitary);
            }
        }
        if military.next_army_id == 0 || ids.last().is_some_and(|id| *id >= military.next_army_id) {
            return Err(SaveError::InvalidMilitary);
        }
        self.military = military;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        Country, Development, GameDate, LandConnection, ManpowerRules, MilitaryRules,
        MilitaryState, Money, Province, ProvinceKind, RecruitmentError, SavedGame, Simulation,
        UnitKind, World,
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
            cavalry_speed: Money::from(2),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: Money::from(1),
            manpower: ManpowerRules {
                base_capacity: 0,
                per_development: 1_000,
                recovery_months: 10,
                minimum_monthly: 100,
                infantry_cost: 1_000,
                cavalry_cost: 1_500,
            },
        }
    }

    fn world() -> World {
        let mut first = Province::new(ProvinceKind::Land);
        let mut second = Province::new(ProvinceKind::Land);
        for province in [&mut first, &mut second] {
            province.owner = Some("AAA".to_owned());
            province.controller = Some("AAA".to_owned());
            province.development = Development {
                tax: Some(1),
                production: Some(1),
                manpower: Some(1),
            };
        }
        first.development.manpower = Some(10);
        let mut provinces = BTreeMap::new();
        provinces.insert(1, first);
        provinces.insert(2, second);
        World::new(
            [Country::new("AAA")],
            provinces,
            [1, 2],
            [],
            rules(),
            [LandConnection {
                from: 1,
                to: 2,
                distance: 10,
            }],
        )
        .unwrap()
    }

    fn fund(world: &mut World, amount: i64) {
        world
            .countries
            .get_mut("AAA")
            .unwrap()
            .finances
            .refund(Money::from(amount));
    }

    fn available_manpower(world: &World) -> u64 {
        world.country_manpower("AAA").unwrap().available
    }

    #[test]
    fn recruits_infantry_and_cavalry_by_unit_rules() {
        let mut world = world();
        let date = date();
        fund(&mut world, 10);
        let infantry = world
            .quote_recruitment("AAA", 1, UnitKind::Infantry, date)
            .unwrap();
        assert_eq!(infantry.cost, Money::from(10));
        assert_eq!(infantry.manpower, 1_000);
        assert_eq!(infantry.completes_on, date.checked_add_days(60).unwrap());
        match world.quote_recruitment("AAA", 1, UnitKind::Cavalry, date) {
            Err(RecruitmentError::InsufficientFunds { cost, available }) => {
                assert_eq!(cost, Money::from(25));
                assert_eq!(available, Money::from(10));
            }
            other => panic!("骑兵报价应为余额不足，实际是 {other:?}"),
        }

        fund(&mut world, 90);
        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, date)
            .unwrap();
        let full = world.quote_cancel_recruitment("AAA", 1, date).unwrap();
        assert_eq!(full.cost, Money::from(10));
        assert_eq!(full.manpower, 1_000);
        world.cancel_recruitment("AAA", 1, date).unwrap();
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(100)
        );
        assert_eq!(available_manpower(&world), 11_000);

        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, date)
            .unwrap();
        world
            .start_recruitment("AAA", 2, UnitKind::Cavalry, date)
            .unwrap();
        assert_eq!(
            world.country("AAA").unwrap().finances.treasury(),
            Money::from(65)
        );
        assert_eq!(available_manpower(&world), 8_500);
        let later = date.checked_add_days(10).unwrap();
        let half = world.quote_cancel_recruitment("AAA", 2, later).unwrap();
        assert_eq!(half.cost, Money::new(125, 1));
        assert_eq!(half.manpower, 1_500);
        world.cancel_recruitment("AAA", 2, later).unwrap();
        world
            .start_recruitment("AAA", 2, UnitKind::Cavalry, later)
            .unwrap();

        let cavalry_ready = later.checked_add_days(90).unwrap();
        world.complete_recruitments(cavalry_ready);
        let infantry_army = world
            .armies()
            .values()
            .find(|army| army.province == 1)
            .unwrap();
        let cavalry_army = world
            .armies()
            .values()
            .find(|army| army.province == 2)
            .unwrap();
        assert_eq!(infantry_army.composition(UnitKind::Infantry), (1, 1_000));
        assert_eq!(infantry_army.composition(UnitKind::Cavalry), (0, 0));
        assert_eq!(cavalry_army.composition(UnitKind::Cavalry), (1, 1_000));
        let infantry_id = infantry_army.id();
        let cavalry_id = cavalry_army.id();
        assert_eq!(date_span(&world, infantry_id, 2, cavalry_ready), 10);
        assert_eq!(date_span(&world, cavalry_id, 1, cavalry_ready), 5);

        world
            .start_recruitment("AAA", 2, UnitKind::Infantry, cavalry_ready)
            .unwrap();
        let mixed_ready = cavalry_ready.checked_add_days(60).unwrap();
        world.complete_recruitments(mixed_ready);
        let mixed = &world.armies()[&cavalry_id];
        assert_eq!(mixed.composition(UnitKind::Infantry), (1, 1_000));
        assert_eq!(mixed.composition(UnitKind::Cavalry), (1, 1_000));
        assert_eq!(mixed.march_speed(world.military_rules()), Money::from(1));
        assert_eq!(date_span(&world, cavalry_id, 1, mixed_ready), 10);
        assert_eq!(
            world
                .country_monthly_budget("AAA")
                .unwrap()
                .army_maintenance,
            Money::from(45) / Money::from(12)
        );

        let split_id = world.split_army("AAA", cavalry_id, 1).unwrap();
        assert_eq!(
            world.armies()[&cavalry_id].composition(UnitKind::Cavalry),
            (1, 1_000)
        );
        assert_eq!(
            world.armies()[&split_id].composition(UnitKind::Infantry),
            (1, 1_000)
        );
        assert_eq!(date_span(&world, cavalry_id, 1, mixed_ready), 5);
        world.merge_armies("AAA", cavalry_id, &[split_id]).unwrap();
        assert!(world.armies().get(&split_id).is_none());
        assert_eq!(
            world.armies()[&cavalry_id].composition(UnitKind::Infantry),
            (1, 1_000)
        );
        assert_eq!(
            world.armies()[&cavalry_id].composition(UnitKind::Cavalry),
            (1, 1_000)
        );

        world.move_army("AAA", cavalry_id, 1, mixed_ready).unwrap();
        let saved = SavedGame::capture(&world, &Simulation::new(mixed_ready), "AAA");
        let text = serde_json::to_string(&saved).unwrap();
        assert!(text.contains("\"infantry\""));
        assert!(text.contains("\"cavalry\""));
        let restored = serde_json::from_str::<SavedGame>(&text)
            .unwrap()
            .restore(&world)
            .unwrap();
        let restored_army = &restored.world.armies()[&cavalry_id];
        assert_eq!(restored_army.composition(UnitKind::Infantry), (1, 1_000));
        assert_eq!(restored_army.composition(UnitKind::Cavalry), (1, 1_000));
        assert_eq!(restored_army.province, 2);
        assert_eq!(restored_army.movement.as_ref().unwrap().destination(), 1);
        assert_eq!(
            restored.world.armies()[&infantry_id].composition(UnitKind::Infantry),
            (1, 1_000)
        );
    }

    #[test]
    fn loads_existing_infantry_save() {
        let mut world = world();
        let date = date();
        let completes = date.checked_add_days(60).unwrap();
        let json = format!(
            r#"{{
                "armies": {{
                    "1": {{
                        "id": 1,
                        "country": "AAA",
                        "province": 1,
                        "regiments": [
                            {{"kind": "infantry", "strength": 1000}},
                            {{"kind": "infantry", "strength": 800}}
                        ],
                        "movement": null
                    }}
                }},
                "recruitments": {{
                    "2": {{
                        "army_id": 2,
                        "country": "AAA",
                        "kind": "infantry",
                        "started_on": "{date}",
                        "completes_on": "{completes}"
                    }}
                }},
                "next_army_id": 3
            }}"#
        );
        let military: MilitaryState = serde_json::from_str(&json).unwrap();
        world.restore_military(military, date).unwrap();
        let army = &world.armies()[&1];
        assert_eq!(army.composition(UnitKind::Infantry), (2, 1_800));
        assert_eq!(army.composition(UnitKind::Cavalry), (0, 0));
        assert!(army.movement.is_none());
        assert_eq!(army.province, 1);
        let project = world.recruitment(2).unwrap();
        assert_eq!(project.kind, UnitKind::Infantry);
        assert_eq!(project.paid_cost, Some(Money::from(10)));
        assert_eq!(project.paid_manpower, Some(1_000));
        let refund = world.quote_cancel_recruitment("AAA", 2, date).unwrap();
        assert_eq!(refund.cost, Money::from(10));
        assert_eq!(refund.manpower, 1_000);
    }

    fn date_span(world: &World, army: u32, destination: u32, date: GameDate) -> i32 {
        let movement = world
            .quote_movement("AAA", army, destination, date)
            .unwrap();
        movement.started_on().days_until(movement.arrives_on())
    }
}
