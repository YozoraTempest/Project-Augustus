// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

const ONE: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AreaStatus {
    State,
    Territory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreType {
    Full,
    Territorial,
    Unspecified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdministrativeCategory {
    StateFullCore,
    StateTerritorialCore,
    StateNonCore,
    TerritoryCore,
    TerritoryNonCore,
}

/// Ratios use millionths: 500_000 means 50 percent. Rules are supplied by content.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdministrationRules {
    pub state_full_minimum_micros: Option<i64>,
    pub state_territorial_minimum_micros: Option<i64>,
    pub state_noncore_minimum_micros: Option<i64>,
    pub territory_core_minimum_micros: Option<i64>,
    pub territory_noncore_minimum_micros: Option<i64>,
    pub capital_maximum_micros: Option<i64>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvinceAdministration {
    pub area: Option<String>,
    pub cores_known: bool,
    pub cores: BTreeMap<EntityId, CoreType>,
    /// The underlying value before applying the floor or capital ceiling.
    pub raw_autonomy_micros: Option<i64>,
    /// Unapplied history operations, retained separately from an unknown baseline.
    pub history_autonomy_delta_micros: i64,
    pub unresolved: BTreeSet<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdministrationEvaluation {
    pub area: Option<String>,
    pub area_status: Option<AreaStatus>,
    pub owner_core: Option<CoreType>,
    pub cores_known: bool,
    pub is_capital: bool,
    pub is_capital_area: bool,
    pub raw_autonomy_micros: Option<i64>,
    pub history_autonomy_delta_micros: i64,
    pub minimum_autonomy_micros: Option<i64>,
    pub effective_autonomy_micros: Option<i64>,
    pub unresolved: BTreeSet<String>,
}

impl AdministrationEvaluation {
    /// Classifies the current owner's claim and area status independently of autonomy.
    pub fn category(&self) -> Option<AdministrativeCategory> {
        if !self.cores_known {
            return None;
        }
        match (self.area_status, self.owner_core) {
            (Some(AreaStatus::State), Some(CoreType::Full)) => {
                Some(AdministrativeCategory::StateFullCore)
            }
            (Some(AreaStatus::State), Some(CoreType::Territorial)) => {
                Some(AdministrativeCategory::StateTerritorialCore)
            }
            (Some(AreaStatus::State), None) => Some(AdministrativeCategory::StateNonCore),
            (Some(AreaStatus::Territory), Some(_)) => Some(AdministrativeCategory::TerritoryCore),
            (Some(AreaStatus::Territory), None) => Some(AdministrativeCategory::TerritoryNonCore),
            _ => None,
        }
    }
}

/// Geographic membership is independent of each country's administrative status.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Administration {
    pub areas: BTreeSet<String>,
    pub country_areas: BTreeMap<EntityId, BTreeMap<String, AreaStatus>>,
    pub provinces: BTreeMap<EntityId, ProvinceAdministration>,
    pub rules: AdministrationRules,
}

impl Administration {
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
            && self.country_areas.is_empty()
            && self.provinces.is_empty()
            && self.rules == AdministrationRules::default()
    }

    pub fn evaluate(
        &self,
        world: &World,
        province: EntityId,
    ) -> Result<AdministrationEvaluation, Error> {
        let entity = world.entity(province)?;
        if entity.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "administration query requires a province".into(),
            ));
        }
        let state = self
            .provinces
            .get(&province)
            .ok_or_else(|| Error::Invalid("missing province administration".into()))?;
        let owner = entity.owner;
        let capital = owner
            .map(|id| world.entity(id).map(|e| e.capital))
            .transpose()?
            .flatten()
            .filter(|id| world.entity(*id).is_ok_and(|e| e.owner == owner));
        let is_capital = capital == Some(province);
        let is_capital_area = state.area.is_some()
            && capital
                .and_then(|id| self.provinces.get(&id))
                .is_some_and(|p| p.area == state.area);
        let status = owner
            .and_then(|id| self.country_areas.get(&id))
            .and_then(|areas| state.area.as_ref().and_then(|a| areas.get(a)))
            .copied();
        let core = owner.and_then(|id| state.cores.get(&id)).copied();
        let mut unresolved = state.unresolved.clone();
        if owner.is_none() {
            unresolved.insert("owner".into());
        }
        if state.area.is_none() {
            unresolved.insert("area_membership".into());
        }
        if status.is_none() {
            unresolved.insert("area_status".into());
        }
        if !state.cores_known {
            unresolved.insert("core_history".into());
        }
        if core == Some(CoreType::Unspecified) {
            unresolved.insert("core_type".into());
        }
        let minimum = if !state.cores_known {
            None
        } else {
            match (status, core) {
                (Some(AreaStatus::State), Some(CoreType::Full)) => {
                    self.rules.state_full_minimum_micros
                }
                (Some(AreaStatus::State), Some(CoreType::Territorial)) => {
                    self.rules.state_territorial_minimum_micros
                }
                (Some(AreaStatus::State), None) => self.rules.state_noncore_minimum_micros,
                (Some(AreaStatus::Territory), Some(_)) => self.rules.territory_core_minimum_micros,
                (Some(AreaStatus::Territory), None) => self.rules.territory_noncore_minimum_micros,
                _ => None,
            }
        };
        let effective = if is_capital && self.rules.capital_maximum_micros == Some(0) {
            Some(0)
        } else {
            if state.raw_autonomy_micros.is_none() {
                unresolved.insert("initial_autonomy".into());
            }
            if minimum.is_none() {
                unresolved.insert("minimum_autonomy".into());
            }
            match (state.raw_autonomy_micros, minimum) {
                (Some(raw), Some(minimum)) => {
                    let value = raw.max(minimum);
                    if is_capital {
                        match self.rules.capital_maximum_micros {
                            Some(maximum) => Some(value.min(maximum)),
                            None => {
                                unresolved.insert("capital_autonomy_ceiling".into());
                                None
                            }
                        }
                    } else {
                        Some(value)
                    }
                }
                _ => None,
            }
        };
        Ok(AdministrationEvaluation {
            area: state.area.clone(),
            area_status: status,
            owner_core: core,
            cores_known: state.cores_known,
            is_capital,
            is_capital_area,
            raw_autonomy_micros: state.raw_autonomy_micros,
            history_autonomy_delta_micros: state.history_autonomy_delta_micros,
            minimum_autonomy_micros: minimum,
            effective_autonomy_micros: effective,
            unresolved,
        })
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        if self.areas.iter().any(String::is_empty) {
            return Err(Error::Invalid("empty administrative area".into()));
        }
        for value in [
            self.rules.state_full_minimum_micros,
            self.rules.state_territorial_minimum_micros,
            self.rules.state_noncore_minimum_micros,
            self.rules.territory_core_minimum_micros,
            self.rules.territory_noncore_minimum_micros,
            self.rules.capital_maximum_micros,
        ]
        .into_iter()
        .flatten()
        {
            if !(0..=ONE).contains(&value) {
                return Err(Error::Invalid("autonomy ratio outside zero to one".into()));
            }
        }
        for (&country, areas) in &self.country_areas {
            if world.entity(country)?.kind != EntityKind::Country
                || areas.keys().any(|a| !self.areas.contains(a))
            {
                return Err(Error::Invalid("invalid country administrative area".into()));
            }
        }
        for (&province, state) in &self.provinces {
            if world.entity(province)?.kind != EntityKind::Province
                || state.area.as_ref().is_some_and(|a| !self.areas.contains(a))
                || state
                    .raw_autonomy_micros
                    .is_some_and(|v| !(0..=ONE).contains(&v))
                || state.unresolved.iter().any(String::is_empty)
                || (!state.cores_known && !state.cores.is_empty())
            {
                return Err(Error::Invalid("invalid province administration".into()));
            }
            for &country in state.cores.keys() {
                if world.entity(country)?.kind != EntityKind::Country {
                    return Err(Error::Invalid(
                        "core references a non-country entity".into(),
                    ));
                }
            }
            self.evaluate(world, province)?;
        }
        Ok(())
    }
}
