// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Army, GameDate, MilitaryRules, MilitaryState, Money, ProvinceKind, REGIMENT_STRENGTH,
    Recruitment, RecruitmentError, RecruitmentQuote, Regiment, SaveError, UnitKind,
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

    pub fn recruitments(&self) -> &BTreeMap<u32, Vec<Recruitment>> {
        &self.military.recruitments
    }

    /// 返回该省按完成顺序排列的招募订单；没有订单时为空。
    pub fn recruitment_queue(&self, province: u32) -> &[Recruitment] {
        match self.military.recruitments.get(&province) {
            Some(queue) => queue.as_slice(),
            None => &[],
        }
    }

    /// 返回队首。队首是正在进行的订单，其后的订单都还没开始。
    pub fn recruitment(&self, province: u32) -> Option<&Recruitment> {
        self.recruitment_queue(province).first()
    }

    /// 当前省份的招募费用和本单工期。国家修正尚无来源，只计已建成建筑和税收发展度。
    pub fn recruitment_terms(&self, province: u32, kind: UnitKind) -> (Money, u32) {
        let (cost_modifier, time_modifier) = self.recruitment_modifiers(province);
        (
            self.military_rules.modified_cost(kind, cost_modifier),
            self.military_rules.modified_days(kind, time_modifier),
        )
    }

    fn recruitment_modifiers(&self, province: u32) -> (Money, Money) {
        let Some(province) = self.province(province) else {
            return (Money::ZERO, Money::ZERO);
        };
        let mut cost = Money::ZERO;
        let mut time = Money::ZERO;
        for building in &province.buildings {
            let definition = &self.buildings[building];
            cost += definition.local_regiment_cost;
            time += definition.regiment_recruit_speed;
        }
        if let Some(tax) = province.development.tax {
            time += self.military_rules.tax_recruitment_time * Money::from(tax);
        }
        (cost, time)
    }

    pub fn quote_recruitment(
        &self,
        country: &str,
        province: u32,
        kind: UnitKind,
        date: GameDate,
    ) -> Result<RecruitmentQuote, RecruitmentError> {
        let (cost, days) = self.recruitment_terms(province, kind);
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
        if self
            .recruitment_queue(province)
            .first()
            .is_some_and(|order| order.country != country.tag())
        {
            return Err(RecruitmentError::ForeignQueue);
        }
        if self.military.next_army_id == u32::MAX {
            return Err(RecruitmentError::ArmyLimit);
        }
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
        // 队列末单的完成日就是新订单的开始日；空队列从当天下单。
        let started_on = self
            .recruitment_queue(province)
            .last()
            .map(|order| order.completes_on)
            .unwrap_or(date);
        Ok(RecruitmentQuote {
            cost,
            manpower,
            started_on,
            completes_on: started_on
                .checked_add_days(days)
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
        self.military
            .recruitments
            .entry(province)
            .or_default()
            .push(Recruitment {
                army_id,
                country: country.to_owned(),
                kind,
                started_on: quote.started_on,
                completes_on: quote.completes_on,
                paid_cost: Some(quote.cost),
                paid_manpower: Some(quote.manpower),
            });
        Ok(())
    }

    pub(crate) fn complete_recruitments(&mut self, date: GameDate) {
        let provinces: Vec<_> = self.military.recruitments.keys().copied().collect();
        for province in provinces {
            // 一次跨过多个完成日时，同一省份按队列顺序逐单交付。
            while self
                .recruitment_queue(province)
                .first()
                .is_some_and(|order| order.completes_on <= date)
            {
                let order = self
                    .military
                    .recruitments
                    .get_mut(&province)
                    .expect("到期招募队列存在")
                    .remove(0);
                if self
                    .military
                    .recruitments
                    .get(&province)
                    .is_some_and(Vec::is_empty)
                {
                    self.military.recruitments.remove(&province);
                }
                self.add_recruited_regiment(province, order);
            }
        }
    }

    fn add_recruited_regiment(&mut self, province: u32, recruitment: Recruitment) {
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
        for (&province, queue) in &mut military.recruitments {
            if queue.is_empty() || queue.iter().any(|order| order.country != queue[0].country) {
                return Err(SaveError::InvalidMilitary);
            }
            let mut previous_completion = None;
            for (index, project) in queue.iter_mut().enumerate() {
                // 旧档没有实付金额时按兵种基础费用补齐，不套用读取时的省份修正。
                let cost = project
                    .paid_cost
                    .get_or_insert(self.military_rules.cost(project.kind));
                project
                    .paid_manpower
                    .get_or_insert(self.military_rules.manpower_cost(project.kind));
                // 队首必须正在进行；后续订单紧接前一笔的完成日，可以晚于当前日期。
                let linked = if index == 0 {
                    project.started_on <= date && project.completes_on > date
                } else {
                    previous_completion == Some(project.started_on)
                };
                if project.army_id == 0
                    || !ids.insert(project.army_id)
                    || !valid_province(province)
                    || self.country(&project.country).is_none()
                    || !linked
                    || project.completes_on <= project.started_on
                    || *cost < crate::Money::ZERO
                {
                    return Err(SaveError::InvalidMilitary);
                }
                previous_completion = Some(project.completes_on);
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
        MilitaryState, Money, Province, ProvinceKind, RecruitmentError, SaveError, SavedGame,
        Simulation, UnitKind, World,
    };

    fn date() -> GameDate {
        GameDate::new(1444, 11, 11).unwrap()
    }

    fn unit_rules() -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 60,
            cavalry_days: 90,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(2),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: Money::from(1),
            monthly_reinforce: Money::ZERO,
            extra_land_reinforce_cost: Money::ZERO,
            supply_base: 6,
            base_land_forcelimit: Money::ZERO,
            development_land_forcelimit: Money::ZERO,
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::new(),
            tax_recruitment_time: Money::ZERO,
            starting_army_size: Money::ZERO,
            starting_army_size_at_war: Money::ZERO,
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

    fn rules() -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 100,
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
            unit_rules(),
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
        let order = world.recruitment(1).unwrap().id();
        let full = world
            .quote_cancel_recruitment("AAA", 1, order, date)
            .unwrap();
        assert_eq!(full.cost, Money::from(10));
        assert_eq!(full.manpower, 1_000);
        world.cancel_recruitment("AAA", 1, order, date).unwrap();
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
        let cavalry_order = world.recruitment(2).unwrap().id();
        let half = world
            .quote_cancel_recruitment("AAA", 2, cavalry_order, later)
            .unwrap();
        assert_eq!(half.cost, Money::new(125, 1));
        assert_eq!(half.manpower, 1_500);
        world
            .cancel_recruitment("AAA", 2, cavalry_order, later)
            .unwrap();
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
    fn cavalry_order_starts_when_the_previous_order_finishes() {
        let mut world = world();
        let date = date();
        fund(&mut world, 100);
        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, date)
            .unwrap();
        world
            .start_recruitment("AAA", 1, UnitKind::Cavalry, date)
            .unwrap();
        let queue = world.recruitment_queue(1);
        assert_eq!(queue[0].kind, UnitKind::Infantry);
        assert_eq!(queue[0].completes_on, date.checked_add_days(60).unwrap());
        assert_eq!(queue[1].kind, UnitKind::Cavalry);
        assert_eq!(queue[1].started_on, queue[0].completes_on);
        assert_eq!(
            queue[1].completes_on,
            queue[0].completes_on.checked_add_days(90).unwrap()
        );
        assert_eq!(queue[1].paid_cost, Some(Money::from(25)));
        assert_eq!(queue[1].paid_manpower, Some(1_500));
    }

    #[test]
    fn province_modifiers_change_the_quote_and_leave_existing_orders() {
        let mut rules = unit_rules();
        rules.tax_recruitment_time = Money::new(-2, 2);
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = Some("AAA".to_owned());
        province.controller = Some("AAA".to_owned());
        province.development = Development {
            tax: Some(3),
            production: Some(1),
            manpower: Some(10),
        };
        province.buildings.insert("camp".to_owned());
        province.buildings.insert("yard".to_owned());
        let mut world = World::new(
            [Country::new("AAA")],
            BTreeMap::from([(1, province)]),
            [1],
            [
                building("camp", Money::new(-1, 1), Money::new(-1, 1)),
                building("yard", Money::new(-5, 2), Money::ZERO),
            ],
            rules,
            Vec::<LandConnection>::new(),
        )
        .unwrap();
        let date = date();
        let (cost, days) = world.recruitment_terms(1, UnitKind::Infantry);
        assert_eq!(cost, Money::new(85, 1));
        assert_eq!(days, 51);
        assert_eq!(
            world.recruitment_terms(1, UnitKind::Cavalry),
            (Money::new(2125, 2), 76)
        );
        match world.quote_recruitment("AAA", 1, UnitKind::Infantry, date) {
            Err(RecruitmentError::InsufficientFunds { cost, available }) => {
                assert_eq!(cost, Money::new(85, 1));
                assert_eq!(available, Money::ZERO);
            }
            other => panic!("报价应为余额不足，实际是 {other:?}"),
        }
        let legacy_completion = date.checked_add_days(60).unwrap();
        let legacy: MilitaryState = serde_json::from_str(&format!(
            r#"{{
                "armies": {{}},
                "recruitments": {{
                    "1": {{
                        "army_id": 1,
                        "country": "AAA",
                        "kind": "infantry",
                        "started_on": "{date}",
                        "completes_on": "{legacy_completion}"
                    }}
                }},
                "next_army_id": 2
            }}"#
        ))
        .unwrap();
        world.restore_military(legacy, date).unwrap();
        let legacy_order = world.recruitment(1).unwrap();
        let legacy_id = legacy_order.id();
        assert_eq!(legacy_order.paid_cost, Some(Money::from(10)));
        assert_eq!(legacy_order.completes_on, legacy_completion);
        world.cancel_recruitment("AAA", 1, legacy_id, date).unwrap();

        fund(&mut world, 100);
        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, date)
            .unwrap();
        let order = world.recruitment(1).unwrap();
        let order_id = order.id();
        assert_eq!(order.paid_cost, Some(Money::new(85, 1)));
        assert_eq!(order.completes_on, date.checked_add_days(51).unwrap());
        assert_eq!(
            world
                .quote_cancel_recruitment("AAA", 1, order_id, date)
                .unwrap()
                .cost,
            Money::new(85, 1)
        );

        let province = world.provinces.get_mut(&1).unwrap();
        province.buildings.clear();
        province.development.tax = Some(10);
        assert_eq!(
            world.recruitment_terms(1, UnitKind::Infantry),
            (Money::from(10), 48)
        );
        let kept_completion = world.recruitment(1).unwrap().completes_on;
        assert_eq!(
            world.recruitment(1).unwrap().paid_cost,
            Some(Money::new(85, 1))
        );
        assert_eq!(kept_completion, date.checked_add_days(51).unwrap());

        world.complete_recruitments(kept_completion);
        assert_eq!(
            world
                .country_monthly_budget("AAA")
                .unwrap()
                .army_maintenance,
            Money::from(10) / Money::from(12)
        );

        let mut floor = unit_rules();
        floor.tax_recruitment_time = Money::from(-1);
        let mut bare = Province::new(ProvinceKind::Land);
        bare.owner = Some("AAA".to_owned());
        bare.controller = Some("AAA".to_owned());
        bare.development.tax = Some(2);
        bare.development.production = Some(1);
        bare.development.manpower = Some(1);
        bare.buildings.insert("cheap".to_owned());
        let floor_world = World::new(
            [Country::new("AAA")],
            BTreeMap::from([(1, bare)]),
            [1],
            [building("cheap", Money::new(-9, 1), Money::ZERO)],
            floor,
            Vec::<LandConnection>::new(),
        )
        .unwrap();
        assert_eq!(
            floor_world.recruitment_terms(1, UnitKind::Infantry),
            (Money::new(2, 0), 1)
        );
        assert_eq!(
            floor_world
                .military_rules()
                .modified_cost(UnitKind::Infantry, Money::ONE / Money::from(3) - Money::ONE),
            Money::new(3333, 3)
        );
    }

    fn building(id: &str, cost_modifier: Money, time_modifier: Money) -> crate::BuildingDefinition {
        crate::BuildingDefinition {
            id: id.to_owned(),
            cost: Money::from(100),
            months: 12,
            tax_modifier: Money::ZERO,
            land_forcelimit: Money::ZERO,
            local_regiment_cost: cost_modifier,
            regiment_recruit_speed: time_modifier,
            replaces: None,
            constructible: false,
        }
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
        let refund = world
            .quote_cancel_recruitment("AAA", 2, project.id(), date)
            .unwrap();
        assert_eq!(refund.cost, Money::from(10));
        assert_eq!(refund.manpower, 1_000);
    }

    fn date_span(world: &World, army: u32, destination: u32, date: GameDate) -> i32 {
        let movement = world
            .quote_movement("AAA", army, destination, date)
            .unwrap();
        movement.started_on().days_until(movement.arrives_on())
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

    fn sample() -> (World, GameDate) {
        let date = GameDate::new(1444, 11, 11).expect("日期有效");
        let mut world = World::new(
            [Country::new("AAA"), Country::new("BBB")],
            BTreeMap::from([(1, province("AAA")), (2, province("AAA"))]),
            [1, 2],
            Vec::<crate::BuildingDefinition>::new(),
            rules(),
            Vec::<crate::LandConnection>::new(),
        )
        .expect("世界有效");
        for tag in ["AAA", "BBB"] {
            world
                .countries
                .get_mut(tag)
                .expect("国家已注册")
                .finances
                .refund(Money::from(1_000));
        }
        (world, date)
    }

    fn recruit(world: &mut World, province: u32, date: GameDate) -> u32 {
        world
            .start_recruitment("AAA", province, UnitKind::Infantry, date)
            .expect("招募成功");
        world
            .recruitment_queue(province)
            .last()
            .expect("订单已入队")
            .id()
    }

    #[test]
    fn three_orders_finish_serially_per_province() {
        let (mut world, date) = sample();
        let first = recruit(&mut world, 1, date);
        let second = recruit(&mut world, 1, date);
        let third = recruit(&mut world, 1, date);
        let other = recruit(&mut world, 2, date);
        assert_eq!([first, second, third, other], [1, 2, 3, 4]);

        let queue = world.recruitment_queue(1);
        assert_eq!(queue[0].started_on, date);
        assert_eq!(queue[1].started_on, queue[0].completes_on);
        assert_eq!(queue[2].started_on, queue[1].completes_on);
        assert!(queue.iter().all(|order| {
            order.paid_cost == Some(Money::from(10)) && order.paid_manpower == Some(1_000)
        }));
        assert_eq!(queue[1].progress(date), 0.0);
        assert_eq!(queue[2].progress(date), 0.0);
        assert_eq!(
            world.country("AAA").expect("国家存在").finances.treasury(),
            Money::from(960)
        );
        assert_eq!(
            world.country_manpower("AAA").expect("人力存在").available,
            16_000
        );

        let day_100 = date.checked_add_days(100).expect("日期有效");
        assert_eq!(queue[1].progress(day_100), 0.0);
        world.complete_recruitments(day_100);
        assert_eq!(
            world
                .recruitment_queue(1)
                .iter()
                .map(crate::Recruitment::id)
                .collect::<Vec<_>>(),
            vec![second, third]
        );
        assert!(world.recruitment_queue(2).is_empty());
        assert_eq!(world.armies().len(), 2);

        let day_250 = date.checked_add_days(250).expect("日期有效");
        world.complete_recruitments(day_250);
        let remaining = world.recruitment(1).expect("第三单仍在进行");
        assert_eq!(remaining.id(), third);
        assert_eq!(remaining.progress(day_250), 50.0);
        assert_eq!(
            world
                .armies()
                .values()
                .find(|army| army.province == 1)
                .expect("本省军队")
                .regiments
                .len(),
            2
        );

        world.complete_recruitments(date.checked_add_days(300).expect("日期有效"));
        assert!(world.recruitment_queue(1).is_empty());
        assert_eq!(
            world
                .armies()
                .values()
                .find(|army| army.province == 1)
                .expect("本省军队")
                .regiments
                .len(),
            3
        );
        assert_eq!(world.military.next_army_id, 5);
    }

    #[test]
    fn cancelling_head_or_middle_updates_following_orders() {
        let (mut world, date) = sample();
        let head = recruit(&mut world, 1, date);
        let middle = recruit(&mut world, 1, date);
        let tail = recruit(&mut world, 1, date);
        let head_start = world.recruitment(1).expect("队首").started_on;
        let head_end = world.recruitment(1).expect("队首").completes_on;

        world
            .cancel_recruitment("AAA", 1, middle, date)
            .expect("取消中间订单");
        let queue = world.recruitment_queue(1);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue[0].id(), head);
        assert_eq!(queue[0].started_on, head_start);
        assert_eq!(queue[0].completes_on, head_end);
        assert_eq!(queue[1].id(), tail);
        assert_eq!(queue[1].started_on, head_end);
        assert_eq!(
            queue[1].completes_on,
            head_end.checked_add_days(100).expect("日期有效")
        );

        let day_40 = date.checked_add_days(40).expect("日期有效");
        world
            .cancel_recruitment("AAA", 1, head, day_40)
            .expect("取消队首");
        let queue = world.recruitment_queue(1);
        assert_eq!(queue.len(), 1);
        assert_eq!(queue[0].id(), tail);
        assert_eq!(queue[0].started_on, day_40);
        assert_eq!(
            queue[0].completes_on,
            day_40.checked_add_days(100).expect("日期有效")
        );
        assert_eq!(world.military.next_army_id, 4);
        let replacement = recruit(&mut world, 1, day_40);
        assert_eq!(replacement, 4);
    }

    #[test]
    fn queued_refund_is_full_and_manpower_may_exceed_capacity() {
        let (mut world, date) = sample();
        let _head = recruit(&mut world, 1, date);
        let queued = recruit(&mut world, 1, date);
        assert_eq!(world.recruitment_queue(1)[1].progress(date), 0.0);
        let capacity = world.country_manpower("AAA").expect("人力存在").capacity;
        world
            .countries
            .get_mut("AAA")
            .expect("国家存在")
            .manpower
            .as_mut()
            .expect("人力已初始化")
            .available = capacity;
        let treasury = world.country("AAA").expect("国家存在").finances.treasury();
        let refund = world
            .cancel_recruitment("AAA", 1, queued, date)
            .expect("取消排队订单");
        assert_eq!(refund.cost, Money::from(10));
        assert_eq!(refund.manpower, 1_000);
        assert_eq!(
            world.country("AAA").expect("国家存在").finances.treasury(),
            treasury + Money::from(10)
        );
        assert_eq!(
            world.country_manpower("AAA").expect("人力存在").available,
            capacity + 1_000
        );
        assert_eq!(world.recruitment_queue(1).len(), 1);
    }

    #[test]
    fn active_gold_refund_uses_the_existing_threshold() {
        let (mut world, date) = sample();
        let within = recruit(&mut world, 1, date);
        let beyond = recruit(&mut world, 2, date);
        let full = world
            .cancel_recruitment(
                "AAA",
                1,
                within,
                date.checked_add_days(10).expect("日期有效"),
            )
            .expect("进度恰好 10%");
        let half = world
            .cancel_recruitment(
                "AAA",
                2,
                beyond,
                date.checked_add_days(11).expect("日期有效"),
            )
            .expect("进度超过 10%");
        assert_eq!(full.cost, Money::from(10));
        assert_eq!(half.cost, Money::from(5));
        assert_eq!(full.manpower, 1_000);
        assert_eq!(half.manpower, 1_000);
    }

    #[test]
    fn legacy_zero_manpower_is_not_refunded() {
        let (mut world, date) = sample();
        let order = recruit(&mut world, 1, date);
        let available = world.country_manpower("AAA").expect("人力存在").available;
        world.military.recruitments.get_mut(&1).expect("队列")[0].paid_manpower = Some(0);
        let refund = world
            .cancel_recruitment("AAA", 1, order, date)
            .expect("取消旧订单");
        assert_eq!(refund.manpower, 0);
        assert_eq!(
            world.country_manpower("AAA").expect("人力存在").available,
            available
        );
    }

    #[test]
    fn save_keeps_queue_and_reads_old_single_order() {
        let (mut world, date) = sample();
        recruit(&mut world, 1, date);
        recruit(&mut world, 1, date);
        recruit(&mut world, 1, date);
        let saved = SavedGame::capture(&world, &Simulation::new(date), "AAA");
        let restored = serde_json::from_value::<SavedGame>(
            serde_json::to_value(&saved).expect("存档可序列化"),
        )
        .expect("存档可读取")
        .restore(&world)
        .expect("存档可恢复");
        let queue = restored.world.recruitment_queue(1);
        assert_eq!(queue.len(), 3);
        assert_eq!(
            queue.iter().map(crate::Recruitment::id).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert_eq!(
            queue[0].started_on,
            world.recruitment_queue(1)[0].started_on
        );
        assert_eq!(
            queue[2].completes_on,
            world.recruitment_queue(1)[2].completes_on
        );
        assert!(
            queue
                .iter()
                .all(|order| order.paid_cost == Some(Money::from(10))
                    && order.paid_manpower == Some(1_000))
        );

        let mut legacy = serde_json::to_value(&saved).expect("存档可序列化");
        let slot = legacy
            .pointer_mut("/military/recruitments/1")
            .expect("招募字段");
        let mut order = slot.as_array_mut().expect("新存档是数组").remove(0);
        let fields = order.as_object_mut().expect("订单对象");
        fields.remove("paid_cost");
        fields.remove("paid_manpower");
        *slot = order;
        let loaded = serde_json::from_value::<SavedGame>(legacy)
            .expect("旧单对象可读取")
            .restore(&world)
            .expect("旧单对象可恢复");
        let order = loaded.world.recruitment(1).expect("队首");
        assert_eq!(order.id(), 1);
        assert_eq!(order.paid_cost, Some(Money::from(10)));
        assert_eq!(order.paid_manpower, Some(1_000));
        assert_eq!(loaded.world.recruitment_queue(1).len(), 1);

        let mut broken = world.military.clone();
        broken.recruitments.get_mut(&1).expect("队列")[1].started_on = date;
        let error = world
            .restore_military(broken, date)
            .expect_err("断开的队列无效");
        assert!(matches!(error, SaveError::InvalidMilitary));
    }

    #[test]
    fn another_country_cannot_append_to_the_queue() {
        let (mut world, date) = sample();
        recruit(&mut world, 1, date);
        world.provinces.get_mut(&1).expect("省份").owner = Some("BBB".to_owned());
        world.provinces.get_mut(&1).expect("省份").controller = Some("BBB".to_owned());
        let error = world
            .start_recruitment("BBB", 1, UnitKind::Infantry, date)
            .expect_err("不能加入他国队列");
        assert_eq!(error, RecruitmentError::ForeignQueue);
        assert_eq!(world.recruitment_queue(1).len(), 1);
    }
}
