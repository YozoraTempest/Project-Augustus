// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Self-authored construction and event scenes using the shared simulation core.
//! Its JSON format and extension trait are experimental, not a released mod SDK.

#![forbid(unsafe_code)]

mod construction;
mod events;

use std::path::Path;

use augustus_core::{Action, EntityId, EntityKind, FieldId, Plan, Schema, Target, World};
use augustus_eu4::{
    ContentRoot, ContentSet, ContentSource, ScriptCompiler, ScriptLibrary, TextDocument,
    TextEncoding,
};
use serde::{Deserialize, Serialize};

pub use construction::{CapacityConstruction, ConstructionSystem, FixedConstruction, Slots};
pub use events::{EventDemo, EventEntityView, EventView, PendingChoiceView};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Core(#[from] augustus_core::Error),
    #[error(transparent)]
    Content(#[from] augustus_eu4::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Invalid(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    Eu4,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum System {
    Fixed,
    Capacity,
}

impl System {
    fn provider(self) -> Box<dyn ConstructionSystem> {
        match self {
            Self::Fixed => Box::new(FixedConstruction),
            Self::Capacity => Box::new(CapacityConstruction),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Scene {
    version: u32,
    treasury_milli: i64,
    provinces: Vec<Province>,
    buildings: Vec<BuildingInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Province {
    name: String,
    capacity_milli_per_day: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BuildingInput {
    id: String,
    name: String,
    cost_milli: i64,
    work_milli: i64,
    standard_days: u64,
    completion: Vec<NativeEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "effect", rename_all = "snake_case", deny_unknown_fields)]
enum NativeEffect {
    OwnerTreasury { amount_milli: i64 },
    Variable { name: String, amount_milli: i64 },
    Log { message: String },
}

#[derive(Debug, Clone, Serialize)]
pub struct Building {
    pub id: String,
    pub name: String,
    pub cost_milli: i64,
    pub work_milli: i64,
    pub standard_days: u64,
    pub built_field: FieldId,
    pub completion: Plan,
}

#[derive(Debug, Serialize)]
pub struct ProvinceView {
    pub id: usize,
    pub name: String,
    pub capacity_milli_per_day: i64,
    pub progress_milli: i64,
    pub queued_building: Option<usize>,
    pub buildings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct BuildingView {
    pub id: String,
    pub name: String,
    pub cost_milli: i64,
    pub work_milli: i64,
    pub standard_days: u64,
}

#[derive(Debug, Serialize)]
pub struct View {
    pub day: u64,
    pub treasury_milli: i64,
    pub construction_system: String,
    pub input: String,
    pub provinces: Vec<ProvinceView>,
    pub buildings: Vec<BuildingView>,
    pub pending_tasks: usize,
    pub events: Vec<(u64, usize, String)>,
}

pub struct Demo {
    world: World,
    slots: Slots,
    buildings: Vec<Building>,
    country: EntityId,
    provinces: Vec<EntityId>,
    system: Box<dyn ConstructionSystem>,
    content: String,
    input: Input,
}

impl Demo {
    /// Load only this self-authored demonstration's supported files.
    pub fn from_directory(
        root: &Path,
        input: Input,
        system: System,
        optimise: bool,
    ) -> Result<Self, Error> {
        Self::from_directory_with_system(root, input, system.provider(), optimise)
    }

    pub fn from_directory_with_system(
        root: &Path,
        input: Input,
        system: Box<dyn ConstructionSystem>,
        optimise: bool,
    ) -> Result<Self, Error> {
        let root = ContentRoot::open(root)?;
        let scene = root.read_text("scene.json", TextEncoding::Utf8)?;
        let scene: Scene = serde_json::from_slice(
            &std::fs::read(scene.source()).map_err(|error| Error::Invalid(error.to_string()))?,
        )?;
        let library = if input == Input::Eu4 {
            let content = ContentSet::resolve(vec![ContentSource::base(ContentRoot::open(
                root.path().join("eu4"),
            )?)])?;
            ScriptLibrary::from_content(&content, TextEncoding::Utf8)?
        } else {
            ScriptLibrary::new()
        };
        Self::load(scene, input, system, optimise, &library, |id| {
            root.read_text(format!("eu4/hooks/{id}.txt"), TextEncoding::Utf8)
                .map_err(Error::from)
        })
    }

    /// Embedded files use the exact same compiler and core as the directory input.
    pub fn embedded(input: Input, system: System) -> Result<Self, Error> {
        Self::embedded_with_system(input, system.provider())
    }

    pub fn embedded_with_system(
        input: Input,
        system: Box<dyn ConstructionSystem>,
    ) -> Result<Self, Error> {
        let scene: Scene =
            serde_json::from_str(include_str!("../../../examples/construction/scene.json"))?;
        let document = |name: &str, text: &str| {
            TextDocument::from_bytes(name, text.as_bytes().to_vec(), TextEncoding::Utf8)
        };
        let mut library = ScriptLibrary::new();
        if input == Input::Eu4 {
            library.add_effects(&document(
                "demo/effects.txt",
                include_str!("../../../examples/construction/eu4/common/scripted_effects/demo.txt"),
            ))?;
            library.add_triggers(&document(
                "demo/triggers.txt",
                include_str!(
                    "../../../examples/construction/eu4/common/scripted_triggers/demo.txt"
                ),
            ))?;
        }
        Self::load(scene, input, system, true, &library, |id| {
            let text = match id {
                "market" => include_str!("../../../examples/construction/eu4/hooks/market.txt"),
                "workshop" => include_str!("../../../examples/construction/eu4/hooks/workshop.txt"),
                _ => {
                    return Err(Error::Invalid(format!("unknown embedded building {id}")));
                }
            };
            Ok(document(&format!("demo/{id}.txt"), text))
        })
    }

    fn load(
        scene: Scene,
        input: Input,
        system: Box<dyn ConstructionSystem>,
        optimise: bool,
        library: &ScriptLibrary,
        mut hook: impl FnMut(&str) -> Result<TextDocument, Error>,
    ) -> Result<Self, Error> {
        if scene.version != 1
            || scene.provinces.len() != 2
            || scene.buildings.len() != 2
            || scene.treasury_milli < 0
        {
            return Err(Error::Invalid(
                "expected scene version 1, two provinces and two buildings".into(),
            ));
        }
        let mut fields = vec![
            "treasury".into(),
            "queued".into(),
            "progress".into(),
            "capacity".into(),
            "variable/completed".into(),
        ];
        let mut ids = std::collections::BTreeSet::new();
        for building in &scene.buildings {
            if building.id.is_empty()
                || !building
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
                || !ids.insert(&building.id)
                || building.cost_milli < 0
                || building.work_milli <= 0
                || building.standard_days == 0
            {
                return Err(Error::Invalid(
                    "invalid or duplicate building definition".into(),
                ));
            }
            fields.push(format!("building/{}", building.id));
        }
        let schema = Schema::new(fields)?;
        let slots = Slots {
            treasury: schema.field("treasury").unwrap(),
            queued: schema.field("queued").unwrap(),
            progress: schema.field("progress").unwrap(),
            capacity: schema.field("capacity").unwrap(),
        };
        let mut buildings = Vec::new();
        for definition in &scene.buildings {
            let completion = match input {
                Input::Eu4 => {
                    ScriptCompiler::new(&schema, library, optimise)
                        .compile(&hook(&definition.id)?, EntityKind::Province)?
                        .plan
                }
                Input::Native => definition
                    .completion
                    .iter()
                    .map(|effect| match effect {
                        NativeEffect::OwnerTreasury { amount_milli } => Ok(Action::With {
                            target: Target::Owner,
                            plan: vec![Action::Add {
                                target: Target::Current,
                                field: slots.treasury,
                                value: *amount_milli,
                            }],
                        }),
                        NativeEffect::Variable { name, amount_milli } => Ok(Action::Add {
                            target: Target::Current,
                            field: schema.field(&format!("variable/{name}")).ok_or_else(|| {
                                Error::Invalid(format!("unknown variable {name}"))
                            })?,
                            value: *amount_milli,
                        }),
                        NativeEffect::Log { message } => Ok(Action::Emit(message.clone())),
                    })
                    .collect::<Result<Plan, Error>>()?,
            };
            buildings.push(Building {
                id: definition.id.clone(),
                name: definition.name.clone(),
                cost_milli: definition.cost_milli,
                work_milli: definition.work_milli,
                standard_days: definition.standard_days,
                built_field: schema
                    .field(&format!("building/{}", definition.id))
                    .unwrap(),
                completion,
            });
        }
        let rules = system.rules(&buildings, slots);
        // The exact canonical rules and definitions identify the session, including
        // compiler output and the selected system. No filesystem paths enter saves.
        let content = serde_json::to_string(&(
            "augustus-construction:1",
            &scene,
            &buildings,
            system.id(),
            &rules,
        ))?;
        let mut world = World::with_rules(schema, rules);
        let country = world.add_entity("Augustus", EntityKind::Country);
        world.set(country, slots.treasury, scene.treasury_milli)?;
        let mut provinces = Vec::new();
        for definition in &scene.provinces {
            if definition.capacity_milli_per_day <= 0 {
                return Err(Error::Invalid("province capacity must be positive".into()));
            }
            let province = world.add_entity(&definition.name, EntityKind::Province);
            world.set_owner(province, country)?;
            world.set(province, slots.capacity, definition.capacity_milli_per_day)?;
            provinces.push(province);
        }
        world.set_capital(country, provinces[0])?;
        Ok(Self {
            world,
            slots,
            buildings,
            country,
            provinces,
            system,
            content,
            input,
        })
    }

    pub fn build(&mut self, province: usize, building: usize) -> Result<(), Error> {
        let province = *self
            .provinces
            .get(province)
            .ok_or_else(|| Error::Invalid("unknown province".into()))?;
        let definition = self
            .buildings
            .get(building)
            .ok_or_else(|| Error::Invalid("unknown building".into()))?;
        let plan = self
            .system
            .start(&self.world, province, building, definition, self.slots)?;
        self.world.execute(province, &plan)?;
        Ok(())
    }

    pub fn advance(&mut self, days: u64) -> Result<(), Error> {
        self.world.advance_to(
            self.world
                .day()
                .checked_add(days)
                .ok_or(augustus_core::Error::Overflow)?,
        )?;
        Ok(())
    }

    /// Self-authored test action: choose one of two daily capacities using core RNG.
    pub fn reorganise(&mut self, province: usize) -> Result<(), Error> {
        let province = *self
            .provinces
            .get(province)
            .ok_or_else(|| Error::Invalid("unknown province".into()))?;
        let plan = self.system.reorganise(self.slots)?;
        self.world.execute(province, &plan)?;
        Ok(())
    }

    pub fn save(&self) -> Result<String, Error> {
        Ok(self.world.save(&self.content)?)
    }

    pub fn restore(&mut self, json: &str) -> Result<(), Error> {
        let world = World::restore_with_rules(
            json,
            &self.content,
            self.world.schema(),
            self.system.rules(&self.buildings, self.slots),
        )?;
        if world.entities().len() != 3 || world.entity(self.country)?.kind != EntityKind::Country {
            return Err(Error::Invalid("invalid demonstration world".into()));
        }
        if world.entity(self.country)?.capital != Some(self.provinces[0]) {
            return Err(Error::Invalid("invalid demonstration capital".into()));
        }
        let mut queues = 0;
        for province in &self.provinces {
            if world.entity(*province)?.kind != EntityKind::Province
                || world.entity(*province)?.owner != Some(self.country)
            {
                return Err(Error::Invalid("invalid demonstration province".into()));
            }
            let queued = world.value(*province, self.slots.queued)?;
            if queued < 0
                || queued > self.buildings.len() as i64
                || world.value(*province, self.slots.capacity)? <= 0
                || world.value(*province, self.slots.progress)? < 0
            {
                return Err(Error::Invalid("invalid construction state".into()));
            }
            for (index, building) in self.buildings.iter().enumerate() {
                let built = world.value(*province, building.built_field)?;
                if !(0..=1).contains(&built) || (queued == index as i64 + 1 && built == 1) {
                    return Err(Error::Invalid("invalid building state".into()));
                }
            }
            queues += usize::from(queued != 0);
        }
        if world.pending_tasks() != queues {
            return Err(Error::Invalid(
                "construction queue and tasks disagree".into(),
            ));
        }
        self.world = world;
        Ok(())
    }

    pub fn view(&self) -> Result<View, Error> {
        let provinces = self
            .provinces
            .iter()
            .enumerate()
            .map(|(id, province)| {
                let queued = self.world.value(*province, self.slots.queued)?;
                Ok(ProvinceView {
                    id,
                    name: self.world.entity(*province)?.name.clone(),
                    capacity_milli_per_day: self.world.value(*province, self.slots.capacity)?,
                    progress_milli: self.world.value(*province, self.slots.progress)?,
                    queued_building: if queued == 0 {
                        None
                    } else {
                        Some(
                            usize::try_from(queued - 1)
                                .map_err(|_| Error::Invalid("invalid queued building".into()))?,
                        )
                    },
                    buildings: self
                        .buildings
                        .iter()
                        .filter_map(|building| {
                            match self.world.value(*province, building.built_field) {
                                Ok(1) => Some(Ok(building.id.clone())),
                                Ok(0) => None,
                                Ok(_) => Some(Err(Error::Invalid("invalid building flag".into()))),
                                Err(error) => Some(Err(error.into())),
                            }
                        })
                        .collect::<Result<Vec<_>, Error>>()?,
                })
            })
            .collect::<Result<Vec<_>, Error>>()?;
        Ok(View {
            day: self.world.day(),
            treasury_milli: self.world.value(self.country, self.slots.treasury)?,
            construction_system: self.system.id().into(),
            input: match self.input {
                Input::Eu4 => "eu4",
                Input::Native => "native",
            }
            .into(),
            provinces,
            buildings: self
                .buildings
                .iter()
                .map(|building| BuildingView {
                    id: building.id.clone(),
                    name: building.name.clone(),
                    cost_milli: building.cost_milli,
                    work_milli: building.work_milli,
                    standard_days: building.standard_days,
                })
                .collect(),
            pending_tasks: self.world.pending_tasks(),
            events: self
                .world
                .events()
                .iter()
                .map(|(day, entity, text)| (*day, entity.0, text.clone()))
                .collect(),
        })
    }
}
