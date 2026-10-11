// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, Plan, World};

/// Immutable session content. Both native definitions and compiled inputs use plans.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventDefinition {
    pub id: String,
    pub scope: EntityKind,
    pub immediate: Plan,
    pub options: Vec<EventOption>,
    pub after: Plan,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventOption {
    pub id: String,
    pub effects: Plan,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EventCatalog {
    definitions: Arc<BTreeMap<String, EventDefinition>>,
}

impl EventCatalog {
    pub fn new(definitions: impl IntoIterator<Item = EventDefinition>) -> Result<Self, Error> {
        let mut result = BTreeMap::new();
        for definition in definitions {
            let mut options = BTreeSet::new();
            if definition.id.is_empty()
                || definition.options.is_empty()
                || definition
                    .options
                    .iter()
                    .any(|option| option.id.is_empty() || !options.insert(&option.id))
            {
                return Err(Error::Invalid("invalid event or option identifiers".into()));
            }
            let id = definition.id.clone();
            if result.insert(id.clone(), definition).is_some() {
                return Err(Error::Invalid(format!("duplicate event {id}")));
            }
        }
        Ok(Self {
            definitions: Arc::new(result),
        })
    }

    pub fn definitions(&self) -> &BTreeMap<String, EventDefinition> {
        &self.definitions
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for definition in self.definitions.values() {
            for plan in std::iter::once(&definition.immediate)
                .chain(definition.options.iter().map(|option| &option.effects))
                .chain(std::iter::once(&definition.after))
            {
                crate::world::validate_plan(plan, world.schema(), world.rules.len(), self)?;
            }
        }
        Ok(())
    }

    fn get(&self, id: &str) -> Result<&EventDefinition, Error> {
        self.definitions
            .get(id)
            .ok_or_else(|| Error::Invalid(format!("unknown event {id}")))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PendingEventId(pub u64);

/// Captures entity identity; numeric state and entity relationships remain live.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PendingEvent {
    pub id: PendingEventId,
    pub event: String,
    pub current: EntityId,
    pub day: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventChoice {
    pub pending: PendingEventId,
    pub option: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EventQueue {
    next_id: u64,
    pending: Vec<PendingEvent>,
}

impl EventQueue {
    pub(crate) fn is_empty(&self) -> bool {
        self.next_id == 0 && self.pending.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        if self.next_id > 0 && world.event_catalog.definitions.is_empty() {
            return Err(Error::Invalid(
                "event content is required to restore this session".into(),
            ));
        }
        let mut previous = None;
        for pending in &self.pending {
            if pending.id.0 >= self.next_id
                || previous.is_some_and(|id| id >= pending.id)
                || pending.day > world.day()
            {
                return Err(Error::Invalid("invalid saved pending event order".into()));
            }
            previous = Some(pending.id);
            let definition = world.event_catalog.get(&pending.event)?;
            if world.entity(pending.current)?.kind != definition.scope {
                return Err(Error::Invalid("invalid saved event scope".into()));
            }
        }
        Ok(())
    }
}

impl World {
    /// Install content before the first event is triggered. Content is not save state.
    pub fn set_event_catalog(&mut self, catalog: EventCatalog) -> Result<(), Error> {
        self.require_running()?;
        if !self.event_queue.is_empty() {
            return Err(Error::Invalid("event content is already in use".into()));
        }
        catalog.validate(self)?;
        self.validate_event_content(&catalog)?;
        self.event_catalog = catalog;
        Ok(())
    }

    pub fn event_catalog(&self) -> &EventCatalog {
        &self.event_catalog
    }

    pub fn pending_events(&self) -> &[PendingEvent] {
        &self.event_queue.pending
    }

    /// Explicit dispatch only: candidate generation and AI selection are separate.
    pub fn trigger_event(
        &mut self,
        event: &str,
        current: EntityId,
    ) -> Result<PendingEventId, Error> {
        self.require_running()?;
        let catalog = self.event_catalog.clone();
        let definition = catalog.get(event)?;
        if self.entity(current)?.kind != definition.scope {
            return Err(Error::Invalid("event scope does not match entity".into()));
        }
        self.event_queue
            .next_id
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        let result = self.trigger_event_inner(event, current, &mut 100_000, 0);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub(crate) fn trigger_event_inner(
        &mut self,
        event: &str,
        current: EntityId,
        budget: &mut usize,
        depth: usize,
    ) -> Result<PendingEventId, Error> {
        self.event_queue
            .next_id
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        let catalog = self.event_catalog.clone();
        let definition = catalog.get(event)?;
        if self.entity(current)?.kind != definition.scope {
            return Err(Error::Invalid("event scope does not match entity".into()));
        }
        self.execute_inner(current, &definition.immediate, budget, depth)?;
        // Nested immediate effects can append their own pending records first.
        let id = PendingEventId(self.event_queue.next_id);
        let next_id = id.0.checked_add(1).ok_or(Error::Overflow)?;
        self.event_queue.next_id = next_id;
        self.event_queue.pending.push(PendingEvent {
            id,
            event: event.into(),
            current,
            day: self.day(),
        });
        Ok(id)
    }

    /// Validate the command before any write, then run option and after in one plan.
    /// Execution errors keep preceding writes and stop the simulation, as execute does.
    pub fn choose_event(&mut self, choice: &EventChoice) -> Result<(), Error> {
        self.require_running()?;
        let index = self
            .event_queue
            .pending
            .iter()
            .position(|pending| pending.id == choice.pending)
            .ok_or_else(|| Error::Invalid(format!("unknown pending event {}", choice.pending.0)))?;
        let pending = &self.event_queue.pending[index];
        let current = pending.current;
        let catalog = self.event_catalog.clone();
        let definition = catalog.get(&pending.event)?;
        let option = definition
            .options
            .iter()
            .find(|option| option.id == choice.option)
            .ok_or_else(|| Error::Invalid(format!("unknown event option {}", choice.option)))?;
        self.execute_phases(current, &[&option.effects, &definition.after])?;
        self.event_queue.pending.remove(index);
        Ok(())
    }
}
