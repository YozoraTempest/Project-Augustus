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
        let selected = self
            .estates
            .countries
            .get_mut(&entity)
            .and_then(|s| s.get_mut(estate))
            .filter(|s| s.active == Some(true))
            .and_then(|s| s.privileges.as_mut())
            .ok_or_else(|| Error::Invalid("estate privilege selection is unknown".into()))?;
        let changed = if granted {
            selected.insert(privilege.into())
        } else {
            selected.remove(privilege)
        };
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
