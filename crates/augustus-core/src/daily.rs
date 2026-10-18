// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

/// A replaceable daily simulation phase.
///
/// The provider owns any state that is specific to its ruleset and uses the
/// shared [`World`] as the only mutable game state. The selected provider id is
/// saved with the session; callers must supply the same provider again when a
/// saved world is advanced.
pub trait DailySystem {
    fn id(&self) -> &str;
    fn validate(&self, world: &World) -> Result<(), Error>;
    fn tick(&self, world: &mut World) -> Result<(), Error>;
}

/// A content-defined event candidate used by a daily event group.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyEventCandidate {
    /// The event to trigger. `None` is an intentional empty weighted entry:
    /// it consumes its weight without dispatching an event.
    pub event: Option<String>,
    pub weight: i64,
}

/// The two candidate traversal modes observed by the compatibility layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DailyEventGroupMode {
    /// Freeze all eligible candidates in declaration order, then dispatch them.
    All,
    /// Select at most one eligible candidate using its nonnegative weight.
    WeightedOne,
}

/// An ordered group of country-scoped event candidates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyEventGroup {
    pub mode: DailyEventGroupMode,
    pub candidates: Vec<DailyEventCandidate>,
}

impl DailyEventGroup {
    pub fn all(events: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            mode: DailyEventGroupMode::All,
            candidates: events
                .into_iter()
                .map(|event| DailyEventCandidate {
                    event: Some(event.into()),
                    weight: 0,
                })
                .collect(),
        }
    }

    pub fn weighted(candidates: Vec<DailyEventCandidate>) -> Self {
        Self {
            mode: DailyEventGroupMode::WeightedOne,
            candidates,
        }
    }
}

/// Ordered daily event groups for one country.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyEventCountry {
    pub country: EntityId,
    pub groups: Vec<DailyEventGroup>,
}

/// A generic daily event provider for translated content.
///
/// The provider deliberately stores only semantic event candidates. EU4
/// compatibility code is responsible for translating on-actions, script
/// triggers and threshold inputs into this structure. Its `id` must change
/// whenever the candidate rules or their event content changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DailyEventRules {
    pub id: String,
    pub countries: Vec<DailyEventCountry>,
}

impl DailyEventRules {
    pub fn new(id: impl Into<String>, countries: Vec<DailyEventCountry>) -> Result<Self, Error> {
        let rules = Self {
            id: id.into(),
            countries,
        };
        if rules.id.trim().is_empty() {
            return Err(Error::Invalid("daily event rules require an id".into()));
        }
        Ok(rules)
    }

    pub(crate) fn validate_for_schedule(&self, world: &World) -> Result<(), Error> {
        let mut countries = BTreeSet::new();
        for entry in &self.countries {
            if !countries.insert(entry.country) {
                return Err(Error::Invalid("duplicate daily event country".into()));
            }
            if world.entity(entry.country)?.kind != EntityKind::Country {
                return Err(Error::Invalid(
                    "daily event rules require country entities".into(),
                ));
            }
            for group in &entry.groups {
                for candidate in &group.candidates {
                    if group.mode == DailyEventGroupMode::WeightedOne && candidate.weight < 0 {
                        return Err(Error::Invalid(
                            "daily event weights cannot be negative".into(),
                        ));
                    }
                    let Some(event) = candidate.event.as_deref() else {
                        if group.mode == DailyEventGroupMode::All {
                            return Err(Error::Invalid(
                                "empty daily event candidates require weighted groups".into(),
                            ));
                        }
                        continue;
                    };
                    if event.trim().is_empty() {
                        return Err(Error::Invalid("empty daily event candidate".into()));
                    }
                    let definition = world.event_catalog().get(event)?;
                    if definition.scope != EntityKind::Country {
                        return Err(Error::Invalid(
                            "daily event candidates require country events".into(),
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn dispatch_group_for_schedule(
        &self,
        world: &mut World,
        country: EntityId,
        group: &DailyEventGroup,
        budget: &mut usize,
    ) -> Result<(), Error> {
        match group.mode {
            DailyEventGroupMode::All => {
                // Eligibility is frozen before any immediate effect runs.
                let mut eligible = Vec::new();
                for candidate in &group.candidates {
                    let Some(event) = candidate.event.as_deref() else {
                        return Err(Error::Invalid(
                            "empty daily event candidates require weighted groups".into(),
                        ));
                    };
                    if world.event_is_eligible(event, country)? {
                        eligible.push(event.to_owned());
                    }
                }
                for event in eligible {
                    world.trigger_event_for_daily_dispatch(&event, country, budget)?;
                }
            }
            DailyEventGroupMode::WeightedOne => {
                let mut eligible = Vec::new();
                let mut total = 0_i64;
                for candidate in &group.candidates {
                    if candidate.weight == 0 {
                        continue;
                    }
                    let Some(event) = candidate.event.as_deref() else {
                        total = total.checked_add(candidate.weight).ok_or(Error::Overflow)?;
                        eligible.push((None, candidate.weight));
                        continue;
                    };
                    if world.event_is_eligible(event, country)? {
                        total = total.checked_add(candidate.weight).ok_or(Error::Overflow)?;
                        eligible.push((Some(event), candidate.weight));
                    }
                }
                if total == 0 {
                    return Ok(());
                }
                let mut draw = world.random_source().random_range(0..total);
                for (event, weight) in eligible {
                    if draw < weight {
                        if let Some(event) = event {
                            world.trigger_event_for_daily_dispatch(event, country, budget)?;
                        }
                        break;
                    }
                    draw -= weight;
                }
            }
        }
        Ok(())
    }
}

impl DailySystem for DailyEventRules {
    fn id(&self) -> &str {
        &self.id
    }

    fn validate(&self, world: &World) -> Result<(), Error> {
        self.validate_for_schedule(world)
    }

    fn tick(&self, world: &mut World) -> Result<(), Error> {
        let mut budget = 100_000;
        for entry in &self.countries {
            for group in &entry.groups {
                self.dispatch_group_for_schedule(world, entry.country, group, &mut budget)?;
            }
        }
        Ok(())
    }
}
