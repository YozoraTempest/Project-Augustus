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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_province_owners_to_registered_countries() {
        let countries = [Country::new("AAA"), Country::new("BBB")];
        let provinces = BTreeMap::from([
            (
                1,
                Province {
                    owner: Some("AAA".into()),
                    ..Province::new(ProvinceKind::Land)
                },
            ),
            (
                2,
                Province {
                    owner: None,
                    ..Province::new(ProvinceKind::Land)
                },
            ),
            (
                3,
                Province {
                    owner: None,
                    ..Province::new(ProvinceKind::Sea)
                },
            ),
            (
                4,
                Province {
                    owner: Some("AAA".into()),
                    ..Province::new(ProvinceKind::Land)
                },
            ),
        ]);
        let world = World::new(countries, provinces).unwrap();
        let country = world.country("AAA").unwrap();
        assert_eq!(country.tag(), "AAA");
        assert!(std::ptr::eq(country, world.province_owner(1).unwrap()));
        assert!(std::ptr::eq(country, world.province_owner(4).unwrap()));
        assert_eq!(world.countries().len(), 2);
        assert_eq!(world.country("BBB").unwrap().tag(), "BBB");
        assert!(world.country("ZZZ").is_none());
        assert!(world.province_owner(2).is_none());
        assert!(world.province_owner(3).is_none());
        assert!(world.province_owner(0).is_none());
        assert_eq!(world.province(2).unwrap().kind, ProvinceKind::Land);
        assert_eq!(world.province(3).unwrap().kind, ProvinceKind::Sea);
        assert!(world.province(0).is_none());
    }

    #[test]
    fn rejects_duplicate_country_entities() {
        let error =
            World::new([Country::new("AAA"), Country::new("AAA")], BTreeMap::new()).unwrap_err();
        assert!(matches!(error, WorldError::DuplicateCountry(tag) if tag == "AAA"));
    }

    #[test]
    fn world_rejects_invalid_ownership() {
        let make = |kind, owner: &str| {
            World::new(
                [Country::new("AAA")],
                BTreeMap::from([(
                    1,
                    Province {
                        owner: Some(owner.into()),
                        ..Province::new(kind)
                    },
                )]),
            )
        };
        assert!(matches!(
            make(ProvinceKind::Land, "ZZZ"),
            Err(WorldError::UnknownOwner { .. })
        ));
        assert!(matches!(
            make(ProvinceKind::Lake, "AAA"),
            Err(WorldError::NonLandOwner(1))
        ));
    }
}
