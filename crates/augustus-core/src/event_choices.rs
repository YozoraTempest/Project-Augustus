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
    option_conditions: Arc<BTreeMap<String, BTreeMap<String, crate::Condition>>>,
    once_events: Arc<BTreeSet<String>>,
    pictures: Arc<BTreeMap<String, crate::EventPictures>>,
    locations: Arc<BTreeMap<String, crate::EventLocations>>,
    weights: Arc<crate::EventOptionWeights>,
    decision_system: Option<crate::event_decisions::DecisionProvider>,
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
            option_conditions: Arc::default(),
            once_events: Arc::default(),
            pictures: Arc::default(),
            locations: Arc::default(),
            weights: Arc::default(),
            decision_system: None,
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

    /// Conditions describe available options; commands keep stable declaration identifiers.
    pub fn with_option_conditions(
        mut self,
        conditions: BTreeMap<String, BTreeMap<String, crate::Condition>>,
    ) -> Result<Self, Error> {
        for (event, options) in &conditions {
            let definition = self.get(event)?;
            for option in options.keys() {
                if !definition.options.iter().any(|item| &item.id == option) {
                    return Err(Error::Invalid(format!(
                        "unknown option {option} for event {event}"
                    )));
                }
            }
        }
        self.option_conditions = Arc::new(conditions);
        Ok(self)
    }

    pub fn option_conditions(&self) -> &BTreeMap<String, BTreeMap<String, crate::Condition>> {
        &self.option_conditions
    }

    pub fn with_option_weights(
        mut self,
        weights: crate::EventOptionWeights,
    ) -> Result<Self, Error> {
        for (event, options) in &weights {
            let definition = self.get(event)?;
            for (option, weight) in options {
                if !definition.options.iter().any(|item| &item.id == option) {
                    return Err(Error::Invalid(format!(
                        "unknown option {option} for event {event}"
                    )));
                }
                weight.validate()?;
            }
        }
        self.weights = Arc::new(weights);
        Ok(self)
    }

    pub fn option_weights(&self) -> &crate::EventOptionWeights {
        &self.weights
    }

    pub fn with_decision_system(
        mut self,
        system: Arc<dyn crate::EventDecisionSystem>,
    ) -> Result<Self, Error> {
        if system.content_key().trim().is_empty() {
            return Err(Error::Invalid("empty event decision content key".into()));
        }
        self.decision_system = Some(crate::event_decisions::DecisionProvider(system));
        Ok(self)
    }

    pub fn decision_system_key(&self) -> Option<String> {
        self.decision_system
            .as_ref()
            .map(|system| system.0.content_key())
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

    pub fn with_pictures(
        mut self,
        pictures: BTreeMap<String, crate::EventPictures>,
    ) -> Result<Self, Error> {
        for (event, definition) in &pictures {
            self.get(event)?;
            definition.validate_keys()?;
        }
        self.pictures = Arc::new(pictures);
        Ok(self)
    }

    pub fn pictures(&self) -> &BTreeMap<String, crate::EventPictures> {
        &self.pictures
    }

    pub fn with_locations(
        mut self,
        locations: BTreeMap<String, crate::EventLocations>,
    ) -> Result<Self, Error> {
        for (event, locations) in &locations {
            let definition = self.get(event)?;
            if locations.names().next().is_none()
                || locations.names().any(|name| name.trim().is_empty())
            {
                return Err(Error::Invalid(
                    "empty event location definition or province name".into(),
                ));
            }
            for option in locations.options.keys() {
                if !definition.options.iter().any(|item| &item.id == option) {
                    return Err(Error::Invalid(format!(
                        "unknown option {option} for event {event}"
                    )));
                }
            }
        }
        self.locations = Arc::new(locations);
        Ok(self)
    }

    pub fn locations(&self) -> &BTreeMap<String, crate::EventLocations> {
        &self.locations
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for name in self
            .locations
            .values()
            .flat_map(crate::EventLocations::names)
        {
            crate::EventLocations::resolve(world, name)?;
        }
        for condition in self
            .conditions
            .values()
            .chain(
                self.option_conditions
                    .values()
                    .flat_map(|options| options.values()),
            )
            .chain(
                self.pictures.values().flat_map(|pictures| {
                    pictures.variants.iter().map(|variant| &variant.condition)
                }),
            )
            .chain(
                self.weights
                    .values()
                    .flat_map(|options| options.values())
                    .flat_map(|weight| weight.modifiers.iter().map(|modifier| &modifier.condition)),
            )
        {
            crate::world::validate_condition(
                condition,
                world.schema(),
                world.completions(),
                world.modifiers(),
                world.governments(),
                world.metrics(),
                world.justifications(),
                world.subjects(),
            )?;
        }
        for definition in self.definitions.values() {
            for plan in std::iter::once(&definition.immediate)
                .chain(definition.options.iter().map(|option| &option.effects))
                .chain(std::iter::once(&definition.after))
            {
                crate::world::validate_plan(
                    plan,
                    world.schema(),
                    world.rules.len(),
                    self,
                    world.completions(),
                    world.modifiers(),
                    world.governments(),
                    world.metrics(),
                    world.justifications(),
                    world.subjects(),
                )?;
            }
        }
        Ok(())
    }

    pub(crate) fn get(&self, id: &str) -> Result<&EventDefinition, Error> {
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pictures: BTreeMap<PendingEventId, Option<String>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) choices: Vec<EventChoice>,
}

impl EventQueue {
    pub(crate) fn is_empty(&self) -> bool {
        self.next_id == 0
            && self.pending.is_empty()
            && self.fired.is_empty()
            && self.pictures.is_empty()
            && self.choices.is_empty()
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let mut commands = BTreeSet::new();
        for choice in &self.choices {
            if !commands.insert(choice.pending) {
                return Err(Error::Invalid("duplicate saved event choice".into()));
            }
            world.validate_event_choice(choice)?;
        }
        if self
            .pictures
            .keys()
            .any(|id| !self.pending.iter().any(|pending| pending.id == *id))
        {
            return Err(Error::Invalid(
                "picture refers to an unknown pending event".into(),
            ));
        }
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
            match (
                world.event_catalog.pictures.get(&pending.event),
                self.pictures.get(&pending.id),
            ) {
                (None, None) => (),
                (Some(pictures), Some(key)) if pictures.contains(key.as_deref()) => (),
                _ => {
                    return Err(Error::Invalid(
                        "invalid saved event picture selection".into(),
                    ));
                }
            }
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

    /// The picture selected at dispatch, retained even if display conditions
    /// subsequently change. This returns a resource key, not a rendered asset.
    pub fn pending_event_picture(&self, id: PendingEventId) -> Result<Option<&str>, Error> {
        if !self
            .event_queue
            .pending
            .iter()
            .any(|pending| pending.id == id)
        {
            return Err(Error::Invalid(format!("unknown pending event {}", id.0)));
        }
        Ok(self
            .event_queue
            .pictures
            .get(&id)
            .and_then(|key| key.as_deref()))
    }

    /// Read current availability without executing effects or consuming randomness.
    pub fn available_event_options(&self, id: PendingEventId) -> Result<Vec<&EventOption>, Error> {
        self.require_running()?;
        let pending = self
            .event_queue
            .pending
            .iter()
            .find(|pending| pending.id == id)
            .ok_or_else(|| Error::Invalid(format!("unknown pending event {}", id.0)))?;
        let definition = self.event_catalog.get(&pending.event)?;
        let conditions = self.event_catalog.option_conditions.get(&pending.event);
        let mut available = Vec::new();
        for option in &definition.options {
            if let Some(condition) = conditions.and_then(|conditions| conditions.get(&option.id))
                && !condition.evaluate(self, pending.current)?
            {
                continue;
            }
            available.push(option);
        }
        Ok(available)
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
        self.execute_inner(
            crate::ScopeContext::new(current),
            &definition.immediate,
            budget,
            depth,
        )?;
        let picture = catalog
            .pictures
            .get(event)
            .map(|pictures| {
                pictures
                    .select(self, current)
                    .map(|key| key.map(str::to_owned))
            })
            .transpose()?;
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
        if let Some(picture) = picture {
            self.event_queue.pictures.insert(id, picture);
        }
        if let Some(system) = &catalog.decision_system {
            let country = match self.entity(current)?.kind {
                EntityKind::Country => Some(current),
                EntityKind::Province => self.entity(current)?.owner,
            };
            let controller = country
                .map(|country| self.country_controller(country))
                .transpose()?
                .flatten();
            match controller {
                Some(crate::CountryController::Human) => (),
                Some(crate::CountryController::Ai) => {
                    let pending = self
                        .event_queue
                        .pending
                        .last()
                        .expect("just appended")
                        .clone();
                    if let Some(option) = system.0.select(self, &pending)? {
                        self.queue_event_choice(EventChoice {
                            pending: id,
                            option,
                        })?;
                    }
                }
                None => {
                    return Err(Error::Invalid(
                        "event decision requires a known country controller".into(),
                    ));
                }
            }
        }
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
        self.validate_event_choice(choice)?;
        let result = self.choose_event_inner(choice, &mut 100_000);
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    pub(crate) fn validate_event_choice(&self, choice: &EventChoice) -> Result<(), Error> {
        let pending = self
            .event_queue
            .pending
            .iter()
            .find(|pending| pending.id == choice.pending)
            .ok_or_else(|| Error::Invalid(format!("unknown pending event {}", choice.pending.0)))?;
        if !self
            .event_catalog
            .get(&pending.event)?
            .options
            .iter()
            .any(|option| option.id == choice.option)
        {
            return Err(Error::Invalid(format!(
                "unknown event option {}",
                choice.option
            )));
        }
        if self
            .event_queue
            .choices
            .iter()
            .any(|item| item.pending == choice.pending && item.option != choice.option)
        {
            return Err(Error::Invalid(
                "event already has a different accepted choice".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn choose_event_inner(
        &mut self,
        choice: &EventChoice,
        budget: &mut usize,
    ) -> Result<(), Error> {
        self.validate_event_choice(choice)?;
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
        self.execute_inner(
            crate::ScopeContext::new(current),
            &option.effects,
            budget,
            0,
        )?;
        self.execute_inner(
            crate::ScopeContext::new(current),
            &definition.after,
            budget,
            0,
        )?;
        let completed = self
            .event_queue
            .pending
            .iter()
            .position(|pending| pending.id == choice.pending)
            .ok_or_else(|| Error::Invalid("pending event was consumed during execution".into()))?;
        self.event_queue.pending.remove(completed);
        self.event_queue.pictures.remove(&choice.pending);
        self.event_queue
            .choices
            .retain(|item| item.pending != choice.pending);
        Ok(())
    }
}
