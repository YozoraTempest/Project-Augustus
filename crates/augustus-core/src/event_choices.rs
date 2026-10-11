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
    conditions: Arc<BTreeMap<String, crate::Condition>>,
    once_events: Arc<BTreeSet<String>>,
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
            conditions: Arc::default(),
            once_events: Arc::default(),
        })
    }

    pub fn definitions(&self) -> &BTreeMap<String, EventDefinition> {
        &self.definitions
    }

    /// Conditions apply to candidate lists and conditional calls, not forced calls.
    pub fn with_conditions(
        mut self,
        conditions: BTreeMap<String, crate::Condition>,
    ) -> Result<Self, Error> {
        for event in conditions.keys() {
            self.get(event)?;
        }
        self.conditions = Arc::new(conditions);
        Ok(self)
    }

    pub fn conditions(&self) -> &BTreeMap<String, crate::Condition> {
        &self.conditions
    }

    /// Events that may complete dispatch only once across a saved session.
    pub fn with_once_events(mut self, events: BTreeSet<String>) -> Result<Self, Error> {
        for event in &events {
            self.get(event)?;
        }
        self.once_events = Arc::new(events);
        Ok(self)
    }

    pub fn once_events(&self) -> &BTreeSet<String> {
        &self.once_events
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for condition in self.conditions.values() {
            crate::world::validate_condition(condition, world.schema())?;
        }
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
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    fired: BTreeSet<String>,
}

impl EventQueue {
    pub(crate) fn is_empty(&self) -> bool {
        self.next_id == 0 && self.pending.is_empty() && self.fired.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        if self.next_id > 0 && world.event_catalog.definitions.is_empty() {
            return Err(Error::Invalid(
                "event content is required to restore this session".into(),
            ));
        }
        for event in &self.fired {
            world.event_catalog.get(event)?;
            if self.next_id == 0 || !world.event_catalog.once_events.contains(event) {
                return Err(Error::Invalid(
                    "invalid saved once-only event record".into(),
                ));
            }
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
            if world.event_catalog.once_events.contains(&pending.event)
                && !self.fired.contains(&pending.event)
            {
                return Err(Error::Invalid(
                    "missing saved once-only event record".into(),
                ));
            }
        }
        Ok(())
    }
}

impl World {
    fn event_eligible(&self, event: &str, current: EntityId) -> Result<bool, Error> {
        let definition = self.event_catalog.get(event)?;
        if self.entity(current)?.kind != definition.scope {
            return Err(Error::Invalid("event scope does not match entity".into()));
        }
        if self.event_queue.fired.contains(event) {
            return Ok(false);
        }
        match self.event_catalog.conditions.get(event) {
            Some(condition) => condition.evaluate(self, current),
            None => Ok(true),
        }
    }

    pub(crate) fn event_candidates_inner(
        &mut self,
        events: &[String],
        current: EntityId,
        budget: &mut usize,
        depth: usize,
    ) -> Result<(), Error> {
        let mut selected = Vec::new();
        for event in events {
            if *budget == 0 {
                return Err(Error::Invalid("plan instruction limit exceeded".into()));
            }
            *budget -= 1;
            if self.event_eligible(event, current)? {
                selected.push(event);
            }
        }
        for event in selected {
            // Earlier immediate effects must not change membership in this batch.
            self.trigger_event_inner(event, current, budget, depth)?;
        }
        Ok(())
    }

    pub(crate) fn try_event_inner(
        &mut self,
        event: &str,
        current: EntityId,
        budget: &mut usize,
        depth: usize,
    ) -> Result<Option<PendingEventId>, Error> {
        if !self.event_eligible(event, current)? {
            return Ok(None);
        }
        self.trigger_event_inner(event, current, budget, depth)
    }

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

    /// Completed once-only dispatches, shared by every entity in this session.
    pub fn fired_events(&self) -> &BTreeSet<String> {
        &self.event_queue.fired
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
        if self.event_queue.fired.contains(event) {
            return Err(Error::Invalid(format!(
                "once-only event {event} has already fired"
            )));
        }
        self.event_queue
            .next_id
            .checked_add(1)
            .ok_or(Error::Overflow)?;
        let result = self
            .trigger_event_inner(event, current, &mut 100_000, 0)
            .and_then(|pending| {
                pending.ok_or_else(|| Error::Invalid("event was not dispatched".into()))
            });
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
    ) -> Result<Option<PendingEventId>, Error> {
        let catalog = self.event_catalog.clone();
        let definition = catalog.get(event)?;
        if self.entity(current)?.kind != definition.scope {
            return Err(Error::Invalid("event scope does not match entity".into()));
        }
        if self.event_queue.fired.contains(event) {
            return Ok(None);
        }
        self.event_queue
            .next_id
            .checked_add(1)
            .ok_or(Error::Overflow)?;
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
        // Registration follows immediate execution, including any nested dispatches.
        if catalog.once_events.contains(event) {
            self.event_queue.fired.insert(event.into());
        }
        Ok(Some(id))
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
