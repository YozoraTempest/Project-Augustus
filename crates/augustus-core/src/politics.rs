// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{EntityId, EntityKind, Error, World, WorldDate};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RulerSkill {
    Administration,
    Diplomacy,
    Military,
}

impl RulerSkill {
    pub(crate) fn field(self, person: &PersonState) -> Option<u8> {
        match self {
            Self::Administration => person.administration,
            Self::Diplomacy => person.diplomacy,
            Self::Military => person.military,
        }
    }

    fn field_mut(self, person: &mut PersonState) -> &mut Option<u8> {
        match self {
            Self::Administration => &mut person.administration,
            Self::Diplomacy => &mut person.diplomacy,
            Self::Military => &mut person.military,
        }
    }
}

/// An explicit choice. Absence of this record means the choice is unknown.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NationalFocus {
    pub skill: Option<RulerSkill>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulerSkillBounds {
    pub minimum: u8,
    pub maximum: u8,
}

impl RulerSkillBounds {
    pub(crate) fn validate(self) -> Result<(), Error> {
        if self.minimum > self.maximum {
            return Err(Error::Invalid("invalid ruler skill bounds".into()));
        }
        Ok(())
    }
}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub national_focus: Option<NationalFocus>,
    /// Explicit political facts. Missing entries and None both mean unknown.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub statuses: BTreeMap<String, Option<bool>>,
    /// Known preferred holders, keyed by political office.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub office_preferences: BTreeMap<String, EntityId>,
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
    /// Bounds for skill changes. Absence leaves the rule unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ruler_skill_bounds: Option<RulerSkillBounds>,
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
        self.ruler_skill_bounds.is_none()
            && self.countries.is_empty()
            && self.relations.is_empty()
            && self.offices.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        if let Some(bounds) = self.ruler_skill_bounds {
            bounds.validate()?;
        }
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
            if state.office_preferences.keys().any(String::is_empty) {
                return Err(Error::Invalid("empty political office preference".into()));
            }
            for &preferred_holder in state.office_preferences.values() {
                country(preferred_holder)?;
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

    pub(crate) fn set_office_holder(
        &mut self,
        office: &str,
        holder: EntityId,
    ) -> Result<(), Error> {
        if office.is_empty() {
            return Err(Error::Invalid("empty political office".into()));
        }
        if self.entity(holder)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "political office holder must be a country".into(),
            ));
        }
        self.politics.offices.insert(office.to_owned(), holder);
        Ok(())
    }

    pub fn national_focus(&self, entity: EntityId) -> Result<NationalFocus, Error> {
        if self.entity(entity)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "national focus requires country scope".into(),
            ));
        }
        self.politics
            .countries
            .get(&entity)
            .and_then(|country| country.national_focus)
            .ok_or_else(|| Error::Invalid("national focus is unknown".into()))
    }

    pub fn change_national_focus(
        &mut self,
        entity: EntityId,
        focus: NationalFocus,
    ) -> Result<(), Error> {
        self.require_running()?;
        if self.entity(entity)?.kind != EntityKind::Country {
            return Err(Error::Invalid(
                "national focus requires country scope".into(),
            ));
        }
        self.politics
            .countries
            .entry(entity)
            .or_default()
            .national_focus = Some(focus);
        Ok(())
    }

    pub fn ruler_skill(&self, entity: EntityId, skill: RulerSkill) -> Result<u8, Error> {
        if self.entity(entity)?.kind != EntityKind::Country {
            return Err(Error::Invalid("ruler skill requires country scope".into()));
        }
        self.politics
            .countries
            .get(&entity)
            .and_then(|country| country.ruler.as_ref())
            .and_then(|person| skill.field(person))
            .ok_or_else(|| Error::Invalid(format!("ruler skill {skill:?} is unknown")))
    }

    /// Content supplies the bounds; only the current ruler's known skill changes.
    pub fn change_ruler_skill(
        &mut self,
        entity: EntityId,
        skill: RulerSkill,
        delta: i64,
    ) -> Result<(), Error> {
        self.require_running()?;
        let bounds = self
            .politics
            .ruler_skill_bounds
            .ok_or_else(|| Error::Invalid("ruler skill bounds are unknown".into()))?;
        bounds.validate()?;
        let previous = self.ruler_skill(entity, skill)?;
        let value = (i128::from(previous) + i128::from(delta))
            .clamp(i128::from(bounds.minimum), i128::from(bounds.maximum))
            as u8;
        let ruler = self
            .politics
            .countries
            .get_mut(&entity)
            .and_then(|country| country.ruler.as_mut())
            .expect("validated current ruler");
        *skill.field_mut(ruler) = Some(value);
        Ok(())
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
        // Replacing relationships does not establish the subject system's count.
        // Unrelated political updates retain explicit observations.
        let subjects = |state: &Politics| {
            state
                .relations
                .iter()
                .filter(|relation| relation.kind == DiplomaticKind::Subject)
                .cloned()
                .collect::<BTreeSet<_>>()
        };
        let previous = subjects(&self.politics);
        let current = subjects(&politics);
        for relation in previous.symmetric_difference(&current) {
            self.subjects.counts.remove(&relation.first);
        }
        let mut index = crate::subjects::SubjectIndex::from_politics(&politics);
        index.apply_order(&self.subject_country_order)?;
        self.subject_index = index;
        self.politics = politics;
        Ok(())
    }
}
