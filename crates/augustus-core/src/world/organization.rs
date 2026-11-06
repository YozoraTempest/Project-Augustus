// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeSet;

use crate::{Army, ArmyDisbandQuote, ArmyOrganizationError, Money};

use super::World;

impl World {
    pub fn quote_disband_army(
        &self,
        country: &str,
        id: u32,
    ) -> Result<ArmyDisbandQuote, ArmyOrganizationError> {
        let army = self.stationary_army(country, id)?;
        let before = self.country_army_cost(country, None);
        let after = self.country_army_cost(country, Some(id));
        Ok(ArmyDisbandQuote {
            regiment_count: army.regiments.len(),
            strength: army.strength(),
            // 超编时减少一团还会降低其余部队的系数，所以按全国年费差额折成月费。
            monthly_maintenance: (before.total_annual() - after.total_annual()) / Money::from(12),
            // 基础规则不返还人力；原版返还能力依赖尚未实现的陆军职业度。
            manpower_returned: 0,
        })
    }

    pub fn disband_army(
        &mut self,
        country: &str,
        id: u32,
    ) -> Result<ArmyDisbandQuote, ArmyOrganizationError> {
        let quote = self.quote_disband_army(country, id)?;
        // 只移除选定军队；已支付的费用和人力、独立的招募项目均保持原状态。
        self.military.armies.remove(&id).expect("解散军队已检查");
        Ok(quote)
    }

    /// 目标军队保留编号；所有参与军队通过检查后才开始转移部队。
    pub fn merge_armies(
        &mut self,
        country: &str,
        target: u32,
        sources: &[u32],
    ) -> Result<(), ArmyOrganizationError> {
        let province = self.stationary_army(country, target)?.province;
        if sources.is_empty() {
            return Err(ArmyOrganizationError::NothingToMerge);
        }
        let mut ids = BTreeSet::from([target]);
        for &id in sources {
            if !ids.insert(id) {
                return Err(ArmyOrganizationError::DuplicateArmy);
            }
            if self.stationary_army(country, id)?.province != province {
                return Err(ArmyOrganizationError::DifferentProvince);
            }
        }
        for id in sources {
            let source = self.military.armies.remove(id).expect("合并军队已检查");
            self.military
                .armies
                .get_mut(&target)
                .expect("目标军队已检查")
                .regiments
                .extend(source.regiments);
        }
        Ok(())
    }

    pub fn quote_split_army(
        &self,
        country: &str,
        id: u32,
        count: usize,
    ) -> Result<(), ArmyOrganizationError> {
        let army = self.stationary_army(country, id)?;
        if count == 0 || count >= army.regiments.len() {
            return Err(ArmyOrganizationError::InvalidRegimentCount);
        }
        if self.military.next_army_id == u32::MAX {
            return Err(ArmyOrganizationError::ArmyLimit);
        }
        Ok(())
    }

    /// 按原有顺序拆出末尾的完整团，保留各团的兵种和实际兵力。
    pub fn split_army(
        &mut self,
        country: &str,
        id: u32,
        count: usize,
    ) -> Result<u32, ArmyOrganizationError> {
        self.quote_split_army(country, id, count)?;
        let army = self.military.armies.get_mut(&id).expect("拆分军队已检查");
        let new_id = self.military.next_army_id;
        let new_army = Army {
            id: new_id,
            country: army.country.clone(),
            province: army.province,
            regiments: army.regiments.split_off(army.regiments.len() - count),
            movement: None,
        };
        // 招募和拆分共用编号序列，已预留给招募的编号不会被重新分配。
        self.military.next_army_id += 1;
        self.military.armies.insert(new_id, new_army);
        Ok(new_id)
    }

    fn stationary_army(&self, country: &str, id: u32) -> Result<&Army, ArmyOrganizationError> {
        if self.country(country).is_none() {
            return Err(ArmyOrganizationError::UnknownCountry);
        }
        let army = self
            .armies()
            .get(&id)
            .ok_or(ArmyOrganizationError::UnknownArmy)?;
        if army.country != country {
            return Err(ArmyOrganizationError::NotPlayerArmy);
        }
        if army.movement.is_some() {
            return Err(ArmyOrganizationError::ArmyMoving);
        }
        Ok(army)
    }
}
