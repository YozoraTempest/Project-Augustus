// SPDX-License-Identifier: GPL-3.0-only

//! 管理国家和省份实体，并解析它们之间的归属关系。

use std::collections::{BTreeMap, btree_map::Entry};

use crate::{Country, Province, ProvinceKind};

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
}

#[derive(Debug)]
pub struct World {
    countries: BTreeMap<String, Country>,
    provinces: BTreeMap<u32, Province>,
}

impl World {
    /// 索引使用实体自身的标识，避免调用者提供不一致的键和值。
    pub fn new(
        countries: impl IntoIterator<Item = Country>,
        provinces: BTreeMap<u32, Province>,
    ) -> Result<Self, WorldError> {
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
        Ok(Self {
            countries: registry,
            provinces,
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

    /// 借用注册表中的同一国家，不在每个省份内复制国家状态。
    /// 无所有者或省份不存在时返回 None。
    pub fn province_owner(&self, id: u32) -> Option<&Country> {
        self.country(self.province(id)?.owner.as_deref()?)
    }
}
