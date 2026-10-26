// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{Condition, DiplomaticKind, EntityId, EntityKind, Error, Politics, World};

/// Type declarations and completeness of the current graph in Politics::relations.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Subjects {
    pub types: BTreeSet<String>,
    pub known: bool,
    /// Explicit count observations. Missing countries have an unknown count.
    /// Membership and count can differ until the owning system refreshes it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub counts: BTreeMap<EntityId, u32>,
}

impl Subjects {
    pub fn is_empty(&self) -> bool {
        !self.known && self.types.is_empty() && self.counts.is_empty()
    }

    pub(crate) fn identifier(&self, name: &str) -> Result<(), Error> {
        if name.is_empty() || !self.types.contains(name) {
            return Err(Error::Invalid(format!("unregistered subject type {name}")));
        }
        Ok(())
    }

    pub(crate) fn validate(&self, politics: &Politics) -> Result<(), Error> {
        if self.types.contains("") {
            return Err(Error::Invalid("empty subject type declaration".into()));
        }
        if self.known {
            for relation in &politics.relations {
                if relation.kind == DiplomaticKind::Subject {
                    self.identifier(relation.subject_type.as_deref().unwrap_or_default())?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn validate_counts(&self, world: &World) -> Result<(), Error> {
        for &country in self.counts.keys() {
            if world.entity(country)?.kind != EntityKind::Country {
                return Err(Error::Invalid("subject counts require a country".into()));
            }
        }
        Ok(())
    }
}

/// Derived lookup data. Relation records remain the only source of membership and type.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SubjectIndex {
    members: BTreeMap<EntityId, Vec<EntityId>>,
    incoming: BTreeMap<EntityId, usize>,
}

impl SubjectIndex {
    pub(crate) fn apply_order(&mut self, order: &[EntityId]) -> Result<(), Error> {
        if order.is_empty() {
            return Ok(());
        }
        let ranks: BTreeMap<_, _> = order
            .iter()
            .enumerate()
            .map(|(rank, &id)| (id, rank))
            .collect();
        if self
            .members
            .values()
            .flatten()
            .any(|id| !ranks.contains_key(id))
        {
            return Err(Error::Invalid(
                "subject relation references a country outside the declared order".into(),
            ));
        }
        for members in self.members.values_mut() {
            members.sort_by_key(|id| ranks[id]);
        }
        Ok(())
    }
    pub(crate) fn from_politics(politics: &Politics) -> Self {
        let mut result = Self::default();
        for (index, relation) in politics.relations.iter().enumerate() {
            if relation.kind == DiplomaticKind::Subject {
                result
                    .members
                    .entry(relation.first)
                    .or_default()
                    .push(relation.second);
                result.incoming.insert(relation.second, index);
            }
        }
        result
    }
}

impl World {
    /// An optional priority for every country, independent of membership records.
    /// Empty uses entity order. Explicit priorities must list every country once.
    pub fn set_subject_country_order(&mut self, order: Vec<EntityId>) -> Result<(), Error> {
        self.require_running()?;
        let countries: Vec<_> = self
            .entities()
            .iter()
            .enumerate()
            .filter(|(_, entity)| entity.kind == EntityKind::Country)
            .map(|(id, _)| EntityId(id))
            .collect();
        if !order.is_empty()
            && (order.len() != countries.len()
                || order.iter().copied().collect::<BTreeSet<_>>()
                    != countries.iter().copied().collect())
        {
            return Err(Error::Invalid(
                "subject country order must contain every country exactly once".into(),
            ));
        }
        let order = if order == countries {
            Vec::new()
        } else {
            order
        };
        let mut index = SubjectIndex::from_politics(&self.politics);
        index.apply_order(&order)?;
        self.subject_country_order = order;
        self.subject_index = index;
        Ok(())
    }

    pub fn subject_country_order(&self) -> &[EntityId] {
        &self.subject_country_order
    }

    pub fn subjects(&self) -> &Subjects {
        &self.subjects
    }

    pub fn set_subjects(&mut self, subjects: Subjects) -> Result<(), Error> {
        self.require_running()?;
        subjects.validate(&self.politics)?;
        subjects.validate_counts(self)?;
        let previous = std::mem::replace(&mut self.subjects, subjects);
        let validation = self
            .event_catalog
            .validate(self)
            .and_then(|()| self.validate_event_content(&self.event_catalog));
        if let Err(error) = validation {
            self.subjects = previous;
            return Err(error);
        }
        Ok(())
    }

    fn require_subject_country(&self, country: EntityId) -> Result<(), Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("subject query requires a country".into()));
        }
        if !self.subjects.known {
            return Err(Error::Invalid(
                "current subject relations are unknown".into(),
            ));
        }
        Ok(())
    }

    /// Direct subjects only. An incomplete graph cannot establish current membership.
    pub fn subject_countries(&self, country: EntityId) -> Result<&[EntityId], Error> {
        self.require_subject_country(country)?;
        Ok(self
            .subject_index
            .members
            .get(&country)
            .map(Vec::as_slice)
            .unwrap_or_default())
    }

    /// Read an explicit count without deriving it from current membership.
    pub fn subject_count(&self, country: EntityId) -> Result<u32, Error> {
        if self.entity(country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("subject count requires a country".into()));
        }
        self.subjects.counts.get(&country).copied().ok_or_else(|| {
            Error::Invalid(format!(
                "subject count is unknown for country {}",
                country.0
            ))
        })
    }

    pub fn is_subject_of_type(&self, country: EntityId, subject_type: &str) -> Result<bool, Error> {
        self.subjects.identifier(subject_type)?;
        self.require_subject_country(country)?;
        Ok(self
            .subject_index
            .incoming
            .get(&country)
            .is_some_and(|&index| {
                self.politics.relations[index].subject_type.as_deref() == Some(subject_type)
            }))
    }

    /// Check whether a country currently has a direct incoming subject relation.
    pub fn is_subject_country(&self, country: EntityId) -> Result<bool, Error> {
        self.require_subject_country(country)?;
        Ok(self.subject_index.incoming.contains_key(&country))
    }

    /// Resolve a direct subject's overlord without treating an unknown graph as independence.
    pub fn subject_overlord(&self, country: EntityId) -> Result<Option<EntityId>, Error> {
        self.require_subject_country(country)?;
        Ok(self
            .subject_index
            .incoming
            .get(&country)
            .map(|&index| self.politics.relations[index].first))
    }

    /// Compare a direct incoming subject relation with the selected overlord.
    pub fn is_subject_of(&self, country: EntityId, overlord: EntityId) -> Result<bool, Error> {
        self.require_subject_country(country)?;
        self.require_subject_country(overlord)?;
        Ok(self
            .subject_index
            .incoming
            .get(&country)
            .is_some_and(|&index| self.politics.relations[index].first == overlord))
    }

    pub fn any_subject_country(
        &self,
        country: EntityId,
        condition: &Condition,
    ) -> Result<bool, Error> {
        self.any_subject_country_in_context(country, condition, crate::ScopeContext::new(country))
    }

    pub(crate) fn any_subject_country_in_context(
        &self,
        country: EntityId,
        condition: &Condition,
        context: crate::ScopeContext,
    ) -> Result<bool, Error> {
        let members = self.subject_countries(country)?;
        for &member in members {
            if condition.evaluate_in_context(self, context.with_current(member))? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
