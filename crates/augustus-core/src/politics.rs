// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{EntityId, EntityKind, Error, World, WorldDate};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonState {
    pub name: Option<String>,
    pub regnal_name: Option<String>,
    pub dynasty: Option<String>,
    pub birth_date: Option<WorldDate>,
    pub historical_death_date: Option<WorldDate>,
    pub administration: Option<u8>,
    pub diplomacy: Option<u8>,
    pub military: Option<u8>,
    pub claim_milli: Option<i64>,
    pub female: Option<bool>,
    pub regent: Option<bool>,
    pub block_disinherit: Option<bool>,
    pub culture: Option<String>,
    pub religion: Option<String>,
    pub country_of_origin: Option<EntityId>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountryPolitics {
    /// Explicit political facts. Missing entries and None both mean unknown.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub statuses: BTreeMap<String, Option<bool>>,
    pub ruler: Option<PersonState>,
    pub heir: Option<PersonState>,
    pub consort: Option<PersonState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiplomaticKind {
    Alliance,
    RoyalMarriage,
    Guarantee,
    Warning,
    TradeLeague,
    Subject,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DiplomaticRelation {
    pub kind: DiplomaticKind,
    pub first: EntityId,
    pub second: EntityId,
    pub subject_type: Option<String>,
}

impl DiplomaticRelation {
    pub fn canonicalize(&mut self) {
        if matches!(
            self.kind,
            DiplomaticKind::Alliance | DiplomaticKind::RoyalMarriage
        ) && self.second < self.first
        {
            std::mem::swap(&mut self.first, &mut self.second);
        }
    }
}

/// Current authoritative political state. Historical future dates do not schedule changes.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Politics {
    pub countries: BTreeMap<EntityId, CountryPolitics>,
    pub relations: Vec<DiplomaticRelation>,
    pub offices: BTreeMap<String, EntityId>,
}

impl Politics {
    pub fn status(&self, entity: EntityId, name: &str) -> Option<bool> {
        self.countries
            .get(&entity)?
            .statuses
            .get(name)
            .copied()
            .flatten()
    }
    pub fn is_empty(&self) -> bool {
        self.countries.is_empty() && self.relations.is_empty() && self.offices.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let country = |id| -> Result<(), Error> {
            if world.entity(id)?.kind != EntityKind::Country {
                return Err(Error::Invalid("politics requires a country entity".into()));
            }
            Ok(())
        };
        for (&id, state) in &self.countries {
            country(id)?;
            if state.statuses.keys().any(String::is_empty) {
                return Err(Error::Invalid("empty political status".into()));
            }
            for person in [&state.ruler, &state.heir, &state.consort]
                .into_iter()
                .flatten()
            {
                if let Some(id) = person.country_of_origin {
                    country(id)?;
                }
                if person.claim_milli.is_some_and(|claim| claim < 0) {
                    return Err(Error::Invalid("negative claim strength".into()));
                }
                for date in [person.birth_date, person.historical_death_date]
                    .into_iter()
                    .flatten()
                {
                    if !(1..=12).contains(&date.month) || !(1..=31).contains(&date.day) {
                        return Err(Error::Invalid("invalid person calendar label".into()));
                    }
                }
                if let (Some(birth), Some(death)) =
                    (person.birth_date, person.historical_death_date)
                    && death < birth
                {
                    return Err(Error::Invalid("person death precedes birth".into()));
                }
            }
        }
        let mut unique = BTreeSet::new();
        if self.relations.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(Error::Invalid("unordered diplomatic relations".into()));
        }
        let mut overlords = BTreeMap::new();
        let mut league_leaders = BTreeMap::new();
        for relation in &self.relations {
            country(relation.first)?;
            country(relation.second)?;
            if relation.first == relation.second || !unique.insert(relation) {
                return Err(Error::Invalid(
                    "self or duplicate diplomatic relation".into(),
                ));
            }
            if matches!(
                relation.kind,
                DiplomaticKind::Alliance | DiplomaticKind::RoyalMarriage
            ) && relation.second < relation.first
            {
                return Err(Error::Invalid("noncanonical symmetric relation".into()));
            }
            if relation.kind == DiplomaticKind::Subject {
                if relation.subject_type.as_ref().is_none_or(|s| s.is_empty()) {
                    return Err(Error::Invalid("subject relation requires a type".into()));
                }
                if overlords.insert(relation.second, relation.first).is_some() {
                    return Err(Error::Invalid(
                        "subject has multiple dependency relations".into(),
                    ));
                }
            } else if relation.subject_type.is_some() {
                return Err(Error::Invalid(
                    "subject type on a non-subject relation".into(),
                ));
            }
            if relation.kind == DiplomaticKind::TradeLeague
                && league_leaders
                    .insert(relation.second, relation.first)
                    .is_some()
            {
                return Err(Error::Invalid(
                    "country belongs to multiple trade leagues".into(),
                ));
            }
        }
        for &subject in overlords.keys() {
            let mut visited = BTreeSet::new();
            let mut current = subject;
            while let Some(&next) = overlords.get(&current) {
                if !visited.insert(current) {
                    return Err(Error::Invalid("cyclic subject hierarchy".into()));
                }
                current = next;
            }
        }
        for (name, &holder) in &self.offices {
            if name.is_empty() {
                return Err(Error::Invalid("empty political office".into()));
            }
            country(holder)?;
        }
        Ok(())
    }
}

impl World {
    pub fn politics(&self) -> &Politics {
        &self.politics
    }

    /// Validates the complete replacement before changing authoritative state.
    pub fn set_politics(&mut self, mut politics: Politics) -> Result<(), Error> {
        self.require_running()?;
        for relation in &mut politics.relations {
            relation.canonicalize();
        }
        politics.relations.sort();
        politics.validate(self)?;
        self.subjects.validate(&politics)?;
        self.subject_index = crate::subjects::SubjectIndex::from_politics(&politics);
        self.politics = politics;
        Ok(())
    }
}
