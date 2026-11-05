// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use crate::{GameDate, Money, MonthlyBudget};

use super::World;

impl World {
    pub fn province_monthly_tax(&self, id: u32) -> Option<Money> {
        if !self.active_provinces.contains(&id) {
            return None;
        }
        Some(self.province_annual_tax(id)? / Money::from(12))
    }

    pub fn country_monthly_budget(&self, tag: &str) -> Option<MonthlyBudget> {
        self.country(tag)?;
        let annual_tax: Money = self
            .active_provinces
            .iter()
            .filter(|id| self.provinces[id].owner.as_deref() == Some(tag))
            .map(|id| self.province_annual_tax(*id).expect("有效领土具备基础税收"))
            .sum();
        let annual_maintenance: Money = self
            .armies()
            .values()
            .filter(|army| army.country == tag)
            .flat_map(|army| &army.regiments)
            .map(|regiment| self.military_rules.annual_maintenance(regiment.kind))
            .sum();
        Some(MonthlyBudget {
            tax: annual_tax / Money::from(12),
            army_maintenance: annual_maintenance / Money::from(12),
        })
    }

    pub(crate) fn settle_monthly_finances(&mut self, date: GameDate) {
        // 先汇总全年金额，再按国家统一除以十二，避免按省或按军队舍入改变总额。
        let mut totals: BTreeMap<_, _> = self
            .countries
            .keys()
            .map(|tag| (tag.clone(), (Money::ZERO, Money::ZERO)))
            .collect();
        for id in &self.active_provinces {
            if let Some(tag) = &self.provinces[id].owner {
                totals.get_mut(tag).expect("省份所有者已注册").0 +=
                    self.province_annual_tax(*id).expect("有效领土具备基础税收");
            }
        }
        for army in self.armies().values() {
            let annual: Money = army
                .regiments
                .iter()
                .map(|regiment| self.military_rules.annual_maintenance(regiment.kind))
                .sum();
            totals.get_mut(&army.country).expect("军队国家已注册").1 += annual;
        }
        for (tag, country) in &mut self.countries {
            let (tax, maintenance) = totals[tag];
            country.finances.settle(tax, maintenance, date);
        }
    }
}
