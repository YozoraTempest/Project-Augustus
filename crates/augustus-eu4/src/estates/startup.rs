// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use super::initialization::HistoryMetric;
use super::{Condition, ENCODING, EstateReport, Reader};
use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::{PropertyDiagnostics, block, micros};
use crate::{ContentRoot, Error, FileEntry, LoadedSource, ParsedText, TextDocument};
use augustus_core::{Action, EntityKind, EventCatalog, InitialWorld, Schema, ScopeSelector};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

enum Step {
    Metric(Action, LoadedSource),
    Privilege(String, bool, LoadedSource),
    Branch(Vec<(Condition, Vec<Self>)>, Option<Vec<Self>>, LoadedSource),
    Countries(Condition, Vec<Self>, LoadedSource),
    Country(String, Vec<Self>, LoadedSource),
    Scoped(ScopeSelector, Vec<Self>, LoadedSource),
    Group(Vec<Self>),
    Pending(String, LoadedSource),
}

struct Callback {
    valid: Condition,
    granted: Vec<Step>,
    revoked: Vec<Step>,
    unsupported_land: bool,
    unsupported_provinces: bool,
}

struct Compiler {
    schema: Schema,
    effects: Vec<Arc<TextDocument>>,
    effect_index: BTreeMap<String, Option<usize>>,
    triggers: BTreeMap<String, Condition>,
    visiting: Vec<String>,
    needed_privileges: BTreeSet<String>,
    startup_metadata: bool,
    mission_ids: BTreeSet<String>,
    modifier_ids: BTreeSet<String>,
    government_metric_ids: BTreeSet<String>,
}

pub(crate) struct EstateStartup {
    steps: Vec<Step>,
    callbacks: BTreeMap<String, Callback>,
    events: EventCatalog,
    mission_ids: BTreeSet<String>,
}

impl Compiler {
    fn effects(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        report: &mut EstateReport,
        depth: usize,
        skip_limit: bool,
    ) -> Result<Vec<Step>, Error> {
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "startup effect nesting exceeds limit",
            ));
        }
        let mut result = Vec::new();
        for (key, op, value) in reader.fields() {
            let name = key.read_string();
            let at = parsed.field_location(&key);
            let location = source(parsed, at);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            equal(op, &fail)?;
            match name.as_str() {
                "add_legitimacy" => {
                    let library = crate::ScriptLibrary::new();
                    let compiled = crate::ScriptCompiler::new(&self.schema, &library, true)
                        .with_government_metric_ids(self.government_metric_ids.clone())
                        .compile_scalar_effect_reader(
                            parsed,
                            &name,
                            value
                                .read_string()
                                .map_err(|_| fail("expected scalar government metric value"))?,
                            at,
                            EntityKind::Country,
                        );
                    match compiled {
                        Ok(plan) => result.extend(
                            plan.into_iter()
                                .map(|action| Step::Metric(action, location.clone())),
                        ),
                        Err(error) => result.push(Step::Pending(
                            format!("uncompiled_startup_effect:{error}"),
                            source(parsed, error.location().unwrap_or(at)),
                        )),
                    }
                }
                "set_country_flag" | "clr_country_flag" => {
                    let id = value
                        .read_string()
                        .map_err(|_| fail("expected country flag name"))?;
                    if !crate::country_markers::literal_name(&id) {
                        return Err(fail("country flag requires a literal name"));
                    }
                    result.push(Step::Metric(
                        Action::SetMarker {
                            target: augustus_core::Target::Current,
                            name: id,
                            present: name == "set_country_flag",
                        },
                        location,
                    ));
                }
                "remove_country_modifier" => {
                    let id = value
                        .read_string()
                        .map_err(|_| fail("expected modifier name"))?;
                    if id.is_empty() || !self.modifier_ids.contains(&id) {
                        return Err(fail("unknown modifier name"));
                    }
                    result.push(Step::Metric(
                        Action::RemoveModifier {
                            target: augustus_core::Target::Current,
                            name: id,
                        },
                        location,
                    ));
                }
                "complete_mission" => {
                    let id = value
                        .read_string()
                        .map_err(|_| fail("expected mission identifier"))?;
                    if !self.mission_ids.contains(&id) {
                        return Err(fail("unknown mission identifier"));
                    }
                    result.push(Step::Metric(Action::RecordCompletion { id }, location));
                }
                "events" if self.startup_metadata && depth == 0 => (),
                "log" => result.push(Step::Metric(
                    Action::Emit(
                        value
                            .read_string()
                            .map_err(|_| fail("expected log message"))?,
                    ),
                    location,
                )),
                "country_event"
                | "tooltip"
                | "add_country_modifier"
                | "set_variable"
                | "change_variable"
                | "subtract_variable"
                | "multiply_variable"
                | "divide_variable" => {
                    let library = crate::ScriptLibrary::new();
                    let compiled = crate::ScriptCompiler::new(&self.schema, &library, true)
                        .with_government_metric_ids(self.government_metric_ids.clone())
                        .with_mission_ids(self.mission_ids.clone())
                        .with_modifier_ids(self.modifier_ids.clone())
                        .compile_effect_reader(
                            parsed,
                            &name,
                            block(&value, &fail)?,
                            at,
                            EntityKind::Country,
                        );
                    match compiled {
                        Ok(plan) => result.extend(
                            plan.into_iter()
                                .map(|action| Step::Metric(action, location.clone())),
                        ),
                        Err(error) => result.push(Step::Pending(
                            format!("uncompiled_startup_effect:{error}"),
                            source(parsed, error.location().unwrap_or(at)),
                        )),
                    }
                }
                "emperor" => {
                    let office = "imperial_emperor";
                    result.push(Step::Scoped(
                        ScopeSelector::Office(office.into()),
                        self.effects(parsed, block(&value, &fail)?, report, depth + 1, false)?,
                        location,
                    ));
                }
                "save_global_event_target_as" | "clear_global_event_target" => {
                    let id = value
                        .read_string()
                        .map_err(|_| fail("expected global event target name"))?;
                    if id.is_empty() {
                        return Err(fail("empty global event target name"));
                    }
                    let action = if name == "save_global_event_target_as" {
                        Action::BindGlobalScope { name: id }
                    } else {
                        Action::ClearGlobalScope { name: id }
                    };
                    result.push(Step::Metric(action, location));
                }
                _ if name.starts_with("event_target:") => {
                    let id = name.strip_prefix("event_target:").expect("matched prefix");
                    if id.is_empty() {
                        return Err(fail("empty global event target name"));
                    }
                    result.push(Step::Scoped(
                        ScopeSelector::Global(id.into()),
                        self.effects(parsed, block(&value, &fail)?, report, depth + 1, false)?,
                        location,
                    ));
                }
                "limit" if skip_limit => (),
                "hidden_effect" => result.push(Step::Group(self.effects(
                    parsed,
                    block(&value, &fail)?,
                    report,
                    depth + 1,
                    false,
                )?)),
                "add_estate_loyalty" | "change_estate_land_share" => {
                    let metric = HistoryMetric::parse(parsed, &name, block(&value, &fail)?, at)?;
                    result.push(Step::Metric(metric.action(), location));
                }
                "set_estate_privilege" | "remove_estate_privilege" => {
                    let id = value
                        .read_string()
                        .map_err(|_| fail("expected estate privilege identifier"))?;
                    if id.is_empty() {
                        return Err(fail("empty estate privilege identifier"));
                    }
                    self.needed_privileges.insert(id.clone());
                    result.push(Step::Privilege(
                        id,
                        name == "set_estate_privilege",
                        location,
                    ));
                }
                "if" | "else_if" => {
                    let body = block(&value, &fail)?;
                    let limits: Vec<_> = body
                        .fields()
                        .filter(|(k, _, _)| k.read_string() == "limit")
                        .collect();
                    if limits.len() != 1 {
                        return Err(fail("startup branch requires one limit"));
                    }
                    let condition = Condition::All(Condition::compile(
                        parsed,
                        block(&limits[0].2, &fail)?,
                        report,
                        0,
                        true,
                        Some(&self.triggers),
                        false,
                    )?);
                    let effects = self.effects(parsed, body, report, depth + 1, true)?;
                    if name == "if" {
                        result.push(Step::Branch(vec![(condition, effects)], None, location));
                    } else if let Some(Step::Branch(branches, otherwise, _)) = result.last_mut() {
                        if otherwise.is_some() {
                            return Err(fail("else_if follows else"));
                        }
                        branches.push((condition, effects));
                    } else {
                        return Err(fail("else_if without preceding if"));
                    }
                }
                "else" => {
                    let body =
                        self.effects(parsed, block(&value, &fail)?, report, depth + 1, false)?;
                    if let Some(Step::Branch(_, otherwise, _)) = result.last_mut() {
                        if otherwise.is_some() {
                            return Err(fail("duplicate else"));
                        }
                        *otherwise = Some(body);
                    } else {
                        return Err(fail("else without preceding if"));
                    }
                }
                "every_country" => {
                    let body = block(&value, &fail)?;
                    let limits: Vec<_> = body
                        .fields()
                        .filter(|(k, _, _)| k.read_string() == "limit")
                        .collect();
                    if limits.len() > 1 {
                        return Err(fail("duplicate country iterator limit"));
                    }
                    let condition = match limits.first() {
                        Some((_, _, value)) => Condition::All(Condition::compile(
                            parsed,
                            block(value, &fail)?,
                            report,
                            0,
                            true,
                            Some(&self.triggers),
                            false,
                        )?),
                        None => Condition::Literal(true),
                    };
                    result.push(Step::Countries(
                        condition,
                        self.effects(parsed, body, report, depth + 1, true)?,
                        location,
                    ));
                }
                _ if name.len() == 3
                    && name
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
                    && value.read_object().is_ok() =>
                {
                    result.push(Step::Country(
                        name,
                        self.effects(parsed, block(&value, &fail)?, report, depth + 1, false)?,
                        location,
                    ));
                }
                _ if self.effect_index.contains_key(&name) => {
                    if value.read_string().ok().as_deref() != Some("yes") {
                        result.push(Step::Pending(
                            format!("parameterized_effect:{name}"),
                            location,
                        ));
                        continue;
                    }
                    let Some(index) = self.effect_index[&name] else {
                        result.push(Step::Pending(format!("ambiguous_effect:{name}"), location));
                        continue;
                    };
                    if self.visiting.contains(&name) {
                        result.push(Step::Pending(format!("recursive_effect:{name}"), location));
                        continue;
                    }
                    self.visiting.push(name.clone());
                    let document = Arc::clone(&self.effects[index]);
                    let parsed = document.parse()?;
                    let (_, _, value) = parsed
                        .reader()
                        .fields()
                        .find(|(k, _, _)| k.read_string() == name)
                        .expect("indexed scripted effect");
                    let effects =
                        self.effects(&parsed, block(&value, &fail)?, report, depth + 1, false)?;
                    self.visiting.pop();
                    result.push(Step::Group(effects));
                }
                _ => {
                    report.unsupported(parsed, "startup_effect", &name, at);
                    result.push(Step::Pending(
                        format!("unsupported_effect:{name}"),
                        location,
                    ));
                }
            }
        }
        Ok(result)
    }
}

impl EstateStartup {
    pub(crate) fn load(
        root: &ContentRoot,
        files: &[FileEntry],
        schema: &Schema,
        modifier_ids: BTreeSet<String>,
        government_metric_ids: BTreeSet<String>,
        report: &mut EstateReport,
    ) -> Result<Self, Error> {
        let mut compiler = Compiler {
            schema: schema.clone(),
            effects: Vec::new(),
            effect_index: BTreeMap::new(),
            triggers: super::library(root, files, report)?,
            visiting: Vec::new(),
            needed_privileges: BTreeSet::new(),
            startup_metadata: false,
            mission_ids: crate::missions::identifiers(root, files)?,
            modifier_ids,
            government_metric_ids,
        };
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/scripted_effects"))
        {
            let document = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = document.parse()?;
            for (key, _, _) in parsed.reader().fields() {
                let id = key.read_string();
                let index = compiler.effects.len();
                compiler
                    .effect_index
                    .entry(id)
                    .and_modify(|value| *value = None)
                    .or_insert(Some(index));
            }
            compiler.effects.push(Arc::new(document));
        }
        let mut steps = Vec::new();
        let mut seen = false;
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/on_actions"))
        {
            let document = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = document.parse()?;
            for (key, op, value) in parsed
                .reader()
                .fields()
                .filter(|(k, _, _)| k.read_string() == "on_startup")
            {
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                if seen {
                    return Err(fail("duplicate startup action"));
                }
                seen = true;
                let body = block(&value, &fail)?;
                let mut candidates = None;
                for (key, op, value) in body
                    .fields()
                    .filter(|(key, _, _)| key.read_string() == "events")
                {
                    equal(op, &fail)?;
                    if candidates.is_some() {
                        return Err(fail("duplicate startup event list"));
                    }
                    let values = value
                        .read_array()
                        .map_err(|_| fail("expected startup event id list"))?
                        .values()
                        .map(|v| v.read_string().map_err(|_| fail("expected event id")))
                        .collect::<Result<Vec<_>, _>>()?;
                    if values.iter().any(String::is_empty) {
                        return Err(fail("empty startup event id"));
                    }
                    candidates = Some((values, source(&parsed, parsed.field_location(&key))));
                }
                if let Some((events, at)) = candidates {
                    steps.push(Step::Metric(Action::EventCandidates { events }, at));
                }
                compiler.startup_metadata = true;
                steps.extend(compiler.effects(&parsed, block(&value, &fail)?, report, 0, false)?);
                compiler.startup_metadata = false;
            }
        }
        let mut callbacks = BTreeMap::new();
        let mut handled = BTreeSet::new();
        loop {
            let requested: BTreeSet<_> = compiler
                .needed_privileges
                .difference(&handled)
                .cloned()
                .collect();
            if requested.is_empty() {
                break;
            }
            handled.extend(requested.iter().cloned());
            for file in files
                .iter()
                .filter(|f| direct_txt(&f.relative_path, "common/estate_privileges"))
            {
                let document = root.read_text(&file.relative_path, ENCODING)?;
                let parsed = document.parse()?;
                for (key, _, value) in parsed
                    .reader()
                    .fields()
                    .filter(|(k, _, _)| requested.contains(&k.read_string()))
                {
                    let fail = |m: &str| {
                        Error::message(parsed.source(), m)
                            .with_location(parsed.field_location(&key))
                    };
                    let mut callback = Callback {
                        valid: Condition::Literal(true),
                        granted: Vec::new(),
                        revoked: Vec::new(),
                        unsupported_land: false,
                        unsupported_provinces: false,
                    };
                    let mut seen = BTreeSet::new();
                    for (key, _, value) in block(&value, &fail)?.fields() {
                        if matches!(
                            key.read_string().as_str(),
                            "is_valid"
                                | "on_granted"
                                | "on_revoked"
                                | "on_granted_province"
                                | "land_share"
                        ) && !seen.insert(key.read_string())
                        {
                            return Err(fail("duplicate estate callback field"));
                        }
                        match key.read_string().as_str() {
                            "is_valid" => {
                                callback.valid = Condition::All(Condition::compile(
                                    &parsed,
                                    block(&value, &fail)?,
                                    report,
                                    0,
                                    true,
                                    Some(&compiler.triggers),
                                    false,
                                )?)
                            }
                            "on_granted" => {
                                callback.granted = compiler.effects(
                                    &parsed,
                                    block(&value, &fail)?,
                                    report,
                                    0,
                                    false,
                                )?
                            }
                            "on_revoked" => {
                                callback.revoked = compiler.effects(
                                    &parsed,
                                    block(&value, &fail)?,
                                    report,
                                    0,
                                    false,
                                )?
                            }
                            "on_granted_province" => {
                                callback.unsupported_provinces =
                                    block(&value, &fail)?.fields().next().is_some()
                            }
                            "land_share" => {
                                callback.unsupported_land =
                                    value.read_string().ok().and_then(|v| micros(&v)) != Some(0)
                            }
                            _ => (),
                        }
                    }
                    callbacks.insert(key.read_string(), callback);
                }
            }
        }
        let mut references = BTreeSet::new();
        collect_references(&steps, &mut references);
        for callback in callbacks.values() {
            collect_references(&callback.granted, &mut references);
            collect_references(&callback.revoked, &mut references);
        }
        let events = match load_events(
            root,
            files,
            schema,
            references,
            &compiler.mission_ids,
            &compiler.modifier_ids,
            &compiler.government_metric_ids,
        ) {
            Ok(events) => events,
            Err(error) => {
                let at = error
                    .location()
                    .unwrap_or(crate::SourceLocation { line: 1, column: 1 });
                steps.insert(
                    0,
                    Step::Pending(
                        format!("uncompiled_startup_event:{error}"),
                        LoadedSource {
                            path: error.path().into(),
                            line: at.line,
                            column: at.column,
                        },
                    ),
                );
                EventCatalog::default()
            }
        };
        Ok(Self {
            steps,
            callbacks,
            events,
            mission_ids: compiler.mission_ids,
        })
    }

    pub(crate) fn run(
        &self,
        initial: &mut InitialWorld,
        report: &mut EstateReport,
    ) -> Result<(), Error> {
        let mut candidate = initial.clone();
        if initial
            .world()
            .date()
            .map_err(|e| Error::caused_by("variables", e))?
            == Some(augustus_core::WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            })
        {
            candidate
                .set_named_values(augustus_core::NamedValues {
                    entities: (0..initial.world().entities().len())
                        .map(|id| (augustus_core::EntityId(id), BTreeMap::new()))
                        .collect(),
                    ..Default::default()
                })
                .map_err(|e| Error::caused_by("variables", e))?;
        }
        if self.steps.is_empty() {
            *initial = candidate;
            return Ok(());
        }
        if !self.mission_ids.is_empty() {
            let entities = if initial
                .world()
                .date()
                .map_err(|e| Error::caused_by("missions", e))?
                == Some(augustus_core::WorldDate {
                    year: 1444,
                    month: 11,
                    day: 11,
                }) {
                initial
                    .countries()
                    .iter()
                    .map(|country| {
                        (
                            initial
                                .country_entity(&country.id)
                                .expect("registered country"),
                            BTreeSet::new(),
                        )
                    })
                    .collect()
            } else {
                BTreeMap::new()
            };
            candidate
                .set_completions(augustus_core::Completions {
                    identifiers: self.mission_ids.clone(),
                    entities,
                })
                .map_err(|e| Error::caused_by("missions", e))?;
        }
        candidate
            .set_event_catalog(self.events.clone())
            .map_err(|e| Error::caused_by("events", e))?;
        let mut trace = Vec::new();
        let countries: Vec<_> = candidate
            .countries()
            .iter()
            .filter(|c| !candidate.owned_provinces(&c.id).is_empty())
            .map(|c| c.id.clone())
            .collect();
        let mut budget = 100_000;
        let result = countries.iter().try_for_each(|tag| {
            self.execute(&self.steps, &mut candidate, tag, &mut trace, &mut budget, 0)
        });
        match result {
            Ok(()) => {
                *initial = candidate;
                report.applied_startup_effects = trace.len();
                report.startup_effect_sources = trace;
            }
            Err(pending) => {
                report.pending_startup_effects.push(pending.source);
                report.startup_unresolved.extend(pending.reasons);
                let mut markers = initial.world().markers().clone();
                for tag in &countries {
                    markers.mark_unknown(initial.country_entity(tag).expect("registered country"));
                }
                initial
                    .set_markers(markers)
                    .map_err(|e| Error::caused_by("common/on_actions", e))?;
                let mut modifiers = initial.world().modifiers().clone();
                modifiers.unknown_entities.extend(
                    countries
                        .iter()
                        .filter_map(|tag| initial.country_entity(tag)),
                );
                initial
                    .set_modifiers(modifiers)
                    .map_err(|e| Error::caused_by("common/on_actions", e))?;
                let mut governments = initial.world().governments().clone();
                for entity in countries
                    .iter()
                    .filter_map(|tag| initial.country_entity(tag))
                {
                    if let Some(metrics) = governments
                        .countries
                        .get_mut(&entity)
                        .and_then(|state| state.metrics.as_mut())
                    {
                        for value in metrics.values_mut() {
                            *value = None;
                        }
                    }
                }
                initial
                    .set_governments(governments)
                    .map_err(|e| Error::caused_by("common/on_actions", e))?;
                let mut estates = initial.world().estates().clone();
                for states in estates.countries.values_mut() {
                    for state in states.values_mut().filter(|s| s.active != Some(false)) {
                        state.loyalty_milli = None;
                        state.land_share_micros = None;
                        state.privileges = None;
                    }
                }
                initial
                    .set_estates(estates)
                    .map_err(|e| Error::caused_by("common/on_actions", e))?;
                for country in initial.countries() {
                    report
                        .unresolved_initialization
                        .entry(country.id.clone())
                        .or_default()
                        .insert("unexecuted_startup_estate_effects".into());
                }
            }
        }
        report.initial_land_countries = 0;
        report.initial_loyalty_countries = 0;
        for states in initial.world().estates().countries.values() {
            let active: Vec<_> = states.values().filter(|s| s.active == Some(true)).collect();
            if !active.is_empty() && active.iter().all(|s| s.land_share_micros.is_some()) {
                report.initial_land_countries += 1;
            }
            if !active.is_empty() && active.iter().all(|s| s.loyalty_milli.is_some()) {
                report.initial_loyalty_countries += 1;
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute(
        &self,
        steps: &[Step],
        initial: &mut InitialWorld,
        tag: &str,
        trace: &mut Vec<LoadedSource>,
        budget: &mut usize,
        depth: usize,
    ) -> Result<(), Pending> {
        for step in steps {
            let at = match step {
                Step::Metric(_, at)
                | Step::Privilege(_, _, at)
                | Step::Branch(_, _, at)
                | Step::Countries(_, _, at)
                | Step::Country(_, _, at)
                | Step::Scoped(_, _, at)
                | Step::Pending(_, at) => at,
                Step::Group(body) => {
                    self.execute(body, initial, tag, trace, budget, depth + 1)?;
                    continue;
                }
            };
            if depth > 64 || *budget == 0 {
                return Err(Pending::new(at, "startup execution limit exceeded"));
            }
            *budget -= 1;
            match step {
                Step::Metric(action, at) => {
                    let entity = initial.country_entity(tag).expect("bound startup country");
                    initial
                        .execute(entity, std::slice::from_ref(action))
                        .map_err(|e| Pending::new(at, e.to_string()))?;
                    trace.push(at.clone());
                }
                Step::Privilege(id, granted, at) => {
                    let callback = self
                        .callbacks
                        .get(id)
                        .ok_or_else(|| Pending::new(at, format!("missing_privilege:{id}")))?;
                    if callback.unsupported_land || (*granted && callback.unsupported_provinces) {
                        return Err(Pending::new(
                            at,
                            format!("unsupported_privilege_initialization:{id}"),
                        ));
                    }
                    if *granted && !self.condition(&callback.valid, initial, tag, at)? {
                        return Err(Pending::new(
                            at,
                            format!("unvalidated_privilege_validity:{id}"),
                        ));
                    }
                    let entity = initial.country_entity(tag).expect("bound startup country");
                    let estates = initial.world().estates();
                    let targets: Vec<_> = estates
                        .definitions
                        .iter()
                        .filter(|(_, d)| d.privileges.contains(id))
                        .map(|(id, _)| id.clone())
                        .collect();
                    if targets.len() != 1 {
                        return Err(Pending::new(
                            at,
                            format!("ambiguous_privilege_binding:{id}"),
                        ));
                    }
                    let selected = estates
                        .countries
                        .get(&entity)
                        .and_then(|s| s.get(&targets[0]))
                        .and_then(|s| s.privileges.as_ref())
                        .ok_or_else(|| Pending::new(at, format!("unknown_privileges:{id}")))?;
                    if selected.contains(id) == *granted {
                        return Err(Pending::new(
                            at,
                            format!("unvalidated_repeated_privilege:{id}"),
                        ));
                    }
                    self.execute(
                        if *granted {
                            &callback.granted
                        } else {
                            &callback.revoked
                        },
                        initial,
                        tag,
                        trace,
                        budget,
                        depth + 1,
                    )?;
                    initial
                        .execute(
                            entity,
                            &[Action::SetEstatePrivilege {
                                estate: targets[0].clone(),
                                privilege: id.clone(),
                                granted: *granted,
                            }],
                        )
                        .map_err(|e| Pending::new(at, e.to_string()))?;
                    trace.push(at.clone());
                    if *granted {
                        for effect in ["is_valid", "on_granted"] {
                            if initial.world().estates().privileges[id]
                                .effect
                                .unresolved_effects
                                .contains(effect)
                            {
                                initial
                                    .execute(
                                        entity,
                                        &[Action::ResolveEstatePrivilegeEffect {
                                            privilege: id.clone(),
                                            effect: effect.into(),
                                        }],
                                    )
                                    .map_err(|e| Pending::new(at, e.to_string()))?;
                            }
                        }
                    }
                }
                Step::Branch(branches, otherwise, at) => {
                    let mut selected: &[Step] = otherwise.as_deref().unwrap_or(&[]);
                    for (condition, body) in branches {
                        if self.condition(condition, initial, tag, at)? {
                            selected = body;
                            break;
                        }
                    }
                    self.execute(selected, initial, tag, trace, budget, depth + 1)?;
                }
                Step::Countries(condition, body, at) => {
                    let countries: Vec<_> = initial
                        .countries()
                        .iter()
                        .filter(|c| !initial.owned_provinces(&c.id).is_empty())
                        .map(|c| c.id.clone())
                        .collect();
                    for tag in countries {
                        if self.condition(condition, initial, &tag, at)? {
                            self.execute(body, initial, &tag, trace, budget, depth + 1)?;
                        }
                    }
                }
                Step::Country(tag, body, at) => {
                    if initial.country(tag).is_none() {
                        return Err(Pending::new(at, format!("unknown_country_scope:{tag}")));
                    }
                    if !initial.owned_provinces(tag).is_empty() {
                        self.execute(body, initial, tag, trace, budget, depth + 1)?;
                    }
                }
                Step::Scoped(selector, body, at) => {
                    if let Some(entity) = initial
                        .world()
                        .resolve_scope(selector)
                        .map_err(|e| Pending::new(at, e.to_string()))?
                    {
                        let selected = initial
                            .world()
                            .entity(entity)
                            .map_err(|e| Pending::new(at, e.to_string()))?;
                        if selected.kind != EntityKind::Country {
                            return Err(Pending::new(
                                at,
                                "startup named scope requires country type",
                            ));
                        }
                        let target = initial
                            .countries()
                            .iter()
                            .find(|c| initial.country_entity(&c.id) == Some(entity))
                            .map(|c| c.id.clone())
                            .ok_or_else(|| Pending::new(at, "unbound startup scope entity"))?;
                        self.execute(body, initial, &target, trace, budget, depth + 1)?;
                    }
                }
                Step::Pending(reason, at) => return Err(Pending::new(at, reason)),
                Step::Group(_) => unreachable!(),
            }
        }
        Ok(())
    }

    fn condition(
        &self,
        condition: &Condition,
        initial: &InitialWorld,
        tag: &str,
        at: &LoadedSource,
    ) -> Result<bool, Pending> {
        let mut reasons = BTreeSet::new();
        let entity = initial.country_entity(tag).expect("bound startup country");
        let lookup = |id: &str| {
            initial
                .world()
                .estates()
                .countries
                .get(&entity)
                .and_then(|s| s.get(id))
                .and_then(|s| s.active)
        };
        let value = condition.evaluate(initial, tag, &mut reasons, None, Some(&lookup));
        value.ok_or_else(|| {
            if reasons.is_empty() {
                reasons.insert("unknown_startup_condition".into());
            }
            Pending {
                source: at.clone(),
                reasons,
            }
        })
    }
}

fn collect_references(steps: &[Step], references: &mut BTreeSet<String>) {
    for step in steps {
        match step {
            Step::Metric(action, _) => {
                crate::script::event_references(std::slice::from_ref(action), references)
            }
            Step::Branch(branches, otherwise, _) => {
                for (_, body) in branches {
                    collect_references(body, references);
                }
                if let Some(body) = otherwise {
                    collect_references(body, references);
                }
            }
            Step::Countries(_, body, _)
            | Step::Country(_, body, _)
            | Step::Scoped(_, body, _)
            | Step::Group(body) => collect_references(body, references),
            _ => (),
        }
    }
}

fn load_events(
    root: &ContentRoot,
    files: &[FileEntry],
    schema: &Schema,
    mut needed: BTreeSet<String>,
    mission_ids: &BTreeSet<String>,
    modifier_ids: &BTreeSet<String>,
    government_metric_ids: &BTreeSet<String>,
) -> Result<EventCatalog, Error> {
    if needed.is_empty() {
        return Ok(EventCatalog::default());
    }
    let library = crate::ScriptLibrary::new();
    let mut definitions = BTreeMap::new();
    let mut conditions = BTreeMap::new();
    let mut once_events = BTreeSet::new();
    while !needed.is_empty() {
        let requested = std::mem::take(&mut needed);
        let mut found = BTreeSet::new();
        for file in files
            .iter()
            .filter(|file| direct_txt(&file.relative_path, "events"))
        {
            let document = root.read_text(&file.relative_path, ENCODING)?;
            let compiled = crate::ScriptCompiler::new(schema, &library, true)
                .with_government_metric_ids(government_metric_ids.clone())
                .with_mission_ids(mission_ids.clone())
                .with_modifier_ids(modifier_ids.clone())
                .compile_selected_events(&document, &requested)?;
            for definition in compiled.definitions {
                if !found.insert(definition.id.clone()) {
                    return Err(Error::message(
                        document.source(),
                        format!("ambiguous startup event {}", definition.id),
                    ));
                }
                for plan in std::iter::once(&definition.immediate)
                    .chain(definition.options.iter().map(|option| &option.effects))
                    .chain(std::iter::once(&definition.after))
                {
                    crate::script::event_references(plan, &mut needed);
                }
                definitions.insert(definition.id.clone(), definition);
            }
            conditions.extend(compiled.conditions);
            once_events.extend(compiled.once_events);
        }
        if let Some(missing) = requested.difference(&found).next() {
            return Err(Error::message(
                "events",
                format!("missing startup event {missing}"),
            ));
        }
        needed.retain(|id| !definitions.contains_key(id));
    }
    EventCatalog::new(definitions.into_values())
        .and_then(|catalog| catalog.with_conditions(conditions))
        .and_then(|catalog| catalog.with_once_events(once_events))
        .map_err(|e| Error::caused_by("events", e))
}

struct Pending {
    source: LoadedSource,
    reasons: BTreeSet<String>,
}
impl Pending {
    fn new(at: &LoadedSource, reason: impl Into<String>) -> Self {
        Self {
            source: at.clone(),
            reasons: [reason.into()].into(),
        }
    }
}
