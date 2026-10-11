// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::{EntityId, Error, World};

/// A live reference resolved from authoritative world state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeSelector {
    Office(String),
    Global(String),
}

impl ScopeSelector {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        let (Self::Office(name) | Self::Global(name)) = self;
        validate_name(name)
    }
}

pub(crate) fn validate_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::Invalid("empty scope name".into()));
    }
    Ok(())
}

impl World {
    /// Absence is distinct from an invalid reference or a wrong entity type.
    pub fn resolve_scope(&self, selector: &ScopeSelector) -> Result<Option<EntityId>, Error> {
        selector.validate()?;
        let selected = match selector {
            ScopeSelector::Office(name) => self.politics.offices.get(name),
            ScopeSelector::Global(name) => self.global_scopes.get(name),
        };
        if let Some(&entity) = selected {
            self.entity(entity)?;
        }
        Ok(selected.copied())
    }

    pub fn global_scopes(&self) -> &std::collections::BTreeMap<String, EntityId> {
        &self.global_scopes
    }

    pub fn bind_global_scope(&mut self, name: &str, entity: EntityId) -> Result<(), Error> {
        self.require_running()?;
        validate_name(name)?;
        self.entity(entity)?;
        self.global_scopes.insert(name.into(), entity);
        Ok(())
    }

    pub fn clear_global_scope(&mut self, name: &str) -> Result<(), Error> {
        self.require_running()?;
        validate_name(name)?;
        self.global_scopes.remove(name);
        Ok(())
    }
}
