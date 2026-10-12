// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use augustus_core::{
    Action, Condition, EntityKind, EventDefinition, EventOption, FieldId, Plan, Schema,
    ScopeSelector, Target,
};
use jomini::{
    TextToken,
    text::{ObjectReader, Operator},
};

use crate::{
    ContentSet, DefinitionKind, Error, ParsedText, SourceLocation, TextDocument, TextEncoding,
};

#[derive(Debug, Clone)]
struct Field {
    name: String,
    value: Value,
    path: PathBuf,
    location: SourceLocation,
}

#[derive(Debug, Clone)]
enum Value {
    Scalar(String),
    Block(Vec<Field>),
}

impl Field {
    fn error(&self, message: impl Into<String>) -> Error {
        Error::message(&self.path, message).with_location(self.location)
    }
    fn scalar(&self) -> Result<&str, Error> {
        match &self.value {
            Value::Scalar(value) => Ok(value),
            _ => Err(self.error("expected scalar")),
        }
    }
    fn block(&self) -> Result<&[Field], Error> {
        match &self.value {
            Value::Block(fields) => Ok(fields),
            _ => Err(self.error("expected block")),
        }
    }
}

fn read_fields(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    depth: usize,
) -> Result<Vec<Field>, Error> {
    if depth > 64 {
        return Err(Error::message(parsed.source(), "script nesting exceeds 64"));
    }
    reader
        .fields()
        .map(|(key, operator, value)| {
            let location = parsed.field_location(&key);
            let fail = |message| Error::message(parsed.source(), message).with_location(location);
            if operator.is_some_and(|operator| operator != Operator::Equal) {
                return Err(fail("unsupported script operator"));
            }
            let value = match value.token() {
                TextToken::Unquoted(_) | TextToken::Quoted(_) => {
                    Value::Scalar(value.read_string().map_err(|_| fail("invalid scalar"))?)
                }
                TextToken::Object { mixed: false, .. } => Value::Block(read_fields(
                    parsed,
                    value.read_object().map_err(|_| fail("invalid block"))?,
                    depth + 1,
                )?),
                TextToken::Array { mixed: false, .. }
                    if value
                        .read_array()
                        .is_ok_and(|array| array.values().next().is_none()) =>
                {
                    Value::Block(Vec::new())
                }
                _ => return Err(fail("unsupported script value")),
            };
            Ok(Field {
                name: key.read_string(),
                value,
                path: parsed.source().into(),
                location,
            })
        })
        .collect()
}

/// Ordered, explicitly supplied declarations. The first same-name script is retained.
/// File enumeration is the caller's responsibility; this is not a full-game loader.
#[derive(Debug, Default)]
pub struct ScriptLibrary {
    effects: BTreeMap<String, Field>,
    triggers: BTreeMap<String, Field>,
    ignored: Vec<(String, PathBuf, SourceLocation)>,
}

impl ScriptLibrary {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the supported script categories from selected effective files.
    /// Only direct .txt children participate; identifiers retain their spelling.
    pub fn from_content(content: &ContentSet, encoding: TextEncoding) -> Result<Self, Error> {
        let mut library = Self::new();
        for (kind, triggers) in [
            (DefinitionKind::ScriptedEffects, false),
            (DefinitionKind::ScriptedTriggers, true),
        ] {
            for (logical_path, _) in Self::files(content, kind)? {
                library.add(&content.read_text(logical_path, encoding)?, triggers)?;
            }
        }
        Ok(library)
    }

    /// Execution order differs from the catalogue's normalized path order.
    /// This view is restricted to scripted effects and scripted triggers.
    pub fn files(
        content: &ContentSet,
        kind: DefinitionKind,
    ) -> Result<Vec<(&str, &crate::FileOrigin)>, Error> {
        if kind == DefinitionKind::Buildings {
            return Err(Error::message(
                "common/buildings",
                "building definition binding is not implemented",
            ));
        }
        let prefix = format!("{}/", kind.directory());
        let mut files = content
            .effective_files()
            .filter(|(path, _)| {
                path.strip_prefix(&prefix)
                    .is_some_and(|name| !name.contains('/') && name.ends_with(".txt"))
            })
            .collect::<Vec<_>>();
        files.sort_by_cached_key(|(_, origin)| {
            origin
                .relative_path
                .file_name()
                .and_then(|name| name.to_str())
                .expect("manifest validated UTF-8 filenames")
                .as_bytes()
                .to_vec()
        });
        Ok(files)
    }
    pub fn add_effects(&mut self, document: &TextDocument) -> Result<(), Error> {
        self.add(document, false)
    }
    pub fn add_triggers(&mut self, document: &TextDocument) -> Result<(), Error> {
        self.add(document, true)
    }
    pub fn ignored_definitions(&self) -> &[(String, PathBuf, SourceLocation)] {
        &self.ignored
    }
    fn add(&mut self, document: &TextDocument, triggers: bool) -> Result<(), Error> {
        let parsed = document.parse()?;
        let fields = read_fields(&parsed, parsed.reader(), 0)?;
        let destination = if triggers {
            &mut self.triggers
        } else {
            &mut self.effects
        };
        for field in fields {
            field.block()?;
            if destination.contains_key(&field.name) {
                self.ignored.push((field.name, field.path, field.location));
            } else {
                destination.insert(field.name.clone(), field);
            }
        }
        Ok(())
    }
}

#[derive(Debug)]
pub struct CompiledScript {
    pub plan: Plan,
    /// Every compiled input field, including fields reached through script calls.
    pub sources: Vec<(PathBuf, SourceLocation)>,
    pub folded_branches: usize,
}

#[derive(Debug)]
pub struct CompiledEvents {
    pub definitions: Vec<EventDefinition>,
    pub conditions: BTreeMap<String, Condition>,
    pub once_events: BTreeSet<String>,
    pub sources: Vec<(PathBuf, SourceLocation)>,
    pub folded_branches: usize,
}

pub struct ScriptCompiler<'a> {
    schema: &'a Schema,
    library: &'a ScriptLibrary,
    optimise: bool,
    stack: Vec<String>,
    sources: Vec<(PathBuf, SourceLocation)>,
    folded: usize,
    global_scope_types: BTreeMap<String, EntityKind>,
    mission_ids: BTreeSet<String>,
    modifier_ids: BTreeSet<String>,
    government_metric_ids: BTreeSet<String>,
}

impl<'a> ScriptCompiler<'a> {
    pub fn new(schema: &'a Schema, library: &'a ScriptLibrary, optimise: bool) -> Self {
        Self {
            schema,
            library,
            optimise,
            stack: Vec::new(),
            sources: Vec::new(),
            folded: 0,
            global_scope_types: BTreeMap::new(),
            mission_ids: BTreeSet::new(),
            modifier_ids: BTreeSet::new(),
            government_metric_ids: BTreeSet::new(),
        }
    }

    /// Types come from selected content declarations, not current target values.
    pub fn with_global_scope_types(mut self, types: BTreeMap<String, EntityKind>) -> Self {
        self.global_scope_types = types;
        self
    }

    fn variable_slot(
        &self,
        field: &Field,
        name: &str,
    ) -> Result<augustus_core::NumericSlot, Error> {
        if name.is_empty() {
            return Err(field.error("empty variable name"));
        }
        Ok(match self.schema.field(&format!("variable/{name}")) {
            Some(field) => augustus_core::NumericSlot::Field(field),
            None if name.chars().any(|c| matches!(c, '$' | '@' | ':' | '.')) => {
                return Err(field.error("unsupported dynamic variable name"));
            }
            None => augustus_core::NumericSlot::Named(name.into()),
        })
    }

    pub(crate) fn compile_numeric_condition_reader(
        mut self,
        parsed: &ParsedText<'_>,
        name: &str,
        reader: ObjectReader<'_, '_, TextEncoding>,
        location: SourceLocation,
        scope: EntityKind,
    ) -> Result<Condition, Error> {
        let Condition::All(mut conditions) = self.conditions(
            &[Field {
                name: name.into(),
                value: Value::Block(read_fields(parsed, reader, 1)?),
                path: parsed.source().into(),
                location,
            }],
            scope,
        )?
        else {
            unreachable!()
        };
        Ok(conditions.pop().expect("one numeric condition"))
    }

    pub fn with_mission_ids(mut self, ids: BTreeSet<String>) -> Self {
        self.mission_ids = ids;
        self
    }

    pub fn with_modifier_ids(mut self, ids: BTreeSet<String>) -> Self {
        self.modifier_ids = ids;
        self
    }

    pub fn with_government_metric_ids(mut self, ids: BTreeSet<String>) -> Self {
        self.government_metric_ids = ids;
        self
    }

    fn government_metric(&self, field: &Field, scope: EntityKind, name: &str) -> Result<(), Error> {
        if scope != EntityKind::Country {
            return Err(field.error("government metrics require country scope"));
        }
        if !self.government_metric_ids.contains(name) {
            return Err(field.error(format!("unregistered government metric {name}")));
        }
        Ok(())
    }

    pub(crate) fn compile_scalar_effect_reader(
        mut self,
        parsed: &ParsedText<'_>,
        name: &str,
        value: String,
        location: SourceLocation,
        scope: EntityKind,
    ) -> Result<Plan, Error> {
        self.effects(
            &[Field {
                name: name.into(),
                value: Value::Scalar(value),
                path: parsed.source().into(),
                location,
            }],
            scope,
        )
    }

    pub(crate) fn compile_scalar_condition_reader(
        mut self,
        parsed: &ParsedText<'_>,
        name: &str,
        value: String,
        location: SourceLocation,
        scope: EntityKind,
    ) -> Result<Condition, Error> {
        self.conditions(
            &[Field {
                name: name.into(),
                value: Value::Scalar(value),
                path: parsed.source().into(),
                location,
            }],
            scope,
        )
    }

    fn country_flag_name(&self, field: &Field, scope: EntityKind) -> Result<String, Error> {
        if scope != EntityKind::Country {
            return Err(field.error("country flags require country scope"));
        }
        let name = field.scalar()?;
        if !crate::country_markers::literal_name(name) {
            return Err(field.error("country flag requires a literal name"));
        }
        Ok(name.into())
    }

    fn mission_id(&self, field: &Field, scope: EntityKind) -> Result<String, Error> {
        if scope != EntityKind::Country {
            return Err(field.error("mission records require country scope"));
        }
        let id = field.scalar()?;
        if id.is_empty() || !self.mission_ids.contains(id) {
            return Err(field.error(format!("unknown or undeclared mission identifier {id}")));
        }
        Ok(id.into())
    }

    pub fn compile(
        mut self,
        document: &TextDocument,
        scope: EntityKind,
    ) -> Result<CompiledScript, Error> {
        let parsed = document.parse()?;
        let fields = read_fields(&parsed, parsed.reader(), 0)?;
        let plan = self.effects(&fields, scope)?;
        Ok(CompiledScript {
            plan,
            sources: self.sources,
            folded_branches: self.folded,
        })
    }

    pub(crate) fn compile_effect_reader(
        mut self,
        parsed: &ParsedText<'_>,
        name: &str,
        reader: ObjectReader<'_, '_, TextEncoding>,
        location: SourceLocation,
        scope: EntityKind,
    ) -> Result<Plan, Error> {
        self.effects(
            &[Field {
                name: name.into(),
                value: Value::Block(read_fields(parsed, reader, 1)?),
                path: parsed.source().into(),
                location,
            }],
            scope,
        )
    }

    /// Compile explicitly dispatched events with literal IDs and unconditional options.
    /// Semantic fields outside this subset are errors, including automatic selection.
    pub fn compile_events(mut self, document: &TextDocument) -> Result<CompiledEvents, Error> {
        let parsed = document.parse()?;
        let fields = read_fields(&parsed, parsed.reader(), 0)?;
        self.events(&fields)
    }

    /// Only selected IDs are compiled; unrelated definitions are not bound content.
    pub(crate) fn compile_selected_events(
        mut self,
        document: &TextDocument,
        selected: &BTreeSet<String>,
    ) -> Result<CompiledEvents, Error> {
        let parsed = document.parse()?;
        let mut fields = Vec::new();
        for (key, operator, value) in parsed.reader().fields() {
            if !matches!(
                key.read_string().as_str(),
                "country_event" | "province_event"
            ) {
                continue;
            }
            let location = parsed.field_location(&key);
            let body = value.read_object().map_err(|_| {
                Error::message(parsed.source(), "expected event block").with_location(location)
            })?;
            let ids = body
                .fields()
                .filter(|(key, _, _)| key.read_string() == "id")
                .map(|(_, _, value)| value.read_string())
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| {
                    Error::message(parsed.source(), "invalid event identifier")
                        .with_location(location)
                })?;
            if ids.iter().any(|id| selected.contains(id)) {
                if operator.is_some_and(|operator| operator != Operator::Equal) {
                    return Err(Error::message(
                        parsed.source(),
                        "unsupported event declaration operator",
                    )
                    .with_location(location));
                }
                fields.push(Field {
                    name: key.read_string(),
                    value: Value::Block(read_fields(&parsed, body, 1)?),
                    path: parsed.source().into(),
                    location,
                });
            }
        }
        self.events(&fields)
    }

    fn events(&mut self, fields: &[Field]) -> Result<CompiledEvents, Error> {
        let mut definitions = Vec::new();
        let mut conditions = BTreeMap::new();
        let mut once_events = BTreeSet::new();
        for field in fields {
            self.sources.push((field.path.clone(), field.location));
            let scope = match field.name.as_str() {
                "namespace" => {
                    if field.scalar()?.is_empty() {
                        return Err(field.error("empty event namespace"));
                    }
                    continue;
                }
                "country_event" => EntityKind::Country,
                "province_event" => EntityKind::Province,
                _ => {
                    return Err(
                        field.error(format!("unsupported event declaration {}", field.name))
                    );
                }
            };
            let mut definition = EventDefinition {
                id: String::new(),
                scope,
                immediate: Vec::new(),
                options: Vec::new(),
                after: Vec::new(),
            };
            let mut seen = std::collections::BTreeSet::new();
            let mut explicit = false;
            let mut condition = None;
            let mut once = false;
            for part in field.block()? {
                self.sources.push((part.path.clone(), part.location));
                if part.name != "option" && !seen.insert(&part.name) {
                    return Err(part.error(format!("duplicate event field {}", part.name)));
                }
                match part.name.as_str() {
                    "id" => definition.id = part.scalar()?.into(),
                    "is_triggered_only" => {
                        explicit = boolean(part)?;
                        if !explicit {
                            return Err(part.error("automatic event dispatch is not supported"));
                        }
                    }
                    "fire_only_once" => once = boolean(part)?,
                    "title" | "desc" | "picture" => {
                        part.scalar()?;
                    }
                    "immediate" => definition.immediate = self.effects(part.block()?, scope)?,
                    "trigger" => condition = Some(self.conditions(part.block()?, scope)?),
                    "after" => definition.after = self.effects(part.block()?, scope)?,
                    "option" => {
                        let mut effects = Vec::new();
                        let mut name = false;
                        for item in part.block()? {
                            if item.name == "name" {
                                if name || item.scalar()?.is_empty() {
                                    return Err(item.error("invalid option name"));
                                }
                                self.sources.push((item.path.clone(), item.location));
                                name = true;
                            } else {
                                effects.push(item.clone());
                            }
                        }
                        if !name {
                            return Err(part.error("option requires a scalar name"));
                        }
                        definition.options.push(EventOption {
                            id: definition.options.len().to_string(),
                            effects: self.effects(&effects, scope)?,
                        });
                    }
                    _ => return Err(part.error(format!("unsupported event field {}", part.name))),
                }
            }
            if !explicit || definition.id.is_empty() || definition.options.is_empty() {
                return Err(field.error("event requires id, is_triggered_only=yes and options"));
            }
            if let Some(condition) = condition
                && conditions
                    .insert(definition.id.clone(), condition)
                    .is_some()
            {
                return Err(field.error("duplicate event trigger identifier"));
            }
            if once {
                once_events.insert(definition.id.clone());
            }
            definitions.push(definition);
        }
        if self.optimise {
            let unconditional: BTreeSet<_> = definitions
                .iter()
                .filter(|definition| !conditions.contains_key(&definition.id))
                .map(|definition| definition.id.clone())
                .collect();
            for definition in &mut definitions {
                for plan in std::iter::once(&mut definition.immediate)
                    .chain(
                        definition
                            .options
                            .iter_mut()
                            .map(|option| &mut option.effects),
                    )
                    .chain(std::iter::once(&mut definition.after))
                {
                    simplify_event_calls(plan, &unconditional);
                }
            }
        }
        Ok(CompiledEvents {
            definitions,
            conditions,
            once_events,
            sources: std::mem::take(&mut self.sources),
            folded_branches: self.folded,
        })
    }

    fn bind(&self, field: &Field, name: &str) -> Result<FieldId, Error> {
        self.schema
            .field(name)
            .ok_or_else(|| field.error(format!("unregistered state field {name}")))
    }

    fn enter(&mut self, field: &Field, name: String) -> Result<(), Error> {
        if self.stack.len() >= 64 || self.stack.contains(&name) {
            return Err(field.error(format!("recursive or excessive script call {name}")));
        }
        self.stack.push(name);
        Ok(())
    }

    fn effects(&mut self, fields: &[Field], scope: EntityKind) -> Result<Plan, Error> {
        let mut plan = Vec::new();
        let mut index = 0;
        while index < fields.len() {
            let field = &fields[index];
            self.sources.push((field.path.clone(), field.location));
            match field.name.as_str() {
                "add_legitimacy" => {
                    self.government_metric(field, scope, "legitimacy")?;
                    plan.push(Action::If {
                        condition: Condition::GovernmentMetricActive {
                            target: Target::Current,
                            name: "legitimacy".into(),
                        },
                        then: vec![Action::UpdateNumeric {
                            target: Target::Current,
                            slot: augustus_core::NumericSlot::Government("legitimacy".into()),
                            operation: augustus_core::NumericOperation::Add,
                            value: decimal(field)?,
                        }],
                        otherwise: vec![],
                    });
                }
                "set_country_flag" | "clr_country_flag" => {
                    let name = self.country_flag_name(field, scope)?;
                    plan.push(Action::SetMarker {
                        target: Target::Current,
                        name,
                        present: field.name == "set_country_flag",
                    });
                }
                "add_country_modifier" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("country modifiers require country scope"));
                    }
                    let mut name = None;
                    let mut duration = None;
                    let mut hidden = None;
                    let mut seen = BTreeSet::new();
                    for parameter in field.block()? {
                        if !seen.insert(&parameter.name) {
                            return Err(parameter.error("duplicate modifier parameter"));
                        }
                        match parameter.name.as_str() {
                            "name" => name = Some(parameter.scalar()?.to_owned()),
                            "duration" => {
                                let value: i32 = parameter
                                    .scalar()?
                                    .parse()
                                    .map_err(|_| parameter.error("invalid modifier duration"))?;
                                if value < -1 {
                                    return Err(parameter.error("invalid modifier duration"));
                                }
                                duration = Some(value);
                            }
                            "hidden" => hidden = Some(boolean(parameter)?),
                            _ => return Err(parameter.error("unsupported modifier parameter")),
                        }
                    }
                    let name = name.ok_or_else(|| field.error("missing modifier name"))?;
                    if name.is_empty() || !self.modifier_ids.contains(&name) {
                        return Err(field.error(format!("unknown modifier {name}")));
                    }
                    plan.push(Action::AttachModifier {
                        target: Target::Current,
                        name,
                        duration: match duration.unwrap_or(0) {
                            -1 => None,
                            value => Some(u64::try_from(value.max(1)).expect("positive duration")),
                        },
                        hidden,
                        renewal: augustus_core::ModifierRenewal::KeepLatestDated,
                    });
                }
                "remove_country_modifier" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("country modifiers require country scope"));
                    }
                    let name = field.scalar()?;
                    if name.is_empty() || !self.modifier_ids.contains(name) {
                        return Err(field.error(format!("unknown modifier {name}")));
                    }
                    plan.push(Action::RemoveModifier {
                        target: Target::Current,
                        name: name.into(),
                    });
                }
                "complete_mission" => plan.push(Action::RecordCompletion {
                    id: self.mission_id(field, scope)?,
                }),
                "hidden_effect" => plan.extend(self.effects(field.block()?, scope)?),
                "save_global_event_target_as" | "clear_global_event_target" => {
                    let name = field.scalar()?;
                    if name.is_empty() {
                        return Err(field.error("empty global event target name"));
                    }
                    if field.name == "save_global_event_target_as" {
                        if self
                            .global_scope_types
                            .get(name)
                            .is_some_and(|kind| *kind != scope)
                        {
                            return Err(
                                field.error("global event target type does not match declaration")
                            );
                        }
                        plan.push(Action::BindGlobalScope { name: name.into() });
                    } else {
                        plan.push(Action::ClearGlobalScope { name: name.into() });
                    }
                }
                "emperor" => {
                    let name = "imperial_emperor";
                    plan.push(Action::InScope {
                        selector: ScopeSelector::Office(name.into()),
                        kind: EntityKind::Country,
                        plan: self.effects(field.block()?, EntityKind::Country)?,
                    });
                }
                name if name.starts_with("event_target:") => {
                    let name = name.strip_prefix("event_target:").expect("matched prefix");
                    let kind = self.global_scope_types.get(name).copied().ok_or_else(|| {
                        field.error("global event target requires a declared entity type")
                    })?;
                    plan.push(Action::InScope {
                        selector: ScopeSelector::Global(name.into()),
                        kind,
                        plan: self.effects(field.block()?, kind)?,
                    });
                }
                "log" => plan.push(Action::Emit(field.scalar()?.into())),
                "country_event" | "province_event" => {
                    let required = if field.name == "country_event" {
                        EntityKind::Country
                    } else {
                        EntityKind::Province
                    };
                    if scope != required {
                        return Err(field.error("event call requires a matching entity scope"));
                    }
                    let mut event = None;
                    let mut delay = None;
                    for parameter in field.block()? {
                        self.sources
                            .push((parameter.path.clone(), parameter.location));
                        match parameter.name.as_str() {
                            "id" if event.is_none() => {
                                let id = parameter.scalar()?;
                                if id.is_empty() {
                                    return Err(parameter.error("empty event id"));
                                }
                                event = Some(id.to_owned());
                            }
                            "days" if delay.is_none() => {
                                delay = Some(parameter.scalar()?.parse::<u64>().map_err(|_| {
                                    parameter.error("event delay requires nonnegative whole days")
                                })?);
                            }
                            _ => {
                                return Err(parameter.error(format!(
                                    "unsupported or duplicate event call field {}",
                                    parameter.name
                                )));
                            }
                        }
                    }
                    let event = event.ok_or_else(|| field.error("event call requires id"))?;
                    let action = Action::TryEvent { event };
                    plan.push(match delay.unwrap_or(0) {
                        0 => action,
                        delay => Action::Schedule {
                            delay,
                            plan: vec![action],
                        },
                    });
                }
                "add_treasury" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("add_treasury requires a country scope"));
                    }
                    plan.push(Action::Add {
                        target: Target::Current,
                        field: self.bind(field, "treasury")?,
                        value: decimal(field)?,
                    });
                }
                "set_variable" | "change_variable" | "subtract_variable" | "multiply_variable"
                | "divide_variable" => {
                    let (name, value) = variable(field)?;
                    let slot = self.variable_slot(field, name)?;
                    let operation = match field.name.as_str() {
                        "set_variable" => augustus_core::NumericOperation::Set,
                        "change_variable" => augustus_core::NumericOperation::Add,
                        "subtract_variable" => augustus_core::NumericOperation::Subtract,
                        "multiply_variable" => {
                            augustus_core::NumericOperation::Multiply { scale: 1000 }
                        }
                        "divide_variable" => {
                            augustus_core::NumericOperation::Divide { scale: 1000 }
                        }
                        _ => unreachable!(),
                    };
                    // Preserve already-published explicit field bindings and their content IDs.
                    plan.push(match (&slot, operation) {
                        (
                            augustus_core::NumericSlot::Field(field),
                            augustus_core::NumericOperation::Set,
                        ) => Action::Set {
                            target: Target::Current,
                            field: *field,
                            value,
                        },
                        (
                            augustus_core::NumericSlot::Field(field),
                            augustus_core::NumericOperation::Add,
                        ) => Action::Add {
                            target: Target::Current,
                            field: *field,
                            value,
                        },
                        _ => Action::UpdateNumeric {
                            target: Target::Current,
                            slot,
                            operation,
                            value,
                        },
                    });
                }
                "capital_scope" | "owner" => {
                    let (target, required, nested) = if field.name == "capital_scope" {
                        (Target::Capital, EntityKind::Country, EntityKind::Province)
                    } else {
                        (Target::Owner, EntityKind::Province, EntityKind::Country)
                    };
                    if scope != required {
                        return Err(field.error("invalid entity type for scope transition"));
                    }
                    plan.push(Action::With {
                        target,
                        plan: self.effects(field.block()?, nested)?,
                    });
                }
                "if" => {
                    let body = field.block()?;
                    let limit = body
                        .first()
                        .filter(|limit| limit.name == "limit")
                        .ok_or_else(|| field.error("if requires a leading limit block"))?;
                    let condition = self.conditions(limit.block()?, scope)?;
                    let then = self.effects(&body[1..], scope)?;
                    let otherwise = if fields
                        .get(index + 1)
                        .is_some_and(|next| next.name == "else")
                    {
                        index += 1;
                        self.effects(fields[index].block()?, scope)?
                    } else {
                        Vec::new()
                    };
                    if self.optimise
                        && let Some(value) = constant(&condition)
                    {
                        self.folded += 1;
                        plan.extend(if value { then } else { otherwise });
                    } else {
                        plan.push(Action::If {
                            condition,
                            then,
                            otherwise,
                        });
                    }
                }
                name if self.library.effects.contains_key(name) => {
                    if field.scalar()? != "yes" {
                        return Err(
                            field.error("only parameterless '= yes' script calls are supported")
                        );
                    }
                    let definition = self.library.effects[name].clone();
                    self.enter(field, format!("effect:{name}"))?;
                    plan.extend(self.effects(definition.block()?, scope)?);
                    self.stack.pop();
                }
                name => return Err(field.error(format!("unsupported effect {name}"))),
            }
            index += 1;
        }
        Ok(plan)
    }

    fn conditions(&mut self, fields: &[Field], scope: EntityKind) -> Result<Condition, Error> {
        let mut conditions = Vec::new();
        for field in fields {
            self.sources.push((field.path.clone(), field.location));
            conditions.push(match field.name.as_str() {
                "legitimacy" => {
                    self.government_metric(field, scope, "legitimacy")?;
                    let value = decimal(field)?;
                    if !(0..=100_000).contains(&value) {
                        return Err(field.error("legitimacy threshold must be between 0 and 100"));
                    }
                    let active = Condition::GovernmentMetricActive {
                        target: Target::Current,
                        name: "legitimacy".into(),
                    };
                    let compare = Condition::NumericCompare {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Government("legitimacy".into()),
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value,
                    };
                    if value == 0 {
                        Condition::Any(vec![Condition::Not(Box::new(active)), compare])
                    } else {
                        Condition::All(vec![active, compare])
                    }
                }
                "is_emperor" | "is_emperor_of_china" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("office holder trigger requires country scope"));
                    }
                    let office = if field.name == "is_emperor" {
                        "imperial_emperor"
                    } else {
                        "celestial_emperor"
                    };
                    let condition = Condition::ScopeIsCurrent(ScopeSelector::Office(office.into()));
                    if boolean(field)? {
                        condition
                    } else {
                        Condition::Not(Box::new(condition))
                    }
                }
                "always" => Condition::Always(boolean(field)?),
                "has_country_flag" => Condition::HasMarker {
                    target: Target::Current,
                    name: self.country_flag_name(field, scope)?,
                },
                "has_dlc" => {
                    let expansion = field.scalar()?;
                    if !crate::is_known_dlc(expansion) {
                        return Err(field.error(format!("unsupported expansion {expansion}")));
                    }
                    Condition::Always(true)
                }
                "mission_completed" => Condition::CompletionRecorded {
                    target: Target::Current,
                    id: self.mission_id(field, scope)?,
                },
                "tag" | "TAG" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("tag trigger requires country scope"));
                    }
                    let tag = field.scalar()?;
                    if tag.len() != 3
                        || !tag
                            .bytes()
                            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                    {
                        return Err(field.error(
                            "tag trigger requires a literal three-character country identifier",
                        ));
                    }
                    Condition::EntityNamed {
                        target: Target::Current,
                        name: tag.into(),
                    }
                }
                "is_year" => {
                    let year = field
                        .scalar()?
                        .parse::<i16>()
                        .ok()
                        .filter(|year| *year > 0)
                        .ok_or_else(|| field.error("expected a positive integer year"))?;
                    Condition::YearAtLeast(year)
                }
                "AND" | "and" => self.conditions(field.block()?, scope)?,
                "OR" | "or" => {
                    let Condition::All(children) = self.conditions(field.block()?, scope)? else {
                        unreachable!("condition blocks produce conjunctions")
                    };
                    Condition::Any(children)
                }
                "NOT" | "not" => {
                    let body = field.block()?;
                    if body.is_empty() {
                        return Err(field.error("NOT requires at least one enclosed trigger"));
                    }
                    let condition = self.conditions(body, scope)?;
                    if body.len() == 1 {
                        Condition::Not(Box::new(condition))
                    } else {
                        let Condition::All(children) = condition else {
                            unreachable!("condition blocks produce conjunctions")
                        };
                        Condition::Not(Box::new(Condition::Any(children)))
                    }
                }
                "has_country_modifier" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("country modifiers require country scope"));
                    }
                    let name = field.scalar()?;
                    if name.is_empty() {
                        return Err(field.error("empty modifier name"));
                    }
                    if self.modifier_ids.contains(name) {
                        Condition::HasModifier {
                            target: Target::Current,
                            name: name.into(),
                        }
                    } else {
                        if name.chars().any(|c| matches!(c, '$' | '@')) {
                            return Err(field.error("unsupported dynamic modifier name"));
                        }
                        Condition::Always(false)
                    }
                }
                "check_variable" | "is_variable_equal" => {
                    let (name, value) = variable(field)?;
                    let slot = self.variable_slot(field, name)?;
                    match (&slot, field.name.as_str()) {
                        (augustus_core::NumericSlot::Field(field), "check_variable") => {
                            Condition::AtLeast {
                                target: Target::Current,
                                field: *field,
                                value,
                            }
                        }
                        _ => Condition::NumericCompare {
                            target: Target::Current,
                            slot,
                            comparison: if field.name == "check_variable" {
                                augustus_core::NumericComparison::AtLeast
                            } else {
                                augustus_core::NumericComparison::Equal
                            },
                            value,
                        },
                    }
                }
                "treasury" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("treasury requires country scope"));
                    }
                    Condition::AtLeast {
                        target: Target::Current,
                        field: self.bind(field, "treasury")?,
                        value: decimal(field)?,
                    }
                }
                name if self.library.triggers.contains_key(name) => {
                    let positive = boolean(field)?;
                    let definition = self.library.triggers[name].clone();
                    self.enter(field, format!("trigger:{name}"))?;
                    let condition = self.conditions(definition.block()?, scope)?;
                    self.stack.pop();
                    if positive {
                        condition
                    } else {
                        Condition::Not(Box::new(condition))
                    }
                }
                name => return Err(field.error(format!("unsupported trigger {name}"))),
            });
        }
        Ok(Condition::All(conditions))
    }
}

fn boolean(field: &Field) -> Result<bool, Error> {
    match field.scalar()? {
        "yes" => Ok(true),
        "no" => Ok(false),
        _ => Err(field.error("expected yes or no")),
    }
}

fn variable(field: &Field) -> Result<(&str, i64), Error> {
    let body = field.block()?;
    if body.len() != 2 {
        return Err(field.error("variable operation requires exactly which and value"));
    }
    let which = body
        .iter()
        .find(|value| value.name == "which")
        .ok_or_else(|| field.error("missing variable name"))?;
    let value = body
        .iter()
        .find(|value| value.name == "value")
        .ok_or_else(|| field.error("missing variable value"))?;
    Ok((which.scalar()?, decimal(value)?))
}

/// Restricted literal decimals, exactly represented in thousandths. No float parsing.
fn decimal(field: &Field) -> Result<i64, Error> {
    let text = field.scalar()?;
    let (negative, unsigned) = text
        .strip_prefix('-')
        .map_or((false, text), |value| (true, value));
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || fraction.len() > 3
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(field.error("expected literal decimal with at most three fractional digits"));
    }
    let whole: i64 = whole.parse().map_err(|_| field.error("decimal overflow"))?;
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        fraction
            .parse()
            .map_err(|_| field.error("invalid decimal"))?
    };
    let value = whole
        .checked_mul(1000)
        .and_then(|value| {
            value.checked_add(
                fraction
                    * 10_i64.pow(
                        3 - unsigned
                            .split_once('.')
                            .map_or(0, |(_, fraction)| fraction.len())
                            as u32,
                    ),
            )
        })
        .ok_or_else(|| field.error("decimal overflow"))?;
    Ok(if negative { -value } else { value })
}

fn constant(condition: &Condition) -> Option<bool> {
    match condition {
        Condition::Always(value) => Some(*value),
        Condition::Not(condition) => constant(condition).map(|value| !value),
        Condition::All(conditions) => conditions
            .iter()
            .map(constant)
            .collect::<Option<Vec<_>>>()
            .map(|values| values.iter().all(|value| *value)),
        Condition::Any(conditions) => conditions
            .iter()
            .map(constant)
            .collect::<Option<Vec<_>>>()
            .map(|values| values.iter().any(|value| *value)),
        _ => None,
    }
}

fn simplify_event_calls(plan: &mut Plan, unconditional: &BTreeSet<String>) {
    for action in plan {
        match action {
            Action::TryEvent { event } if unconditional.contains(event) => {
                *action = Action::TriggerEvent {
                    event: event.clone(),
                };
            }
            Action::If {
                then, otherwise, ..
            }
            | Action::RandomBranch { then, otherwise } => {
                simplify_event_calls(then, unconditional);
                simplify_event_calls(otherwise, unconditional);
            }
            Action::With { plan, .. }
            | Action::InScope { plan, .. }
            | Action::Schedule { plan, .. } => simplify_event_calls(plan, unconditional),
            _ => (),
        }
    }
}

pub(crate) fn event_references(plan: &[Action], references: &mut BTreeSet<String>) {
    for action in plan {
        match action {
            Action::TryEvent { event } | Action::TriggerEvent { event } => {
                references.insert(event.clone());
            }
            Action::EventCandidates { events } => references.extend(events.iter().cloned()),
            Action::If {
                then, otherwise, ..
            }
            | Action::RandomBranch { then, otherwise } => {
                event_references(then, references);
                event_references(otherwise, references);
            }
            Action::With { plan, .. }
            | Action::InScope { plan, .. }
            | Action::Schedule { plan, .. } => event_references(plan, references),
            _ => (),
        }
    }
}
