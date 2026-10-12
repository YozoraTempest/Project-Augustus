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
}

impl Subjects {
    pub fn is_empty(&self) -> bool {
        !self.known && self.types.is_empty()
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
}

/// Derived lookup data. Relation records remain the only source of membership and type.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct SubjectIndex {
    members: BTreeMap<EntityId, Vec<EntityId>>,
    incoming: BTreeMap<EntityId, usize>,
}

impl SubjectIndex {
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
    pub fn subjects(&self) -> &Subjects {
        &self.subjects
    }

    pub fn set_subjects(&mut self, subjects: Subjects) -> Result<(), Error> {
        self.require_running()?;
        subjects.validate(&self.politics)?;
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

    pub fn any_subject_country(
        &self,
        country: EntityId,
        condition: &Condition,
    ) -> Result<bool, Error> {
        let members = self.subject_countries(country)?;
        for &member in members {
            if condition.evaluate(self, member)? {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
