// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::{EntityId, Error, World};

/// Current selection and the entity that owns this execution or query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeContext {
    pub current: EntityId,
    pub root: EntityId,
}

impl ScopeContext {
    pub fn new(current: EntityId) -> Self {
        Self {
            current,
            root: current,
        }
    }

    pub fn with_current(self, current: EntityId) -> Self {
        Self { current, ..self }
    }

    pub(crate) fn validate(self, world: &World) -> Result<(), Error> {
        world.entity(self.current)?;
        world.entity(self.root)?;
        Ok(())
    }
}

/// An endpoint selected relative to the executing entity or by a live name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityRef {
    Relative(crate::Target),
    Selected(ScopeSelector),
}

impl EntityRef {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Relative(_) => Ok(()),
            Self::Selected(selector) => selector.validate(),
        }
    }

    pub fn resolve(&self, world: &World, current: EntityId) -> Result<Option<EntityId>, Error> {
        self.resolve_in_context(world, ScopeContext::new(current))
    }

    pub fn resolve_in_context(
        &self,
        world: &World,
        context: ScopeContext,
    ) -> Result<Option<EntityId>, Error> {
        match self {
            Self::Relative(target) => world.condition_target(context.current, *target),
            Self::Selected(selector) => world.resolve_scope_in_context(selector, context),
        }
    }
}

/// A live reference resolved from authoritative world state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScopeSelector {
    Root,
    Office(String),
    Global(String),
    Named {
        kind: crate::EntityKind,
        name: String,
    },
}

impl ScopeSelector {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        match self {
            Self::Root => Ok(()),
            Self::Office(name) | Self::Global(name) | Self::Named { name, .. } => {
                validate_name(name)
            }
        }
    }
}

pub(crate) fn validate_name(name: &str) -> Result<(), Error> {
    if name.is_empty() {
        return Err(Error::Invalid("empty scope name".into()));
    }
    Ok(())
}

impl World {
    pub fn known_absent_targets(
        &self,
    ) -> &std::collections::BTreeMap<EntityId, std::collections::BTreeSet<crate::Target>> {
        &self.known_absent_targets
    }

    /// Declare confirmed absence separately from an unbound, unknown relationship.
    pub fn mark_target_absent(
        &mut self,
        current: EntityId,
        target: crate::Target,
    ) -> Result<(), Error> {
        self.require_running()?;
        self.check_absent_target(current, target)?;
        if target == crate::Target::Owner
            && !self
                .known_absent_targets
                .get(&current)
                .is_some_and(|targets| targets.contains(&target))
        {
            std::sync::Arc::make_mut(&mut self.ownership).resolve_absence();
        }
        self.known_absent_targets
            .entry(current)
            .or_default()
            .insert(target);
        Ok(())
    }

    fn check_absent_target(&self, current: EntityId, target: crate::Target) -> Result<(), Error> {
        let entity = self.entity(current)?;
        let absent = match target {
            crate::Target::Owner => {
                entity.kind == crate::EntityKind::Province && entity.owner.is_none()
            }
            crate::Target::Capital => {
                entity.kind == crate::EntityKind::Country && entity.capital.is_none()
            }
            crate::Target::Current => false,
        };
        if !absent {
            return Err(Error::Invalid(
                "invalid absent relationship declaration".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn clear_absent_target(&mut self, current: EntityId, target: crate::Target) {
        if let Some(targets) = self.known_absent_targets.get_mut(&current) {
            targets.remove(&target);
            if targets.is_empty() {
                self.known_absent_targets.remove(&current);
            }
        }
    }

    pub(crate) fn validate_absent_targets(&self) -> Result<(), Error> {
        for (&current, targets) in &self.known_absent_targets {
            if targets.is_empty() {
                return Err(Error::Invalid(
                    "empty absent relationship declaration".into(),
                ));
            }
            for &target in targets {
                self.check_absent_target(current, target)?;
            }
        }
        Ok(())
    }

    pub(crate) fn condition_target(
        &self,
        current: EntityId,
        target: crate::Target,
    ) -> Result<Option<EntityId>, Error> {
        let entity = self.entity(current)?;
        let selected = match target {
            crate::Target::Current => Some(current),
            crate::Target::Owner => {
                if entity.kind != crate::EntityKind::Province {
                    return Err(Error::Invalid(
                        "owner condition scope requires a province".into(),
                    ));
                }
                entity.owner
            }
            crate::Target::Capital => {
                if entity.kind != crate::EntityKind::Country {
                    return Err(Error::Invalid(
                        "capital condition scope requires a country".into(),
                    ));
                }
                if let Some(capital) = entity.capital
                    && self.entity(capital)?.owner != Some(current)
                {
                    return Err(Error::Invalid(
                        "capital condition scope is not an owned province".into(),
                    ));
                }
                entity.capital
            }
        };
        if selected.is_none()
            && !self
                .known_absent_targets
                .get(&current)
                .is_some_and(|targets| targets.contains(&target))
        {
            return Err(Error::Invalid(
                "unknown condition scope relationship".into(),
            ));
        }
        Ok(selected)
    }

    /// Absence is distinct from an invalid reference or a wrong entity type.
    pub fn resolve_scope(&self, selector: &ScopeSelector) -> Result<Option<EntityId>, Error> {
        selector.validate()?;
        if let ScopeSelector::Named { kind, name } = selector {
            let mut matches = self
                .entity_names
                .get(name)
                .into_iter()
                .flatten()
                .copied()
                .filter(|id| self.entities()[id.0].kind == *kind);
            let Some(id) = matches.next() else {
                return Err(Error::Invalid(format!("unregistered named entity {name}")));
            };
            if matches.next().is_some() {
                return Err(Error::Invalid(format!("ambiguous named entity {name}")));
            }
            return Ok(Some(id));
        }
        let selected = match selector {
            ScopeSelector::Root => {
                return Err(Error::Invalid(
                    "root selection requires an execution context".into(),
                ));
            }
            ScopeSelector::Office(name) => self.politics.offices.get(name),
            ScopeSelector::Global(name) => self.global_scopes.get(name),
            ScopeSelector::Named { .. } => unreachable!("named scope resolved above"),
        };
        if let Some(&entity) = selected {
            self.entity(entity)?;
        }
        Ok(selected.copied())
    }

    pub fn resolve_scope_in_context(
        &self,
        selector: &ScopeSelector,
        context: ScopeContext,
    ) -> Result<Option<EntityId>, Error> {
        if matches!(selector, ScopeSelector::Root) {
            self.entity(context.root)?;
            Ok(Some(context.root))
        } else {
            self.resolve_scope(selector)
        }
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
