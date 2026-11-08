// SPDX-License-Identifier: GPL-3.0-only

//! 管理国家和省份实体，并解析它们之间的归属关系。

mod combat;
mod construction;
mod economy;
mod manpower;
mod military;
mod movement;
mod occupation;
mod organization;
mod peace;
mod recruitment_cancellation;
mod reinforcement;
mod saved_state;
mod starting_army;
mod supply;
mod war;

use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use crate::{
    BuildingDefinition, Country, LandConnection, MilitaryRules, MilitaryState, Money,
    OCCUPATION_PHASE_DAYS, Province, ProvinceKind, WarState,
};

pub use supply::SupplyForecast;

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
    #[error("owned active province {0} has no base manpower")]
    MissingBaseManpower(u32),
    #[error("owned active province {0} has no base production")]
    MissingBaseProduction(u32),
    #[error("invalid manpower reserve: {0}")]
    InvalidManpower(String),
    #[error("invalid or duplicate building definition: {0}")]
    InvalidBuilding(String),
    #[error("province {province} references unknown building {building}")]
    UnknownBuilding { province: u32, building: String },
    #[error("invalid military rules")]
    InvalidMilitaryRules,
    #[error("invalid land connection: {0} - {1}")]
    InvalidLandConnection(u32, u32),
    #[error("province {province} has invalid occupation progress")]
    InvalidOccupation { province: u32 },
}

#[derive(Debug)]
pub struct World {
    countries: BTreeMap<String, Country>,
    provinces: BTreeMap<u32, Province>,
    // 内容可以保留废弃省份定义，只有实际存在于地图上的省份参与模拟。
    active_provinces: BTreeSet<u32>,
    buildings: BTreeMap<String, BuildingDefinition>,
    military_rules: MilitaryRules,
    military: MilitaryState,
    war_state: WarState,
    land_routes: BTreeMap<u32, BTreeMap<u32, u32>>,
    /// 控制者发生变化后由界面刷新政治地图。运行时标记，不写入存档。
    control_changed: bool,
}

impl World {
    /// 索引使用实体自身的标识，避免调用者提供不一致的键和值。
    pub fn new(
        countries: impl IntoIterator<Item = Country>,
        mut provinces: BTreeMap<u32, Province>,
        active_provinces: impl IntoIterator<Item = u32>,
        buildings: impl IntoIterator<Item = BuildingDefinition>,
        military_rules: MilitaryRules,
        connections: impl IntoIterator<Item = LandConnection>,
    ) -> Result<Self, WorldError> {
        if military_rules.infantry_cost < Money::ZERO
            || military_rules.cavalry_cost < Money::ZERO
            || military_rules.infantry_days == 0
            || military_rules.cavalry_days == 0
            || military_rules.infantry_speed <= Money::ZERO
            || military_rules.cavalry_speed <= Money::ZERO
            || !(Money::ZERO..=Money::ONE)
                .contains(&military_rules.recruitment_full_refund_progress)
            || military_rules.land_maintenance_factor < Money::ZERO
            || military_rules.monthly_reinforce < Money::ZERO
            || military_rules.extra_land_reinforce_cost < Money::ZERO
            || military_rules.manpower.recovery_months == 0
            || military_rules.manpower.infantry_cost == 0
            || military_rules.manpower.cavalry_cost == 0
            || military_rules.supply_base == 0
            || military_rules.starting_army_size < Money::ZERO
            || military_rules.starting_army_size_at_war < Money::ZERO
            || !military_rules.combat.is_playable()
        {
            return Err(WorldError::InvalidMilitaryRules);
        }
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
            if let Some(progress) = &province.occupation
                && (province.kind != ProvinceKind::Land
                    || progress.days == 0
                    || progress.days >= OCCUPATION_PHASE_DAYS
                    || !registry.contains_key(&progress.occupier))
            {
                return Err(WorldError::InvalidOccupation { province: id });
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
            if province.kind == ProvinceKind::Land && province.owner.is_some() {
                if province.development.manpower.is_none() {
                    return Err(WorldError::MissingBaseManpower(id));
                }
                if province.development.production.is_none() {
                    return Err(WorldError::MissingBaseProduction(id));
                }
            }
        }
        let mut land_routes: BTreeMap<u32, BTreeMap<u32, u32>> = BTreeMap::new();
        let slowest_speed = military_rules
            .infantry_speed
            .min(military_rules.cavalry_speed);
        for connection in connections {
            let LandConnection { from, to, distance } = connection;
            // 最慢兵种的天数能表示时，更快的兵种也不会超出范围。
            if from == to
                || distance == 0
                || u32::try_from((Money::from(distance) / slowest_speed).ceil()).is_err()
                || [from, to].iter().any(|id| {
                    !active_provinces.contains(id) || provinces[id].kind != ProvinceKind::Land
                })
            {
                return Err(WorldError::InvalidLandConnection(from, to));
            }
            land_routes.entry(from).or_default().insert(to, distance);
            land_routes.entry(to).or_default().insert(from, distance);
        }
        let mut world = Self {
            countries: registry,
            provinces,
            active_provinces,
            buildings: definitions,
            military_rules,
            military: MilitaryState::default(),
            war_state: WarState::default(),
            land_routes,
            control_changed: false,
        };
        world.initialize_manpower()?;
        Ok(world)
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

    /// 取出并清除“控制者已变化”标记，供政治地图刷新一次。
    pub fn take_control_changed(&mut self) -> bool {
        let changed = self.control_changed;
        self.control_changed = false;
        changed
    }
}
