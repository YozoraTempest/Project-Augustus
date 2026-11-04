// SPDX-License-Identifier: GPL-3.0-only

//! 与内容格式和显示引擎无关的国家、省份及归属状态。

use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProvinceKind {
    Land,
    Sea,
    Lake,
    Wasteland,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Province {
    pub kind: ProvinceKind,
    /// 无主陆地与海洋、荒地均没有所有者，由 kind 区分其用途。
    pub owner: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WorldError {
    #[error("province {province} references unknown country {owner}")]
    UnknownOwner { province: u32, owner: String },
    #[error("non-land province {0} cannot have an owner")]
    NonLandOwner(u32),
    #[error("province ID 0 is reserved")]
    ReservedProvince,
}

#[derive(Debug)]
pub struct World {
    countries: BTreeSet<String>,
    provinces: BTreeMap<u32, Province>,
}

impl World {
    /// 内容适配层负责构建初始状态；核心层保证省份归属引用有效。
    pub fn new(
        countries: BTreeSet<String>,
        provinces: BTreeMap<u32, Province>,
    ) -> Result<Self, WorldError> {
        for (&id, province) in &provinces {
            if id == 0 {
                return Err(WorldError::ReservedProvince);
            }
            if let Some(owner) = &province.owner {
                if !countries.contains(owner) {
                    return Err(WorldError::UnknownOwner {
                        province: id,
                        owner: owner.clone(),
                    });
                }
                if province.kind != ProvinceKind::Land {
                    return Err(WorldError::NonLandOwner(id));
                }
            }
        }
        Ok(Self {
            countries,
            provinces,
        })
    }

    pub fn countries(&self) -> &BTreeSet<String> {
        &self.countries
    }

    pub fn provinces(&self) -> &BTreeMap<u32, Province> {
        &self.provinces
    }

    pub fn province(&self, id: u32) -> Option<&Province> {
        self.provinces.get(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_keeps_owned_and_unowned_provinces_distinct() {
        let countries = BTreeSet::from(["AAA".to_owned(), "BBB".to_owned()]);
        let provinces = BTreeMap::from([
            (
                1,
                Province {
                    kind: ProvinceKind::Land,
                    owner: Some("AAA".into()),
                },
            ),
            (
                2,
                Province {
                    kind: ProvinceKind::Land,
                    owner: None,
                },
            ),
            (
                3,
                Province {
                    kind: ProvinceKind::Sea,
                    owner: None,
                },
            ),
        ]);
        let world = World::new(countries, provinces).unwrap();
        assert_eq!(world.province(1).unwrap().owner.as_deref(), Some("AAA"));
        assert_eq!(world.province(2).unwrap().kind, ProvinceKind::Land);
        assert_eq!(world.province(3).unwrap().kind, ProvinceKind::Sea);
        assert!(world.countries().contains("BBB"));
        assert!(world.province(0).is_none());
    }

    #[test]
    fn world_rejects_invalid_ownership() {
        let make = |kind, owner: &str| {
            World::new(
                BTreeSet::from(["AAA".into()]),
                BTreeMap::from([(
                    1,
                    Province {
                        kind,
                        owner: Some(owner.into()),
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
