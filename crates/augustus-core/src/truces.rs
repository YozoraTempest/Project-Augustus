// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, GrantTerm, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Truce {
    pub expires_on: Option<u64>,
    pub monthly: bool,
}

/// Each symmetric pair has one authoritative record. A null record confirms
/// absence; an omitted pair is unknown unless one endpoint has complete coverage.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Truces {
    pub known_countries: BTreeSet<EntityId>,
    pub records: BTreeMap<EntityId, BTreeMap<EntityId, Option<Truce>>>,
}

pub(crate) type TruceExpirations = BTreeMap<u64, BTreeSet<(EntityId, EntityId)>>;

fn pair(first: EntityId, second: EntityId) -> (EntityId, EntityId) {
    if first <= second {
        (first, second)
    } else {
        (second, first)
    }
}

impl Truce {
    fn retirement_day(self, world: &World) -> Result<Option<u64>, Error> {
        self.expires_on
            .map(|deadline| {
                if self.monthly {
                    world
                        .calendar()
                        .ok_or_else(|| Error::Invalid("monthly truce requires a calendar".into()))?
                        .next_month(deadline)
                } else {
                    Ok(deadline)
                }
            })
            .transpose()
    }
}

impl Truces {
    pub fn is_empty(&self) -> bool {
        self.known_countries.is_empty() && self.records.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<TruceExpirations, Error> {
        for &country in &self.known_countries {
            world.truce_country(country)?;
        }
        let mut index = TruceExpirations::new();
        for (&first, records) in &self.records {
            world.truce_country(first)?;
            if records.is_empty() {
                return Err(Error::Invalid("empty truce records".into()));
            }
            for (&second, record) in records {
                world.truce_country(second)?;
                if first > second {
                    return Err(Error::Invalid("noncanonical truce pair".into()));
                }
                if let Some(record) = record {
                    if record.monthly && record.expires_on.is_none() {
                        return Err(Error::Invalid("monthly truce has no deadline".into()));
                    }
                    if let Some(day) = record.retirement_day(world)? {
                        if day <= world.day() {
                            return Err(Error::Invalid("expired saved truce".into()));
                        }
                        if let Some(calendar) = world.calendar() {
                            calendar.date(day)?;
                        }
                        index.entry(day).or_default().insert((first, second));
                    }
                }
            }
        }
        Ok(index)
    }
}

impl World {
    pub fn truces(&self) -> &Truces {
        &self.truces
    }

    fn truce_country(&self, country: EntityId) -> Result<(), Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("truce requires country endpoints".into()));
        }
        Ok(())
    }

    pub fn truce_with(&self, first: EntityId, second: EntityId) -> Result<bool, Error> {
        self.truce_country(first)?;
        self.truce_country(second)?;
        let (first, second) = pair(first, second);
        if let Some(record) = self.truces.records.get(&first).and_then(|r| r.get(&second)) {
            return Ok(record.is_some());
        }
        if self.truces.known_countries.contains(&first)
            || self.truces.known_countries.contains(&second)
        {
            return Ok(false);
        }
        Err(Error::Invalid("truce membership is unknown".into()))
    }

    pub fn has_truce(&self, country: EntityId) -> Result<bool, Error> {
        self.truce_country(country)?;
        if self.truces.records.iter().any(|(&first, records)| {
            records.iter().any(|(&second, record)| {
                record.is_some() && (first == country || second == country)
            })
        }) {
            return Ok(true);
        }
        if self.truces.known_countries.contains(&country) {
            return Ok(false);
        }
        Err(Error::Invalid(
            "complete truce membership is unknown".into(),
        ))
    }

    pub fn set_truces(&mut self, state: Truces) -> Result<(), Error> {
        self.require_running()?;
        let index = state.validate(self)?;
        self.truces = state;
        self.truce_expirations = index;
        Ok(())
    }

    /// Replace this pair's explicit grant and deadline. Self pairs are permitted.
    pub fn grant_truce(
        &mut self,
        first: EntityId,
        second: EntityId,
        term: GrantTerm,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.truce_country(first)?;
        self.truce_country(second)?;
        term.validate()?;
        let (expires_on, monthly) = match term {
            GrantTerm::Permanent => (None, false),
            GrantTerm::Days(days) => (
                Some(self.day().checked_add(days).ok_or(Error::Overflow)?),
                false,
            ),
            GrantTerm::Months { count, monthly } => (
                Some(
                    self.calendar()
                        .ok_or_else(|| Error::Invalid("truce term requires a calendar".into()))?
                        .after_months(self.day(), count)?,
                ),
                monthly,
            ),
        };
        let record = Truce {
            expires_on,
            monthly,
        };
        let retirement = record.retirement_day(self)?;
        if let (Some(calendar), Some(day)) = (self.calendar(), retirement) {
            calendar.date(day)?;
        }
        self.write_truce(first, second, Some(record), retirement);
        Ok(())
    }

    pub fn clear_truce(&mut self, first: EntityId, second: EntityId) -> Result<(), Error> {
        self.require_running()?;
        self.truce_country(first)?;
        self.truce_country(second)?;
        self.write_truce(first, second, None, None);
        Ok(())
    }

    fn write_truce(
        &mut self,
        first: EntityId,
        second: EntityId,
        record: Option<Truce>,
        retirement: Option<u64>,
    ) {
        let (first, second) = pair(first, second);
        if let Some(previous) = self
            .truces
            .records
            .get(&first)
            .and_then(|r| r.get(&second))
            .copied()
            .flatten()
            && let Some(day) = previous
                .retirement_day(self)
                .expect("validated truce deadline")
            && let Some(entries) = self.truce_expirations.get_mut(&day)
        {
            entries.remove(&(first, second));
            if entries.is_empty() {
                self.truce_expirations.remove(&day);
            }
        }
        self.truces
            .records
            .entry(first)
            .or_default()
            .insert(second, record);
        if let Some(day) = retirement {
            self.truce_expirations
                .entry(day)
                .or_default()
                .insert((first, second));
        }
    }

    pub(crate) fn retire_truces(&mut self, day: u64, monthly: bool) {
        let due: Vec<_> = self
            .truce_expirations
            .range(..=day)
            .map(|(&d, _)| d)
            .collect();
        for deadline in due {
            let pairs = self
                .truce_expirations
                .remove(&deadline)
                .expect("indexed truce retirement");
            for (first, second) in pairs {
                let record = self
                    .truces
                    .records
                    .get_mut(&first)
                    .expect("indexed truce pair")
                    .get_mut(&second)
                    .expect("indexed truce record");
                if record.expect("active indexed truce").monthly == monthly {
                    *record = None;
                } else {
                    self.truce_expirations
                        .entry(deadline)
                        .or_default()
                        .insert((first, second));
                }
            }
        }
    }
}
