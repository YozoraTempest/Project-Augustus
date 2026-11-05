// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    BuildingDefinition, Construction, ConstructionError, ConstructionQuote, GameDate, Money,
    Province, ProvinceKind,
};

use super::World;

impl World {
    pub fn building_definitions(&self) -> &BTreeMap<String, BuildingDefinition> {
        &self.buildings
    }

    pub fn province_building_tax_modifier(&self, id: u32) -> Option<Money> {
        let province = self.province(id)?;
        Some(
            province
                .buildings
                .iter()
                .map(|id| self.buildings[id].tax_modifier)
                .sum(),
        )
    }

    pub fn province_annual_tax(&self, id: u32) -> Option<Money> {
        Some(
            self.province(id)?.annual_tax()?
                * (Money::ONE + self.province_building_tax_modifier(id)?),
        )
    }

    pub fn province_monthly_tax_after_building(&self, id: u32, building: &str) -> Option<Money> {
        let province = self.province(id)?;
        let definition = self.buildings.get(building)?;
        let mut completed = province.clone();
        completed.buildings.insert(definition.id.clone());
        remove_obsolete(&mut completed, &self.buildings);
        let modifier: Money = completed
            .buildings
            .iter()
            .map(|id| self.buildings[id].tax_modifier)
            .sum();
        Some(completed.annual_tax()? * (Money::ONE + modifier) / Money::from(12))
    }

    /// 预览和实际建造共用条件；界面禁用按钮不能替代核心层对命令的检查。
    pub fn quote_construction(
        &self,
        country: &str,
        id: u32,
        building: &str,
        date: GameDate,
    ) -> Result<ConstructionQuote, ConstructionError> {
        let country = self
            .country(country)
            .ok_or(ConstructionError::UnknownCountry)?;
        let province = self
            .province(id)
            .filter(|province| {
                self.active_provinces.contains(&id) && province.kind == ProvinceKind::Land
            })
            .ok_or(ConstructionError::InvalidProvince)?;
        if province.owner.as_deref() != Some(country.tag())
            || province.controller.as_deref() != Some(country.tag())
        {
            return Err(ConstructionError::NotOwnedAndControlled);
        }
        let definition = self
            .buildings
            .get(building)
            .filter(|definition| definition.constructible)
            .ok_or(ConstructionError::UnavailableBuilding)?;
        let mut completed = province.clone();
        completed.buildings.insert(building.to_owned());
        remove_obsolete(&mut completed, &self.buildings);
        if province.buildings.contains(building) || !completed.buildings.contains(building) {
            return Err(ConstructionError::AlreadyBuilt);
        }
        if province.construction.is_some() {
            return Err(ConstructionError::AlreadyConstructing);
        }
        let available = country.finances.treasury();
        if available < definition.cost {
            return Err(ConstructionError::InsufficientFunds {
                cost: definition.cost,
                available,
            });
        }
        Ok(ConstructionQuote {
            cost: definition.cost,
            completes_on: date
                .checked_add_months(definition.months)
                .ok_or(ConstructionError::DateOutOfRange)?,
            monthly_tax_before: self.province_monthly_tax(id).expect("有效领土具备税收"),
            monthly_tax_after: self
                .province_monthly_tax_after_building(id, building)
                .expect("有效建筑可以预览税收"),
        })
    }

    pub fn start_construction(
        &mut self,
        country: &str,
        id: u32,
        building: &str,
        date: GameDate,
    ) -> Result<(), ConstructionError> {
        let quote = self.quote_construction(country, id, building, date)?;
        self.countries
            .get_mut(country)
            .expect("建造国家已注册")
            .finances
            .spend(quote.cost);
        self.provinces
            .get_mut(&id)
            .expect("建造省份已存在")
            .construction = Some(Construction {
            building: building.to_owned(),
            country: country.to_owned(),
            started_on: date,
            completes_on: quote.completes_on,
        });
        Ok(())
    }

    pub(crate) fn complete_constructions(&mut self, date: GameDate) {
        for id in &self.active_provinces {
            let province = self.provinces.get_mut(id).expect("有效省份具有定义");
            if province
                .construction
                .as_ref()
                .is_some_and(|project| project.completes_on <= date)
            {
                let project = province.construction.take().expect("建设项目已到期");
                province.buildings.insert(project.building);
                remove_obsolete(province, &self.buildings);
            }
        }
    }
}

pub(super) fn remove_obsolete(
    province: &mut Province,
    definitions: &BTreeMap<String, BuildingDefinition>,
) {
    let mut obsolete = BTreeSet::new();
    for building in &province.buildings {
        let mut next = definitions
            .get(building)
            .and_then(|definition| definition.replaces.as_ref());
        while let Some(id) = next {
            if !obsolete.insert(id.clone()) {
                break;
            }
            next = definitions
                .get(id)
                .and_then(|definition| definition.replaces.as_ref());
        }
    }
    province.buildings.retain(|id| !obsolete.contains(id));
}
