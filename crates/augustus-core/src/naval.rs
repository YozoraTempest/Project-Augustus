// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NavalUnitId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FleetId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NavalUnit {
    pub id: NavalUnitId,
    pub owner: EntityId,
    pub location: EntityId,
    pub unit_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub morale_milli: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_province: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct NavalUnitState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub morale_milli: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub home_province: Option<EntityId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fleet {
    pub id: FleetId,
    pub owner: EntityId,
    pub location: EntityId,
    #[serde(default)]
    pub name: Option<String>,
    pub ships: Vec<NavalUnitId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fleets {
    next_id: u64,
    fleets: Vec<Fleet>,
}

impl Fleets {
    pub(crate) fn is_empty(&self) -> bool {
        self.fleets.is_empty()
    }

    fn next_id(&self) -> Result<FleetId, Error> {
        self.next_id.checked_add(1).ok_or(Error::Overflow)?;
        Ok(FleetId(self.next_id))
    }

    fn push(
        &mut self,
        id: FleetId,
        owner: EntityId,
        location: EntityId,
        name: Option<String>,
        ships: Vec<NavalUnitId>,
    ) {
        self.next_id = id.0 + 1;
        self.fleets.push(Fleet {
            id,
            owner,
            location,
            name,
            ships,
        });
    }

    pub(crate) fn ensure_singleton_fleets(
        &mut self,
        naval_units: &NavalUnits,
    ) -> Result<(), Error> {
        let mut assigned: BTreeSet<_> = self
            .fleets
            .iter()
            .flat_map(|fleet| fleet.ships.iter().copied())
            .collect();
        let unassigned = naval_units
            .units
            .iter()
            .filter(|ship| !assigned.contains(&ship.id))
            .collect::<Vec<_>>();
        self.next_id
            .checked_add(u64::try_from(unassigned.len()).map_err(|_| Error::Overflow)?)
            .ok_or(Error::Overflow)?;

        for ship in unassigned {
            let id = self.next_id()?;
            self.push(id, ship.owner, ship.location, None, vec![ship.id]);
            assigned.insert(ship.id);
        }
        Ok(())
    }

    pub(crate) fn validate(
        &self,
        entities: &[crate::Entity],
        naval_units: &NavalUnits,
    ) -> Result<(), Error> {
        let ships = naval_units
            .units
            .iter()
            .map(|ship| (ship.id, ship))
            .collect::<std::collections::BTreeMap<_, _>>();
        let mut fleet_ids = BTreeSet::new();
        let mut assigned_ships = BTreeSet::new();
        for fleet in &self.fleets {
            if !fleet_ids.insert(fleet.id) || fleet.id.0 >= self.next_id {
                return Err(Error::Invalid("invalid fleet identifier".into()));
            }
            let owner = entities
                .get(fleet.owner.0)
                .ok_or_else(|| Error::Invalid("fleet references unknown owner".into()))?;
            let location = entities
                .get(fleet.location.0)
                .ok_or_else(|| Error::Invalid("fleet references unknown location".into()))?;
            if owner.kind != EntityKind::Country
                || location.kind != EntityKind::Province
                || fleet.ships.is_empty()
            {
                return Err(Error::Invalid(
                    "invalid fleet owner, location, or ships".into(),
                ));
            }
            for id in &fleet.ships {
                if !assigned_ships.insert(*id) {
                    return Err(Error::Invalid("ship belongs to multiple fleets".into()));
                }
                let ship = ships
                    .get(id)
                    .ok_or_else(|| Error::Invalid("fleet references unknown ship".into()))?;
                if ship.owner != fleet.owner || ship.location != fleet.location {
                    return Err(Error::Invalid("fleet and ship state do not match".into()));
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub(crate) struct NavalUnits {
    next_id: u64,
    units: Vec<NavalUnit>,
}

impl NavalUnits {
    pub(crate) fn is_empty(&self) -> bool {
        self.units.is_empty()
    }

    pub(crate) fn validate_type(unit_type: &str) -> Result<(), Error> {
        if unit_type.is_empty() || unit_type.bytes().any(|byte| byte.is_ascii_whitespace()) {
            return Err(Error::Invalid("invalid naval unit type".into()));
        }
        Ok(())
    }

    fn create_many(
        &mut self,
        owner: EntityId,
        location: EntityId,
        ship_types: &[String],
        states: Option<&[NavalUnitState]>,
    ) -> Result<Vec<NavalUnitId>, Error> {
        if ship_types.is_empty() {
            return Err(Error::Invalid("fleet requires at least one ship".into()));
        }
        for ship_type in ship_types {
            Self::validate_type(ship_type)?;
        }
        if states.is_some_and(|states| !states.is_empty() && states.len() != ship_types.len()) {
            return Err(Error::Invalid(
                "fleet ship state count does not match ship count".into(),
            ));
        }
        if states.is_some_and(|states| {
            states
                .iter()
                .any(|state| state.morale_milli.is_some_and(|morale| morale < 0))
        }) {
            return Err(Error::Invalid("invalid naval unit morale".into()));
        }
        let count = u64::try_from(ship_types.len()).map_err(|_| Error::Overflow)?;
        let next_id = self.next_id.checked_add(count).ok_or(Error::Overflow)?;
        let ids = (self.next_id..next_id).map(NavalUnitId).collect::<Vec<_>>();
        self.units
            .extend(ship_types.iter().enumerate().zip(ids.iter().copied()).map(
                |((index, unit_type), id)| {
                    let state = states.and_then(|states| states.get(index));
                    NavalUnit {
                        id,
                        owner,
                        location,
                        unit_type: unit_type.clone(),
                        name: state.and_then(|state| state.name.clone()),
                        morale_milli: state.and_then(|state| state.morale_milli),
                        home_province: state.and_then(|state| state.home_province),
                    }
                },
            ));
        self.next_id = next_id;
        Ok(ids)
    }

    pub(crate) fn validate(&self, entities: &[crate::Entity]) -> Result<(), Error> {
        let mut ids = BTreeSet::new();
        for unit in &self.units {
            if !ids.insert(unit.id) || unit.id.0 >= self.next_id {
                return Err(Error::Invalid("invalid naval unit identifier".into()));
            }
            Self::validate_type(&unit.unit_type)?;
            let owner = entities
                .get(unit.owner.0)
                .ok_or_else(|| Error::Invalid("naval unit references unknown owner".into()))?;
            let location = entities
                .get(unit.location.0)
                .ok_or_else(|| Error::Invalid("naval unit references unknown location".into()))?;
            if owner.kind != EntityKind::Country || location.kind != EntityKind::Province {
                return Err(Error::Invalid(
                    "invalid naval unit owner or location".into(),
                ));
            }
            if unit.morale_milli.is_some_and(|morale| morale < 0) {
                return Err(Error::Invalid("invalid naval unit morale".into()));
            }
            if let Some(home) = unit.home_province
                && entities
                    .get(home.0)
                    .is_none_or(|entity| entity.kind != EntityKind::Province)
            {
                return Err(Error::Invalid(
                    "naval unit references an invalid home province".into(),
                ));
            }
        }
        Ok(())
    }
}

impl World {
    pub fn fleets(&self) -> &[Fleet] {
        &self.fleets.fleets
    }

    pub fn naval_units(&self) -> &[NavalUnit] {
        &self.naval_units.units
    }

    fn insert_fleet(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: Option<String>,
        ship_types: &[String],
        states: Option<&[NavalUnitState]>,
    ) -> Result<(FleetId, Vec<NavalUnitId>), Error> {
        let fleet_id = self.fleets.next_id()?;
        let ship_ids = self
            .naval_units
            .create_many(owner, location, ship_types, states)?;
        self.fleets
            .push(fleet_id, owner, location, name, ship_ids.clone());
        Ok((fleet_id, ship_ids))
    }

    pub(crate) fn bind_fleet(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: Option<String>,
        ship_types: &[String],
    ) -> Result<FleetId, Error> {
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("fleet owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid("fleet location must be a province".into()));
        }
        self.insert_fleet(owner, location, name, ship_types, None)
            .map(|(id, _)| id)
    }

    pub(crate) fn bind_fleet_with_ship_states(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: Option<String>,
        ship_types: &[String],
        states: &[NavalUnitState],
    ) -> Result<FleetId, Error> {
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("fleet owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid("fleet location must be a province".into()));
        }
        self.validate_ship_home_provinces(states)?;
        self.insert_fleet(owner, location, name, ship_types, Some(states))
            .map(|(id, _)| id)
    }

    pub fn create_fleet(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: Option<String>,
        ship_types: &[String],
    ) -> Result<FleetId, Error> {
        self.require_running()?;
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("fleet owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid("fleet location must be a province".into()));
        }
        if !self.province_has_port(location)? {
            return Err(Error::Invalid(
                "fleet creation requires a port province".into(),
            ));
        }
        self.insert_fleet(owner, location, name, ship_types, None)
            .map(|(id, _)| id)
    }

    pub fn create_fleet_with_ship_states(
        &mut self,
        owner: EntityId,
        location: EntityId,
        name: Option<String>,
        ship_types: &[String],
        states: &[NavalUnitState],
    ) -> Result<FleetId, Error> {
        self.require_running()?;
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("fleet owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid("fleet location must be a province".into()));
        }
        if !self.province_has_port(location)? {
            return Err(Error::Invalid(
                "fleet creation requires a port province".into(),
            ));
        }
        self.validate_ship_home_provinces(states)?;
        self.insert_fleet(owner, location, name, ship_types, Some(states))
            .map(|(id, _)| id)
    }

    pub fn create_naval_unit(
        &mut self,
        owner: EntityId,
        location: EntityId,
        unit_type: &str,
    ) -> Result<NavalUnitId, Error> {
        self.require_running()?;
        if self.entity(owner)?.kind != EntityKind::Country {
            return Err(Error::Invalid("naval unit owner must be a country".into()));
        }
        if self.entity(location)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "naval unit location must be a province".into(),
            ));
        }
        if !self.province_has_port(location)? {
            return Err(Error::Invalid("naval unit requires a port province".into()));
        }
        let ship_type = unit_type.to_owned();
        self.insert_fleet(
            owner,
            location,
            None,
            std::slice::from_ref(&ship_type),
            None,
        )
        .map(|(_, mut ships)| ships.remove(0))
    }

    fn validate_ship_home_provinces(&self, states: &[NavalUnitState]) -> Result<(), Error> {
        for state in states {
            if let Some(home) = state.home_province
                && self.entity(home)?.kind != EntityKind::Province
            {
                return Err(Error::Invalid(
                    "naval unit home province must be a province".into(),
                ));
            }
        }
        Ok(())
    }
}
