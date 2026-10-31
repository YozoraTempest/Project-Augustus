// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{ArmyId, EntityId, FleetId, NavalUnitId, RegimentId};
use serde::Serialize;

use crate::{Error, VanillaWorld};

/// Current units present in the shared world, rather than a completeness claim
/// about the military state of the source campaign.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMilitaryReport {
    pub armies: Vec<VanillaMilitaryArmy>,
    pub fleets: Vec<VanillaMilitaryFleet>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMilitaryArmy {
    pub id: ArmyId,
    pub country: String,
    pub province_id: u32,
    pub province_name: String,
    pub name: String,
    pub regiments: Vec<VanillaMilitaryRegiment>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMilitaryRegiment {
    pub id: RegimentId,
    pub unit_type: String,
    pub name: Option<String>,
    pub strength_milli: Option<i64>,
    pub morale_milli: Option<i64>,
    pub home_province: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMilitaryFleet {
    pub id: FleetId,
    pub country: String,
    pub province_id: u32,
    pub province_name: String,
    pub name: Option<String>,
    pub ships: Vec<VanillaMilitaryShip>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaMilitaryShip {
    pub id: NavalUnitId,
    pub unit_type: String,
    pub name: Option<String>,
    pub morale_milli: Option<i64>,
    pub home_province: Option<u32>,
}

impl VanillaWorld {
    /// Reads live armies, fleet membership and unit state after advancement or
    /// restoration. Both optional filters apply together and must refer to
    /// loaded entities. IDs identify Augustus units; province references use
    /// EU4 province numbers. This query does not advance time or consume RNG.
    pub fn military_roster(
        &self,
        country: Option<&str>,
        province: Option<u32>,
    ) -> Result<VanillaMilitaryReport, Error> {
        let invalid = |message| Error::message("military roster", message);
        let country_filter = country
            .map(|tag| {
                self.initial
                    .country_entity(tag)
                    .ok_or_else(|| invalid("unknown military country"))
            })
            .transpose()?;
        let province_filter = province
            .map(|id| {
                self.initial
                    .province_entity(id)
                    .ok_or_else(|| invalid("unknown military province"))
            })
            .transpose()?;
        let countries = self
            .initial
            .countries()
            .iter()
            .filter_map(|state| {
                self.initial
                    .country_entity(&state.id)
                    .map(|entity| (entity, state.id.as_str()))
            })
            .collect::<BTreeMap<_, _>>();
        let provinces = self
            .initial
            .provinces()
            .iter()
            .filter_map(|state| {
                self.initial
                    .province_entity(state.id)
                    .map(|entity| (entity, state))
            })
            .collect::<BTreeMap<_, _>>();
        let country_tag = |entity| {
            countries
                .get(&entity)
                .copied()
                .ok_or_else(|| invalid("unit country is not loaded"))
        };
        let province_state = |entity| {
            provinces
                .get(&entity)
                .copied()
                .ok_or_else(|| invalid("unit province is not loaded"))
        };
        let home_province = |entity: Option<EntityId>| -> Result<Option<u32>, Error> {
            entity
                .map(|entity| province_state(entity).map(|state| state.id))
                .transpose()
        };
        let matches = |owner, location| {
            country_filter.is_none_or(|country| country == owner)
                && province_filter.is_none_or(|province| province == location)
        };
        let armies = self
            .initial
            .armies()
            .iter()
            .filter(|army| matches(army.owner, army.location))
            .map(|army| {
                let province = province_state(army.location)?;
                let regiments = army
                    .regiments
                    .iter()
                    .map(|regiment| {
                        Ok(VanillaMilitaryRegiment {
                            id: regiment.id,
                            unit_type: regiment.unit_type.clone(),
                            name: regiment.name.clone(),
                            strength_milli: regiment.strength_milli,
                            morale_milli: regiment.morale_milli,
                            home_province: home_province(regiment.home_province)?,
                        })
                    })
                    .collect::<Result<_, Error>>()?;
                Ok(VanillaMilitaryArmy {
                    id: army.id,
                    country: country_tag(army.owner)?.to_owned(),
                    province_id: province.id,
                    province_name: province.name.clone(),
                    name: army.name.clone(),
                    regiments,
                })
            })
            .collect::<Result<_, Error>>()?;
        let ships = self
            .initial
            .naval_units()
            .iter()
            .map(|ship| (ship.id, ship))
            .collect::<BTreeMap<_, _>>();
        let fleets = self
            .initial
            .fleets()
            .iter()
            .filter(|fleet| matches(fleet.owner, fleet.location))
            .map(|fleet| {
                let province = province_state(fleet.location)?;
                let members = fleet
                    .ships
                    .iter()
                    .map(|id| {
                        let ship = ships
                            .get(id)
                            .ok_or_else(|| invalid("fleet member is not loaded"))?;
                        Ok(VanillaMilitaryShip {
                            id: ship.id,
                            unit_type: ship.unit_type.clone(),
                            name: ship.name.clone(),
                            morale_milli: ship.morale_milli,
                            home_province: home_province(ship.home_province)?,
                        })
                    })
                    .collect::<Result<_, Error>>()?;
                Ok(VanillaMilitaryFleet {
                    id: fleet.id,
                    country: country_tag(fleet.owner)?.to_owned(),
                    province_id: province.id,
                    province_name: province.name.clone(),
                    name: fleet.name.clone(),
                    ships: members,
                })
            })
            .collect::<Result<_, Error>>()?;
        Ok(VanillaMilitaryReport { armies, fleets })
    }
}
