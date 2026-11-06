// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use crate::{GameDate, Money, MonthlyBudget, ProvinceKind};

use super::World;

/// 一国当前陆军的基础年费和超编超额年费。上限不随团数变化。
pub(super) struct CountryArmyCost {
    pub base_annual: Money,
    pub excess_annual: Money,
}

impl CountryArmyCost {
    pub(super) fn total_annual(&self) -> Money {
        self.base_annual + self.excess_annual
    }
}

impl World {
    pub fn province_monthly_tax(&self, id: u32) -> Option<Money> {
        if !self.active_provinces.contains(&id) {
            return None;
        }
        Some(self.province_annual_tax(id)? / Money::from(12))
    }

    /// 显示和超编判定使用向下取整。不足 1 以及负数都按 0。
    pub fn country_land_force_limit(&self, tag: &str) -> Option<u64> {
        self.country(tag)?;
        Some(whole_limit(self.raw_land_force_limit(tag)))
    }

    pub fn country_monthly_budget(&self, tag: &str) -> Option<MonthlyBudget> {
        self.country(tag)?;
        let annual_tax: Money = self
            .active_provinces
            .iter()
            .filter(|id| self.provinces[id].owner.as_deref() == Some(tag))
            .map(|id| self.province_annual_tax(*id).expect("有效领土具备基础税收"))
            .sum();
        let upkeep = self.country_army_cost(tag, None);
        let months = Money::from(12);
        Some(MonthlyBudget {
            tax: annual_tax / months,
            army_maintenance: upkeep.total_annual() / months,
            base_army_maintenance: upkeep.base_annual / months,
            excess_army_maintenance: upkeep.excess_annual / months,
        })
    }

    pub(crate) fn settle_monthly_finances(&mut self, date: GameDate) {
        // 先汇总全年金额，再按国家统一除以十二，避免按省或按军队舍入改变总额。
        let mut tax: BTreeMap<_, _> = self
            .countries
            .keys()
            .map(|tag| (tag.clone(), Money::ZERO))
            .collect();
        for id in &self.active_provinces {
            if let Some(tag) = &self.provinces[id].owner {
                *tax.get_mut(tag).expect("省份所有者已注册") +=
                    self.province_annual_tax(*id).expect("有效领土具备基础税收");
            }
        }
        let upkeep: BTreeMap<_, _> = self
            .countries
            .keys()
            .map(|tag| (tag.clone(), self.country_army_cost(tag, None)))
            .collect();
        for (tag, country) in &mut self.countries {
            let cost = &upkeep[tag];
            country
                .finances
                .settle(tax[tag], cost.base_annual, cost.excess_annual, date);
        }
    }

    /// `exclude_army` 用于预览解散后的全国维护，上限仍按当前领土计算。
    pub(super) fn country_army_cost(&self, tag: &str, exclude_army: Option<u32>) -> CountryArmyCost {
        let mut base_annual = Money::ZERO;
        let mut regiments = 0_u64;
        for army in self
            .armies()
            .values()
            .filter(|army| army.country == tag && Some(army.id()) != exclude_army)
        {
            regiments += army.regiments.len() as u64;
            base_annual += army
                .regiments
                .iter()
                .map(|regiment| self.military_rules.annual_maintenance(regiment.kind))
                .sum::<Money>();
        }
        let limit = whole_limit(self.raw_land_force_limit(tag));
        CountryArmyCost {
            excess_annual: self
                .military_rules
                .excess_annual_maintenance(base_annual, regiments, limit),
            base_annual,
        }
    }

    fn raw_land_force_limit(&self, tag: &str) -> Money {
        let mut raw = Money::ZERO;
        let mut landed = false;
        for id in &self.active_provinces {
            let province = &self.provinces[id];
            if province.kind != ProvinceKind::Land || province.owner.as_deref() != Some(tag) {
                continue;
            }
            if !landed {
                landed = true;
                raw += self.military_rules.base_land_forcelimit;
            }
            let development = province
                .development
                .total()
                .expect("有效领土具备三项发展度");
            raw += self.military_rules.development_land_forcelimit * Money::from(development);
            for building in &province.buildings {
                raw += self.buildings[building].land_forcelimit;
            }
            if let Some(good) = &province.trade_good {
                raw += self
                    .military_rules
                    .goods_land_forcelimit
                    .get(good)
                    .copied()
                    .unwrap_or(Money::ZERO);
            }
        }
        raw
    }
}

fn whole_limit(raw: Money) -> u64 {
    let whole = raw.floor();
    if whole <= Money::ZERO {
        return 0;
    }
    u64::try_from(whole).expect("陆军上限在支持范围内")
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::{
        Army, BuildingDefinition, Country, CountryFinances, Development, GameDate, ManpowerRules,
        MilitaryRules, Money, Province, ProvinceKind, Regiment, SavedGame, Simulation, UnitKind,
        World,
    };

    fn date() -> GameDate {
        GameDate::new(1444, 12, 1).unwrap()
    }

    fn rules(base: i64, per_development: Money) -> MilitaryRules {
        MilitaryRules {
            infantry_cost: Money::from(10),
            cavalry_cost: Money::from(25),
            infantry_days: 30,
            cavalry_days: 30,
            infantry_speed: Money::from(1),
            cavalry_speed: Money::from(1),
            recruitment_full_refund_progress: Money::new(1, 1),
            land_maintenance_factor: Money::from(1),
            base_land_forcelimit: Money::from(base),
            development_land_forcelimit: per_development,
            land_forcelimit_extra_cost_factor: Money::from(2),
            goods_land_forcelimit: BTreeMap::from([("grain".to_owned(), Money::new(5, 1))]),
            manpower: ManpowerRules {
                base_capacity: 0,
                per_development: 1_000,
                recovery_months: 10,
                minimum_monthly: 1,
                infantry_cost: 1_000,
                cavalry_cost: 1_000,
            },
        }
    }

    fn building(id: &str, limit: i64, replaces: Option<&str>) -> BuildingDefinition {
        BuildingDefinition {
            id: id.to_owned(),
            cost: Money::from(100),
            months: 12,
            tax_modifier: Money::ZERO,
            land_forcelimit: Money::from(limit),
            replaces: replaces.map(str::to_owned),
            constructible: false,
        }
    }

    fn province(tax: u32, production: u32, manpower: u32) -> Province {
        let mut province = Province::new(ProvinceKind::Land);
        province.owner = Some("AAA".to_owned());
        province.controller = Some("AAA".to_owned());
        province.development = Development {
            tax: Some(tax),
            production: Some(production),
            manpower: Some(manpower),
        };
        province
    }

    fn opened(
        military_rules: MilitaryRules,
        province: Province,
        buildings: Vec<BuildingDefinition>,
    ) -> World {
        World::new(
            [Country::new("AAA"), Country::new("BBB")],
            BTreeMap::from([(1, province)]),
            [1],
            buildings,
            military_rules,
            [],
        )
        .unwrap()
    }

    fn add_regiments(world: &mut World, count: usize, strength: u32) {
        let mut regiments = Vec::with_capacity(count);
        for _ in 0..count {
            regiments.push(Regiment {
                kind: UnitKind::Infantry,
                strength,
            });
        }
        let id = world.military.next_army_id;
        world.military.next_army_id += 1;
        world.military.armies.insert(
            id,
            Army {
                id,
                country: "AAA".to_owned(),
                province: 1,
                regiments,
                movement: None,
            },
        );
    }

    #[test]
    fn force_limit_uses_base_development_buildings_and_goods() {
        let mut home = province(1, 1, 1);
        let under = opened(rules(6, Money::new(1, 1)), home.clone(), vec![]);
        assert_eq!(under.country_land_force_limit("AAA"), Some(6));
        assert_eq!(under.country_land_force_limit("BBB"), Some(0));

        home.development.tax = Some(3);
        home.development.production = Some(3);
        home.development.manpower = Some(3);
        let developed = opened(rules(6, Money::new(1, 1)), home.clone(), vec![]);
        assert_eq!(developed.country_land_force_limit("AAA"), Some(6));

        home.trade_good = Some("grain".to_owned());
        let grain = opened(rules(6, Money::new(1, 1)), home.clone(), vec![]);
        assert_eq!(grain.country_land_force_limit("AAA"), Some(7));

        home.buildings.insert("regimental_camp".to_owned());
        home.buildings.insert("conscription_center".to_owned());
        let built = opened(
            rules(6, Money::new(1, 1)),
            home,
            vec![
                building("regimental_camp", 1, None),
                building("conscription_center", 2, Some("regimental_camp")),
            ],
        );
        assert_eq!(
            built.provinces()[&1].buildings.iter().collect::<Vec<_>>(),
            vec!["conscription_center"]
        );
        assert_eq!(built.country_land_force_limit("AAA"), Some(9));
    }

    #[test]
    fn fractional_limit_floors_and_zero_limit_has_no_excess() {
        let mut home = province(3, 1, 1);
        home.trade_good = Some("unknown".to_owned());
        let partial = opened(rules(0, Money::new(1, 1)), home, vec![]);
        assert_eq!(partial.country_land_force_limit("AAA"), Some(0));

        let mut world = opened(rules(0, Money::ZERO), province(0, 1, 1), vec![]);
        add_regiments(&mut world, 2, 1);
        let budget = world.country_monthly_budget("AAA").unwrap();
        assert_eq!(budget.base_army_maintenance, Money::from(20) / Money::from(12));
        assert_eq!(budget.excess_army_maintenance, Money::ZERO);
        assert_eq!(budget.army_maintenance, budget.base_army_maintenance);
    }

    #[test]
    fn excess_scales_every_regiment_and_ignores_strength() {
        let mut world = opened(rules(1, Money::ZERO), province(0, 1, 1), vec![]);
        add_regiments(&mut world, 1, 1);
        let under = world.country_monthly_budget("AAA").unwrap();
        assert_eq!(under.excess_army_maintenance, Money::ZERO);
        assert_eq!(under.base_army_maintenance, Money::from(10) / Money::from(12));

        add_regiments(&mut world, 1, 1_000);
        let over = world.country_monthly_budget("AAA").unwrap();
        assert_eq!(over.base_army_maintenance, Money::from(20) / Money::from(12));
        assert_eq!(over.excess_army_maintenance, Money::from(40) / Money::from(12));
        assert_eq!(over.army_maintenance, Money::from(60) / Money::from(12));
    }

    #[test]
    fn regiment_changes_update_count_limit_and_maintenance() {
        let mut world = opened(rules(1, Money::ZERO), province(0, 1, 10), vec![]);
        let started = date();
        world
            .countries
            .get_mut("AAA")
            .unwrap()
            .finances
            .refund(Money::from(100));
        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, started)
            .unwrap();
        assert_eq!(world.country_land_force_limit("AAA"), Some(1));
        assert_eq!(
            world.country_monthly_budget("AAA").unwrap().army_maintenance,
            Money::ZERO
        );

        let ready = started.checked_add_days(30).unwrap();
        world.complete_recruitments(ready);
        assert_eq!(world.armies().values().next().unwrap().regiments.len(), 1);
        assert_eq!(
            world
                .country_monthly_budget("AAA")
                .unwrap()
                .excess_army_maintenance,
            Money::ZERO
        );

        world
            .start_recruitment("AAA", 1, UnitKind::Infantry, ready)
            .unwrap();
        let both = ready.checked_add_days(30).unwrap();
        world.complete_recruitments(both);
        let army_id = world.armies().values().next().unwrap().id();
        let over = world.country_monthly_budget("AAA").unwrap();
        assert_eq!(over.army_maintenance, Money::from(60) / Money::from(12));

        let split_id = world.split_army("AAA", army_id, 1).unwrap();
        assert_eq!(
            world.country_monthly_budget("AAA").unwrap().army_maintenance,
            over.army_maintenance
        );
        world.merge_armies("AAA", army_id, &[split_id]).unwrap();
        assert_eq!(
            world.country_monthly_budget("AAA").unwrap().army_maintenance,
            over.army_maintenance
        );

        let split_id = world.split_army("AAA", army_id, 1).unwrap();
        let quote = world.quote_disband_army("AAA", split_id).unwrap();
        assert_eq!(quote.regiment_count, 1);
        assert_eq!(
            quote.monthly_maintenance,
            Money::from(50) / Money::from(12)
        );
        world.disband_army("AAA", split_id).unwrap();
        let after = world.country_monthly_budget("AAA").unwrap();
        assert_eq!(after.base_army_maintenance, Money::from(10) / Money::from(12));
        assert_eq!(after.excess_army_maintenance, Money::ZERO);
    }

    #[test]
    fn twelve_settlements_sum_to_annual_cost_and_survive_reload() {
        let mut world = opened(rules(1, Money::ZERO), province(0, 1, 1), vec![]);
        add_regiments(&mut world, 2, 1_000);
        let mut base_total = Money::ZERO;
        let mut excess_total = Money::ZERO;
        let mut treasury_drop = Money::ZERO;
        for month in 0..12 {
            let before = world.country("AAA").unwrap().finances.treasury();
            world.settle_monthly_finances(date().checked_add_months(month).unwrap());
            let settlement = world
                .country("AAA")
                .unwrap()
                .finances
                .last_settlement()
                .unwrap();
            base_total += settlement.base_maintenance();
            excess_total += settlement.excess_maintenance;
            treasury_drop += before - world.country("AAA").unwrap().finances.treasury();
        }
        assert_eq!(base_total, Money::from(20));
        assert_eq!(excess_total, Money::from(40));
        assert_eq!(treasury_drop, Money::from(60));

        let mut reloaded = opened(rules(1, Money::ZERO), province(0, 1, 1), vec![]);
        add_regiments(&mut reloaded, 2, 1_000);
        let settled_on = date();
        reloaded.settle_monthly_finances(settled_on);
        let once = reloaded.country("AAA").unwrap().finances.treasury();
        let saved = SavedGame::capture(&reloaded, &Simulation::new(settled_on), "AAA");
        let text = serde_json::to_string(&saved).unwrap();
        let mut restored = serde_json::from_str::<SavedGame>(&text)
            .unwrap()
            .restore(&reloaded)
            .unwrap();
        assert_eq!(
            restored.world.country("AAA").unwrap().finances.treasury(),
            once
        );
        let next = settled_on.checked_add_months(1).unwrap();
        reloaded.settle_monthly_finances(next);
        restored.world.settle_monthly_finances(next);
        assert_eq!(
            restored.world.country("AAA").unwrap().finances.treasury(),
            reloaded.country("AAA").unwrap().finances.treasury()
        );
        let record = restored
            .world
            .country("AAA")
            .unwrap()
            .finances
            .last_settlement()
            .unwrap();
        assert!(record.excess_maintenance > Money::ZERO);
        assert_eq!(
            record.base_maintenance() + record.excess_maintenance,
            record.army_maintenance
        );
    }

    #[test]
    fn old_finances_without_excess_fields_keep_their_maintenance() {
        let mut finances = CountryFinances::default();
        // 税额为零时每次结算恰好扣 1，用来确认旧档读入后仍继续扣基础维护。
        finances.settle(Money::ZERO, Money::from(12), Money::ZERO, date());
        let mut value = serde_json::to_value(&finances).unwrap();
        value.as_object_mut().unwrap().remove("excess_remainder");
        value["last_settlement"]
            .as_object_mut()
            .unwrap()
            .remove("excess_maintenance");
        let mut restored: CountryFinances = serde_json::from_value(value).unwrap();
        let record = restored.last_settlement().unwrap();
        assert_eq!(record.excess_maintenance, Money::ZERO);
        assert_eq!(record.base_maintenance(), record.army_maintenance);
        let treasury = restored.treasury();
        restored.settle(Money::ZERO, Money::from(12), Money::ZERO, date());
        assert_eq!(restored.treasury(), treasury - Money::from(1));
        assert_eq!(
            restored.last_settlement().unwrap().excess_maintenance,
            Money::ZERO
        );
    }
}
