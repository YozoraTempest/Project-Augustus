// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

const ONE: i64 = 1_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GeographicLevel {
    Area,
    Region,
    Superregion,
    Continent,
}

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MonthlyAutonomyPreview {
    pub raw_autonomy_micros: Option<i64>,
    pub monthly_delta_micros: Option<i64>,
    pub minimum_autonomy_micros: Option<i64>,
    pub maximum_autonomy_micros: Option<i64>,
    pub next_raw_autonomy_micros: Option<i64>,
    pub changed: bool,
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub regions: BTreeMap<String, BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub superregions: BTreeMap<String, BTreeSet<String>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub continents: BTreeMap<String, BTreeSet<EntityId>>,
    pub country_areas: BTreeMap<EntityId, BTreeMap<String, AreaStatus>>,
    pub provinces: BTreeMap<EntityId, ProvinceAdministration>,
    pub rules: AdministrationRules,
}

impl Administration {
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
            && self.regions.is_empty()
            && self.superregions.is_empty()
            && self.continents.is_empty()
            && self.country_areas.is_empty()
            && self.provinces.is_empty()
            && self.rules == AdministrationRules::default()
    }

    pub fn in_region(&self, province: EntityId, name: &str) -> Option<bool> {
        let area = self.provinces.get(&province)?.area.as_ref()?;
        Some(self.regions.get(name)?.contains(area))
    }

    pub fn geographic_member(
        &self,
        province: EntityId,
        level: GeographicLevel,
        name: &str,
    ) -> Option<bool> {
        match level {
            GeographicLevel::Area => {
                if !self.areas.contains(name) {
                    return None;
                }
                Some(self.provinces.get(&province)?.area.as_ref()? == name)
            }
            GeographicLevel::Region => self.in_region(province, name),
            GeographicLevel::Superregion => self.in_superregion(province, name),
            GeographicLevel::Continent => Some(self.continents.get(name)?.contains(&province)),
        }
    }

    pub fn in_superregion(&self, province: EntityId, name: &str) -> Option<bool> {
        let area = self.provinces.get(&province)?.area.as_ref()?;
        let regions = self.superregions.get(name)?;
        let mut inside = false;
        for region in regions {
            inside |= self.regions.get(region)?.contains(area);
        }
        Some(inside)
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

    pub(crate) fn autonomy_bounds(
        &self,
        world: &World,
        province: EntityId,
    ) -> Result<Option<(i64, i64)>, Error> {
        let evaluation = self.evaluate(world, province)?;
        let Some(minimum) = evaluation.minimum_autonomy_micros else {
            return Ok(None);
        };
        let maximum = if evaluation.is_capital {
            let Some(maximum) = self.rules.capital_maximum_micros else {
                return Ok(None);
            };
            maximum
        } else {
            ONE
        };
        if minimum > maximum {
            return Err(Error::Invalid("autonomy bounds are inverted".into()));
        }
        Ok(Some((minimum, maximum)))
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        fn hierarchy(
            groups: &BTreeMap<String, BTreeSet<String>>,
            members: &BTreeSet<String>,
        ) -> Result<(), Error> {
            let mut seen = BTreeSet::new();
            for (name, entries) in groups {
                if name.is_empty()
                    || entries
                        .iter()
                        .any(|id| !members.contains(id) || !seen.insert(id))
                {
                    return Err(Error::Invalid(
                        "invalid or overlapping geographic group".into(),
                    ));
                }
            }
            Ok(())
        }
        hierarchy(&self.regions, &self.areas)?;
        hierarchy(&self.superregions, &self.regions.keys().cloned().collect())?;
        let mut continental_members = BTreeSet::new();
        for (name, provinces) in &self.continents {
            if name.trim().is_empty() {
                return Err(Error::Invalid("empty continent identifier".into()));
            }
            for &province in provinces {
                if world.entity(province)?.kind != EntityKind::Province
                    || !continental_members.insert(province)
                {
                    return Err(Error::Invalid(
                        "invalid or overlapping continent membership".into(),
                    ));
                }
            }
        }
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

impl World {
    /// Previews the shared monthly province-autonomy phase without replacing
    /// administration state. The calculation follows the same attribute,
    /// bounds, missing-input, and clamp order as `update_monthly_autonomy`.
    pub fn preview_monthly_autonomy(
        &self,
        province: EntityId,
    ) -> Result<MonthlyAutonomyPreview, Error> {
        if self.entity(province)?.kind != EntityKind::Province {
            return Err(Error::Invalid(
                "monthly autonomy preview requires a province".into(),
            ));
        }
        let state = self
            .administration()
            .provinces
            .get(&province)
            .ok_or_else(|| Error::Invalid("missing province administration".into()))?;
        let raw = state.raw_autonomy_micros;
        let mut unresolved = BTreeSet::new();
        if self.entity(province)?.owner.is_none() {
            unresolved.insert("monthly_autonomy:owner".into());
            return Ok(MonthlyAutonomyPreview {
                raw_autonomy_micros: raw,
                monthly_delta_micros: None,
                minimum_autonomy_micros: None,
                maximum_autonomy_micros: None,
                next_raw_autonomy_micros: None,
                changed: false,
                unresolved,
            });
        }
        let attribute = crate::MONTHLY_AUTONOMY_DELTA_ATTRIBUTE;
        let has_any_formula = self
            .attributes()
            .formulas
            .values()
            .any(|formulas| formulas.contains_key(attribute));
        if !has_any_formula
            || !self
                .attributes()
                .formulas
                .get(&province)
                .is_some_and(|formulas| formulas.contains_key(attribute))
        {
            unresolved.insert("monthly_autonomy:no_formula".into());
            return Ok(MonthlyAutonomyPreview {
                raw_autonomy_micros: raw,
                monthly_delta_micros: None,
                minimum_autonomy_micros: None,
                maximum_autonomy_micros: None,
                next_raw_autonomy_micros: None,
                changed: false,
                unresolved,
            });
        }

        let autonomy = self.attributes().evaluate(self, province, attribute)?;
        let monthly_delta_micros = autonomy.value_micros;
        if monthly_delta_micros.is_none() {
            let detail = autonomy
                .unresolved
                .into_iter()
                .collect::<Vec<_>>()
                .join(",");
            unresolved.insert(format!("monthly_autonomy:{detail}"));
        }
        let bounds = self.administration().autonomy_bounds(self, province)?;
        if bounds.is_none() {
            unresolved.insert("monthly_autonomy:autonomy_bounds".into());
        }
        if raw.is_none() {
            unresolved.insert("monthly_autonomy:initial_autonomy".into());
        }
        let (minimum_autonomy_micros, maximum_autonomy_micros) = bounds
            .map_or((None, None), |(minimum, maximum)| {
                (Some(minimum), Some(maximum))
            });
        let next_raw_autonomy_micros = match (raw, monthly_delta_micros, bounds) {
            (Some(raw), Some(delta), Some((minimum, maximum))) => Some(
                i64::try_from(
                    (i128::from(raw) + i128::from(delta))
                        .clamp(i128::from(minimum), i128::from(maximum)),
                )
                .map_err(|_| Error::Overflow)?,
            ),
            _ => None,
        };
        let changed = next_raw_autonomy_micros.is_some_and(|next| Some(next) != raw);
        Ok(MonthlyAutonomyPreview {
            raw_autonomy_micros: raw,
            monthly_delta_micros,
            minimum_autonomy_micros,
            maximum_autonomy_micros,
            next_raw_autonomy_micros,
            changed,
            unresolved,
        })
    }

    /// Applies the shared monthly province-autonomy phase and reports whether
    /// any administration state was replaced. Custom monthly systems can use
    /// this before their own tax or country work.
    pub fn update_monthly_autonomy(&mut self) -> Result<bool, Error> {
        self.require_running()?;
        let Some(administration) = self.prepare_monthly_autonomy()? else {
            return Ok(false);
        };
        self.replace_administration(administration)?;
        Ok(true)
    }

    pub(crate) fn prepare_monthly_autonomy(&self) -> Result<Option<Administration>, Error> {
        let attribute = crate::MONTHLY_AUTONOMY_DELTA_ATTRIBUTE;
        if !self
            .attributes()
            .formulas
            .values()
            .any(|formulas| formulas.contains_key(attribute))
        {
            return Ok(None);
        }

        let mut updated = self.administration().clone();
        let mut changed = false;
        for (index, entity) in self.entities().iter().enumerate() {
            if entity.kind != EntityKind::Province || entity.owner.is_none() {
                continue;
            }
            let province = EntityId(index);
            if !self
                .attributes()
                .formulas
                .get(&province)
                .is_some_and(|formulas| formulas.contains_key(attribute))
            {
                continue;
            }

            let autonomy = self.attributes().evaluate(self, province, attribute)?;
            let bounds = self.administration().autonomy_bounds(self, province)?;
            let state = updated
                .provinces
                .get_mut(&province)
                .ok_or_else(|| Error::Invalid("missing province administration".into()))?;
            let unresolved_prefix = "monthly_autonomy:";
            let previous_unresolved = state.unresolved.clone();
            state
                .unresolved
                .retain(|reason| !reason.starts_with(unresolved_prefix));

            let Some(delta) = autonomy.value_micros else {
                let detail = autonomy
                    .unresolved
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(",");
                state
                    .unresolved
                    .insert(format!("{unresolved_prefix}{detail}"));
                changed |= state.unresolved != previous_unresolved;
                continue;
            };
            let Some((minimum, maximum)) = bounds else {
                state
                    .unresolved
                    .insert(format!("{unresolved_prefix}autonomy_bounds"));
                changed |= state.unresolved != previous_unresolved;
                continue;
            };
            let Some(raw) = state.raw_autonomy_micros else {
                state
                    .unresolved
                    .insert(format!("{unresolved_prefix}initial_autonomy"));
                changed |= state.unresolved != previous_unresolved;
                continue;
            };

            let next = i64::try_from(
                (i128::from(raw) + i128::from(delta))
                    .clamp(i128::from(minimum), i128::from(maximum)),
            )
            .map_err(|_| Error::Overflow)?;
            changed |= next != raw || state.unresolved != previous_unresolved;
            state.raw_autonomy_micros = Some(next);
        }
        Ok(changed.then_some(updated))
    }
}
