// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use augustus_core::{
    EstateDefinition, EstateEffect, EstatePrivilege, EstateRules, EstateState, Estates,
    InitialWorld,
};
use jomini::text::ObjectReader;
use serde::Serialize;

use crate::bootstrap::{dated_name, direct_txt, equal, source};
use crate::country_conditions::Condition;
use crate::setup::{PropertyDiagnostics, block, properties};
use crate::{
    ContentRoot, Error, FileEntry, LoadedSource, ParsedText, SourceLocation, TextEncoding,
};

mod initialization;
mod startup;
use initialization::{EstateInitialization, HistoryMetric};
pub(super) use startup::EstateStartup;

type Reader<'a, 'b> = ObjectReader<'a, 'b, TextEncoding>;
const ENCODING: TextEncoding = TextEncoding::Windows1252;

#[derive(Debug, Default, Serialize)]
pub struct EstateReport {
    pub definitions: usize,
    pub privileges: usize,
    pub active_estates: usize,
    pub inactive_estates: usize,
    pub unknown_estates: usize,
    pub historical_privileges: usize,
    pub initial_land_countries: usize,
    pub initial_loyalty_countries: usize,
    pub unresolved_initialization: BTreeMap<String, BTreeSet<String>>,
    pub definition_sources: BTreeMap<String, LoadedSource>,
    pub history_sources: BTreeMap<String, LoadedSource>,
    pub unsupported_fields: BTreeMap<String, usize>,
    pub unsupported_examples: BTreeMap<String, LoadedSource>,
    pub ambiguous_properties: BTreeMap<String, Vec<LoadedSource>>,
    pub unresolved_countries: BTreeMap<String, BTreeSet<String>>,
    pub pending_land_changes: BTreeMap<String, Vec<LoadedSource>>,
    pub pending_loyalty_changes: BTreeMap<String, Vec<LoadedSource>>,
    pub applied_startup_effects: usize,
    pub startup_effect_sources: Vec<LoadedSource>,
    pub pending_startup_effects: Vec<LoadedSource>,
    pub startup_unresolved: BTreeSet<String>,
}

impl PropertyDiagnostics for EstateReport {
    fn unsupported(&mut self, parsed: &ParsedText<'_>, kind: &str, name: &str, at: SourceLocation) {
        let key = format!("{kind}:{name}");
        *self.unsupported_fields.entry(key.clone()).or_default() += 1;
        self.unsupported_examples
            .entry(key)
            .or_insert_with(|| source(parsed, at));
    }
    fn ambiguous(&mut self, key: String, first: LoadedSource, next: LoadedSource) {
        self.ambiguous_properties
            .entry(key)
            .or_insert_with(|| vec![first])
            .push(next);
    }
}

#[derive(Default)]
struct History {
    privileges: BTreeSet<String>,
    privilege_operations: BTreeSet<String>,
    revoked_privileges: BTreeSet<String>,
    unknown_privileges: bool,
    metrics: Vec<HistoryMetric>,
    unknown_land: bool,
    unknown_loyalty: bool,
}

#[derive(Default)]
struct SelectionCache {
    values: BTreeMap<String, Option<bool>>,
    visiting: BTreeSet<String>,
}

pub(super) struct EstateDraft {
    content: Estates,
    conditions: BTreeMap<String, Condition>,
    histories: BTreeMap<String, History>,
    initialization: EstateInitialization,
}

impl EstateDraft {
    pub(super) fn load(
        root: &ContentRoot,
        files: &[FileEntry],
        report: &mut EstateReport,
    ) -> Result<Self, Error> {
        let mut draft = Self {
            content: Estates::default(),
            conditions: BTreeMap::new(),
            histories: BTreeMap::new(),
            initialization: EstateInitialization::default(),
        };
        if !files
            .iter()
            .any(|f| direct_txt(&f.relative_path, "common/estates"))
        {
            return Ok(draft);
        }
        let library = library(root, files, report)?;
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/estates"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let id = key.read_string();
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                let mut definition = EstateDefinition::default();
                let mut condition = Condition::Unknown("missing_estate_trigger".into());
                let mut seen = BTreeSet::new();
                let body = block(&value, &fail)?;
                draft
                    .initialization
                    .definition(&parsed, body.clone(), &id, &library, report)?;
                for (key, op, value) in body.fields() {
                    let name = key.read_string();
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    equal(op, &fail)?;
                    if matches!(
                        name.as_str(),
                        "trigger"
                            | "country_modifier_angry"
                            | "country_modifier_neutral"
                            | "country_modifier_happy"
                            | "land_ownership_modifier"
                            | "privileges"
                    ) && !seen.insert(name.clone())
                    {
                        return Err(fail("duplicate estate field"));
                    }
                    match name.as_str() {
                        "trigger" => {
                            condition = Condition::All(Condition::compile(
                                &parsed,
                                block(&value, &fail)?,
                                report,
                                0,
                                true,
                                Some(&library),
                                false,
                            )?)
                        }
                        "country_modifier_angry" => {
                            definition.angry = effect(
                                &parsed,
                                block(&value, &fail)?,
                                &format!("{id}:angry"),
                                report,
                            )?
                        }
                        "country_modifier_neutral" => {
                            definition.neutral = effect(
                                &parsed,
                                block(&value, &fail)?,
                                &format!("{id}:neutral"),
                                report,
                            )?
                        }
                        "country_modifier_happy" => {
                            definition.happy = effect(
                                &parsed,
                                block(&value, &fail)?,
                                &format!("{id}:happy"),
                                report,
                            )?
                        }
                        "land_ownership_modifier" => {
                            definition.land_ownership = effect(
                                &parsed,
                                block(&value, &fail)?,
                                &format!("{id}:land"),
                                report,
                            )?
                        }
                        "privileges" => {
                            for value in value
                                .read_array()
                                .map_err(|_| fail("expected privilege list"))?
                                .values()
                            {
                                let id = value
                                    .read_string()
                                    .map_err(|_| fail("expected privilege identifier"))?;
                                if id.is_empty() || !definition.privileges.insert(id) {
                                    return Err(fail("empty or duplicate privilege reference"));
                                }
                            }
                        }
                        "base_influence" | "influence_modifier" => (),
                        _ => report.unsupported(&parsed, "estate", &name, at),
                    }
                }
                if id.is_empty()
                    || draft
                        .content
                        .definitions
                        .insert(id.clone(), definition)
                        .is_some()
                {
                    return Err(fail("empty or duplicate estate definition"));
                }
                draft.conditions.insert(id.clone(), condition);
                report
                    .definition_sources
                    .insert(format!("estate:{id}"), source(&parsed, at));
            }
        }
        if draft.content.definitions.is_empty() {
            return Ok(draft);
        }
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/estate_privileges"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let id = key.read_string();
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                let mut privilege = EstatePrivilege::default();
                let mut seen = BTreeSet::new();
                let privilege_body = block(&value, &fail)?;
                for (key, op, value) in privilege_body.clone().fields() {
                    let name = key.read_string();
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    equal(op, &fail)?;
                    match name.as_str() {
                        "benefits" | "penalties" | "modifier_by_land_ownership" => {
                            if !seen.insert(name.clone()) {
                                return Err(fail("duplicate privilege modifier block"));
                            }
                            let data = effect(
                                &parsed,
                                block(&value, &fail)?,
                                &format!("{id}:{name}"),
                                report,
                            )?;
                            if name == "modifier_by_land_ownership" {
                                privilege.land_ownership = data;
                            } else {
                                merge(&mut privilege.effect, data)?;
                            }
                        }
                        "conditional_modifier"
                        | "loyalty_scaled_conditional_modifier"
                        | "influence_scaled_conditional_modifier" => {
                            let body = block(&value, &fail)?;
                            for (_, _, value) in body
                                .fields()
                                .filter(|(k, _, _)| k.read_string() == "modifier")
                            {
                                for (key, _, _) in block(&value, &fail)?.fields() {
                                    let name = key.read_string();
                                    privilege.effect.modifier.values_micros.remove(&name);
                                    privilege.effect.modifier.flags.remove(&name);
                                    privilege.effect.unresolved_properties.insert(name);
                                }
                            }
                            report.unsupported(&parsed, "privilege", &name, at);
                        }
                        "on_granted" | "on_granted_province" | "is_valid" => {
                            let body = block(&value, &fail)?;
                            if body.fields().next().is_some()
                                && !(name == "is_valid"
                                    && crate::autonomy::constant_condition(
                                        &parsed,
                                        privilege_body.clone(),
                                        "is_valid",
                                    )? == Some(true))
                            {
                                privilege.effect.unresolved_effects.insert(name.clone());
                                report.unsupported(&parsed, "privilege", &name, at);
                            }
                        }
                        _ => report.unsupported(&parsed, "privilege", &name, at),
                    }
                }
                // A later direct block must not overwrite an uncertain conditional property.
                for name in &privilege.effect.unresolved_properties {
                    privilege.effect.modifier.values_micros.remove(name);
                    privilege.effect.modifier.flags.remove(name);
                }
                if id.is_empty()
                    || draft
                        .content
                        .privileges
                        .insert(id.clone(), privilege)
                        .is_some()
                {
                    return Err(fail("empty or duplicate estate privilege"));
                }
                report
                    .definition_sources
                    .insert(format!("privilege:{id}"), source(&parsed, at));
            }
        }
        let mut rules = EstateRules::default();
        for (name, slot) in [
            ("ESTATE_ANGRY_THRESHOLD", &mut rules.angry_below_milli),
            ("ESTATE_HAPPY_THRESHOLD", &mut rules.happy_from_milli),
        ] {
            if let Some((value, at)) =
                crate::defines::optional_literal(root, files, "NCountry", name)?
            {
                if value % 1000 != 0 {
                    return Err(Error::message(
                        &at.path,
                        "estate threshold exceeds supported precision",
                    ));
                }
                *slot = Some(value / 1000);
                report
                    .definition_sources
                    .insert(format!("define:{name}"), at);
            }
        }
        draft.initialization.load_rules(root, files, report)?;
        // Current influence scaling is independent of initial land allocation.
        draft.content.rules = rules;
        report.definitions = draft.content.definitions.len();
        report.privileges = draft.content.privileges.len();
        Ok(draft)
    }

    pub(super) fn country_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        tag: &str,
        report: &mut EstateReport,
    ) -> Result<(), Error> {
        let history = self.histories.entry(tag.into()).or_default();
        for (key, op, value) in reader.fields() {
            let name = key.read_string();
            if dated_name(&name) {
                continue;
            }
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            match name.as_str() {
                "set_estate_privilege" | "remove_estate_privilege" => {
                    equal(op, &fail)?;
                    let id = value
                        .read_string()
                        .map_err(|_| fail("expected estate history identifier"))?;
                    if id.is_empty() {
                        return Err(fail("empty estate history identifier"));
                    }
                    match name.as_str() {
                        "set_estate_privilege" => {
                            history.privileges.insert(id.clone());
                        }
                        _ => {
                            history.privileges.remove(&id);
                        }
                    }
                    if name.contains("privilege") {
                        history.privilege_operations.insert(id.clone());
                        if name == "remove_estate_privilege" {
                            history.revoked_privileges.insert(id.clone());
                        }
                        report
                            .history_sources
                            .insert(format!("{tag}:{id}"), source(parsed, at));
                    }
                }
                "change_estate_land_share" | "add_estate_loyalty" => {
                    equal(op, &fail)?;
                    history.metrics.push(HistoryMetric::parse(
                        parsed,
                        &name,
                        block(&value, &fail)?,
                        at,
                    )?);
                    report.history_sources.insert(
                        format!("{tag}:{name}:{}", history.metrics.len()),
                        source(parsed, at),
                    );
                }
                "add_estate_influence" => {
                    report
                        .pending_land_changes
                        .entry(tag.into())
                        .or_default()
                        .push(source(parsed, at));
                    report.unsupported(parsed, "history", &name, at);
                }
                _ => {
                    if let Ok(body) = value.read_object() {
                        scoped(body, history, 0)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn bind(
        mut self,
        initial: &InitialWorld,
        report: &mut EstateReport,
    ) -> Result<Estates, Error> {
        if self.content.definitions.is_empty() {
            return Ok(self.content);
        }
        for country in initial.countries() {
            let history = self.histories.get(&country.id);
            let ambiguous: BTreeSet<_> = history
                .into_iter()
                .flat_map(|h| &h.privileges)
                .filter(|p| {
                    self.content
                        .definitions
                        .values()
                        .filter(|d| d.privileges.contains(*p))
                        .count()
                        != 1
                })
                .cloned()
                .collect();
            let cache = RefCell::new(SelectionCache::default());
            let mut states = BTreeMap::new();
            let mut reasons = BTreeSet::new();
            for (id, definition) in &self.content.definitions {
                let mut pending = BTreeSet::new();
                let active = self.select(id, initial, &country.id, history, &cache, &mut pending);
                if active.is_none() && pending.is_empty() {
                    pending.insert("unknown_estate_condition".into());
                }
                match active {
                    Some(true) => report.active_estates += 1,
                    Some(false) => report.inactive_estates += 1,
                    None => report.unknown_estates += 1,
                }
                reasons.extend(pending.into_iter().map(|p| format!("{id}:{p}")));
                let privileges = history
                    .filter(|h| {
                        !h.unknown_privileges && definition.privileges.is_disjoint(&ambiguous)
                    })
                    .map(|h| {
                        h.privileges
                            .iter()
                            .filter(|p| definition.privileges.contains(*p))
                            .cloned()
                            .collect::<BTreeSet<_>>()
                    });
                report.historical_privileges += privileges.as_ref().map_or(0, BTreeSet::len);
                states.insert(
                    id.clone(),
                    EstateState {
                        active,
                        privileges,
                        ..Default::default()
                    },
                );
            }
            if let Some(h) = history {
                for p in &h.privileges {
                    if ambiguous.contains(p) {
                        let reason = format!("unresolved_privilege_binding:{p}");
                        reasons.insert(reason.clone());
                        self.content
                            .unresolved_countries
                            .entry(
                                initial
                                    .country_entity(&country.id)
                                    .expect("registered country"),
                            )
                            .or_default()
                            .insert(reason);
                    }
                }
            }
            let lookup = |id: &str| {
                self.select(
                    id,
                    initial,
                    &country.id,
                    history,
                    &cache,
                    &mut BTreeSet::new(),
                )
            };
            self.initialization.bind(
                initial,
                &country.id,
                history,
                &self.content.privileges,
                &mut states,
                &lookup,
                report,
            )?;
            if !reasons.is_empty() {
                report
                    .unresolved_countries
                    .insert(country.id.clone(), reasons);
            }
            self.content.countries.insert(
                initial
                    .country_entity(&country.id)
                    .expect("registered country"),
                states,
            );
        }
        Ok(self.content)
    }

    fn select(
        &self,
        id: &str,
        initial: &InitialWorld,
        tag: &str,
        history: Option<&History>,
        cache: &RefCell<SelectionCache>,
        reasons: &mut BTreeSet<String>,
    ) -> Option<bool> {
        if history.is_none() {
            reasons.insert("missing_or_ambiguous_history".into());
            return None;
        }
        if let Some(value) = cache.borrow().values.get(id) {
            return *value;
        }
        if !cache.borrow_mut().visiting.insert(id.into()) {
            reasons.insert(format!("cyclic_estate_condition:{id}"));
            return None;
        }
        let value = self.conditions.get(id).and_then(|condition| {
            let lookup =
                |id: &str| self.select(id, initial, tag, history, cache, &mut BTreeSet::new());
            condition.evaluate(initial, tag, reasons, None, Some(&lookup))
        });
        cache.borrow_mut().visiting.remove(id);
        cache.borrow_mut().values.insert(id.into(), value);
        value
    }
}

fn effect(
    parsed: &ParsedText<'_>,
    reader: Reader<'_, '_>,
    id: &str,
    report: &mut EstateReport,
) -> Result<EstateEffect, Error> {
    let modifier = properties(parsed, reader.clone(), "estate", id, report)?;
    let unresolved_properties = reader
        .fields()
        .map(|(k, _, _)| k.read_string())
        .filter(|p| !modifier.values_micros.contains_key(p) && !modifier.flags.contains_key(p))
        .collect();
    Ok(EstateEffect {
        modifier,
        unresolved_properties,
        ..Default::default()
    })
}

fn merge(target: &mut EstateEffect, source: EstateEffect) -> Result<(), Error> {
    target
        .unresolved_properties
        .extend(source.unresolved_properties);
    for (p, value) in source.modifier.values_micros {
        let total = target.modifier.values_micros.entry(p).or_default();
        *total = total.checked_add(value).ok_or_else(|| {
            Error::message("common/estate_privileges", "privilege modifier overflow")
        })?;
    }
    for (p, value) in source.modifier.flags {
        if target
            .modifier
            .flags
            .get(&p)
            .is_some_and(|old| *old != value)
        {
            target.modifier.flags.remove(&p);
            target.unresolved_properties.insert(p);
        } else {
            target.modifier.flags.insert(p, value);
        }
    }
    Ok(())
}

fn scoped(reader: Reader<'_, '_>, history: &mut History, depth: usize) -> Result<(), Error> {
    if depth > 64 {
        return Err(Error::message(
            "history/countries",
            "estate history nesting exceeds limit",
        ));
    }
    for (key, _, value) in reader.fields() {
        match key.read_string().as_str() {
            "set_estate_privilege" | "remove_estate_privilege" => history.unknown_privileges = true,
            "change_estate_land_share" => history.unknown_land = true,
            "add_estate_loyalty" => history.unknown_loyalty = true,
            _ => (),
        }
        if let Ok(body) = value.read_object() {
            scoped(body, history, depth + 1)?;
        }
    }
    Ok(())
}

fn library(
    root: &ContentRoot,
    files: &[FileEntry],
    report: &mut EstateReport,
) -> Result<BTreeMap<String, Condition>, Error> {
    let mut result = BTreeMap::new();
    for file in files
        .iter()
        .filter(|f| direct_txt(&f.relative_path, "common/scripted_triggers"))
    {
        let doc = root.read_text(&file.relative_path, ENCODING)?;
        let parsed = doc.parse()?;
        for (key, op, value) in parsed.reader().fields().filter(|(k, _, _)| {
            matches!(
                k.read_string().as_str(),
                "has_enabled_estate"
                    | "has_disabled_estate"
                    | "has_unlocked_estate"
                    | "is_revolutionary_republic_trigger"
            )
        }) {
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            equal(op, &fail)?;
            let id = key.read_string();
            let condition = Condition::All(Condition::compile(
                &parsed,
                block(&value, &fail)?,
                report,
                0,
                true,
                None,
                false,
            )?);
            if result.insert(id.clone(), condition).is_some() {
                return Err(fail("duplicate scripted estate condition"));
            }
            report
                .definition_sources
                .insert(format!("condition:{id}"), source(&parsed, at));
        }
    }
    Ok(result)
}
