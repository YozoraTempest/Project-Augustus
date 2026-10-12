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
    #[serde(default, skip_serializing_if = "augustus_core::Politics::is_empty")]
    politics: augustus_core::Politics,
    #[serde(default, skip_serializing_if = "augustus_core::Subjects::is_empty")]
    subjects: augustus_core::Subjects,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    global_scope_types: BTreeMap<String, EntityKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    calendar: Option<augustus_core::Calendar>,
    #[serde(default, skip_serializing_if = "augustus_core::Completions::is_empty")]
    completions: augustus_core::Completions,
    #[serde(default, skip_serializing_if = "augustus_core::Objectives::is_empty")]
    objectives: augustus_core::Objectives,
    #[serde(default, skip_serializing_if = "augustus_core::NamedValues::is_empty")]
    named_values: augustus_core::NamedValues,
    #[serde(default, skip_serializing_if = "augustus_core::Modifiers::is_empty")]
    modifiers: augustus_core::Modifiers,
    #[serde(
        default,
        skip_serializing_if = "augustus_core::Justifications::is_empty"
    )]
    justifications: augustus_core::Justifications,
    #[serde(default, skip_serializing_if = "augustus_core::Markers::is_empty")]
    markers: augustus_core::Markers,
    #[serde(
        default,
        skip_serializing_if = "augustus_core::GlobalMarkers::is_empty"
    )]
    global_markers: augustus_core::GlobalMarkers,
    #[serde(default, skip_serializing_if = "augustus_core::Governments::is_empty")]
    governments: augustus_core::Governments,
    #[serde(default, skip_serializing_if = "augustus_core::Metrics::is_empty")]
    metrics: augustus_core::Metrics,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    area_scopes: BTreeMap<String, Vec<u32>>,
    #[serde(default, skip_serializing_if = "augustus_core::Affiliations::is_empty")]
    affiliations: augustus_core::Affiliations,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    country_controllers: BTreeMap<EntityId, augustus_core::CountryController>,
    #[serde(
        default,
        skip_serializing_if = "augustus_core::Administration::is_empty"
    )]
    administration: augustus_core::Administration,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    known_absent_targets: BTreeMap<EntityId, std::collections::BTreeSet<augustus_core::Target>>,
    #[serde(default, skip_serializing_if = "augustus_core::Attributes::is_empty")]
    attributes: augustus_core::Attributes,
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
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, augustus_core::AttributeEvaluation>,
}

#[derive(Debug, Serialize)]
pub struct PendingChoiceView {
    pub id: PendingEventId,
    pub event: String,
    pub current: EntityId,
    pub day: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture: Option<String>,
    pub options: Vec<String>,
    /// Declared duration labels in available options; effect conditions are not evaluated.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub modifier_duration_labels: BTreeMap<String, Vec<String>>,
    /// Declared non-hidden keys in available options; effect conditions are not evaluated.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub description_keys: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct EventView {
    #[serde(skip_serializing_if = "augustus_core::Subjects::is_empty")]
    pub subjects: augustus_core::Subjects,
    pub day: u64,
    pub pending_tasks: usize,
    pub entities: Vec<EventEntityView>,
    pub pending_events: Vec<PendingChoiceView>,
    pub log: Vec<(u64, EntityId, String)>,
    #[serde(skip_serializing_if = "augustus_core::Completions::is_empty")]
    pub completions: augustus_core::Completions,
    #[serde(skip_serializing_if = "augustus_core::Objectives::is_empty")]
    pub objectives: augustus_core::Objectives,
    #[serde(skip_serializing_if = "augustus_core::NamedValues::is_empty")]
    pub named_values: augustus_core::NamedValues,
    #[serde(skip_serializing_if = "augustus_core::Modifiers::is_empty")]
    pub modifiers: augustus_core::Modifiers,
    #[serde(skip_serializing_if = "augustus_core::Justifications::is_empty")]
    pub justifications: augustus_core::Justifications,
    #[serde(skip_serializing_if = "augustus_core::Markers::is_empty")]
    pub markers: augustus_core::Markers,
    #[serde(skip_serializing_if = "augustus_core::GlobalMarkers::is_empty")]
    pub global_markers: augustus_core::GlobalMarkers,
    #[serde(skip_serializing_if = "augustus_core::Governments::is_empty")]
    pub governments: augustus_core::Governments,
    #[serde(skip_serializing_if = "augustus_core::Metrics::is_empty")]
    pub metrics: augustus_core::Metrics,
    #[serde(skip_serializing_if = "augustus_core::Affiliations::is_empty")]
    pub affiliations: augustus_core::Affiliations,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub country_controllers: BTreeMap<EntityId, augustus_core::CountryController>,
    #[serde(skip_serializing_if = "augustus_core::Administration::is_empty")]
    pub administration: augustus_core::Administration,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub known_absent_targets: BTreeMap<EntityId, std::collections::BTreeSet<augustus_core::Target>>,
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
        let (definitions, conditions, once_events, option_conditions, pictures) = match input {
            Input::Native => (
                serde_json::from_str(include_str!("../../../examples/events/events.json"))?,
                BTreeMap::new(),
                Default::default(),
                BTreeMap::new(),
                BTreeMap::new(),
            ),
            Input::Eu4 => {
                let schema = Schema::new(scene.fields.clone())?;
                let mut library = ScriptLibrary::new();
                library.add_effects(&document(
                    "demo/effects.txt",
                    include_str!("../../../examples/events/eu4/common/scripted_effects/demo.txt"),
                ))?;
                let compiled = ScriptCompiler::new(&schema, &library, true)
                    .with_subject_type_ids(scene.subjects.types.clone())
                    .with_government_metric_ids(scene.governments.metrics.keys().cloned().collect())
                    .with_metric_ids(scene.metrics.definitions.keys().cloned().collect())
                    .with_area_scopes(scene.area_scopes.clone())
                    .with_modifier_ids(scene.modifiers.definitions.keys().cloned().collect())
                    .with_religious_school_groups(
                        scene
                            .affiliations
                            .0
                            .get("religious_school")
                            .map(|data| data.groups.clone())
                            .unwrap_or_default(),
                    )
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
                    compiled.option_conditions,
                    compiled.pictures,
                )
            }
        };
        Self::load(
            scene,
            definitions,
            conditions,
            once_events,
            option_conditions,
            pictures,
        )
    }

    pub fn from_directory(root: &Path, input: Input) -> Result<Self, Error> {
        let root = ContentRoot::open(root)?;
        let read_json = |name| -> Result<String, Error> {
            let document = root.read_text(name, TextEncoding::Utf8)?;
            std::fs::read_to_string(document.source())
                .map_err(|error| Error::Invalid(error.to_string()))
        };
        let scene: Scene = serde_json::from_str(&read_json("scene.json")?)?;
        let (definitions, conditions, once_events, option_conditions, pictures) = match input {
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
                let option_conditions = if root.path().join("option_conditions.json").exists() {
                    serde_json::from_str(&read_json("option_conditions.json")?)?
                } else {
                    BTreeMap::new()
                };
                let pictures = if root.path().join("pictures.json").exists() {
                    serde_json::from_str(&read_json("pictures.json")?)?
                } else {
                    BTreeMap::new()
                };
                (
                    serde_json::from_str(&read_json("events.json")?)?,
                    conditions,
                    once_events,
                    option_conditions,
                    pictures,
                )
            }
            Input::Eu4 => {
                let content = ContentSet::resolve(vec![ContentSource::base(ContentRoot::open(
                    root.path().join("eu4"),
                )?)])?;
                let library = ScriptLibrary::from_content(&content, TextEncoding::Utf8)?;
                let schema = Schema::new(scene.fields.clone())?;
                let casus_belli = augustus_eu4::CasusBelliTypes::from_content(
                    &content,
                    TextEncoding::Utf8,
                    &schema,
                    &library,
                )?;
                if casus_belli.definitions() != scene.justifications.definitions {
                    return Err(Error::Invalid(
                        "translated justification definitions do not match the scene".into(),
                    ));
                }
                let compiled = ScriptCompiler::new(&schema, &library, true)
                    .with_subject_type_ids(scene.subjects.types.clone())
                    .with_casus_belli_types(casus_belli.types)
                    .with_government_metric_ids(scene.governments.metrics.keys().cloned().collect())
                    .with_metric_ids(scene.metrics.definitions.keys().cloned().collect())
                    .with_area_scopes(scene.area_scopes.clone())
                    .with_modifier_ids(scene.modifiers.definitions.keys().cloned().collect())
                    .with_religious_school_groups(
                        scene
                            .affiliations
                            .0
                            .get("religious_school")
                            .map(|data| data.groups.clone())
                            .unwrap_or_default(),
                    )
                    .with_mission_ids(scene.completions.identifiers.clone())
                    .with_global_scope_types(scene.global_scope_types.clone())
                    .compile_events(&content.read_text("events/demo.txt", TextEncoding::Utf8)?)?;
                (
                    compiled.definitions,
                    compiled.conditions,
                    compiled.once_events,
                    compiled.option_conditions,
                    compiled.pictures,
                )
            }
        };
        Self::load(
            scene,
            definitions,
            conditions,
            once_events,
            option_conditions,
            pictures,
        )
    }

    fn load(
        scene: Scene,
        definitions: Vec<EventDefinition>,
        conditions: BTreeMap<String, augustus_core::Condition>,
        once_events: std::collections::BTreeSet<String>,
        option_conditions: BTreeMap<String, BTreeMap<String, augustus_core::Condition>>,
        pictures: BTreeMap<String, augustus_core::EventPictures>,
    ) -> Result<Self, Error> {
        if scene.version != 1 || scene.entities.is_empty() {
            return Err(Error::Invalid(
                "expected event scene version 1 and entities".into(),
            ));
        }
        let catalog = EventCatalog::new(definitions)?
            .with_conditions(conditions)?
            .with_option_conditions(option_conditions)?
            .with_pictures(pictures)?
            .with_once_events(once_events)?;
        // Canonical effective content, including initial state and compiled plans.
        let content = if !catalog.pictures().is_empty() {
            serde_json::to_string(&(
                "augustus-events:1",
                &scene,
                catalog.definitions(),
                catalog.conditions(),
                catalog.once_events(),
                catalog.option_conditions(),
                catalog.pictures(),
            ))?
        } else if !catalog.option_conditions().is_empty() {
            serde_json::to_string(&(
                "augustus-events:1",
                &scene,
                catalog.definitions(),
                catalog.conditions(),
                catalog.once_events(),
                catalog.option_conditions(),
            ))?
        } else if !catalog.once_events().is_empty() {
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
        let mut politics = scene.politics;
        for (name, holder) in scene.offices {
            if politics.offices.insert(name, holder).is_some() {
                return Err(Error::Invalid("duplicate scene political office".into()));
            }
        }
        if !politics.is_empty() {
            world.set_politics(politics)?;
        }
        world.set_subjects(scene.subjects)?;
        world.set_completions(scene.completions)?;
        world.set_named_values(scene.named_values)?;
        world.set_markers(scene.markers)?;
        world.set_global_markers(scene.global_markers)?;
        world.set_modifiers(scene.modifiers)?;
        world.set_governments(scene.governments)?;
        world.set_metrics(scene.metrics)?;
        world.set_justifications(scene.justifications)?;
        world.set_objectives(scene.objectives)?;
        world.set_affiliations(scene.affiliations)?;
        for (country, controller) in scene.country_controllers {
            world.set_country_controller(country, controller)?;
        }
        world.set_administration(scene.administration)?;
        world.set_attributes(scene.attributes)?;
        for (current, targets) in scene.known_absent_targets {
            if targets.is_empty() {
                return Err(Error::Invalid(
                    "empty absent relationship declaration".into(),
                ));
            }
            for target in targets {
                world.mark_target_absent(current, target)?;
            }
        }
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
            || restored.objectives().groups != self.world.objectives().groups
            || restored.justifications().definitions != self.world.justifications().definitions
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
                    attributes: self
                        .world
                        .attributes()
                        .formulas
                        .get(&EntityId(index))
                        .into_iter()
                        .flat_map(|formulas| formulas.keys())
                        .map(|name| {
                            self.world
                                .attributes()
                                .evaluate(&self.world, EntityId(index), name)
                                .map(|value| (name.clone(), value))
                        })
                        .collect::<Result<BTreeMap<_, _>, augustus_core::Error>>()?,
                })
            })
            .collect::<Result<Vec<_>, augustus_core::Error>>()?;
        let pending_events = self
            .world
            .pending_events()
            .iter()
            .map(|pending| {
                let options = self.world.available_event_options(pending.id)?;
                let mut modifier_duration_labels = BTreeMap::new();
                let mut description_keys = BTreeMap::new();
                for option in &options {
                    let mut durations = Vec::new();
                    let mut descriptions = Vec::new();
                    effect_labels(&option.effects, &mut durations, &mut descriptions);
                    if !durations.is_empty() {
                        modifier_duration_labels.insert(option.id.clone(), durations);
                    }
                    if !descriptions.is_empty() {
                        description_keys.insert(option.id.clone(), descriptions);
                    }
                }
                Ok(PendingChoiceView {
                    id: pending.id,
                    event: pending.event.clone(),
                    current: pending.current,
                    day: pending.day,
                    picture: self
                        .world
                        .pending_event_picture(pending.id)?
                        .map(str::to_owned),
                    options: options.iter().map(|option| option.id.clone()).collect(),
                    modifier_duration_labels,
                    description_keys,
                })
            })
            .collect::<Result<Vec<_>, augustus_core::Error>>()?;
        Ok(EventView {
            subjects: self.world.subjects().clone(),
            day: self.world.day(),
            pending_tasks: self.world.pending_tasks(),
            entities,
            pending_events,
            log: self.world.events().to_vec(),
            completions: self.world.completions().clone(),
            objectives: self.world.objectives().clone(),
            named_values: self.world.named_values().clone(),
            modifiers: self.world.modifiers().clone(),
            justifications: self.world.justifications().clone(),
            markers: self.world.markers().clone(),
            global_markers: self.world.global_markers().clone(),
            governments: self.world.governments().clone(),
            metrics: self.world.metrics().clone(),
            affiliations: self.world.affiliations().clone(),
            country_controllers: self.world.country_controllers().clone(),
            administration: self.world.administration().clone(),
            known_absent_targets: self.world.known_absent_targets().clone(),
        })
    }
}

fn effect_labels(
    plan: &[augustus_core::Action],
    durations: &mut Vec<String>,
    descriptions: &mut Vec<String>,
) {
    use augustus_core::Action;
    for action in plan {
        match action {
            Action::Description { key, hidden: false } => descriptions.push(key.clone()),
            Action::AttachModifier {
                duration_label: Some(label),
                ..
            } => durations.push(label.clone()),
            Action::If {
                then, otherwise, ..
            }
            | Action::RandomBranch { then, otherwise } => {
                effect_labels(then, durations, descriptions);
                effect_labels(otherwise, durations, descriptions);
            }
            Action::With { plan, .. }
            | Action::InScope { plan, .. }
            | Action::ForEach { plan, .. }
            | Action::ForEachOwnedProvince { plan, .. }
            | Action::RandomOwnedProvince { plan, .. }
            | Action::Schedule { plan, .. }
            | Action::Preview { plan } => effect_labels(plan, durations, descriptions),
            _ => (),
        }
    }
}

fn document(name: &str, text: &str) -> TextDocument {
    TextDocument::from_bytes(name, text.as_bytes().to_vec(), TextEncoding::Utf8)
}
