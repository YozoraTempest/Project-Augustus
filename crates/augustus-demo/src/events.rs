// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{collections::BTreeMap, path::Path};

use augustus_core::{
    EntityId, EntityKind, EventCatalog, EventChoice, EventDefinition, PendingEventId, Schema, World,
};
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, ScriptCompiler, ScriptLibrary, TextDocument,
    TextEncoding,
};
use serde::{Deserialize, Serialize};

use crate::{Error, Input};

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scene {
    version: u32,
    fields: Vec<String>,
    entities: Vec<SceneEntity>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    offices: BTreeMap<String, EntityId>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    global_scope_types: BTreeMap<String, EntityKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    calendar: Option<augustus_core::Calendar>,
    #[serde(default, skip_serializing_if = "augustus_core::Completions::is_empty")]
    completions: augustus_core::Completions,
    #[serde(default, skip_serializing_if = "augustus_core::NamedValues::is_empty")]
    named_values: augustus_core::NamedValues,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SceneEntity {
    name: String,
    kind: EntityKind,
    owner: Option<usize>,
    capital: Option<usize>,
    values: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize)]
pub struct EventEntityView {
    pub name: String,
    pub kind: EntityKind,
    pub values: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize)]
pub struct PendingChoiceView {
    pub id: PendingEventId,
    pub event: String,
    pub current: EntityId,
    pub day: u64,
    pub options: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct EventView {
    pub day: u64,
    pub pending_tasks: usize,
    pub entities: Vec<EventEntityView>,
    pub pending_events: Vec<PendingChoiceView>,
    pub log: Vec<(u64, EntityId, String)>,
    #[serde(skip_serializing_if = "augustus_core::Completions::is_empty")]
    pub completions: augustus_core::Completions,
    #[serde(skip_serializing_if = "augustus_core::NamedValues::is_empty")]
    pub named_values: augustus_core::NamedValues,
}

/// Self-authored scenario exercising the same core from two content formats.
pub struct EventDemo {
    world: World,
    content: String,
}

impl EventDemo {
    pub fn embedded(input: Input) -> Result<Self, Error> {
        let scene: Scene =
            serde_json::from_str(include_str!("../../../examples/events/scene.json"))?;
        let (definitions, conditions, once_events) = match input {
            Input::Native => (
                serde_json::from_str(include_str!("../../../examples/events/events.json"))?,
                BTreeMap::new(),
                Default::default(),
            ),
            Input::Eu4 => {
                let schema = Schema::new(scene.fields.clone())?;
                let mut library = ScriptLibrary::new();
                library.add_effects(&document(
                    "demo/effects.txt",
                    include_str!("../../../examples/events/eu4/common/scripted_effects/demo.txt"),
                ))?;
                let compiled = ScriptCompiler::new(&schema, &library, true)
                    .with_mission_ids(scene.completions.identifiers.clone())
                    .with_global_scope_types(scene.global_scope_types.clone())
                    .compile_events(&document(
                        "demo/events.txt",
                        include_str!("../../../examples/events/eu4/events/demo.txt"),
                    ))?;
                (
                    compiled.definitions,
                    compiled.conditions,
                    compiled.once_events,
                )
            }
        };
        Self::load(scene, definitions, conditions, once_events)
    }

    pub fn from_directory(root: &Path, input: Input) -> Result<Self, Error> {
        let root = ContentRoot::open(root)?;
        let read_json = |name| -> Result<String, Error> {
            let document = root.read_text(name, TextEncoding::Utf8)?;
            std::fs::read_to_string(document.source())
                .map_err(|error| Error::Invalid(error.to_string()))
        };
        let scene: Scene = serde_json::from_str(&read_json("scene.json")?)?;
        let (definitions, conditions, once_events) = match input {
            Input::Native => {
                let conditions = if root.path().join("conditions.json").exists() {
                    serde_json::from_str(&read_json("conditions.json")?)?
                } else {
                    BTreeMap::new()
                };
                let once_events = if root.path().join("once_events.json").exists() {
                    serde_json::from_str(&read_json("once_events.json")?)?
                } else {
                    Default::default()
                };
                (
                    serde_json::from_str(&read_json("events.json")?)?,
                    conditions,
                    once_events,
                )
            }
            Input::Eu4 => {
                let content = ContentSet::resolve(vec![ContentSource::base(ContentRoot::open(
                    root.path().join("eu4"),
                )?)])?;
                let library = ScriptLibrary::from_content(&content, TextEncoding::Utf8)?;
                let schema = Schema::new(scene.fields.clone())?;
                let compiled = ScriptCompiler::new(&schema, &library, true)
                    .with_mission_ids(scene.completions.identifiers.clone())
                    .with_global_scope_types(scene.global_scope_types.clone())
                    .compile_events(&content.read_text("events/demo.txt", TextEncoding::Utf8)?)?;
                (
                    compiled.definitions,
                    compiled.conditions,
                    compiled.once_events,
                )
            }
        };
        Self::load(scene, definitions, conditions, once_events)
    }

    fn load(
        scene: Scene,
        definitions: Vec<EventDefinition>,
        conditions: BTreeMap<String, augustus_core::Condition>,
        once_events: std::collections::BTreeSet<String>,
    ) -> Result<Self, Error> {
        if scene.version != 1 || scene.entities.is_empty() {
            return Err(Error::Invalid(
                "expected event scene version 1 and entities".into(),
            ));
        }
        let catalog = EventCatalog::new(definitions)?
            .with_conditions(conditions)?
            .with_once_events(once_events)?;
        // Canonical effective content, including initial state and compiled plans.
        let content = if !catalog.once_events().is_empty() {
            serde_json::to_string(&(
                "augustus-events:1",
                &scene,
                catalog.definitions(),
                catalog.conditions(),
                catalog.once_events(),
            ))?
        } else if catalog.conditions().is_empty() {
            serde_json::to_string(&("augustus-events:1", &scene, catalog.definitions()))?
        } else {
            serde_json::to_string(&(
                "augustus-events:1",
                &scene,
                catalog.definitions(),
                catalog.conditions(),
            ))?
        };
        let mut world = World::new(Schema::new(scene.fields.clone())?);
        if let Some(calendar) = scene.calendar {
            world.set_calendar(calendar)?;
        }
        for entity in &scene.entities {
            world.add_entity(&entity.name, entity.kind);
        }
        for (index, entity) in scene.entities.iter().enumerate() {
            let id = EntityId(index);
            if let Some(owner) = entity.owner {
                world.set_owner(id, EntityId(owner))?;
            }
            for (name, value) in &entity.values {
                let field = world
                    .schema()
                    .field(name)
                    .ok_or_else(|| Error::Invalid(format!("unregistered scene field {name}")))?;
                world.set(id, field, *value)?;
            }
        }
        for (index, entity) in scene.entities.iter().enumerate() {
            if let Some(capital) = entity.capital {
                world.set_capital(EntityId(index), EntityId(capital))?;
            }
        }
        if !scene.offices.is_empty() {
            world.set_politics(augustus_core::Politics {
                offices: scene.offices.clone(),
                ..Default::default()
            })?;
        }
        world.set_completions(scene.completions)?;
        world.set_named_values(scene.named_values)?;
        world.set_event_catalog(catalog)?;
        Ok(Self { world, content })
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn trigger(&mut self, event: &str, entity: usize) -> Result<PendingEventId, Error> {
        Ok(self.world.trigger_event(event, EntityId(entity))?)
    }

    pub fn choose(&mut self, choice: &EventChoice) -> Result<(), Error> {
        Ok(self.world.choose_event(choice)?)
    }

    pub fn candidates(&mut self, events: &[String], entity: usize) -> Result<(), Error> {
        if events.is_empty() {
            return Ok(());
        }
        Ok(self.world.execute(
            EntityId(entity),
            &[augustus_core::Action::EventCandidates {
                events: events.to_vec(),
            }],
        )?)
    }

    pub fn advance(&mut self, days: u64) -> Result<(), Error> {
        let day = self
            .world
            .day()
            .checked_add(days)
            .ok_or(augustus_core::Error::Overflow)?;
        Ok(self.world.advance_to(day)?)
    }

    pub fn save(&self) -> Result<String, Error> {
        Ok(self.world.save(&self.content)?)
    }

    pub fn restore(&mut self, json: &str) -> Result<(), Error> {
        let restored = World::restore_with_events(
            json,
            &self.content,
            self.world.schema(),
            Vec::new(),
            self.world.event_catalog().clone(),
        )?;
        if restored.entities().len() != self.world.entities().len()
            || restored.completions().identifiers != self.world.completions().identifiers
            || restored
                .entities()
                .iter()
                .zip(self.world.entities())
                .any(|(left, right)| left.name != right.name || left.kind != right.kind)
        {
            return Err(Error::Invalid("event scene entities do not match".into()));
        }
        self.world = restored;
        Ok(())
    }

    pub fn view(&self) -> Result<EventView, Error> {
        let entities = self
            .world
            .entities()
            .iter()
            .enumerate()
            .map(|(index, entity)| {
                let values = self
                    .world
                    .schema()
                    .fields()
                    .iter()
                    .enumerate()
                    .map(|(slot, name)| {
                        Ok((
                            name.clone(),
                            self.world
                                .value(EntityId(index), augustus_core::FieldId(slot))?,
                        ))
                    })
                    .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?;
                Ok(EventEntityView {
                    name: entity.name.clone(),
                    kind: entity.kind,
                    values,
                })
            })
            .collect::<Result<Vec<_>, augustus_core::Error>>()?;
        let pending_events = self
            .world
            .pending_events()
            .iter()
            .map(|pending| PendingChoiceView {
                id: pending.id,
                event: pending.event.clone(),
                current: pending.current,
                day: pending.day,
                options: self.world.event_catalog().definitions()[&pending.event]
                    .options
                    .iter()
                    .map(|option| option.id.clone())
                    .collect(),
            })
            .collect();
        Ok(EventView {
            day: self.world.day(),
            pending_tasks: self.world.pending_tasks(),
            entities,
            pending_events,
            log: self.world.events().to_vec(),
            completions: self.world.completions().clone(),
            named_values: self.world.named_values().clone(),
        })
    }
}

fn document(name: &str, text: &str) -> TextDocument {
    TextDocument::from_bytes(name, text.as_bytes().to_vec(), TextEncoding::Utf8)
}
