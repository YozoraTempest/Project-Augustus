// SPDX-License-Identifier: GPL-3.0-only

//! 管理国家和省份实体，并解析它们之间的归属关系。

mod construction;
mod saved_state;

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use crate::{BuildingDefinition, Country, GameDate, Money, Province, ProvinceKind};

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("duplicate country tag: {0}")]
    DuplicateCountry(String),
    #[error("province {province} references unknown country {owner}")]
    UnknownOwner { province: u32, owner: String },
    #[error("province {province} references unknown controller {controller}")]
    UnknownController { province: u32, controller: String },
    #[error("country {country} references unknown capital province {province}")]
    UnknownCapital { country: String, province: u32 },
    #[error("non-land province {0} cannot have an owner")]
    NonLandOwner(u32),
    #[error("province ID 0 is reserved")]
    ReservedProvince,
    #[error("active province {0} has no definition")]
    UnknownActiveProvince(u32),
    #[error("owned active province {0} has no base tax")]
    MissingBaseTax(u32),
    #[error("invalid or duplicate building definition: {0}")]
    InvalidBuilding(String),
    #[error("province {province} references unknown building {building}")]
    UnknownBuilding { province: u32, building: String },
}

#[derive(Debug)]
pub struct World {
    countries: BTreeMap<String, Country>,
    provinces: BTreeMap<u32, Province>,
    // 内容可以保留废弃省份定义，只有实际存在于地图上的省份参与模拟。
    active_provinces: BTreeSet<u32>,
    buildings: BTreeMap<String, BuildingDefinition>,
}

impl World {
    /// 索引使用实体自身的标识，避免调用者提供不一致的键和值。
    pub fn new(
        countries: impl IntoIterator<Item = Country>,
        mut provinces: BTreeMap<u32, Province>,
        active_provinces: impl IntoIterator<Item = u32>,
        buildings: impl IntoIterator<Item = BuildingDefinition>,
    ) -> Result<Self, WorldError> {
        let mut definitions = BTreeMap::new();
        for building in buildings {
            let id = building.id.clone();
            if building.cost < Money::ZERO
                || building.months == 0
                || definitions.insert(id.clone(), building).is_some()
            {
                return Err(WorldError::InvalidBuilding(id));
            }
        }
        for (&id, province) in &mut provinces {
            for building in &province.buildings {
                if !definitions.contains_key(building) {
                    return Err(WorldError::UnknownBuilding {
                        province: id,
                        building: building.clone(),
                    });
                }
            }
            construction::remove_obsolete(province, &definitions);
        }
        let mut registry = BTreeMap::new();
        for country in countries {
            match registry.entry(country.tag().to_owned()) {
                Entry::Vacant(entry) => {
                    entry.insert(country);
                }
                Entry::Occupied(entry) => {
                    return Err(WorldError::DuplicateCountry(entry.key().clone()));
                }
            }
        }
        for (&id, province) in &provinces {
            if id == 0 {
                return Err(WorldError::ReservedProvince);
            }
            if let Some(owner) = &province.owner {
                if !registry.contains_key(owner) {
                    return Err(WorldError::UnknownOwner {
                        province: id,
                        owner: owner.clone(),
                    });
                }
                if province.kind != ProvinceKind::Land {
                    return Err(WorldError::NonLandOwner(id));
                }
            }
            if let Some(controller) = &province.controller
                && !registry.contains_key(controller)
            {
                return Err(WorldError::UnknownController {
                    province: id,
                    controller: controller.clone(),
                });
            }
        }
        for country in registry.values() {
            if let Some(capital) = country.capital
                && !provinces.contains_key(&capital)
            {
                return Err(WorldError::UnknownCapital {
                    country: country.tag().into(),
                    province: capital,
                });
            }
        }
        let active_provinces: BTreeSet<_> = active_provinces.into_iter().collect();
        for &id in &active_provinces {
            let province = provinces
                .get(&id)
                .ok_or(WorldError::UnknownActiveProvince(id))?;
            if province.annual_tax().is_none() {
                return Err(WorldError::MissingBaseTax(id));
            }
        }
        Ok(Self {
            countries: registry,
            provinces,
            active_provinces,
            buildings: definitions,
        })
    }

    pub fn countries(&self) -> &BTreeMap<String, Country> {
        &self.countries
    }

    pub fn country(&self, tag: &str) -> Option<&Country> {
        self.countries.get(tag)
    }

    pub fn provinces(&self) -> &BTreeMap<u32, Province> {
        &self.provinces
    }

    pub fn province(&self, id: u32) -> Option<&Province> {
        self.provinces.get(&id)
    }

    pub fn province_monthly_tax(&self, id: u32) -> Option<Money> {
        if !self.active_provinces.contains(&id) {
            return None;
        }
        Some(self.province_annual_tax(id)? / Money::from(12))
    }

    pub fn country_monthly_tax(&self, tag: &str) -> Option<Money> {
        self.country(tag)?;
        let mut annual = Money::ZERO;
        for id in &self.active_provinces {
            let province = &self.provinces[id];
            if province.owner.as_deref() == Some(tag) {
                annual += self.province_annual_tax(*id)?;
            }
        }
        Some(annual / Money::from(12))
    }

    pub(crate) fn settle_monthly_taxes(&mut self, date: GameDate) {
        // 一次遍历汇总各国年税额，最后统一除以 12，避免逐省舍入改变国家总收入。
        let mut annual: BTreeMap<_, _> = self
            .countries
            .keys()
            .map(|tag| (tag.clone(), Money::ZERO))
            .collect();
        for id in &self.active_provinces {
            let province = &self.provinces[id];
            if let Some(tag) = &province.owner {
                *annual.get_mut(tag).expect("省份所有者已注册") +=
                    self.province_annual_tax(*id).expect("有效领土具备基础税收");
            }
        }
        for (tag, country) in &mut self.countries {
            country.finances.settle_tax(annual[tag], date);
        }
    }

    /// 借用注册表中的同一国家，不在每个省份内复制国家状态。
    /// 无所有者或省份不存在时返回 None。
    pub fn province_owner(&self, id: u32) -> Option<&Country> {
        self.country(self.province(id)?.owner.as_deref()?)
    }
}
