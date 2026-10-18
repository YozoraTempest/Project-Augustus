// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{EntityId, EntityKind, Error, World};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EstateMetric {
    Loyalty,
    LandShare,
}

impl World {
    pub(crate) fn estate_has_privilege(
        &self,
        country: EntityId,
        privilege: &str,
    ) -> Result<bool, Error> {
        if self.entity(country)?.kind != EntityKind::Country || privilege.is_empty() {
            return Err(Error::Invalid(
                "estate privilege query requires a country and identifier".into(),
            ));
        }
        let candidates: Vec<_> = self
            .estates
            .definitions
            .iter()
            .filter(|(_, definition)| definition.privileges.contains(privilege))
            .map(|(estate, _)| estate)
            .collect();
        if candidates.is_empty() {
            return Ok(false);
        }
        let states = self
            .estates
            .countries
            .get(&country)
            .ok_or_else(|| Error::Invalid("unknown estate state".into()))?;
        let mut unknown = false;
        for estate in candidates {
            match states.get(estate) {
                Some(state) if state.active == Some(false) => (),
                Some(state) if state.active == Some(true) => match &state.privileges {
                    Some(privileges) if privileges.contains(privilege) => return Ok(true),
                    Some(_) => (),
                    None => unknown = true,
                },
                _ => unknown = true,
            }
        }
        if unknown {
            Err(Error::Invalid("estate privilege state is unknown".into()))
        } else {
            Ok(false)
        }
    }

    pub(crate) fn estate_loyalty_at_least(
        &self,
        country: EntityId,
        estate: &str,
        loyalty_milli: i64,
    ) -> Result<bool, Error> {
        self.estate_metric_at_least(country, estate, "loyalty", loyalty_milli)
    }

    pub(crate) fn estate_influence_at_least(
        &self,
        country: EntityId,
        estate: &str,
        influence_milli: i64,
    ) -> Result<bool, Error> {
        self.estate_metric_at_least(country, estate, "influence", influence_milli)
    }

    fn estate_metric_at_least(
        &self,
        country: EntityId,
        estate: &str,
        metric: &str,
        threshold_milli: i64,
    ) -> Result<bool, Error> {
        if self.entity(country)?.kind != EntityKind::Country || estate.is_empty() {
            return Err(Error::Invalid(
                "estate metric query requires a country and identifier".into(),
            ));
        }
        if !self.estates.definitions.contains_key(estate) {
            return Err(Error::Invalid(format!("unknown estate {estate}")));
        }
        let state = self
            .estates
            .countries
            .get(&country)
            .and_then(|states| states.get(estate))
            .ok_or_else(|| Error::Invalid(format!("unknown estate state for {estate}")))?;
        if state.active == Some(false) {
            return Ok(false);
        }
        if state.active != Some(true) {
            return Err(Error::Invalid(format!(
                "estate presence is unknown for {estate}"
            )));
        }
        let value = match metric {
            "loyalty" => state.loyalty_milli,
            "influence" => state.influence_milli,
            _ => unreachable!("estate metric is validated by its caller"),
        }
        .ok_or_else(|| Error::Invalid(format!("estate {metric} is unknown for {estate}")))?;
        Ok(value >= threshold_milli)
    }

    pub(crate) fn change_estate_metric(
        &mut self,
        entity: EntityId,
        estate: &str,
        metric: EstateMetric,
        amount: i64,
        minimum: Option<i64>,
    ) -> Result<(), Error> {
        let maximum = match metric {
            EstateMetric::Loyalty => 100_000,
            EstateMetric::LandShare => 1_000_000,
        };
        if minimum.is_some_and(|v| !(0..=maximum).contains(&v)) {
            return Err(Error::Invalid("invalid estate metric minimum".into()));
        }
        let mut states = self
            .estates
            .countries
            .get(&entity)
            .ok_or_else(|| Error::Invalid("missing country estates".into()))?
            .clone();
        let state = states
            .get_mut(estate)
            .filter(|s| s.active == Some(true))
            .ok_or_else(|| Error::Invalid("estate metric requires an active estate".into()))?;
        let slot = match metric {
            EstateMetric::Loyalty => &mut state.loyalty_milli,
            EstateMetric::LandShare => &mut state.land_share_micros,
        };
        if let Some(value) = *slot {
            let value = value.checked_add(amount).ok_or(Error::Overflow)?;
            let value = minimum.map_or(value, |minimum| value.max(minimum));
            if !(0..=maximum).contains(&value) {
                return Err(Error::Invalid(
                    "estate metric outside supported range".into(),
                ));
            }
            *slot = Some(value);
        }
        if states
            .values()
            .filter_map(|s| s.land_share_micros)
            .sum::<i64>()
            > 1_000_000
        {
            return Err(Error::Invalid("estate land exceeds total land".into()));
        }
        self.estates.countries.insert(entity, states);
        Ok(())
    }

    pub(crate) fn set_estate_privilege(
        &mut self,
        entity: EntityId,
        estate: &str,
        privilege: &str,
        granted: bool,
    ) -> Result<(), Error> {
        if !self
            .estates
            .definitions
            .get(estate)
            .is_some_and(|d| d.privileges.contains(privilege))
            || !self.estates.privileges.contains_key(privilege)
        {
            return Err(Error::Invalid("invalid estate privilege binding".into()));
        }
        let influence = self
            .estates
            .privileges
            .get(privilege)
            .and_then(|definition| definition.influence_milli)
            .unwrap_or(0);
        let mut states = self
            .estates
            .countries
            .get(&entity)
            .cloned()
            .ok_or_else(|| Error::Invalid("estate privilege selection is unknown".into()))?;
        let state = states
            .get_mut(estate)
            .filter(|state| state.active == Some(true))
            .ok_or_else(|| Error::Invalid("estate privilege selection is unknown".into()))?;
        let selected = state
            .privileges
            .as_mut()
            .ok_or_else(|| Error::Invalid("estate privilege selection is unknown".into()))?;
        let changed = if granted {
            selected.insert(privilege.into())
        } else {
            selected.remove(privilege)
        };
        if changed && let Some(current) = state.influence_milli {
            let delta = if granted {
                influence
            } else {
                influence.checked_neg().ok_or(Error::Overflow)?
            };
            let next = current.checked_add(delta).ok_or(Error::Overflow)?;
            if next < 0 {
                return Err(Error::Invalid(
                    "estate influence outside supported range".into(),
                ));
            }
            state.influence_milli = Some(next);
        }
        self.estates.countries.insert(entity, states);
        if changed
            && let Some(privileges) = self.estates.resolved_privilege_effects.get_mut(&entity)
        {
            privileges.remove(privilege);
            if privileges.is_empty() {
                self.estates.resolved_privilege_effects.remove(&entity);
            }
        }
        Ok(())
    }

    pub(crate) fn resolve_estate_privilege_effect(
        &mut self,
        entity: EntityId,
        privilege: &str,
        effect: &str,
    ) -> Result<(), Error> {
        if self.entity(entity)?.kind != EntityKind::Country
            || !self.estates.countries.get(&entity).is_some_and(|states| {
                states.values().any(|s| {
                    s.active == Some(true)
                        && s.privileges
                            .as_ref()
                            .is_some_and(|ids| ids.contains(privilege))
                })
            })
            || !self
                .estates
                .privileges
                .get(privilege)
                .is_some_and(|p| p.effect.unresolved_effects.contains(effect))
        {
            return Err(Error::Invalid("unknown estate privilege effect".into()));
        }
        self.estates
            .resolved_privilege_effects
            .entry(entity)
            .or_default()
            .entry(privilege.into())
            .or_default()
            .insert(effect.into());
        Ok(())
    }
}
