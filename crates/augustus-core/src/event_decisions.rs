// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{collections::BTreeMap, fmt::Debug, sync::Arc};

use rand::RngExt;
use serde::{Deserialize, Serialize};

use crate::{Condition, EntityId, Error, EventChoice, PendingEvent, World};

/// Ordered fixed-point factors. Each multiplication truncates towards zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventWeight {
    pub base: i64,
    pub unit: i64,
    pub modifiers: Vec<WeightModifier>,
    pub factor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightModifier {
    pub condition: Condition,
    pub factor: i64,
}

impl Default for EventWeight {
    fn default() -> Self {
        Self {
            base: 1_000,
            unit: 1_000,
            modifiers: Vec::new(),
            factor: 1_000,
        }
    }
}

impl EventWeight {
    pub(crate) fn validate(&self) -> Result<(), Error> {
        if self.unit <= 0 {
            return Err(Error::Invalid("event weight unit must be positive".into()));
        }
        Ok(())
    }

    pub fn evaluate(&self, world: &World, current: EntityId) -> Result<i64, Error> {
        self.validate()?;
        world.entity(current)?;
        let scale = |value: i64, factor: i64| {
            i64::try_from(i128::from(value) * i128::from(factor) / i128::from(self.unit))
                .map_err(|_| Error::Overflow)
        };
        let mut value = self.base;
        for modifier in &self.modifiers {
            if modifier.condition.evaluate(world, current)? {
                value = scale(value, modifier.factor)?;
            }
            if value == 0 {
                return Ok(0);
            }
        }
        scale(value, self.factor)
    }
}

/// A decision system owns selection, while the world owns accepted commands and effects.
/// Implementations keep all mutable session state in World. The content key must identify
/// their complete configuration and participate in the caller's content fingerprint.
pub trait EventDecisionSystem: Debug + Send + Sync {
    fn content_key(&self) -> String;
    fn select(&self, world: &mut World, pending: &PendingEvent) -> Result<Option<String>, Error>;
}

#[derive(Debug, Clone)]
pub(crate) struct DecisionProvider(pub Arc<dyn EventDecisionSystem>);

impl PartialEq for DecisionProvider {
    fn eq(&self, other: &Self) -> bool {
        self.0.content_key() == other.0.content_key()
    }
}
impl Eq for DecisionProvider {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EmptyWeightChoice {
    #[default]
    KeepPending,
    FirstDeclared,
}

/// Uses the world's saved random stream and integer cumulative weights.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WeightedEventDecisions {
    pub empty: EmptyWeightChoice,
}

impl EventDecisionSystem for WeightedEventDecisions {
    fn content_key(&self) -> String {
        let empty = match self.empty {
            EmptyWeightChoice::KeepPending => "keep_pending",
            EmptyWeightChoice::FirstDeclared => "first_declared",
        };
        format!("weighted-event-decisions:1:{empty}")
    }

    fn select(&self, world: &mut World, pending: &PendingEvent) -> Result<Option<String>, Error> {
        if world.event_catalog.has_unavailable_options(&pending.event) {
            return Ok(None);
        }
        let weights = world.event_option_weights(pending)?;
        let total = weights.iter().try_fold(0_i64, |sum, (_, weight)| {
            sum.checked_add(*weight).ok_or(Error::Overflow)
        })?;
        if total <= 0 {
            return match self.empty {
                EmptyWeightChoice::KeepPending => Ok(None),
                EmptyWeightChoice::FirstDeclared => Ok(Some(
                    world.event_catalog.definitions()[&pending.event].options[0]
                        .id
                        .clone(),
                )),
            };
        }
        if weights.iter().any(|(_, weight)| *weight < 0) {
            return Err(Error::Invalid(
                "negative event weight with a positive total".into(),
            ));
        }
        let mut draw = world.random_source().random_range(0..total);
        for (option, weight) in weights {
            if draw < weight {
                return Ok(Some(option));
            }
            draw -= weight;
        }
        unreachable!("positive checked total exhausts the draw")
    }
}

impl World {
    /// Evaluate one option at the time it is visited during AI selection.
    pub fn event_option_weight(
        &self,
        pending: &PendingEvent,
        option_index: usize,
    ) -> Result<Option<i64>, Error> {
        let definition = self
            .event_catalog
            .definitions()
            .get(&pending.event)
            .ok_or_else(|| Error::Invalid(format!("unknown event {}", pending.event)))?;
        if self.entity(pending.current)?.kind != definition.scope {
            return Err(Error::Invalid("event scope does not match entity".into()));
        }
        let option = definition
            .options
            .get(option_index)
            .ok_or_else(|| Error::Invalid("event option index is out of range".into()))?;
        if self
            .event_catalog
            .unavailable_option_reason(&pending.event, &option.id)
            .is_some()
        {
            return Ok(None);
        }
        if let Some(condition) = self
            .event_catalog
            .option_conditions()
            .get(&pending.event)
            .and_then(|conditions| conditions.get(&option.id))
            && !condition.evaluate(self, pending.current)?
        {
            return Ok(None);
        }
        let default_weight = EventWeight::default();
        let weight = self
            .event_catalog
            .option_weights()
            .get(&pending.event)
            .and_then(|weights| weights.get(&option.id))
            .unwrap_or(&default_weight);
        Ok(Some(weight.evaluate(self, pending.current)?))
    }

    /// Read weights in declaration order. Filtering leaves stable option identifiers intact.
    pub fn event_option_weights(
        &self,
        pending: &PendingEvent,
    ) -> Result<Vec<(String, i64)>, Error> {
        let definition = self
            .event_catalog
            .definitions()
            .get(&pending.event)
            .ok_or_else(|| Error::Invalid(format!("unknown event {}", pending.event)))?;
        if self.entity(pending.current)?.kind != definition.scope {
            return Err(Error::Invalid("event scope does not match entity".into()));
        }
        definition
            .options
            .iter()
            .enumerate()
            .map(|(index, option)| {
                let weight = self
                    .event_option_weight(pending, index)?
                    .unwrap_or_default();
                Ok((option.id.clone(), weight))
            })
            .collect()
    }

    pub fn queued_event_choices(&self) -> &[EventChoice] {
        &self.event_queue.choices
    }

    /// Accept a stable command now; execute it later against live state.
    pub fn queue_event_choice(&mut self, choice: EventChoice) -> Result<(), Error> {
        self.require_running()?;
        self.validate_event_choice(&choice)?;
        if self
            .event_queue
            .choices
            .iter()
            .any(|item| item.pending == choice.pending)
        {
            return Err(Error::Invalid("event choice is already queued".into()));
        }
        self.event_queue.choices.push(choice);
        Ok(())
    }

    /// Drain in acceptance order, including choices appended by option effects.
    /// One instruction budget covers the entire drain, including empty plans.
    pub fn process_event_commands(&mut self) -> Result<(), Error> {
        self.require_running()?;
        let mut budget = 100_000;
        let result = (|| {
            while let Some(choice) = self.event_queue.choices.first().cloned() {
                if budget == 0 {
                    return Err(Error::Invalid("instruction limit exceeded".into()));
                }
                budget -= 1;
                let context = self
                    .pending_events()
                    .iter()
                    .find(|pending| pending.id.0 == choice.pending.0)
                    .and_then(|pending| {
                        self.entity(pending.current).ok().map(|entity| {
                            format!(
                                "event {} option {} for {} (entity {})",
                                pending.event, choice.option, entity.name, pending.current.0
                            )
                        })
                    });
                if let Err(error) = self.choose_event_inner(&choice, &mut budget) {
                    if let Some(context) = context {
                        let message = match error {
                            Error::Invalid(message) => message,
                            error => error.to_string(),
                        };
                        return Err(Error::Invalid(format!("{context} failed: {message}")));
                    }
                    return Err(error);
                }
            }
            Ok(())
        })();
        if result.is_err() {
            self.failed = true;
        }
        result
    }
}

pub type EventOptionWeights = BTreeMap<String, BTreeMap<String, EventWeight>>;
