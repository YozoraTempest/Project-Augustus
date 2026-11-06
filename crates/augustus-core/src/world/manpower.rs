// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;

use crate::{ManpowerPool, ManpowerSummary, ProvinceKind, WorldError};

use super::World;

impl World {
    pub fn country_manpower(&self, tag: &str) -> Option<ManpowerSummary> {
        let pool = self
            .country(tag)?
            .manpower
            .as_ref()
            .expect("国家人力已初始化");
        let capacity = self.manpower_capacities()[tag];
        Some(ManpowerSummary {
            available: pool.available,
            capacity,
            monthly_recovery: if capacity == 0 {
                0
            } else {
                pool.monthly_recovery(capacity, &self.military_rules.manpower)
            },
        })
    }

    pub(super) fn initialize_manpower(&mut self) -> Result<(), WorldError> {
        let capacities = self.manpower_capacities();
        for (tag, country) in &mut self.countries {
            let capacity = capacities[tag];
            if let Some(pool) = &country.manpower {
                if pool.recovery_remainder >= self.military_rules.manpower.recovery_months {
                    return Err(WorldError::InvalidManpower(tag.clone()));
                }
            } else {
                // 新国家和旧版存档都从满储备开始；已有部队和招募不重复消耗人力。
                country.manpower = Some(ManpowerPool::full(capacity));
            }
        }
        Ok(())
    }

    pub(crate) fn recover_monthly_manpower(&mut self) {
        let capacities = self.manpower_capacities();
        for (tag, country) in &mut self.countries {
            country
                .manpower
                .as_mut()
                .expect("国家人力已初始化")
                .recover(capacities[tag], &self.military_rules.manpower);
        }
    }

    fn manpower_capacities(&self) -> BTreeMap<String, u64> {
        let mut capacities: BTreeMap<_, _> =
            self.countries.keys().map(|tag| (tag.clone(), 0)).collect();
        let mut landed = std::collections::BTreeSet::new();
        let rules = &self.military_rules.manpower;
        for id in &self.active_provinces {
            let province = &self.provinces[id];
            if province.kind == ProvinceKind::Land
                && let Some(tag) = &province.owner
            {
                let capacity = capacities.get_mut(tag).expect("省份国家已注册");
                if landed.insert(tag) {
                    *capacity += rules.base_capacity;
                }
                *capacity += u64::from(
                    province
                        .development
                        .manpower
                        .expect("有效领土具备人力发展度"),
                ) * rules.per_development;
            }
        }
        capacities
    }
}
