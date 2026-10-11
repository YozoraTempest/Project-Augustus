// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use rand_chacha::{ChaCha8Rng, rand_core::SeedableRng};
use serde::{Deserialize, Serialize};

use crate::{Action, Error, Plan, Politics, Target};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EntityId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FieldId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityKind {
    Country,
    Province,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Schema {
    fields: Vec<String>,
}

impl Schema {
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>) -> Result<Self, Error> {
        let fields: Vec<String> = names.into_iter().map(Into::into).collect();
        let mut unique = std::collections::BTreeSet::new();
        if fields
            .iter()
            .any(|name| name.is_empty() || !unique.insert(name))
        {
            return Err(Error::Invalid("empty or duplicate state field".into()));
        }
        Ok(Self { fields })
    }

    pub fn field(&self, name: &str) -> Option<FieldId> {
        self.fields
            .iter()
            .position(|field| field == name)
            .map(FieldId)
    }

    pub fn fields(&self) -> &[String] {
        &self.fields
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entity {
    pub name: String,
    pub kind: EntityKind,
    pub owner: Option<EntityId>,
    pub capital: Option<EntityId>,
    values: Vec<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Task {
    day: u64,
    sequence: u64,
    current: EntityId,
    plan: Plan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct World {
    schema: Schema,
    entities: Vec<Entity>,
    day: u64,
    next_sequence: u64,
    tasks: BTreeMap<(u64, u64), Task>,
    events: Vec<(u64, EntityId, String)>,
    pub(crate) politics: Politics,
    pub(crate) failed: bool,
    pub(crate) rules: Vec<Plan>,
    pub(crate) random: ChaCha8Rng,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    format: u32,
    content: String,
    schema: Schema,
    entities: Vec<Entity>,
    day: u64,
    next_sequence: u64,
    tasks: Vec<Task>,
    events: Vec<(u64, EntityId, String)>,
    random: ChaCha8Rng,
    #[serde(default, skip_serializing_if = "Politics::is_empty")]
    politics: Politics,
}

impl World {
    pub fn new(schema: Schema) -> Self {
        Self::with_rules(schema, Vec::new())
    }

    /// Rules are immutable session content; saves identify the complete content set.
    pub fn with_rules(schema: Schema, rules: Vec<Plan>) -> Self {
        Self::seeded(schema, rules, [0; 32])
    }

    pub fn seeded(schema: Schema, rules: Vec<Plan>, seed: [u8; 32]) -> Self {
        Self {
            schema,
            entities: Vec::new(),
            day: 0,
            next_sequence: 0,
            tasks: BTreeMap::new(),
            events: Vec::new(),
            politics: Politics::default(),
            failed: false,
            rules,
            random: ChaCha8Rng::from_seed(seed),
        }
    }

    pub fn schema(&self) -> &Schema {
        &self.schema
    }
    pub fn day(&self) -> u64 {
        self.day
    }
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }
    pub fn events(&self) -> &[(u64, EntityId, String)] {
        &self.events
    }
    pub fn pending_tasks(&self) -> usize {
        self.tasks.len()
    }

    pub fn add_entity(&mut self, name: impl Into<String>, kind: EntityKind) -> EntityId {
        let id = EntityId(self.entities.len());
        self.entities.push(Entity {
            name: name.into(),
            kind,
            owner: None,
            capital: None,
            values: vec![0; self.schema.fields.len()],
        });
        id
    }

    pub fn set_owner(&mut self, province: EntityId, country: EntityId) -> Result<(), Error> {
        if self.entity(province)?.kind != EntityKind::Province
            || self.entity(country)?.kind != EntityKind::Country
        {
            return Err(Error::Invalid(
                "owner relation requires province and country".into(),
            ));
        }
        self.entities[province.0].owner = Some(country);
        Ok(())
    }

    pub fn set_capital(&mut self, country: EntityId, province: EntityId) -> Result<(), Error> {
        if self.entity(country)?.kind != EntityKind::Country
            || self.entity(province)?.owner != Some(country)
        {
            return Err(Error::Invalid("capital must be an owned province".into()));
        }
        self.entities[country.0].capital = Some(province);
        Ok(())
    }

    pub fn entity(&self, id: EntityId) -> Result<&Entity, Error> {
        self.entities
            .get(id.0)
            .ok_or_else(|| Error::Invalid(format!("unknown entity {}", id.0)))
    }

    pub fn target(&self, current: EntityId, target: Target) -> Result<EntityId, Error> {
        let entity = self.entity(current)?;
        match target {
            Target::Current => Ok(current),
            Target::Owner => entity
                .owner
                .ok_or_else(|| Error::Invalid("entity has no owner".into())),
            Target::Capital => entity
                .capital
                .ok_or_else(|| Error::Invalid("entity has no capital".into())),
        }
    }

    pub fn value(&self, entity: EntityId, field: FieldId) -> Result<i64, Error> {
        self.entity(entity)?
            .values
            .get(field.0)
            .copied()
            .ok_or_else(|| Error::Invalid(format!("unknown field {}", field.0)))
    }

    pub fn set(&mut self, entity: EntityId, field: FieldId, value: i64) -> Result<(), Error> {
        self.entity(entity)?;
        let slot = self.entities[entity.0]
            .values
            .get_mut(field.0)
            .ok_or_else(|| Error::Invalid(format!("unknown field {}", field.0)))?;
        *slot = value;
        Ok(())
    }

    pub(crate) fn emit(&mut self, current: EntityId, message: String) {
        self.events.push((self.day, current, message));
    }

    pub fn schedule(&mut self, current: EntityId, delay: u64, plan: Plan) -> Result<(), Error> {
        self.require_running()?;
        self.entity(current)?;
        validate_plan(&plan, &self.schema, self.rules.len())?;
        if delay == 0 {
            return Err(Error::Invalid(
                "scheduled tasks require a positive delay".into(),
            ));
        }
        let day = self.day.checked_add(delay).ok_or(Error::Overflow)?;
        let sequence = self.next_sequence;
        self.next_sequence = sequence.checked_add(1).ok_or(Error::Overflow)?;
        self.tasks.insert(
            (day, sequence),
            Task {
                day,
                sequence,
                current,
                plan,
            },
        );
        Ok(())
    }

    /// Jump between due tasks; empty dates do not require a per-entity scan.
    pub fn advance_to(&mut self, day: u64) -> Result<(), Error> {
        self.require_running()?;
        if day < self.day {
            return Err(Error::Invalid("cannot advance backwards".into()));
        }
        while self
            .tasks
            .first_key_value()
            .is_some_and(|(key, _)| key.0 <= day)
        {
            let (_, task) = self.tasks.pop_first().expect("checked nonempty queue");
            self.day = task.day;
            self.execute(task.current, &task.plan)?;
        }
        self.day = day;
        Ok(())
    }

    pub fn save(&self, content: &str) -> Result<String, Error> {
        self.require_running()?;
        Ok(serde_json::to_string_pretty(&Snapshot {
            format: 1,
            content: content.into(),
            schema: self.schema.clone(),
            entities: self.entities.clone(),
            day: self.day,
            next_sequence: self.next_sequence,
            tasks: self.tasks.values().cloned().collect(),
            events: self.events.clone(),
            random: self.random.clone(),
            politics: self.politics.clone(),
        })?)
    }

    pub fn restore(json: &str, content: &str, schema: &Schema) -> Result<Self, Error> {
        Self::restore_with_rules(json, content, schema, Vec::new())
    }

    pub fn restore_with_rules(
        json: &str,
        content: &str,
        schema: &Schema,
        rules: Vec<Plan>,
    ) -> Result<Self, Error> {
        let snapshot: Snapshot = serde_json::from_str(json)?;
        if snapshot.format != 1 || snapshot.content != content || &snapshot.schema != schema {
            return Err(Error::Invalid(
                "save format, content or schema does not match".into(),
            ));
        }
        let mut world = Self {
            schema: snapshot.schema,
            entities: snapshot.entities,
            day: snapshot.day,
            next_sequence: snapshot.next_sequence,
            tasks: BTreeMap::new(),
            events: snapshot.events,
            failed: false,
            rules,
            random: snapshot.random,
            politics: snapshot.politics,
        };
        for rule in &world.rules {
            validate_plan(rule, schema, world.rules.len())?;
        }
        for (index, entity) in world.entities.iter().enumerate() {
            if entity.values.len() != world.schema.fields.len() {
                return Err(Error::Invalid("invalid state field count".into()));
            }
            if let Some(owner) = entity.owner
                && (entity.kind != EntityKind::Province
                    || world.entity(owner)?.kind != EntityKind::Country)
            {
                return Err(Error::Invalid("invalid saved owner".into()));
            }
            if let Some(capital) = entity.capital
                && (entity.kind != EntityKind::Country
                    || world.entity(capital)?.owner != Some(EntityId(index)))
            {
                return Err(Error::Invalid("invalid saved capital".into()));
            }
        }
        world.politics.validate(&world)?;
        let mut sequences = std::collections::BTreeSet::new();
        for task in snapshot.tasks {
            if task.day <= world.day
                || task.sequence >= world.next_sequence
                || !sequences.insert(task.sequence)
            {
                return Err(Error::Invalid("invalid saved task order".into()));
            }
            world.entity(task.current)?;
            validate_plan(&task.plan, schema, world.rules.len())?;
            world.tasks.insert((task.day, task.sequence), task);
        }
        for (day, entity, _) in &world.events {
            if *day > world.day {
                return Err(Error::Invalid("saved event is in the future".into()));
            }
            world.entity(*entity)?;
        }
        Ok(world)
    }

    pub(crate) fn require_running(&self) -> Result<(), Error> {
        if self.failed {
            Err(Error::Invalid(
                "simulation stopped after a runtime error".into(),
            ))
        } else {
            Ok(())
        }
    }
}

fn validate_plan(plan: &[Action], schema: &Schema, rule_count: usize) -> Result<(), Error> {
    for action in plan {
        match action {
            Action::Add { field, .. } | Action::Set { field, .. }
                if field.0 >= schema.fields.len() =>
            {
                return Err(Error::Invalid("invalid saved plan field".into()));
            }
            Action::If {
                condition,
                then,
                otherwise,
            } => {
                validate_condition(condition, schema)?;
                validate_plan(then, schema, rule_count)?;
                validate_plan(otherwise, schema, rule_count)?;
            }
            Action::RandomBranch { then, otherwise } => {
                validate_plan(then, schema, rule_count)?;
                validate_plan(otherwise, schema, rule_count)?;
            }
            Action::With { plan, .. } => validate_plan(plan, schema, rule_count)?,
            Action::Schedule { delay, plan } => {
                if *delay == 0 {
                    return Err(Error::Invalid("invalid saved task delay".into()));
                }
                validate_plan(plan, schema, rule_count)?;
            }
            Action::Accumulate { field, source, .. }
                if field.0 >= schema.fields.len() || source.0 >= schema.fields.len() =>
            {
                return Err(Error::Invalid("invalid saved arithmetic field".into()));
            }
            Action::Run(rule) if rule.0 >= rule_count => {
                return Err(Error::Invalid("invalid saved rule".into()));
            }
            _ => {}
        }
    }
    Ok(())
}

fn validate_condition(condition: &crate::Condition, schema: &Schema) -> Result<(), Error> {
    match condition {
        crate::Condition::AtLeast { field, .. } if field.0 >= schema.fields.len() => {
            Err(Error::Invalid("invalid saved condition field".into()))
        }
        crate::Condition::All(conditions) => {
            for condition in conditions {
                validate_condition(condition, schema)?;
            }
            Ok(())
        }
        crate::Condition::Not(condition) => validate_condition(condition, schema),
        _ => Ok(()),
    }
}
