// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use super::initialization::HistoryMetric;
use super::{Condition, ENCODING, EstateReport, Reader};
use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::{PropertyDiagnostics, block, micros};
use crate::{ContentRoot, Error, FileEntry, LoadedSource, ParsedText, TextDocument};
use augustus_core::{Action, EntityKind, InitialWorld, ScopeSelector};
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
    effects: Vec<Arc<TextDocument>>,
    effect_index: BTreeMap<String, Option<usize>>,
    triggers: BTreeMap<String, Condition>,
    visiting: Vec<String>,
    needed_privileges: BTreeSet<String>,
}

pub(crate) struct EstateStartup {
    steps: Vec<Step>,
    callbacks: BTreeMap<String, Callback>,
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
        report: &mut EstateReport,
    ) -> Result<Self, Error> {
        let mut compiler = Compiler {
            effects: Vec::new(),
            effect_index: BTreeMap::new(),
            triggers: super::library(root, files, report)?,
            visiting: Vec::new(),
            needed_privileges: BTreeSet::new(),
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
                steps.extend(compiler.effects(&parsed, block(&value, &fail)?, report, 0, false)?);
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
        Ok(Self { steps, callbacks })
    }

    pub(crate) fn run(
        &self,
        initial: &mut InitialWorld,
        flags: &BTreeMap<String, Option<BTreeSet<String>>>,
        report: &mut EstateReport,
    ) -> Result<(), Error> {
        if self.steps.is_empty() {
            return Ok(());
        }
        let mut candidate = initial.clone();
        let mut trace = Vec::new();
        let countries: Vec<_> = candidate
            .countries()
            .iter()
            .filter(|c| !candidate.owned_provinces(&c.id).is_empty())
            .map(|c| c.id.clone())
            .collect();
        let mut budget = 100_000;
        let result = countries.iter().try_for_each(|tag| {
            self.execute(
                &self.steps,
                &mut candidate,
                tag,
                flags,
                &mut trace,
                &mut budget,
                0,
            )
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
        flags: &BTreeMap<String, Option<BTreeSet<String>>>,
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
                    self.execute(body, initial, tag, flags, trace, budget, depth + 1)?;
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
                    if *granted && !self.condition(&callback.valid, initial, tag, flags, at)? {
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
                        flags,
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
                        if self.condition(condition, initial, tag, flags, at)? {
                            selected = body;
                            break;
                        }
                    }
                    self.execute(selected, initial, tag, flags, trace, budget, depth + 1)?;
                }
                Step::Countries(condition, body, at) => {
                    let countries: Vec<_> = initial
                        .countries()
                        .iter()
                        .filter(|c| !initial.owned_provinces(&c.id).is_empty())
                        .map(|c| c.id.clone())
                        .collect();
                    for tag in countries {
                        if self.condition(condition, initial, &tag, flags, at)? {
                            self.execute(body, initial, &tag, flags, trace, budget, depth + 1)?;
                        }
                    }
                }
                Step::Country(tag, body, at) => {
                    if initial.country(tag).is_none() {
                        return Err(Pending::new(at, format!("unknown_country_scope:{tag}")));
                    }
                    if !initial.owned_provinces(tag).is_empty() {
                        self.execute(body, initial, tag, flags, trace, budget, depth + 1)?;
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
                        self.execute(body, initial, &target, flags, trace, budget, depth + 1)?;
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
        flags: &BTreeMap<String, Option<BTreeSet<String>>>,
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
        let value = condition.evaluate(
            initial,
            tag,
            flags.get(tag).and_then(Option::as_ref),
            &mut reasons,
            None,
            Some(&lookup),
        );
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
