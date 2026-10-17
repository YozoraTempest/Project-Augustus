// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

use augustus_core::{
    Action, Condition, EntityKind, EventDefinition, EventLocations, EventOption, EventPictures,
    FieldId, PictureVariant, Plan, Schema, ScopeSelector, Target,
};
use jomini::{
    TextToken,
    text::{ObjectReader, Operator},
};

use crate::{
    ContentSet, ContentView, DefinitionKind, Error, FileEntry, ParsedText, SourceLocation,
    TextDocument, TextEncoding,
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
    Unsupported(&'static str),
}

impl Field {
    fn error(&self, message: impl Into<String>) -> Error {
        Error::message(&self.path, message).with_location(self.location)
    }
    fn scalar(&self) -> Result<&str, Error> {
        match &self.value {
            Value::Scalar(value) => Ok(value),
            Value::Unsupported(message) => Err(self.error(*message)),
            _ => Err(self.error("expected scalar")),
        }
    }
    fn block(&self) -> Result<&[Field], Error> {
        match &self.value {
            Value::Block(fields) => Ok(fields),
            Value::Unsupported(message) => Err(self.error(*message)),
            _ => Err(self.error("expected block")),
        }
    }

    fn substitute_parameters(
        &mut self,
        parameters: &BTreeMap<String, String>,
    ) -> Option<BTreeSet<String>> {
        let (name, used, complete_name) = substitute_script_parameters(&self.name, parameters);
        self.name = name;
        if !complete_name {
            return None;
        }
        let mut used_parameters = used;
        let complete_value = match &mut self.value {
            Value::Scalar(value) => {
                let (substituted, used, complete) = substitute_script_parameters(value, parameters);
                *value = substituted;
                used_parameters.extend(used);
                complete
            }
            Value::Block(fields) => {
                for field in fields {
                    used_parameters.extend(field.substitute_parameters(parameters)?);
                }
                true
            }
            Value::Unsupported(_) => true,
        };
        complete_value.then_some(used_parameters)
    }
}

fn substitute_script_parameters(
    text: &str,
    parameters: &BTreeMap<String, String>,
) -> (String, BTreeSet<String>, bool) {
    let mut substituted = text.to_owned();
    let mut used = BTreeSet::new();
    for (name, value) in parameters {
        let placeholder = format!("${name}$");
        if substituted.contains(&placeholder) {
            substituted = substituted.replace(&placeholder, value);
            used.insert(name.clone());
        }
    }
    let complete = !substituted.contains('$');
    (substituted, used, complete)
}

/// Owns parsed syntax until the world's registries are available for compilation.
pub(crate) struct ConditionSource(Field);

impl ConditionSource {
    pub(crate) fn parse(
        parsed: &ParsedText<'_>,
        name: &str,
        reader: ObjectReader<'_, '_, TextEncoding>,
        location: SourceLocation,
    ) -> Result<Self, Error> {
        Ok(Self(Field {
            name: name.into(),
            value: Value::Block(read_fields(parsed, reader, 1, false)?),
            path: parsed.source().into(),
            location,
        }))
    }
}

fn read_fields(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    depth: usize,
    defer_unsupported: bool,
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
                if !defer_unsupported {
                    return Err(fail("unsupported script operator"));
                }
                return Ok(Field {
                    name: key.read_string(),
                    value: Value::Unsupported("unsupported script operator"),
                    path: parsed.source().into(),
                    location,
                });
            }
            let value = match value.token() {
                TextToken::Unquoted(_) | TextToken::Quoted(_) => {
                    Value::Scalar(value.read_string().map_err(|_| fail("invalid scalar"))?)
                }
                TextToken::Object { mixed: false, .. } => Value::Block(read_fields(
                    parsed,
                    value.read_object().map_err(|_| fail("invalid block"))?,
                    depth + 1,
                    defer_unsupported,
                )?),
                TextToken::Array { mixed: false, .. }
                    if value
                        .read_array()
                        .is_ok_and(|array| array.values().next().is_none()) =>
                {
                    Value::Block(Vec::new())
                }
                _ if defer_unsupported => Value::Unsupported("unsupported script value"),
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
    government_callbacks: BTreeMap<String, Field>,
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
        for (path, _) in content.effective_files().filter(|(path, _)| {
            crate::bootstrap::direct_txt(std::path::Path::new(path), "common/on_actions")
        }) {
            library.add_on_actions(&content.read_text(path, encoding)?)?;
        }
        Ok(library)
    }

    pub(crate) fn from_root(
        root: &impl ContentView,
        files: &[FileEntry],
        encoding: TextEncoding,
    ) -> Result<Self, Error> {
        let mut library = Self::new();
        for (kind, triggers) in [
            (DefinitionKind::ScriptedEffects, false),
            (DefinitionKind::ScriptedTriggers, true),
        ] {
            let mut selected: Vec<_> = files
                .iter()
                .filter(|file| crate::bootstrap::direct_txt(&file.relative_path, kind.directory()))
                .map(|file| {
                    let name = file
                        .relative_path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .ok_or_else(|| {
                            Error::message(&file.relative_path, "script filename must be UTF-8")
                        })?;
                    Ok((name.as_bytes().to_vec(), file))
                })
                .collect::<Result<_, Error>>()?;
            selected.sort_by(|left, right| left.0.cmp(&right.0));
            for (_, file) in selected {
                library.add(&root.read_text(&file.relative_path, encoding)?, triggers)?;
            }
        }
        for file in files
            .iter()
            .filter(|file| crate::bootstrap::direct_txt(&file.relative_path, "common/on_actions"))
        {
            library.add_on_actions(&root.read_text(&file.relative_path, encoding)?)?;
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
    /// Supply government callbacks from selected on_action files. Duplicate
    /// declarations are unresolved rather than choosing an unverified winner.
    pub fn add_on_actions(&mut self, document: &TextDocument) -> Result<(), Error> {
        let parsed = document.parse()?;
        for field in read_fields(&parsed, parsed.reader(), 0, true)? {
            if !matches!(
                field.name.as_str(),
                "on_government_change" | "on_native_change_government"
            ) {
                continue;
            }
            let mut field = field;
            if self.government_callbacks.contains_key(&field.name) {
                field.value = Value::Unsupported("ambiguous government callback");
            }
            self.government_callbacks.insert(field.name.clone(), field);
        }
        Ok(())
    }
    pub fn ignored_definitions(&self) -> &[(String, PathBuf, SourceLocation)] {
        &self.ignored
    }
    fn add(&mut self, document: &TextDocument, triggers: bool) -> Result<(), Error> {
        let parsed = document.parse()?;
        let fields = read_fields(&parsed, parsed.reader(), 0, true)?;
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
    pub option_conditions: BTreeMap<String, BTreeMap<String, Condition>>,
    pub option_weights: augustus_core::EventOptionWeights,
    pub pictures: BTreeMap<String, EventPictures>,
    pub locations: BTreeMap<String, EventLocations>,
    pub once_events: BTreeSet<String>,
    pub sources: Vec<(PathBuf, SourceLocation)>,
    pub folded_branches: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct UnavailableEventOption {
    pub reason: String,
    pub path: PathBuf,
    pub location: crate::SourceLocation,
}

type UnavailableEventOptions = BTreeMap<String, BTreeMap<String, UnavailableEventOption>>;
type CompiledEventsWithUnavailableOptions = (CompiledEvents, UnavailableEventOptions);

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
    subject_type_ids: Option<BTreeSet<String>>,
    casus_belli_types: BTreeMap<String, crate::CasusBelliType>,
    government_metric_ids: BTreeSet<String>,
    government_form_ids: BTreeSet<String>,
    metric_ids: BTreeSet<String>,
    truce_months: Option<u32>,
    treasury_slot: Option<augustus_core::NumericSlot>,
    area_scopes: BTreeMap<String, Vec<u32>>,
    religious_school_groups: BTreeMap<String, Option<String>>,
    hidden_descriptions: bool,
    root_scope: Option<EntityKind>,
    defer_unsupported_event_options: bool,
    unavailable_event_options: BTreeMap<String, BTreeMap<String, UnavailableEventOption>>,
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
            subject_type_ids: Some(BTreeSet::new()),
            casus_belli_types: BTreeMap::new(),
            government_metric_ids: BTreeSet::new(),
            government_form_ids: BTreeSet::new(),
            metric_ids: BTreeSet::new(),
            truce_months: None,
            treasury_slot: None,
            area_scopes: BTreeMap::new(),
            religious_school_groups: BTreeMap::new(),
            hidden_descriptions: false,
            root_scope: None,
            defer_unsupported_event_options: false,
            unavailable_event_options: BTreeMap::new(),
        }
    }

    pub(crate) fn for_world(
        world: &'a augustus_core::World,
        library: &'a ScriptLibrary,
        optimise: bool,
    ) -> Self {
        let mut compiler = Self::new(world.schema(), library, optimise)
            .with_subject_type_ids(world.subjects().types.clone())
            .with_mission_ids(world.completions().identifiers.clone())
            .with_modifier_ids(world.modifiers().definitions.keys().cloned().collect())
            .with_metric_ids(world.metrics().definitions.keys().cloned().collect())
            .with_government_metric_ids(world.governments().metrics.keys().cloned().collect())
            .with_government_form_ids(world.governments().forms.keys().cloned().collect())
            .with_treasury_slot(Some(augustus_core::NumericSlot::Resource(
                augustus_core::CountryResource::Treasury,
            )));
        if let Some(schools) = world.affiliations().0.get("religious_school") {
            compiler = compiler.with_religious_school_groups(schools.groups.clone());
        }
        compiler
    }

    pub(crate) fn with_hidden_descriptions(mut self, hidden: bool) -> Self {
        self.hidden_descriptions = hidden;
        self
    }

    pub(crate) fn with_root_scope(mut self, scope: EntityKind) -> Self {
        self.root_scope = Some(scope);
        self
    }

    fn root_kind(&self, field: &Field) -> Result<EntityKind, Error> {
        self.root_scope
            .ok_or_else(|| field.error("ROOT is unavailable in this callback context"))
    }

    fn subject_reference(&self, field: &Field) -> Result<augustus_core::EntityRef, Error> {
        let value = field.scalar()?;
        let selector = if value.eq_ignore_ascii_case("ROOT") {
            self.root_kind(field)?;
            ScopeSelector::Root
        } else if country_identifier(value) {
            ScopeSelector::Named {
                kind: EntityKind::Country,
                name: value.into(),
            }
        } else if value == "emperor" {
            ScopeSelector::Office("imperial_emperor".into())
        } else if let Some(name) = value.strip_prefix("event_target:") {
            if !self.global_scope_types.contains_key(name) {
                return Err(field.error("subject target requires a declared entity reference"));
            }
            ScopeSelector::Global(name.into())
        } else {
            return Err(field.error("unsupported subject target reference"));
        };
        Ok(augustus_core::EntityRef::Selected(selector))
    }

    /// Types come from selected content declarations, not current target values.
    pub fn with_global_scope_types(mut self, types: BTreeMap<String, EntityKind>) -> Self {
        self.global_scope_types = types;
        self
    }

    pub fn with_metric_ids(mut self, ids: BTreeSet<String>) -> Self {
        self.metric_ids = ids;
        self
    }

    pub fn with_truce_months(mut self, months: Option<u32>) -> Self {
        self.truce_months = months;
        self
    }

    /// Bind cash directives to the storage chosen by the host. Existing field
    /// scenarios retain their declared treasury field when no binding is supplied.
    pub fn with_treasury_slot(mut self, slot: Option<augustus_core::NumericSlot>) -> Self {
        self.treasury_slot = slot;
        self
    }

    pub fn with_subject_type_ids(mut self, ids: BTreeSet<String>) -> Self {
        self.subject_type_ids = Some(ids);
        self
    }

    /// Initial conditions bind declarations later against the constructed world.
    pub(crate) fn with_unbound_subject_types(mut self) -> Self {
        self.subject_type_ids = None;
        self
    }

    /// Members retain content declaration order, including repeated entries.
    pub fn with_area_scopes(mut self, areas: BTreeMap<String, Vec<u32>>) -> Self {
        self.area_scopes = areas;
        self
    }

    fn devastation_scope(&self, field: &Field, scope: EntityKind) -> Result<(), Error> {
        if scope != EntityKind::Province || !self.metric_ids.contains("devastation") {
            return Err(field.error("devastation requires province scope and a registered metric"));
        }
        Ok(())
    }

    fn stability_scope(&self, field: &Field, scope: EntityKind) -> Result<(), Error> {
        if scope != EntityKind::Country || !self.metric_ids.contains("stability") {
            return Err(field.error("stability requires country scope and a registered metric"));
        }
        Ok(())
    }

    fn power_scope(&self, field: &Field, scope: EntityKind, name: &str) -> Result<(), Error> {
        if scope != EntityKind::Country || !self.metric_ids.contains(name) {
            return Err(field.error(format!(
                "{name} requires country scope and a registered metric"
            )));
        }
        Ok(())
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

    pub(crate) fn compile_block_condition_reader(
        self,
        parsed: &ParsedText<'_>,
        name: &str,
        reader: ObjectReader<'_, '_, TextEncoding>,
        location: SourceLocation,
        scope: EntityKind,
    ) -> Result<Condition, Error> {
        self.compile_condition_source(
            &ConditionSource::parse(parsed, name, reader, location)?,
            scope,
        )
    }

    pub(crate) fn compile_condition_source(
        mut self,
        source: &ConditionSource,
        scope: EntityKind,
    ) -> Result<Condition, Error> {
        self.root_scope.get_or_insert(scope);
        let Condition::All(mut conditions) =
            self.conditions(std::slice::from_ref(&source.0), scope)?
        else {
            unreachable!()
        };
        Ok(conditions.pop().expect("one condition"))
    }

    /// Translate an ordered set of mission files to core selection conditions.
    pub fn compile_missions(
        mut self,
        documents: &[TextDocument],
    ) -> Result<augustus_core::Objectives, Error> {
        self.root_scope = Some(EntityKind::Country);
        let mut groups = Vec::new();
        for document in documents {
            for mut group in self.compile_mission_groups(document)? {
                if group.priority != u32::MAX {
                    group.priority = u32::try_from(groups.len() + 1).map_err(|_| {
                        Error::message(document.source(), "too many mission series")
                    })?;
                }
                groups.push(group);
            }
        }
        let original = groups.clone();
        for group in &mut groups {
            if group.priority != u32::MAX {
                let exclusions: Vec<_> = original
                    .iter()
                    .filter(|other| {
                        other.priority > group.priority && !other.claims.is_disjoint(&group.claims)
                    })
                    .map(|other| other.condition.clone())
                    .collect();
                if !exclusions.is_empty() {
                    group.condition = Condition::All(vec![
                        group.condition.clone(),
                        Condition::Not(Box::new(Condition::Any(exclusions))),
                    ]);
                }
            }
            group.priority = 0;
            group.claims.clear();
        }
        Ok(augustus_core::Objectives {
            groups,
            ..Default::default()
        })
    }

    /// Translate selection metadata only; objective rewards and prerequisites
    /// are not executed by availability refresh.
    fn compile_mission_groups(
        &mut self,
        document: &TextDocument,
    ) -> Result<Vec<augustus_core::ObjectiveGroup>, Error> {
        let parsed = document.parse()?;
        let fields = read_fields(&parsed, parsed.reader(), 0, true)?;
        let mut groups = Vec::new();
        for series in fields.iter().filter(|f| !f.name.starts_with('@')) {
            let fields = series.block()?;
            let slot = fields
                .iter()
                .find(|f| f.name == "slot")
                .ok_or_else(|| series.error("mission series requires slot"))?;
            let slot = slot
                .scalar()?
                .parse::<u32>()
                .ok()
                .filter(|n| *n > 0)
                .ok_or_else(|| slot.error("mission slot requires a positive literal integer"))?;
            let generic = fields
                .iter()
                .find(|f| f.name == "generic")
                .map(boolean)
                .transpose()?
                .unwrap_or(false);
            let condition = match fields.iter().find(|f| f.name == "potential") {
                Some(field) => self.mission_condition(field.block()?),
                None => Condition::Always(true),
            };
            let mut objectives = BTreeSet::new();
            let mut next = 1_u32;
            let mut positions = Vec::new();
            for mission in fields.iter().filter(|f| {
                !f.name.starts_with('@')
                    && !matches!(
                        f.name.as_str(),
                        "slot"
                            | "generic"
                            | "ai"
                            | "has_country_shield"
                            | "potential"
                            | "potential_on_load"
                    )
            }) {
                let body = mission.block()?;
                let position = match body.iter().find(|f| f.name == "position") {
                    Some(field) => field
                        .scalar()?
                        .parse::<u32>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| {
                            field.error("mission position requires a positive literal integer")
                        })?,
                    None => next,
                };
                next = position
                    .checked_add(1)
                    .ok_or_else(|| mission.error("mission position overflow"))?;
                positions.push(position);
                objectives.insert(mission.name.clone());
            }
            let mut claims = BTreeSet::new();
            if let (Some(start), Some(end)) = (positions.iter().min(), positions.iter().max()) {
                if end - start > 4096 {
                    return Err(series.error("mission row range exceeds 4096"));
                }
                claims.extend((*start..=*end).map(|row| format!("column/{slot}/row/{row}")));
            }
            groups.push(augustus_core::ObjectiveGroup {
                id: series.name.clone(),
                condition,
                objectives,
                claims,
                // The complete content set rebases these fallback priorities.
                priority: if generic {
                    u32::try_from(groups.len() + 1)
                        .map_err(|_| series.error("too many mission series"))?
                } else {
                    u32::MAX
                },
            });
        }
        Ok(groups)
    }

    fn mission_condition(&mut self, fields: &[Field]) -> Condition {
        let mut conditions = Vec::new();
        let mut index = 0;
        while index < fields.len() {
            let field = &fields[index];
            let children = |compiler: &mut Self| -> Result<Condition, Error> {
                let body = field.block()?;
                if body.is_empty() && matches!(field.name.as_str(), "OR" | "or" | "NOT" | "not") {
                    return Err(field.error("OR and NOT require at least one enclosed trigger"));
                }
                Ok(compiler.mission_condition(body))
            };
            let stack = self.stack.clone();
            let result = match field.name.as_str() {
                "if" => self.condition_branch_chain(fields, &mut index, EntityKind::Country),
                "AND" | "and" => children(self),
                "OR" | "or" => children(self).map(|c| {
                    let Condition::All(children) = c else {
                        unreachable!()
                    };
                    Condition::Any(children)
                }),
                "NOT" | "not" => children(self).map(|c| {
                    let Condition::All(children) = c else {
                        unreachable!()
                    };
                    Condition::Not(Box::new(Condition::Any(children)))
                }),
                _ => self.conditions(std::slice::from_ref(field), EntityKind::Country),
            };
            self.stack = stack;
            conditions
                .push(result.unwrap_or_else(|error| Condition::Unavailable(error.to_string())));
            index += 1;
        }
        Condition::All(conditions)
    }

    pub fn with_mission_ids(mut self, ids: BTreeSet<String>) -> Self {
        self.mission_ids = ids;
        self
    }

    pub fn with_modifier_ids(mut self, ids: BTreeSet<String>) -> Self {
        self.modifier_ids = ids;
        self
    }

    pub fn with_casus_belli_types(
        mut self,
        types: BTreeMap<String, crate::CasusBelliType>,
    ) -> Self {
        self.casus_belli_types = types;
        self
    }

    fn core_operands(
        &self,
        field: &Field,
        scope: EntityKind,
    ) -> Result<(augustus_core::EntityRef, augustus_core::EntityRef), Error> {
        use augustus_core::EntityRef;
        let expected = match scope {
            EntityKind::Province => EntityKind::Country,
            EntityKind::Country => EntityKind::Province,
        };
        let value = field.scalar()?;
        let selector = if value.eq_ignore_ascii_case("ROOT") {
            if self.root_kind(field)? != expected {
                return Err(field.error("core operand ROOT has the wrong entity type"));
            }
            ScopeSelector::Root
        } else if let Some(name) = value.strip_prefix("event_target:") {
            if self.global_scope_types.get(name) != Some(&expected) {
                return Err(field.error(
                    "core operand requires a declared reference of the opposite entity type",
                ));
            }
            ScopeSelector::Global(name.into())
        } else if expected == EntityKind::Country && country_identifier(value) {
            ScopeSelector::Named {
                kind: expected,
                name: value.into(),
            }
        } else if expected == EntityKind::Province && province_identifier(value).is_some() {
            ScopeSelector::Named {
                kind: expected,
                name: province_identifier(value)
                    .expect("matched province identifier")
                    .to_string(),
            }
        } else {
            return Err(field.error("core operand requires a literal country or province, typed ROOT, or declared event target"));
        };
        let current = EntityRef::Relative(Target::Current);
        let other = EntityRef::Selected(selector);
        Ok(if scope == EntityKind::Province {
            (current, other)
        } else {
            (other, current)
        })
    }

    fn country_reference(&self, field: &Field) -> Result<augustus_core::EntityRef, Error> {
        let value = field.scalar()?;
        let selector = if country_identifier(value) {
            ScopeSelector::Named {
                kind: EntityKind::Country,
                name: value.into(),
            }
        } else if value == "emperor" {
            ScopeSelector::Office("imperial_emperor".into())
        } else if let Some(name) = value.strip_prefix("event_target:") {
            if self.global_scope_types.get(name) != Some(&EntityKind::Country) {
                return Err(field.error("target requires a declared country reference"));
            }
            ScopeSelector::Global(name.into())
        } else {
            return Err(field.error("unsupported country target reference"));
        };
        Ok(augustus_core::EntityRef::Selected(selector))
    }

    fn typed_country_reference(&self, field: &Field) -> Result<augustus_core::EntityRef, Error> {
        if field.scalar()?.eq_ignore_ascii_case("ROOT") {
            if self.root_kind(field)? != EntityKind::Country {
                return Err(field.error("target ROOT requires country type"));
            }
            Ok(augustus_core::EntityRef::Selected(ScopeSelector::Root))
        } else {
            self.country_reference(field)
        }
    }

    fn casus_belli_operands(
        &mut self,
        field: &Field,
        scope: EntityKind,
        duration: bool,
    ) -> Result<(String, augustus_core::EntityRef, u32), Error> {
        if scope != EntityKind::Country {
            return Err(field.error("casus belli requires country scope"));
        }
        let mut name = None;
        let mut target = None;
        let mut months = None;
        let mut seen = BTreeSet::new();
        for part in field.block()? {
            self.sources.push((part.path.clone(), part.location));
            if !seen.insert(&part.name) {
                return Err(part.error("duplicate casus belli parameter"));
            }
            match part.name.as_str() {
                "type" => name = Some(part.scalar()?.to_owned()),
                "target" => target = Some(self.country_reference(part)?),
                "months" if duration => {
                    months = Some(
                        part.scalar()?
                            .parse::<i32>()
                            .map_err(|_| part.error("invalid casus belli duration"))?,
                    )
                }
                _ => return Err(part.error("unsupported casus belli parameter")),
            }
        }
        let name = name.ok_or_else(|| field.error("missing casus belli type"))?;
        let data = self
            .casus_belli_types
            .get(&name)
            .ok_or_else(|| field.error("undeclared casus belli type"))?;
        let target = target.ok_or_else(|| field.error("missing casus belli target"))?;
        Ok((
            name,
            target,
            months
                .filter(|v| *v > 0)
                .map(|v| v as u32)
                .unwrap_or(data.default_months),
        ))
    }

    pub fn with_government_metric_ids(mut self, ids: BTreeSet<String>) -> Self {
        self.government_metric_ids = ids;
        self
    }

    pub fn with_government_form_ids(mut self, ids: BTreeSet<String>) -> Self {
        self.government_form_ids = ids;
        self
    }

    fn government_callback(&mut self, name: &str) -> Result<Plan, Error> {
        let Some(field) = self.library.government_callbacks.get(name).cloned() else {
            return Ok(Vec::new());
        };
        let key = format!("on_action:{name}");
        if self.stack.contains(&key) {
            return Err(field.error("recursive government callback is unsupported"));
        }
        self.sources.push((field.path.clone(), field.location));
        self.stack.push(key);
        let root_scope = self.root_scope.take();
        let result = self.effects(field.block()?, EntityKind::Country);
        self.root_scope = root_scope;
        self.stack.pop();
        result
    }

    pub fn with_religious_school_groups(
        mut self,
        groups: BTreeMap<String, Option<String>>,
    ) -> Self {
        self.religious_school_groups = groups;
        self
    }

    fn school_operands<'b>(&self, field: &'b Field) -> Result<(&'b str, &'b str), Error> {
        let body = field.block()?;
        if body.len() != 2
            || body.iter().filter(|part| part.name == "group").count() != 1
            || body.iter().filter(|part| part.name == "school").count() != 1
        {
            return Err(field.error("religious school requires exactly one group and one school"));
        }
        let group = body.iter().find(|part| part.name == "group").unwrap();
        let school = body.iter().find(|part| part.name == "school").unwrap();
        for operand in [group, school] {
            let name = operand.scalar()?;
            if name.trim().is_empty()
                || name
                    .chars()
                    .any(|c| matches!(c, '$' | '@' | ':' | '.' | '[' | ']'))
                || matches!(name, "ROOT" | "FROM" | "PREV" | "THIS")
            {
                return Err(operand.error("religious school requires literal identifiers"));
            }
        }
        Ok((group.scalar()?, school.scalar()?))
    }

    fn branch_chain(
        &mut self,
        fields: &[Field],
        index: &mut usize,
        scope: EntityKind,
    ) -> Result<Plan, Error> {
        let mut branches = Vec::new();
        loop {
            let field = &fields[*index];
            if field.name == "else_if" {
                self.sources.push((field.path.clone(), field.location));
            }
            let body = field.block()?;
            let limit = body
                .first()
                .filter(|part| part.name == "limit")
                .ok_or_else(|| field.error("if requires a leading limit block"))?;
            branches.push((
                self.conditions(limit.block()?, scope)?,
                self.effects(&body[1..], scope)?,
            ));
            if fields
                .get(*index + 1)
                .is_some_and(|next| next.name == "else_if")
            {
                *index += 1;
            } else {
                break;
            }
        }
        let mut otherwise = if fields
            .get(*index + 1)
            .is_some_and(|next| next.name == "else")
        {
            *index += 1;
            self.effects(fields[*index].block()?, scope)?
        } else {
            Vec::new()
        };
        for (condition, then) in branches.into_iter().rev() {
            if self.optimise
                && let Some(value) = constant(&condition)
            {
                self.folded += 1;
                otherwise = if value { then } else { otherwise };
            } else {
                otherwise = vec![Action::If {
                    condition,
                    then,
                    otherwise,
                }];
            }
        }
        Ok(otherwise)
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

    fn condition_branch_chain(
        &mut self,
        fields: &[Field],
        index: &mut usize,
        scope: EntityKind,
    ) -> Result<Condition, Error> {
        let mut branches = Vec::new();
        loop {
            let field = &fields[*index];
            if field.name == "else_if" {
                self.sources.push((field.path.clone(), field.location));
            }
            let body = field.block()?;
            let limit = body
                .first()
                .filter(|part| part.name == "limit")
                .ok_or_else(|| field.error("conditional trigger requires a leading limit block"))?;
            branches.push((
                self.conditions(limit.block()?, scope)?,
                self.conditions(&body[1..], scope)?,
            ));
            if fields
                .get(*index + 1)
                .is_some_and(|next| next.name == "else_if")
            {
                *index += 1;
            } else {
                break;
            }
        }
        let mut otherwise = if fields
            .get(*index + 1)
            .is_some_and(|next| next.name == "else")
        {
            *index += 1;
            let field = &fields[*index];
            self.sources.push((field.path.clone(), field.location));
            self.conditions(field.block()?, scope)?
        } else {
            Condition::Always(true)
        };
        for (condition, then) in branches.into_iter().rev() {
            if self.optimise
                && let Some(value) = constant(&condition)
            {
                self.folded += 1;
                otherwise = if value { then } else { otherwise };
            } else {
                otherwise = Condition::If {
                    condition: Box::new(condition),
                    then: Box::new(then),
                    otherwise: Box::new(otherwise),
                };
            }
        }
        Ok(otherwise)
    }

    fn modifier_scope(&self, field: &Field, scope: EntityKind) -> Result<(), Error> {
        let (required, message) = match field.name.as_str() {
            "add_country_modifier" | "remove_country_modifier" | "has_country_modifier" => (
                EntityKind::Country,
                "country modifiers require country scope",
            ),
            "remove_province_modifier" | "has_province_modifier" => (
                EntityKind::Province,
                "province modifiers require province scope",
            ),
            _ => unreachable!("modifier scope is checked only for modifier statements"),
        };
        if scope != required {
            return Err(field.error(message));
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
        self.root_scope.get_or_insert(scope);
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
        self.root_scope.get_or_insert(scope);
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

    fn global_flag_name(&self, field: &Field) -> Result<String, Error> {
        let name = field.scalar()?;
        if !crate::country_markers::literal_name(name) {
            return Err(field.error("global flag requires a literal name"));
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
        self.root_scope.get_or_insert(scope);
        let parsed = document.parse()?;
        let fields = read_fields(&parsed, parsed.reader(), 0, false)?;
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
        self.root_scope.get_or_insert(scope);
        self.effects(
            &[Field {
                name: name.into(),
                value: Value::Block(read_fields(parsed, reader, 1, false)?),
                path: parsed.source().into(),
                location,
            }],
            scope,
        )
    }

    /// Compile explicitly dispatched events with literal IDs and optional option guards.
    /// Semantic fields outside this subset are errors, including automatic selection.
    pub fn compile_events(mut self, document: &TextDocument) -> Result<CompiledEvents, Error> {
        let parsed = document.parse()?;
        let fields = read_fields(&parsed, parsed.reader(), 0, false)?;
        self.events(&fields)
    }

    /// Only selected IDs are compiled; unrelated definitions are not bound content.
    pub(crate) fn compile_selected_events_with_unavailable_options(
        mut self,
        document: &TextDocument,
        selected: &BTreeSet<String>,
    ) -> Result<CompiledEventsWithUnavailableOptions, Error> {
        let compiled = self.compile_selected_events_inner(document, selected, true)?;
        Ok((compiled, self.unavailable_event_options))
    }

    fn compile_selected_events_inner(
        &mut self,
        document: &TextDocument,
        selected: &BTreeSet<String>,
        defer_unsupported_event_options: bool,
    ) -> Result<CompiledEvents, Error> {
        self.defer_unsupported_event_options = defer_unsupported_event_options;
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
                    value: Value::Block(read_fields(&parsed, body, 1, false)?),
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
        let mut option_conditions = BTreeMap::new();
        let mut option_weights = BTreeMap::new();
        let mut pictures = BTreeMap::new();
        let mut locations = BTreeMap::new();
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
            self.root_scope = Some(scope);
            let hidden_event = field
                .block()?
                .iter()
                .find(|part| part.name == "hidden")
                .map(boolean)
                .transpose()?
                .unwrap_or(false);
            let mut seen = std::collections::BTreeSet::new();
            let mut explicit = false;
            let mut condition = None;
            let mut once = false;
            let mut guarded_options = BTreeMap::new();
            let mut weighted_options = BTreeMap::new();
            let mut picture_variants = Vec::new();
            let mut event_locations = EventLocations::default();
            let mut unavailable_options = BTreeMap::new();
            for part in field.block()? {
                self.sources.push((part.path.clone(), part.location));
                if !matches!(part.name.as_str(), "option" | "picture") && !seen.insert(&part.name) {
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
                    "mean_time_to_happen" => self.validate_event_wait(part, scope)?,
                    "hidden" => {
                        let _ = boolean(part)?;
                    }
                    "title" | "desc" => {
                        part.scalar()?;
                    }
                    "picture" => picture_variants.push(self.event_picture(part, scope)?),
                    "goto" => event_locations.event = Some(event_location(part)?),
                    "immediate" => definition.immediate = self.effects(part.block()?, scope)?,
                    "trigger" => condition = Some(self.conditions(part.block()?, scope)?),
                    "after" => definition.after = self.effects(part.block()?, scope)?,
                    "option" => {
                        let mut effects = Vec::new();
                        let mut name = false;
                        let mut option_condition = None;
                        let mut weight = None;
                        let mut location = None;
                        for item in part.block()? {
                            if item.name == "name" {
                                if name || item.scalar()?.is_empty() {
                                    return Err(item.error("invalid option name"));
                                }
                                self.sources.push((item.path.clone(), item.location));
                                name = true;
                            } else if item.name == "trigger" {
                                if option_condition.is_some() {
                                    return Err(item.error("duplicate option trigger"));
                                }
                                self.sources.push((item.path.clone(), item.location));
                                option_condition = Some(self.conditions(item.block()?, scope)?);
                            } else if item.name == "ai_chance" {
                                if weight.is_some() {
                                    return Err(item.error("duplicate option ai_chance"));
                                }
                                weight = Some(self.event_weight(item, scope)?);
                            } else if item.name == "goto" {
                                if location.is_some() {
                                    return Err(item.error("duplicate option goto"));
                                }
                                self.sources.push((item.path.clone(), item.location));
                                location = Some(event_location(item)?);
                            } else {
                                effects.push(item.clone());
                            }
                        }
                        if !name && !hidden_event {
                            return Err(part.error("option requires a scalar name"));
                        }
                        let id = definition.options.len().to_string();
                        if let Some(location) = location {
                            event_locations.options.insert(id.clone(), location);
                        }
                        if let Some(condition) = option_condition {
                            guarded_options.insert(id.clone(), condition);
                        }
                        if let Some(weight) = weight {
                            weighted_options.insert(id.clone(), weight);
                        }
                        let compiled_effects = match self.effects(&effects, scope) {
                            Ok(effects) => effects,
                            Err(error)
                                if self.defer_unsupported_event_options
                                    && error.detail().starts_with("unsupported effect ") =>
                            {
                                unavailable_options.insert(
                                    id.clone(),
                                    UnavailableEventOption {
                                        reason: error.detail().to_owned(),
                                        path: error.path().to_path_buf(),
                                        location: error.location().unwrap_or(part.location),
                                    },
                                );
                                Vec::new()
                            }
                            Err(error) => return Err(error),
                        };
                        definition.options.push(EventOption {
                            id,
                            effects: compiled_effects,
                        });
                    }
                    _ => return Err(part.error(format!("unsupported event field {}", part.name))),
                }
            }
            if !explicit || definition.id.is_empty() || definition.options.is_empty() {
                return Err(field.error("event requires id, is_triggered_only=yes and options"));
            }
            if !unavailable_options.is_empty() {
                self.unavailable_event_options
                    .insert(definition.id.clone(), unavailable_options);
            }
            if !picture_variants.is_empty() {
                let fallback = Some(picture_variants[0].key.clone());
                if pictures
                    .insert(
                        definition.id.clone(),
                        EventPictures {
                            variants: picture_variants,
                            fallback,
                        },
                    )
                    .is_some()
                {
                    return Err(field.error("duplicate event picture identifier"));
                }
            }
            if (event_locations.event.is_some() || !event_locations.options.is_empty())
                && locations
                    .insert(definition.id.clone(), event_locations)
                    .is_some()
            {
                return Err(field.error("duplicate event location identifier"));
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
            if !guarded_options.is_empty()
                && option_conditions
                    .insert(definition.id.clone(), guarded_options)
                    .is_some()
            {
                return Err(field.error("duplicate event option trigger identifier"));
            }
            definitions.push(definition);
            if !weighted_options.is_empty() {
                let id = definitions.last().expect("just inserted").id.clone();
                if option_weights.insert(id, weighted_options).is_some() {
                    return Err(field.error("duplicate event option weight identifier"));
                }
            }
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
            option_conditions,
            option_weights,
            pictures,
            locations,
            once_events,
            sources: std::mem::take(&mut self.sources),
            folded_branches: self.folded,
        })
    }

    /// Explicit calls do not evaluate mean-time declarations. Check their syntax
    /// and references before eliminating them from the executable event content.
    fn validate_event_wait(&mut self, field: &Field, scope: EntityKind) -> Result<(), Error> {
        let mut units = BTreeSet::new();
        for item in field.block()? {
            self.sources.push((item.path.clone(), item.location));
            match item.name.as_str() {
                "days" | "months" | "years" => {
                    if !units.insert(item.name.as_str()) {
                        return Err(item.error("duplicate mean-time unit"));
                    }
                    item.scalar()?.parse::<u32>().map_err(|_| {
                        item.error("mean-time unit requires a nonnegative literal integer")
                    })?;
                }
                "modifier" => {
                    let mut factor = None;
                    let mut conditions = Vec::new();
                    for part in item.block()? {
                        if part.name == "factor" {
                            if factor.is_some() {
                                return Err(part.error("duplicate mean-time modifier factor"));
                            }
                            self.sources.push((part.path.clone(), part.location));
                            factor = Some(crate::setup::micros(part.scalar()?).ok_or_else(|| {
                                part.error("mean-time factor requires a literal decimal with at most six fractional digits")
                            })?);
                        } else {
                            conditions.push(part.clone());
                        }
                    }
                    if factor.is_none() {
                        return Err(item.error("mean-time modifier requires factor"));
                    }
                    self.conditions(&conditions, scope)?;
                }
                _ => return Err(item.error(format!("unsupported mean-time field {}", item.name))),
            }
        }
        if units.len() > 1 {
            return Err(field.error("combined mean-time units are not supported"));
        }
        Ok(())
    }

    fn event_weight(
        &mut self,
        field: &Field,
        scope: EntityKind,
    ) -> Result<augustus_core::EventWeight, Error> {
        self.sources.push((field.path.clone(), field.location));
        let mut weight = augustus_core::EventWeight::default();
        let mut factor = false;
        for item in field.block()? {
            self.sources.push((item.path.clone(), item.location));
            match item.name.as_str() {
                "factor" => {
                    if factor {
                        return Err(item.error("duplicate ai_chance factor"));
                    }
                    weight.factor = weight_factor(item)?;
                    factor = true;
                }
                "modifier" => {
                    let mut factor = None;
                    let mut conditions = Vec::new();
                    for part in item.block()? {
                        if part.name == "factor" {
                            if factor.is_some() {
                                return Err(part.error("duplicate weight modifier factor"));
                            }
                            self.sources.push((part.path.clone(), part.location));
                            factor = Some(weight_factor(part)?);
                        } else {
                            conditions.push(part.clone());
                        }
                    }
                    weight.modifiers.push(augustus_core::WeightModifier {
                        factor: factor
                            .ok_or_else(|| item.error("weight modifier requires factor"))?,
                        condition: self.conditions(&conditions, scope)?,
                    });
                }
                _ => return Err(item.error(format!("unsupported ai_chance field {}", item.name))),
            }
        }
        // Keep all possible modifier branches inside the represented integer range.
        let mut magnitude = i128::from(weight.base.unsigned_abs());
        for modifier in &weight.modifiers {
            let applied =
                magnitude * i128::from(modifier.factor.unsigned_abs()) / i128::from(weight.unit);
            magnitude = match constant(&modifier.condition) {
                Some(false) => magnitude,
                Some(true) => applied,
                None => magnitude.max(applied),
            };
            if magnitude > 300_000 {
                return Err(field
                    .error("ai_chance modifier magnitude exceeds the supported integer subset"));
            }
        }
        if magnitude * i128::from(weight.factor.unsigned_abs()) / i128::from(weight.unit)
            > 1_000_000
        {
            return Err(
                field.error("ai_chance weight magnitude exceeds the supported integer subset")
            );
        }
        Ok(weight)
    }

    fn event_picture(&mut self, field: &Field, scope: EntityKind) -> Result<PictureVariant, Error> {
        let (key, condition) = match &field.value {
            Value::Scalar(key) => (key.clone(), Condition::All(Vec::new())),
            _ => {
                let mut key = None;
                let mut condition = None;
                for part in field.block()? {
                    self.sources.push((part.path.clone(), part.location));
                    match part.name.as_str() {
                        "picture" if key.is_none() => key = Some(part.scalar()?.to_owned()),
                        "trigger" if condition.is_none() => {
                            condition = Some(self.conditions(part.block()?, scope)?);
                        }
                        "picture" | "trigger" => {
                            return Err(
                                part.error(format!("duplicate picture field {}", part.name))
                            );
                        }
                        _ => {
                            return Err(
                                part.error(format!("unsupported picture field {}", part.name))
                            );
                        }
                    }
                }
                (
                    key.ok_or_else(|| field.error("conditional picture requires picture"))?,
                    condition.ok_or_else(|| field.error("conditional picture requires trigger"))?,
                )
            }
        };
        if key.trim().is_empty() {
            return Err(field.error("empty event picture key"));
        }
        Ok(PictureVariant { key, condition })
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
                "ROOT" | "root" => {
                    let kind = self.root_kind(field)?;
                    plan.push(Action::InScope {
                        selector: ScopeSelector::Root,
                        kind,
                        plan: self.effects(field.block()?, kind)?,
                    });
                }
                "every_subject_country" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("subject effects require country scope"));
                    }
                    let mut body = Vec::new();
                    let mut limit = None;
                    for part in field.block()? {
                        if part.name == "limit" {
                            if limit.is_some() {
                                return Err(part.error("duplicate subject limit"));
                            }
                            limit = Some(self.conditions(part.block()?, EntityKind::Country)?);
                        } else {
                            body.push(part.clone());
                        }
                    }
                    plan.push(Action::ForEachSubjectCountry {
                        country: Target::Current,
                        condition: limit.unwrap_or(Condition::Always(true)),
                        plan: self.effects(&body, EntityKind::Country)?,
                    });
                }
                "every_owned_province" | "random_owned_province" | "random_country" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("country scope iteration requires country scope"));
                    }
                    let nested_scope = if field.name == "random_country" {
                        EntityKind::Country
                    } else {
                        EntityKind::Province
                    };
                    let mut body = Vec::new();
                    let mut limit = None;
                    for part in field.block()? {
                        if part.name == "limit" {
                            if limit.is_some() {
                                return Err(part.error("duplicate owned province limit"));
                            }
                            limit = Some(self.conditions(part.block()?, nested_scope)?);
                        } else {
                            body.push(part.clone());
                        }
                    }
                    let condition = limit.unwrap_or(Condition::Always(true));
                    let body = self.effects(&body, nested_scope)?;
                    plan.push(match field.name.as_str() {
                        "every_owned_province" => Action::ForEachOwnedProvince {
                            country: Target::Current,
                            condition,
                            plan: body,
                        },
                        "random_owned_province" => Action::RandomOwnedProvince {
                            country: Target::Current,
                            condition,
                            plan: body,
                        },
                        _ => Action::RandomCountry {
                            condition,
                            plan: body,
                        },
                    });
                }
                "custom_tooltip" => plan.push(Action::Description {
                    key: localisation_key(field, "custom tooltip")?,
                    hidden: self.hidden_descriptions,
                }),
                "add_core" | "add_territorial_core" | "remove_core" => {
                    let (province, country) = self.core_operands(field, scope)?;
                    plan.push(Action::SetProvinceCore {
                        province,
                        country,
                        core: match field.name.as_str() {
                            "add_core" => Some(augustus_core::CoreType::Full),
                            "add_territorial_core" => Some(augustus_core::CoreType::Territorial),
                            _ => None,
                        },
                    });
                }
                name if matches!(name, "heavy_ship" | "light_ship" | "galley" | "transport") => {
                    if scope != EntityKind::Province {
                        return Err(field.error("naval unit creation requires province scope"));
                    }
                    plan.push(Action::CreateNavalUnit {
                        owner: self.typed_country_reference(field)?,
                        unit_type: name.into(),
                    });
                }
                name if matches!(name, "infantry" | "cavalry" | "artillery") => {
                    if scope != EntityKind::Province {
                        return Err(field.error("land unit creation requires province scope"));
                    }
                    plan.push(Action::CreateArmyFromCategories {
                        owner: self.typed_country_reference(field)?,
                        location: augustus_core::EntityRef::Relative(Target::Current),
                        name: String::new(),
                        categories: vec![name.into()],
                    });
                }
                "add_truce_with" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("truce effect requires country scope"));
                    }
                    let second = self.typed_country_reference(field)?;
                    let count = self
                        .truce_months
                        .ok_or_else(|| field.error("default truce duration is unknown"))?;
                    plan.push(Action::GrantTruce {
                        first: augustus_core::EntityRef::Relative(Target::Current),
                        second,
                        term: augustus_core::GrantTerm::Months {
                            count,
                            monthly: true,
                        },
                    });
                }
                "add_manpower" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("manpower effect requires country scope"));
                    }
                    plan.push(Action::UpdateNumeric {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Resource(
                            augustus_core::CountryResource::Manpower,
                        ),
                        operation: augustus_core::NumericOperation::Add,
                        value: manpower_literal(field)?,
                    });
                }
                "add_devastation" => {
                    self.devastation_scope(field, scope)?;
                    plan.push(Action::UpdateNumeric {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Metric("devastation".into()),
                        operation: augustus_core::NumericOperation::Add,
                        value: decimal(field)? / 1000 * 1000,
                    });
                }
                "add_stability" => {
                    self.stability_scope(field, scope)?;
                    plan.push(Action::UpdateNumeric {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Metric("stability".into()),
                        operation: augustus_core::NumericOperation::Add,
                        value: stability_integer(field)?,
                    });
                }
                "add_prestige" => {
                    self.power_scope(field, scope, "prestige")?;
                    plan.push(Action::UpdateNumeric {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Metric("prestige".into()),
                        operation: augustus_core::NumericOperation::Add,
                        value: quantized_decimal(field, "prestige")?,
                    });
                }
                "add_liberty_desire" => {
                    if scope != EntityKind::Country
                        || !self.metric_ids.contains("temporary_liberty_desire")
                    {
                        return Err(field.error("liberty desire requires country scope and a registered temporary metric"));
                    }
                    let value = decimal(field)?;
                    if !(-1_000_000..=1_000_000).contains(&value) {
                        return Err(field.error(
                            "liberty desire effect requires a literal between -1000 and 1000",
                        ));
                    }
                    plan.push(Action::UpdateNumeric {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Metric("temporary_liberty_desire".into()),
                        operation: augustus_core::NumericOperation::Add,
                        value,
                    });
                }
                "add_adm_power" | "add_dip_power" | "add_mil_power" => {
                    let name = field.name.strip_prefix("add_").expect("power effect");
                    self.power_scope(field, scope, name)?;
                    plan.push(Action::UpdateNumeric {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Metric(name.into()),
                        operation: augustus_core::NumericOperation::Add,
                        value: script_integer(field, "monarch power")?,
                    });
                }
                "change_adm" | "change_dip" | "change_mil" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("ruler skill effects require country scope"));
                    }
                    plan.push(Action::ChangeRulerSkill {
                        target: Target::Current,
                        skill: ruler_skill(
                            field.name.strip_prefix("change_").expect("skill effect"),
                        ),
                        delta: script_integer(field, "ruler skill")?,
                    });
                }
                "change_national_focus" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("national focus effects require country scope"));
                    }
                    let skill = match field.scalar()? {
                        "ADM" => Some(augustus_core::RulerSkill::Administration),
                        "DIP" => Some(augustus_core::RulerSkill::Diplomacy),
                        "MIL" => Some(augustus_core::RulerSkill::Military),
                        "none" => None,
                        _ => return Err(field.error("unsupported national focus")),
                    };
                    plan.push(Action::ChangeNationalFocus {
                        target: Target::Current,
                        focus: augustus_core::NationalFocus { skill },
                    });
                }
                name if self.area_scopes.contains_key(name) => {
                    let members = self.area_scopes[name].clone();
                    if members.contains(&0) {
                        return Err(
                            field.error("area scope requires positive province identifiers")
                        );
                    }
                    let mut body = Vec::new();
                    let mut limit = None;
                    for part in field.block()? {
                        if part.name == "limit" {
                            if limit.is_some() {
                                return Err(part.error("duplicate area limit"));
                            }
                            limit = Some(self.conditions(part.block()?, EntityKind::Province)?);
                        } else {
                            body.push(part.clone());
                        }
                    }
                    plan.push(Action::ForEach {
                        selectors: members
                            .into_iter()
                            .map(|id| ScopeSelector::Named {
                                kind: EntityKind::Province,
                                name: id.to_string(),
                            })
                            .collect(),
                        kind: EntityKind::Province,
                        condition: limit.unwrap_or(Condition::Always(true)),
                        plan: self.effects(&body, EntityKind::Province)?,
                    });
                }
                name if province_identifier(name).is_some() => {
                    plan.push(Action::InScope {
                        selector: ScopeSelector::Named {
                            kind: EntityKind::Province,
                            name: province_identifier(name)
                                .expect("matched identifier")
                                .to_string(),
                        },
                        kind: EntityKind::Province,
                        plan: self.effects(field.block()?, EntityKind::Province)?,
                    });
                }
                name if country_identifier(name) && matches!(&field.value, Value::Block(_)) => {
                    plan.push(Action::InScope {
                        selector: ScopeSelector::Named {
                            kind: EntityKind::Country,
                            name: name.into(),
                        },
                        kind: EntityKind::Country,
                        plan: self.effects(field.block()?, EntityKind::Country)?,
                    });
                }
                "change_government" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("government changes require country scope"));
                    }
                    let kind = field.scalar()?;
                    if !self.government_form_ids.contains(kind) {
                        return Err(field.error(format!(
                            "unsupported or undeclared government transition {kind}"
                        )));
                    }
                    let native = self.government_callback("on_native_change_government")?;
                    let ordinary = self.government_callback("on_government_change")?;
                    plan.push(Action::ChangeGovernment {
                        target: Target::Current,
                        kind: kind.into(),
                        before: if native.is_empty() && ordinary.is_empty() {
                            Vec::new()
                        } else {
                            vec![Action::If {
                                condition: Condition::GovernmentFeature {
                                    target: Target::Current,
                                    name: "eu4/native_mechanic".into(),
                                },
                                then: native,
                                otherwise: ordinary,
                            }]
                        },
                    });
                }
                "add_government_reform" | "remove_government_reform" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("government reform effects require country scope"));
                    }
                    let id = field.scalar()?;
                    if !crate::country_markers::literal_name(id) {
                        return Err(field.error("government reform requires a literal identifier"));
                    }
                    let action = if field.name == "add_government_reform" {
                        Action::AddGovernmentReform {
                            target: Target::Current,
                            id: id.into(),
                        }
                    } else {
                        Action::RemoveGovernmentReform {
                            target: Target::Current,
                            id: id.into(),
                        }
                    };
                    plan.push(action);
                }
                "set_government_rank" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("government rank effects require country scope"));
                    }
                    plan.push(Action::SetGovernmentRank {
                        target: Target::Current,
                        rank: government_rank_value(field)?,
                    });
                }
                "set_emperor" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("emperor assignment requires country scope"));
                    }
                    if field.scalar()? != "yes" {
                        return Err(field.error("set_emperor only supports '= yes'"));
                    }
                    plan.push(Action::SetOfficeHolder {
                        office: "imperial_emperor".into(),
                    });
                }
                "add_legitimacy" | "add_republican_tradition" | "add_devotion" => {
                    let name = field.name.strip_prefix("add_").expect("metric effect");
                    self.government_metric(field, scope, name)?;
                    plan.push(Action::If {
                        condition: Condition::GovernmentMetricActive {
                            target: Target::Current,
                            name: name.into(),
                        },
                        then: vec![Action::UpdateNumeric {
                            target: Target::Current,
                            slot: augustus_core::NumericSlot::Government(name.into()),
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
                "set_global_flag" | "clr_global_flag" => {
                    plan.push(Action::SetGlobalMarker {
                        name: self.global_flag_name(field)?,
                        present: field.name == "set_global_flag",
                    });
                }
                "add_country_modifier" => {
                    self.modifier_scope(field, scope)?;
                    let mut name = None;
                    let mut duration = None;
                    let mut hidden = None;
                    let mut duration_label = None;
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
                            "desc" => {
                                duration_label = Some(localisation_key(
                                    parameter,
                                    "modifier duration description",
                                )?);
                            }
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
                        duration_label,
                    });
                }
                "add_casus_belli"
                | "reverse_add_casus_belli"
                | "remove_casus_belli"
                | "reverse_remove_casus_belli" => {
                    let grant = matches!(
                        field.name.as_str(),
                        "add_casus_belli" | "reverse_add_casus_belli"
                    );
                    let (name, selected, months) =
                        self.casus_belli_operands(field, scope, grant)?;
                    let current = augustus_core::EntityRef::Relative(Target::Current);
                    let (issuer, target) = if field.name.starts_with("reverse_") {
                        (selected, current)
                    } else {
                        (current, selected)
                    };
                    plan.push(if grant {
                        Action::GrantJustification {
                            issuer,
                            target,
                            name,
                            term: augustus_core::GrantTerm::Months {
                                count: months,
                                monthly: true,
                            },
                        }
                    } else {
                        Action::RemoveJustification {
                            issuer,
                            target,
                            name,
                        }
                    });
                }
                "remove_country_modifier" | "remove_province_modifier" => {
                    self.modifier_scope(field, scope)?;
                    let name = field.scalar()?;
                    if name.is_empty() || !self.modifier_ids.contains(name) {
                        return Err(field.error(format!("unknown modifier {name}")));
                    }
                    plan.push(Action::RemoveModifier {
                        target: Target::Current,
                        name: name.into(),
                    });
                }
                "set_religious_school" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("religious school effects require country scope"));
                    }
                    let (group, school) = self.school_operands(field)?;
                    match self.religious_school_groups.get(school) {
                        Some(Some(registered)) if registered == group => (),
                        Some(Some(_)) => {
                            return Err(
                                field.error("religious school group does not match its definition")
                            );
                        }
                        Some(None) => {
                            return Err(field.error("ambiguous religious school definition"));
                        }
                        None => return Err(field.error("unregistered religious school definition")),
                    }
                    plan.push(Action::If {
                        condition: Condition::AffiliationGroup {
                            target: Target::Current,
                            dimension: "religion".into(),
                            group: group.into(),
                        },
                        then: vec![Action::SetAffiliation {
                            target: Target::Current,
                            dimension: "religious_school".into(),
                            member: Some(school.into()),
                        }],
                        otherwise: Vec::new(),
                    });
                }
                "set_school_opinion" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("school opinions require country scope"));
                    }
                    let mut target = None;
                    let mut opinion = None;
                    for part in field.block()? {
                        self.sources.push((part.path.clone(), part.location));
                        match part.name.as_str() {
                            "who" if target.is_none() => {
                                target = Some(if part.scalar()?.eq_ignore_ascii_case("PREV") {
                                    augustus_core::SchoolOpinionTarget::Previous
                                } else {
                                    augustus_core::SchoolOpinionTarget::Country(
                                        self.typed_country_reference(part)?,
                                    )
                                });
                            }
                            "opinion" if opinion.is_none() => {
                                let value = decimal(part)?;
                                if value % 1000 != 0 {
                                    return Err(part.error("school opinion must be an integer"));
                                }
                                opinion = Some(value / 1000);
                            }
                            "who" | "opinion" => {
                                return Err(part.error("duplicate school opinion field"));
                            }
                            _ => return Err(part.error("unexpected school opinion field")),
                        }
                    }
                    plan.push(Action::SetSchoolOpinion {
                        target: target.ok_or_else(|| field.error("school opinion requires who"))?,
                        opinion: opinion
                            .ok_or_else(|| field.error("school opinion requires opinion"))?,
                    });
                }
                "swap_non_generic_missions" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("mission refresh requires country scope"));
                    }
                    boolean(field)?;
                    plan.push(Action::RefreshObjectives);
                }
                "complete_mission" => plan.push(Action::RecordCompletion {
                    id: self.mission_id(field, scope)?,
                }),
                "hidden_effect" => {
                    let body = field.block()?;
                    let previous = std::mem::replace(&mut self.hidden_descriptions, true);
                    let compiled = self.effects(body, scope);
                    self.hidden_descriptions = previous;
                    plan.extend(compiled?);
                }
                "tooltip" => plan.push(Action::Preview {
                    plan: self.effects(field.block()?, scope)?,
                }),
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
                    if let Some(slot) = &self.treasury_slot {
                        if matches!(
                            slot,
                            augustus_core::NumericSlot::Attribute { .. }
                                | augustus_core::NumericSlot::StoredGovernment(_)
                                | augustus_core::NumericSlot::SubjectCount
                                | augustus_core::NumericSlot::RulerSkill(_)
                        ) {
                            return Err(field.error("treasury binding is read only"));
                        }
                        plan.push(Action::UpdateNumeric {
                            target: Target::Current,
                            slot: slot.clone(),
                            operation: augustus_core::NumericOperation::Add,
                            value: cash_literal(field)?,
                        });
                    } else {
                        plan.push(Action::Add {
                            target: Target::Current,
                            field: self.bind(field, "treasury")?,
                            value: decimal(field)?,
                        });
                    }
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
                    plan.extend(self.branch_chain(fields, &mut index, scope)?);
                }
                name if self.library.effects.contains_key(name) => {
                    let mut definition = self.library.effects[name].clone();
                    match &field.value {
                        Value::Scalar(value) if value == "yes" => (),
                        Value::Block(parameters) => {
                            let mut bindings = BTreeMap::new();
                            for parameter in parameters {
                                let value = parameter.scalar()?.to_owned();
                                if bindings.insert(parameter.name.clone(), value).is_some() {
                                    return Err(
                                        parameter.error("duplicate scripted effect parameter")
                                    );
                                }
                            }
                            let used =
                                definition.substitute_parameters(&bindings).ok_or_else(|| {
                                    field.error("scripted effect call has unbound parameters")
                                })?;
                            if used.is_empty() && !bindings.is_empty() {
                                return Err(field.error("expected scalar"));
                            }
                            if bindings.keys().any(|name| !used.contains(name)) {
                                return Err(
                                    field.error("scripted effect call has unused parameters")
                                );
                            }
                        }
                        _ => {
                            return Err(
                                field.error("script call requires yes or a parameter block")
                            );
                        }
                    }
                    self.enter(field, format!("effect:{name}"))?;
                    let expanded = self.effects(definition.block()?, scope);
                    self.stack.pop();
                    plan.extend(expanded?);
                }
                name => return Err(field.error(format!("unsupported effect {name}"))),
            }
            index += 1;
        }
        Ok(plan)
    }

    fn conditions(&mut self, fields: &[Field], scope: EntityKind) -> Result<Condition, Error> {
        let mut conditions = Vec::new();
        let mut index = 0;
        while index < fields.len() {
            let field = &fields[index];
            self.sources.push((field.path.clone(), field.location));
            conditions.push(match field.name.as_str() {
                "if" => self.condition_branch_chain(fields, &mut index, scope)?,
                "else" | "else_if" => return Err(field.error("conditional branch requires a preceding if")),
                "ROOT" | "root" => {
                    let kind = self.root_kind(field)?;
                    Condition::InScope {
                        selector: ScopeSelector::Root,
                        kind,
                        condition: Box::new(self.conditions(field.block()?, kind)?),
                    }
                }
                "is_subject_of" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("subject queries require country scope"));
                    }
                    Condition::SubjectOf {
                        subject: augustus_core::EntityRef::Relative(Target::Current),
                        overlord: self.subject_reference(field)?,
                    }
                }
                "num_of_subjects" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("subject count requires country scope"));
                    }
                    let text = field.scalar()?;
                    let count = text
                        .parse::<u32>()
                        .ok()
                        .filter(|_| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
                        .ok_or_else(|| field.error("subject count requires a nonnegative literal integer"))?;
                    Condition::NumericCompare {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::SubjectCount,
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value: i64::from(count),
                    }
                }
                "any_subject_country" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("subject queries require country scope"));
                    }
                    Condition::AnySubjectCountry {
                        target: Target::Current,
                        condition: Box::new(self.conditions(field.block()?, EntityKind::Country)?),
                    }
                }
                "is_subject_of_type" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("subject type queries require country scope"));
                    }
                    let subject_type = field.scalar()?;
                    if !crate::country_markers::literal_name(subject_type) {
                        return Err(field.error("subject type requires a literal identifier"));
                    }
                    if self.subject_type_ids.as_ref().is_some_and(|ids| !ids.contains(subject_type)) {
                        return Err(field.error(format!("unregistered subject type {subject_type}")));
                    }
                    Condition::SubjectType {target: Target::Current, subject_type: subject_type.into()}
                }
                "any_owned_province" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("owned province queries require country scope"));
                    }
                    Condition::AnyOwnedProvince {
                        target: Target::Current,
                        condition: Box::new(self.conditions(field.block()?, EntityKind::Province)?),
                    }
                }
                "devastation" => {
                    self.devastation_scope(field, scope)?;
                    Condition::NumericCompare {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Metric("devastation".into()),
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value: decimal(field)?,
                    }
                }
                "stability" => {
                    self.stability_scope(field, scope)?;
                    Condition::NumericCompare {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Metric("stability".into()),
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value: stability_integer(field)?,
                    }
                }
                "prestige" => {
                    self.power_scope(field, scope, "prestige")?;
                    let value = field.scalar()?;
                    let slot = augustus_core::NumericSlot::Metric("prestige".into());
                    if country_operand(value) {
                        Condition::NumericCompareEntities {
                            first: augustus_core::EntityRef::Relative(Target::Current),
                            second: self.typed_country_reference(field)?,
                            slot,
                            comparison: augustus_core::NumericComparison::AtLeast,
                        }
                    } else {
                        Condition::NumericCompare {
                            target: Target::Current,
                            slot,
                            comparison: augustus_core::NumericComparison::AtLeast,
                            value: quantized_decimal(field, "prestige")?,
                        }
                    }
                }
                "is_subject" => {
                    if scope != EntityKind::Country { return Err(field.error("subject query requires country scope")); }
                    let condition=Condition::SubjectPresent {target:Target::Current};
                    if boolean(field)? { condition } else {Condition::Not(Box::new(condition))}
                }
                "truce_with" => {
                    if scope != EntityKind::Country { return Err(field.error("truce query requires country scope")); }
                    Condition::TruceWith { first: augustus_core::EntityRef::Relative(Target::Current), second:self.typed_country_reference(field)? }
                }
                "manpower" => {
                    if scope != EntityKind::Country { return Err(field.error("manpower query requires country scope")); }
                    Condition::NumericCompare {
                        target:Target::Current,
                        slot:augustus_core::NumericSlot::Resource(augustus_core::CountryResource::Manpower),
                        comparison:augustus_core::NumericComparison::AtLeast,
                        value:manpower_literal(field)?,
                    }
                }
                "has_truce" => {
                    if scope != EntityKind::Country { return Err(field.error("truce presence query requires country scope")); }
                    let condition=Condition::TrucePresent {target:Target::Current};
                    if boolean(field)? {condition} else {Condition::Not(Box::new(condition))}
                }
                "liberty_desire" => {
                    if scope != EntityKind::Country { return Err(field.error("liberty desire query requires country scope")); }
                    let threshold=decimal(field)?;
                    Condition::Any(vec![
                        Condition::All(vec![Condition::SubjectPresent {target:Target::Current},
                            Condition::NumericCompare {target:Target::Current,
                                slot:augustus_core::NumericSlot::Attribute {name:"liberty_desire".into(),divisor:1000},
                                comparison:augustus_core::NumericComparison::AtLeast,value:threshold}]),
                        Condition::All(vec![Condition::Not(Box::new(Condition::SubjectPresent {target:Target::Current})),Condition::Always(threshold<=0)])
                    ])
                }
                "adm_power" | "dip_power" | "mil_power" => {
                    self.power_scope(field, scope, &field.name)?;
                    Condition::NumericCompare {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Metric(field.name.clone()),
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value: script_integer(field, "monarch power")?,
                    }
                }
                "adm" | "dip" | "mil" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("ruler skill queries require country scope"));
                    }
                    Condition::NumericCompare {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::RulerSkill(ruler_skill(&field.name)),
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value: script_integer(field, "ruler skill")?,
                    }
                }
                "monthly_adm" | "monthly_dip" | "monthly_mil" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("monthly income queries require country scope"));
                    }
                    Condition::NumericCompare { target: Target::Current,
                        slot: augustus_core::NumericSlot::Attribute { name: field.name.clone(), divisor: 1_000_000 },
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value: script_integer(field, "monthly income")? }
                }
                "national_focus" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("national focus queries require country scope"));
                    }
                    let skill = match field.scalar()? {
                        "ADM" => augustus_core::RulerSkill::Administration,
                        "DIP" => augustus_core::RulerSkill::Diplomacy,
                        "MIL" => augustus_core::RulerSkill::Military,
                        _ => return Err(field.error("unsupported national focus query")),
                    };
                    Condition::NationalFocus { target: Target::Current, skill }
                }
                "province_id" => {
                    if scope != EntityKind::Province { return Err(field.error("province_id requires province scope")); }
                    let id = province_identifier(field.scalar()?)
                        .ok_or_else(|| field.error("province_id requires a positive literal identifier"))?;
                    Condition::EntityNamed {target: Target::Current, name: id.to_string()}
                }
                "has_port" => {
                    if scope != EntityKind::Province {
                        return Err(field.error("has_port requires province scope"));
                    }
                    let condition = Condition::ProvinceHasPort;
                    if boolean(field)? { condition } else { Condition::Not(Box::new(condition)) }
                }
                name if province_identifier(name).is_some() => Condition::InScope {
                    selector: ScopeSelector::Named {kind: EntityKind::Province,
                        name: province_identifier(name).expect("matched identifier").to_string()},
                    kind: EntityKind::Province,
                    condition: Box::new(self.conditions(field.block()?, EntityKind::Province)?),
                },
                "capital_scope" | "owner" => {
                    let (target, required, nested) = if field.name == "capital_scope" {
                        (Target::Capital, EntityKind::Country, EntityKind::Province)
                    } else { (Target::Owner, EntityKind::Province, EntityKind::Country) };
                    if scope != required { return Err(field.error("invalid entity type for condition scope transition")); }
                    Condition::With { target, kind: nested, condition: Box::new(self.conditions(field.block()?, nested)?) }
                }
                "emperor" => Condition::InScope {
                    selector: ScopeSelector::Office("imperial_emperor".into()), kind: EntityKind::Country,
                    condition: Box::new(self.conditions(field.block()?, EntityKind::Country)?),
                },
                name if name.starts_with("event_target:") => {
                    let name = name.strip_prefix("event_target:").expect("matched prefix");
                    let kind = self.global_scope_types.get(name).copied()
                        .ok_or_else(|| field.error("global event target condition requires a declared entity type"))?;
                    Condition::InScope { selector: ScopeSelector::Global(name.into()), kind,
                        condition: Box::new(self.conditions(field.block()?, kind)?) }
                }
                "area" | "region" | "superregion" | "continent" => {
                    if scope != EntityKind::Province { return Err(field.error("geographic conditions require province scope")); }
                    let name = field.scalar()?;
                    if name.trim().is_empty() || name.chars().any(|c| matches!(c, '$' | '@' | ':' | '.' | '[' | ']'))
                        || ["ROOT", "FROM", "PREV", "THIS"].iter().any(|word| name.eq_ignore_ascii_case(word)) {
                        return Err(field.error("geographic condition requires a literal identifier"));
                    }
                    let level = match field.name.as_str() {
                        "area" => augustus_core::GeographicLevel::Area,
                        "region" => augustus_core::GeographicLevel::Region,
                        "superregion" => augustus_core::GeographicLevel::Superregion,
                        _ => augustus_core::GeographicLevel::Continent,
                    };
                    Condition::GeographicMember { target: Target::Current, level, name: name.into() }
                }
                "ai" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("ai conditions require country scope"));
                    }
                    Condition::CountryControlled {
                        target: Target::Current,
                        controller: if boolean(field)? { augustus_core::CountryController::Ai }
                            else { augustus_core::CountryController::Human },
                    }
                }
                "is_core" | "is_territorial_core" => {
                    let (province, country) = self.core_operands(field, scope)?;
                    Condition::ProvinceCore {
                        province,
                        country,
                        core: (field.name == "is_territorial_core").then_some(augustus_core::CoreType::Territorial),
                    }
                }
                "has_casus_belli" => {
                    let (name, target, _) = self.casus_belli_operands(field, scope, false)?;
                    if !self.casus_belli_types[&name].triggered_only {
                        return Err(field.error("automatic casus belli conditions are not implemented"));
                    }
                    Condition::HasJustification { issuer: augustus_core::EntityRef::Relative(Target::Current), target, name: Some(name) }
                }
                "has_casus_belli_against" => {
                    if scope != EntityKind::Country { return Err(field.error("casus belli requires country scope")); }
                    if self.casus_belli_types.is_empty() || self.casus_belli_types.values().any(|data| !data.triggered_only) {
                        return Err(field.error("automatic casus belli conditions are not implemented"));
                    }
                    Condition::HasJustification { issuer: augustus_core::EntityRef::Relative(Target::Current), target: self.country_reference(field)?, name: None }
                }
                "exists" => {
                    let value = field.scalar()?;
                    let owned = Condition::HasOwnedProvince { target: Target::Current };
                    match value {
                        "yes" | "no" => {
                            let condition = if scope == EntityKind::Country { owned }
                                else { Condition::Always(false) };
                            if value == "yes" { condition } else { Condition::Not(Box::new(condition)) }
                        }
                        tag if country_identifier(tag) => {
                            Condition::InScope {
                                selector: ScopeSelector::Named { kind: EntityKind::Country, name: tag.into() },
                                kind: EntityKind::Country,
                                condition: Box::new(owned),
                            }
                        }
                        _ => return Err(field.error("exists requires yes, no, or a literal three-character country identifier; dynamic operands are unsupported")),
                    }
                }
                "is_part_of_hre" => {
                    let membership = Condition::AffiliationPresent {
                        target: Target::Current,
                        dimension: crate::imperial_membership::DIMENSION.into(),
                    };
                    let condition = if scope == EntityKind::Country {
                        Condition::With {
                            target: Target::Capital,
                            kind: EntityKind::Province,
                            condition: Box::new(membership),
                        }
                    } else {
                        membership
                    };
                    if boolean(field)? { condition } else { Condition::Not(Box::new(condition)) }
                }
                "has_religious_school" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("religious school conditions require country scope"));
                    }
                    let condition = Condition::AffiliationPresent { target: Target::Current, dimension: "religious_school".into() };
                    if boolean(field)? { condition } else { Condition::Not(Box::new(condition)) }
                }
                "religious_school" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("religious school conditions require country scope"));
                    }
                    let (group, school) = self.school_operands(field)?;
                    Condition::All(vec![
                        Condition::AffiliationMember { target: Target::Current, dimension: "religious_school".into(), member: school.into() },
                        Condition::AffiliationGroup { target: Target::Current, dimension: "religious_school".into(), group: group.into() },
                    ])
                }
                "religion" | "religion_group" => {
                    let value = field.scalar()?;
                    if value.trim().is_empty()
                        || value.chars().any(|c| matches!(c, '$' | '@' | '[' | ']' | ':' | '.'))
                        || value.bytes().all(|b| b.is_ascii_digit())
                        || (value.len() == 3 && value.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()))
                        || ["root", "from", "prev", "this", "secondary", "dominant", "heretic", "heathen"].iter().any(|name| value.eq_ignore_ascii_case(name))
                    {
                        return Err(field.error("religion conditions require a literal religion or group identifier; dynamic references are unsupported"));
                    }
                    if field.name == "religion" {
                        Condition::AffiliationMember { target: Target::Current, dimension: "religion".into(), member: value.into() }
                    } else {
                        Condition::AffiliationGroup { target: Target::Current, dimension: "religion".into(), group: value.into() }
                    }
                }
                "primary_culture" | "culture_group" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("culture conditions require country scope"));
                    }
                    let value = field.scalar()?;
                    if value.trim().is_empty()
                        || value.chars().any(|c| matches!(c, '$' | '@' | '[' | ']' | ':' | '.'))
                        || value.bytes().all(|b| b.is_ascii_digit())
                    {
                        return Err(field.error("culture conditions require a literal identifier"));
                    }
                    if field.name == "primary_culture" {
                        Condition::AffiliationMember {
                            target: Target::Current,
                            dimension: "culture".into(),
                            member: value.into(),
                        }
                    } else {
                        Condition::AffiliationGroup {
                            target: Target::Current,
                            dimension: "culture".into(),
                            group: value.into(),
                        }
                    }
                }
                "technology_group" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("technology group conditions require country scope"));
                    }
                    let value = field.scalar()?;
                    if value.trim().is_empty()
                        || value.chars().any(|c| matches!(c, '$' | '@' | '[' | ']' | ':' | '.'))
                        || value.bytes().all(|b| b.is_ascii_digit())
                    {
                        return Err(field.error("technology group requires a literal identifier"));
                    }
                    Condition::AffiliationMember {
                        target: Target::Current,
                        dimension: "technology_group".into(),
                        member: value.into(),
                    }
                }
                "government" | "has_reform" | "has_government_attribute" | "has_government_mechanic" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("government conditions require country scope"));
                    }
                    government_condition(&field.name, field.scalar()?)
                        .map_err(|message| field.error(message))?
                }
                "government_rank" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("government rank conditions require country scope"));
                    }
                    Condition::GovernmentRank {
                        target: Target::Current,
                        rank: government_rank_value(field)?,
                    }
                }
                "has_idea" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("idea conditions require country scope"));
                    }
                    let id = field.scalar()?;
                    if id.trim().is_empty()
                        || id.chars().any(|c| matches!(c, '$' | '@' | '[' | ']'))
                    {
                        return Err(field.error("idea condition requires a literal identifier"));
                    }
                    Condition::HasIdea {
                        target: Target::Current,
                        id: id.into(),
                    }
                }
                "has_estate_privilege" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("estate privilege conditions require country scope"));
                    }
                    let privilege = field.scalar()?;
                    if privilege.trim().is_empty()
                        || privilege.chars().any(|c| matches!(c, '$' | '@' | '[' | ']'))
                    {
                        return Err(field.error("estate privilege requires a literal identifier"));
                    }
                    Condition::EstatePrivilege {
                        target: Target::Current,
                        privilege: privilege.into(),
                    }
                }
                "estate_loyalty" | "estate_influence" => {
                    estate_metric_condition(field, scope)?
                }
                "mercantilism" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("mercantilism conditions require country scope"));
                    }
                    let value = decimal(field)?;
                    if !(0..=100_000).contains(&value) {
                        return Err(field.error("mercantilism threshold must be between 0 and 100"));
                    }
                    Condition::NumericCompare {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::Resource(
                            augustus_core::CountryResource::Mercantilism,
                        ),
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value,
                    }
                }
                "legitimacy" | "republican_tradition" | "devotion" => {
                    let name = &field.name;
                    self.government_metric(field, scope, name)?;
                    let value = decimal(field)?;
                    if !(0..=100_000).contains(&value) {
                        return Err(field.error(format!("{name} threshold must be between 0 and 100")));
                    }
                    let stored = Condition::GovernmentMetricStored {
                        target: Target::Current,
                        name: name.clone(),
                    };
                    let compare = Condition::NumericCompare {
                        target: Target::Current,
                        slot: augustus_core::NumericSlot::StoredGovernment(name.clone()),
                        comparison: augustus_core::NumericComparison::AtLeast,
                        value,
                    };
                    if value == 0 {
                        Condition::Any(vec![Condition::Not(Box::new(stored)), compare])
                    } else {
                        Condition::All(vec![stored, compare])
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
                "has_global_flag" => Condition::HasGlobalMarker {
                    name: self.global_flag_name(field)?,
                },
                "has_dlc" => {
                    let expansion = field.scalar()?;
                    if !crate::is_known_dlc(expansion) {
                        return Err(field.error(format!("unsupported expansion {expansion}")));
                    }
                    Condition::Always(true)
                }
                "map_setup" => {
                    if field.scalar()? != "map_setup_random" {
                        return Err(field.error("unsupported map setup identifier"));
                    }
                    Condition::Always(false)
                }
                "has_mission" => Condition::ObjectiveAvailable {
                    target: Target::Current,
                    id: self.mission_id(field, scope)?,
                },
                "mission_completed" => Condition::CompletionRecorded {
                    target: Target::Current,
                    id: self.mission_id(field, scope)?,
                },
                "not_in_mission_preview_mode" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("mission preview trigger requires country scope"));
                    }
                    let arguments = field.block()?;
                    if arguments.len() != 1 || arguments[0].name != "key" {
                        return Err(field.error("mission preview trigger requires exactly one key"));
                    }
                    let key = arguments[0].scalar()?;
                    if key.is_empty() || key.chars().any(|character| matches!(character, '$' | '@')) {
                        return Err(arguments[0].error("mission preview key must be a literal identifier"));
                    }
                    // Augustus evaluates live game state, where mission preview mode is inactive.
                    Condition::Always(true)
                }
                "tag" | "TAG" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("tag trigger requires country scope"));
                    }
                    let tag = field.scalar()?;
                    if !country_identifier(tag) {
                        return Err(field.error(
                            "tag trigger requires a literal three-character country identifier",
                        ));
                    }
                    Condition::EntityNamed {
                        target: Target::Current,
                        name: tag.into(),
                    }
                }
                "was_tag" => {
                    if scope != EntityKind::Country {
                        return Err(field.error("tag history conditions require country scope"));
                    }
                    let tag = field.scalar()?;
                    if !country_identifier(tag) {
                        return Err(field.error("was_tag requires a literal three-character country identifier"));
                    }
                    Condition::Always(false)
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
                "has_country_modifier" | "has_province_modifier" => {
                    self.modifier_scope(field, scope)?;
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
                    let value = field.scalar()?;
                    if country_operand(value) {
                        Condition::NumericCompareEntities {
                            first: augustus_core::EntityRef::Relative(Target::Current),
                            second: self.typed_country_reference(field)?,
                            slot: self.treasury_slot.clone().map_or_else(
                                || self.bind(field, "treasury").map(augustus_core::NumericSlot::Field),
                                Ok,
                            )?,
                            comparison: augustus_core::NumericComparison::AtLeast,
                        }
                    } else if let Some(slot) = &self.treasury_slot {
                        Condition::NumericCompare {
                            target: Target::Current,
                            slot: slot.clone(),
                            comparison: augustus_core::NumericComparison::AtLeast,
                            value: cash_literal(field)?,
                        }
                    } else {
                        Condition::AtLeast {
                            target: Target::Current,
                            field: self.bind(field, "treasury")?,
                            value: decimal(field)?,
                        }
                    }
                }
                name if country_identifier(name) && matches!(&field.value, Value::Block(_)) => Condition::InScope {
                    selector: ScopeSelector::Named {kind: EntityKind::Country, name: name.into()},
                    kind: EntityKind::Country,
                    condition: Box::new(self.conditions(field.block()?, EntityKind::Country)?),
                },
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
            index += 1;
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

fn weight_factor(field: &Field) -> Result<i64, Error> {
    let value = decimal(field)?;
    if value % 1_000 != 0 {
        return Err(field.error("fractional ai_chance factors are not supported"));
    }
    if value.unsigned_abs() > 10_000_000 {
        return Err(field.error("ai_chance factors outside -10000..10000 are unsupported"));
    }
    Ok(value)
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

fn localisation_key(field: &Field, purpose: &str) -> Result<String, Error> {
    let key = field.scalar()?;
    if key.trim().is_empty() || key.chars().any(|c| matches!(c, '$' | '@' | '[' | ']')) {
        return Err(field.error(format!("{purpose} requires a literal localisation key")));
    }
    Ok(key.into())
}

/// Integer script operands discard fractional digits toward zero.
fn stability_integer(field: &Field) -> Result<i64, Error> {
    script_integer(field, "stability")
}

fn government_rank_value(field: &Field) -> Result<u8, Error> {
    let value = decimal(field)?;
    if !(1000..=3000).contains(&value) || value % 1000 != 0 {
        return Err(field.error("government rank must be an integer from 1 to 3"));
    }
    Ok((value / 1000) as u8)
}

fn estate_metric_condition(field: &Field, scope: EntityKind) -> Result<Condition, Error> {
    if scope != EntityKind::Country {
        return Err(field.error("estate metric conditions require country scope"));
    }
    let metric = match field.name.as_str() {
        "estate_loyalty" => "loyalty",
        "estate_influence" => "influence",
        _ => unreachable!("caller selects an estate metric trigger"),
    };
    let mut estate = None;
    let mut threshold = None;
    for operand in field.block()? {
        match operand.name.as_str() {
            "estate" if estate.is_none() => {
                let id = operand.scalar()?;
                if !crate::country_markers::literal_name(id) {
                    return Err(operand.error("estate metric requires a literal estate identifier"));
                }
                estate = Some(id.to_owned());
            }
            "estate" => return Err(operand.error("duplicate estate metric estate")),
            name if name == metric && threshold.is_none() => {
                let value = decimal(operand)?;
                if value < 0 || (metric == "loyalty" && value > 100_000) {
                    return Err(
                        operand.error(format!("{metric} threshold is outside its supported range"))
                    );
                }
                threshold = Some(value);
            }
            name if name == metric => {
                return Err(operand.error(format!("duplicate estate {metric} threshold")));
            }
            _ => return Err(operand.error(format!("unexpected field in {metric} condition"))),
        }
    }
    let estate = estate.ok_or_else(|| field.error("estate metric condition is missing estate"))?;
    let threshold = threshold
        .ok_or_else(|| field.error(format!("estate metric condition is missing {metric}")))?;
    Ok(if metric == "loyalty" {
        Condition::EstateLoyaltyAtLeast {
            target: Target::Current,
            estate,
            loyalty_milli: threshold,
        }
    } else {
        Condition::EstateInfluenceAtLeast {
            target: Target::Current,
            estate,
            influence_milli: threshold,
        }
    })
}

fn script_integer(field: &Field, purpose: &str) -> Result<i64, Error> {
    let value = decimal(field)? / 1000;
    i32::try_from(value).map_err(|_| field.error(format!("{purpose} integer overflow")))?;
    Ok(value)
}

fn ruler_skill(name: &str) -> augustus_core::RulerSkill {
    match name {
        "adm" => augustus_core::RulerSkill::Administration,
        "dip" => augustus_core::RulerSkill::Diplomacy,
        "mil" => augustus_core::RulerSkill::Military,
        _ => unreachable!("matched ruler skill"),
    }
}

/// Convert thousands of people to stored thousandths of a person, discarding
/// each operand's fractional people.
fn manpower_literal(field: &Field) -> Result<i64, Error> {
    crate::setup::micros(field.scalar()?)
        .map(|value| value / 1000 * 1000)
        .ok_or_else(|| {
            field.error("manpower requires a literal decimal with at most six fractional digits")
        })
}

fn cash_literal(field: &Field) -> Result<i64, Error> {
    quantized_decimal(field, "treasury")
}

fn country_operand(value: &str) -> bool {
    value.eq_ignore_ascii_case("ROOT")
        || value == "emperor"
        || value.starts_with("event_target:")
        || (country_identifier(value) && value.bytes().any(|byte| byte.is_ascii_alphabetic()))
}

fn quantized_decimal(field: &Field, name: &str) -> Result<i64, Error> {
    crate::setup::micros(field.scalar()?)
        .map(|value| value / 1000)
        .ok_or_else(|| {
            field.error(format!(
                "{name} requires a literal decimal with at most six fractional digits"
            ))
        })
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

pub(crate) fn country_identifier(name: &str) -> bool {
    name.len() == 3
        && name
            .bytes()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
}

fn province_identifier(name: &str) -> Option<u32> {
    if name.is_empty() || !name.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    name.parse().ok().filter(|id| *id > 0)
}

fn event_location(field: &Field) -> Result<String, Error> {
    province_identifier(field.scalar()?)
        .map(|id| id.to_string())
        .ok_or_else(|| field.error("goto requires a positive literal province identifier; contextual locations are not supported"))
}

fn constant(condition: &Condition) -> Option<bool> {
    match condition {
        Condition::If {
            condition,
            then,
            otherwise,
        } => constant(condition).and_then(|value| constant(if value { then } else { otherwise })),
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
            | Action::ChangeGovernment { before: plan, .. }
            | Action::InScope { plan, .. }
            | Action::ForEach { plan, .. }
            | Action::ForEachOwnedProvince { plan, .. }
            | Action::ForEachSubjectCountry { plan, .. }
            | Action::RandomOwnedProvince { plan, .. }
            | Action::Schedule { plan, .. }
            | Action::Preview { plan } => simplify_event_calls(plan, unconditional),
            _ => (),
        }
    }
}

pub(crate) fn government_condition(name: &str, value: &str) -> Result<Condition, &'static str> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
        || value.bytes().all(|c| c.is_ascii_digit())
    {
        return Err("government conditions require a literal identifier");
    }
    let target = Target::Current;
    match name {
        "government" => {
            if matches!(value, "dictatorship" | "has_harem") {
                return Err("special government aliases are unsupported");
            }
            if (value.len() == 3
                && value
                    .bytes()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
                || ["root", "from", "prev", "this", "emperor"]
                    .iter()
                    .any(|v| value.eq_ignore_ascii_case(v))
            {
                return Err("dynamic government references are unsupported");
            }
            Ok(Condition::GovernmentKind {
                target,
                kind: value.into(),
            })
        }
        "has_reform" => Ok(Condition::GovernmentReform {
            target,
            id: value.into(),
        }),
        "has_government_attribute" => Ok(Condition::GovernmentFeature {
            target,
            name: value.into(),
        }),
        "has_government_mechanic" => Ok(Condition::GovernmentFeature {
            target,
            name: crate::governments::mechanic_feature_name(value),
        }),
        _ => unreachable!("government trigger name"),
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
            | Action::ChangeGovernment { before: plan, .. }
            | Action::InScope { plan, .. }
            | Action::ForEach { plan, .. }
            | Action::ForEachOwnedProvince { plan, .. }
            | Action::ForEachSubjectCountry { plan, .. }
            | Action::RandomOwnedProvince { plan, .. }
            | Action::Schedule { plan, .. }
            | Action::Preview { plan } => event_references(plan, references),
            _ => (),
        }
    }
}
