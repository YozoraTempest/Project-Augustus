// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{collections::BTreeMap, path::PathBuf};

use augustus_core::{
    Action, Condition, EntityKind, EventDefinition, EventOption, FieldId, Plan, Schema, Target,
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
        }
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

    /// Compile explicitly dispatched events with literal IDs and unconditional options.
    /// Semantic fields outside this subset are errors, including automatic selection.
    pub fn compile_events(mut self, document: &TextDocument) -> Result<CompiledEvents, Error> {
        let parsed = document.parse()?;
        let fields = read_fields(&parsed, parsed.reader(), 0)?;
        let mut definitions = Vec::new();
        for field in &fields {
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
                    "title" | "desc" | "picture" => {
                        part.scalar()?;
                    }
                    "immediate" => definition.immediate = self.effects(part.block()?, scope)?,
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
            definitions.push(definition);
        }
        Ok(CompiledEvents {
            definitions,
            sources: self.sources,
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
                "log" => plan.push(Action::Emit(field.scalar()?.into())),
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
                "set_variable" | "change_variable" => {
                    let (name, value) = variable(field)?;
                    let slot = self.bind(field, &format!("variable/{name}"))?;
                    plan.push(if field.name == "set_variable" {
                        Action::Set {
                            target: Target::Current,
                            field: slot,
                            value,
                        }
                    } else {
                        Action::Add {
                            target: Target::Current,
                            field: slot,
                            value,
                        }
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
                "always" => Condition::Always(boolean(field)?),
                "NOT" => {
                    let body = field.block()?;
                    if body.len() != 1 {
                        return Err(field.error("NOT currently requires one enclosed trigger"));
                    }
                    Condition::Not(Box::new(self.conditions(body, scope)?))
                }
                "check_variable" => {
                    let (name, value) = variable(field)?;
                    Condition::AtLeast {
                        target: Target::Current,
                        field: self.bind(field, &format!("variable/{name}"))?,
                        value,
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
        _ => None,
    }
}
