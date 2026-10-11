// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::bootstrap::{dated_name, direct_txt, equal, nonnegative_milli, source};
use crate::{
    ContentRoot, Error, FileEntry, LoadedSource, ParsedText, SourceLocation, TextEncoding,
};
use augustus_core::{
    CountryResources, InitialWorld, ModifierDefinition, ModifierInstance, Modifiers, Technology,
    TechnologyLevel,
};
use jomini::{
    common::Date,
    text::{ObjectReader, ValueReader},
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

const ENCODING: TextEncoding = TextEncoding::Windows1252;
const BASE_DATE: &str = "1444.11.11";

#[derive(Debug, Default, Serialize)]
pub struct SetupReport {
    pub technology_groups: usize,
    pub technology_levels: usize,
    pub countries_with_technology: usize,
    pub countries_with_mercantilism: usize,
    pub modifier_definitions: usize,
    pub active_modifiers: usize,
    pub unknown_technology_countries: Vec<String>,
    pub unknown_resource_countries: Vec<String>,
    pub definition_sources: BTreeMap<String, LoadedSource>,
    pub history_sources: BTreeMap<String, LoadedSource>,
    pub technology_sources: BTreeMap<String, LoadedSource>,
    pub pending_timed_modifiers: BTreeMap<String, LoadedSource>,
    pub unapplied_definition_fields: BTreeMap<String, usize>,
    pub unapplied_examples: BTreeMap<String, LoadedSource>,
    pub ambiguous_properties: BTreeMap<String, Vec<LoadedSource>>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum HistoryTarget {
    Country(String),
    Province(u32),
}

#[derive(Default)]
pub(super) struct SetupDraft {
    technology: Technology,
    groups: BTreeMap<String, u16>,
    modifiers: Modifiers,
    active: BTreeMap<HistoryTarget, BTreeMap<String, ModifierInstance>>,
    resources: BTreeMap<String, CountryResources>,
}

fn unknown(
    parsed: &ParsedText<'_>,
    kind: &str,
    name: &str,
    location: SourceLocation,
    report: &mut SetupReport,
) {
    let key = format!("{kind}:{name}");
    *report
        .unapplied_definition_fields
        .entry(key.clone())
        .or_default() += 1;
    report
        .unapplied_examples
        .entry(key)
        .or_insert_with(|| source(parsed, location));
}

fn scalar(
    value: &ValueReader<'_, '_, TextEncoding>,
    fail: &impl Fn(&str) -> Error,
) -> Result<String, Error> {
    value
        .read_string()
        .map_err(|_| fail("expected setup scalar"))
}

pub(super) fn block<'a>(
    value: &ValueReader<'a, 'a, TextEncoding>,
    fail: &impl Fn(&str) -> Error,
) -> Result<ObjectReader<'a, 'a, TextEncoding>, Error> {
    let valid = match value.token() {
        jomini::TextToken::Object { mixed: false, .. } => true,
        jomini::TextToken::Array { mixed: false, .. } => value
            .read_array()
            .is_ok_and(|a| a.values().next().is_none()),
        _ => false,
    };
    if !valid {
        return Err(fail("expected setup block"));
    }
    value
        .read_object()
        .map_err(|_| fail("expected setup block"))
}

fn flag(text: &str) -> Option<bool> {
    match text {
        "yes" => Some(true),
        "no" => Some(false),
        _ => None,
    }
}

pub(super) fn micros(text: &str) -> Option<i64> {
    let (negative, text) = text.strip_prefix('-').map_or((false, text), |v| (true, v));
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if whole.is_empty()
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || fraction.len() > 6
        || !fraction.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    let value = whole
        .parse::<i128>()
        .ok()?
        .checked_mul(1_000_000)?
        .checked_add(if fraction.is_empty() {
            0
        } else {
            fraction.parse::<i128>().ok()? * 10_i128.pow(6 - fraction.len() as u32)
        })?;
    i64::try_from(if negative { -value } else { value }).ok()
}

pub(super) trait PropertyDiagnostics {
    fn unsupported(
        &mut self,
        parsed: &ParsedText<'_>,
        kind: &str,
        name: &str,
        location: SourceLocation,
    );
    fn ambiguous(&mut self, key: String, first: LoadedSource, next: LoadedSource);
}

impl PropertyDiagnostics for SetupReport {
    fn unsupported(
        &mut self,
        parsed: &ParsedText<'_>,
        kind: &str,
        name: &str,
        location: SourceLocation,
    ) {
        unknown(parsed, kind, name, location, self);
    }
    fn ambiguous(&mut self, key: String, first: LoadedSource, next: LoadedSource) {
        self.ambiguous_properties
            .entry(key)
            .or_insert_with(|| vec![first])
            .push(next);
    }
}

pub(super) fn properties(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    kind: &str,
    id: &str,
    report: &mut impl PropertyDiagnostics,
) -> Result<ModifierDefinition, Error> {
    let mut definition = ModifierDefinition::default();
    let mut seen = BTreeMap::<String, LoadedSource>::new();
    for (key, op, value) in reader.fields() {
        let name = key.read_string();
        let location = parsed.field_location(&key);
        let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
        if matches!(name.as_str(), "picture" | "desc") {
            report.unsupported(parsed, kind, &name, location);
            continue;
        }
        if let Ok(text) = value.read_string() {
            if micros(&text).is_some() || flag(&text).is_some() {
                equal(op, &fail)?;
                if let Some(first) = seen.get(&name) {
                    definition.values_micros.remove(&name);
                    definition.flags.remove(&name);
                    report.ambiguous(
                        format!("{kind}:{id}:{name}"),
                        first.clone(),
                        source(parsed, location),
                    );
                    continue;
                }
                seen.insert(name.clone(), source(parsed, location));
            }
            if let Some(number) = micros(&text) {
                equal(op, &fail)?;
                if definition.flags.contains_key(&name)
                    || definition.values_micros.insert(name, number).is_some()
                {
                    return Err(fail("duplicate numeric setup property"));
                }
                continue;
            }
            if let Some(flag) = flag(&text) {
                equal(op, &fail)?;
                if definition.values_micros.contains_key(&name)
                    || definition.flags.insert(name, flag).is_some()
                {
                    return Err(fail("duplicate flag setup property"));
                }
                continue;
            }
            if text.starts_with('-') || text.as_bytes().first().is_some_and(u8::is_ascii_digit) {
                return Err(fail(
                    "numeric setup property exceeds supported precision or range",
                ));
            }
        }
        report.unsupported(parsed, kind, &name, location);
    }
    Ok(definition)
}

impl SetupDraft {
    pub(super) fn load(
        root: &ContentRoot,
        files: &[FileEntry],
        report: &mut SetupReport,
    ) -> Result<Self, Error> {
        let mut draft = Self::default();
        let doc = root.read_text("common/technology.txt", ENCODING)?;
        let parsed = doc.parse()?;
        let mut tables = BTreeMap::new();
        let mut root_seen = BTreeSet::new();
        for (key, op, value) in parsed.reader().fields() {
            let name = key.read_string();
            let location = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
            equal(op, &fail)?;
            if !root_seen.insert(name.clone()) {
                return Err(fail("duplicate technology root field"));
            }
            match name.as_str() {
                "groups" => {
                    for (key, op, value) in block(&value, &fail)?.fields() {
                        let group = key.read_string();
                        let location = parsed.field_location(&key);
                        let fail =
                            |m: &str| Error::message(parsed.source(), m).with_location(location);
                        equal(op, &fail)?;
                        let mut start = None;
                        for (key, op, value) in block(&value, &fail)?.fields() {
                            let name = key.read_string();
                            let location = parsed.field_location(&key);
                            let fail = |m: &str| {
                                Error::message(parsed.source(), m).with_location(location)
                            };
                            if name == "start_level" {
                                equal(op, &fail)?;
                                if start.is_some() {
                                    return Err(fail("duplicate technology start level"));
                                }
                                start = Some(
                                    scalar(&value, &fail)?
                                        .parse::<u16>()
                                        .map_err(|_| fail("invalid technology start level"))?,
                                );
                            } else {
                                unknown(&parsed, "technology.group", &name, location, report);
                            }
                        }
                        let start = start.ok_or_else(|| fail("missing technology start level"))?;
                        if draft.groups.insert(group.clone(), start).is_some() {
                            return Err(fail("duplicate technology group"));
                        }
                        report
                            .definition_sources
                            .insert(format!("group:{group}"), source(&parsed, location));
                    }
                }
                "tables" => {
                    for (key, op, value) in block(&value, &fail)?.fields() {
                        let name = key.read_string();
                        let location = parsed.field_location(&key);
                        let fail =
                            |m: &str| Error::message(parsed.source(), m).with_location(location);
                        equal(op, &fail)?;
                        let (branch, power) = match name.as_str() {
                            "adm_tech" => ("administrative", "ADM"),
                            "dip_tech" => ("diplomatic", "DIP"),
                            "mil_tech" => ("military", "MIL"),
                            _ => return Err(fail("unknown technology table branch")),
                        };
                        if tables
                            .insert(branch, (scalar(&value, &fail)?, power))
                            .is_some()
                        {
                            return Err(fail("duplicate technology branch"));
                        }
                    }
                }
                _ => unknown(&parsed, "technology", &name, location, report),
            }
        }
        if tables.len() != 3 || draft.groups.is_empty() {
            return Err(Error::message(
                parsed.source(),
                "missing technology groups or tables",
            ));
        }
        for (branch, (path, power)) in tables {
            let doc = root.read_text(format!("common/{path}"), ENCODING)?;
            let parsed = doc.parse()?;
            let mut levels = Vec::new();
            let mut declared_power = None;
            for (key, op, value) in parsed.reader().fields() {
                let name = key.read_string();
                let location = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
                equal(op, &fail)?;
                match name.as_str() {
                    "monarch_power" => {
                        if declared_power.is_some() {
                            return Err(fail("duplicate technology power"));
                        }
                        declared_power = Some(scalar(&value, &fail)?);
                    }
                    "technology" => {
                        let reader = block(&value, &fail)?;
                        let mut year = None;
                        for (key, op, value) in reader
                            .fields()
                            .filter(|(k, _, _)| k.read_string() == "year")
                        {
                            let fail = |m: &str| {
                                Error::message(parsed.source(), m)
                                    .with_location(parsed.field_location(&key))
                            };
                            equal(op, &fail)?;
                            if year.is_some() {
                                return Err(fail("duplicate technology year"));
                            }
                            year = Some(
                                scalar(&value, &fail)?
                                    .parse::<i16>()
                                    .map_err(|_| fail("invalid technology year"))?,
                            );
                        }
                        let mut props = properties(
                            &parsed,
                            reader,
                            "technology.level",
                            &format!("{branch}:{}", levels.len()),
                            report,
                        )?;
                        props.values_micros.remove("year");
                        report.definition_sources.insert(
                            format!("table:{branch}:{}", levels.len()),
                            source(&parsed, location),
                        );
                        levels.push(TechnologyLevel {
                            year: year.ok_or_else(|| fail("missing technology year"))?,
                            values_micros: props.values_micros,
                            flags: props.flags,
                        });
                    }
                    _ => unknown(&parsed, "technology.table", &name, location, report),
                }
            }
            if declared_power.as_deref() != Some(power) || levels.is_empty() {
                return Err(Error::message(
                    parsed.source(),
                    "technology table power or levels do not match",
                ));
            }
            draft.technology.tables.insert(branch.into(), levels);
        }
        for (group, &start) in &draft.groups {
            if draft
                .technology
                .tables
                .values()
                .any(|levels| usize::from(start) >= levels.len())
            {
                let at = &report.definition_sources[&format!("group:{group}")];
                return Err(Error::message(
                    &at.path,
                    "technology group start level exceeds a table",
                )
                .with_location(SourceLocation {
                    line: at.line,
                    column: at.column,
                }));
            }
        }
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/event_modifiers"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let name = key.read_string();
                let location = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
                equal(op, &fail)?;
                if name.is_empty() || name.starts_with('@') {
                    return Err(fail("invalid modifier identifier"));
                }
                let definition = properties(
                    &parsed,
                    block(&value, &fail)?,
                    "event_modifier",
                    &name,
                    report,
                )?;
                if draft
                    .modifiers
                    .definitions
                    .insert(name.clone(), definition)
                    .is_some()
                {
                    return Err(fail("duplicate event modifier definition"));
                }
                report
                    .definition_sources
                    .insert(format!("modifier:{name}"), source(&parsed, location));
            }
        }
        report.technology_groups = draft.groups.len();
        report.technology_levels = draft.technology.tables.values().map(Vec::len).sum();
        report.modifier_definitions = draft.modifiers.definitions.len();
        Ok(draft)
    }

    pub(super) fn country_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        tag: &str,
        report: &mut SetupReport,
    ) -> Result<BTreeSet<String>, Error> {
        let mut handled = self.history_modifiers(
            parsed,
            reader.clone(),
            HistoryTarget::Country(tag.into()),
            report,
        )?;
        for (key, op, value) in reader
            .fields()
            .filter(|(k, _, _)| k.read_string() == "mercantilism")
        {
            let location = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
            equal(op, &fail)?;
            let amount = nonnegative_milli(&scalar(&value, &fail)?)
                .filter(|v| *v <= 100_000)
                .ok_or_else(|| {
                    fail("mercantilism must be between 0 and 100 with at most three decimal digits")
                })?;
            self.resources
                .entry(tag.into())
                .or_default()
                .mercantilism_milli = Some(amount);
            report.history_sources.insert(
                format!("country:{tag}:mercantilism"),
                source(parsed, location),
            );
            handled.insert("mercantilism".into());
        }
        Ok(handled)
    }

    pub(super) fn province_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        id: u32,
        report: &mut SetupReport,
    ) -> Result<BTreeSet<String>, Error> {
        self.history_modifiers(parsed, reader, HistoryTarget::Province(id), report)
    }

    fn history_modifiers(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        target: HistoryTarget,
        report: &mut SetupReport,
    ) -> Result<BTreeSet<String>, Error> {
        let mut handled = BTreeSet::new();
        let scope = match &target {
            HistoryTarget::Country(tag) => format!("country:{tag}"),
            HistoryTarget::Province(id) => format!("province:{id}"),
        };
        for (key, op, value) in reader.fields() {
            let name = key.read_string();
            if dated_name(&name) {
                continue;
            }
            let country = matches!(target, HistoryTarget::Country(_));
            if !(if country {
                matches!(
                    name.as_str(),
                    "add_country_modifier" | "remove_country_modifier"
                )
            } else {
                matches!(
                    name.as_str(),
                    "add_province_modifier"
                        | "add_permanent_province_modifier"
                        | "remove_province_modifier"
                )
            }) {
                continue;
            }
            let location = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
            equal(op, &fail)?;
            let remove = name.starts_with("remove_");
            let (id, duration, hidden) = if remove {
                (scalar(&value, &fail)?, Some(-1), None)
            } else {
                let mut id = None;
                let mut duration = None;
                let mut hidden = None;
                for (key, op, value) in block(&value, &fail)?.fields() {
                    let name = key.read_string();
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    match name.as_str() {
                        "name" | "duration" | "hidden" => {
                            equal(op, &fail)?;
                            let text = scalar(&value, &fail)?;
                            match name.as_str() {
                                "name" => {
                                    if id.replace(text).is_some() {
                                        return Err(fail("duplicate modifier name"));
                                    }
                                }
                                "duration" => {
                                    let v = text
                                        .parse::<i32>()
                                        .map_err(|_| fail("invalid modifier duration"))?;
                                    if v < -1 {
                                        return Err(fail("invalid modifier duration"));
                                    }
                                    if duration.replace(v).is_some() {
                                        return Err(fail("duplicate modifier duration"));
                                    }
                                }
                                _ => {
                                    let v =
                                        flag(&text).ok_or_else(|| fail("expected yes or no"))?;
                                    if hidden.replace(v).is_some() {
                                        return Err(fail("duplicate hidden modifier flag"));
                                    }
                                }
                            }
                        }
                        _ => unknown(parsed, "history.modifier", &name, at, report),
                    }
                }
                (
                    id.ok_or_else(|| fail("missing modifier name"))?,
                    duration,
                    hidden,
                )
            };
            if !self.modifiers.definitions.contains_key(&id) {
                return Err(fail(&format!("unknown event modifier {id}")));
            }
            let provenance = format!("{scope}:{id}");
            let active = self.active.entry(target.clone()).or_default();
            if remove || duration != Some(-1) {
                active.remove(&id);
            } else {
                active.insert(id, ModifierInstance { hidden });
            }
            if !remove && duration != Some(-1) {
                report
                    .pending_timed_modifiers
                    .insert(provenance.clone(), source(parsed, location));
            } else {
                report.pending_timed_modifiers.remove(&provenance);
            }
            report
                .history_sources
                .insert(provenance, source(parsed, location));
            handled.insert(name);
        }
        Ok(handled)
    }

    pub(super) fn bind(
        mut self,
        initial: &mut InitialWorld,
        date: Date,
        report: &mut SetupReport,
    ) -> Result<(), Error> {
        let mut resources = BTreeMap::new();
        for country in initial.countries() {
            let entity = initial
                .country_entity(&country.id)
                .expect("registered country");
            let state = self.resources.remove(&country.id).unwrap_or_default();
            report.countries_with_mercantilism += usize::from(state.mercantilism_milli.is_some());
            resources.insert(entity, state);
            let start = country
                .technology_group
                .as_ref()
                .map(|group| {
                    self.groups.get(group).ok_or_else(|| {
                        Error::message(
                            "world",
                            format!(
                                "country {} references unknown technology group {group}",
                                country.id
                            ),
                        )
                    })
                })
                .transpose()?;
            if date == Date::parse(BASE_DATE).expect("valid baseline date")
                && let Some(start) = start
            {
                self.technology.countries.insert(
                    entity,
                    self.technology
                        .tables
                        .keys()
                        .map(|b| (b.clone(), *start))
                        .collect(),
                );
                report.technology_sources.insert(
                    country.id.clone(),
                    report.definition_sources[&format!(
                        "group:{}",
                        country.technology_group.as_ref().expect("known group")
                    )]
                        .clone(),
                );
                report.countries_with_technology += 1;
            } else {
                report.unknown_technology_countries.push(country.id.clone());
            }
            if !initial.owned_provinces(&country.id).is_empty() {
                report.unknown_resource_countries.push(country.id.clone());
            }
        }
        for (target, active) in self.active {
            if active.is_empty() {
                continue;
            }
            let entity = match target {
                HistoryTarget::Country(id) => initial.country_entity(&id),
                HistoryTarget::Province(id) => initial.province_entity(id),
            }
            .ok_or_else(|| Error::message("world", "unknown modifier entity"))?;
            report.active_modifiers += active.len();
            self.modifiers.active.insert(entity, active);
        }
        initial
            .set_technology(self.technology)
            .map_err(|e| Error::caused_by("world", e))?;
        initial
            .set_modifiers(self.modifiers)
            .map_err(|e| Error::caused_by("world", e))?;
        initial
            .set_country_resources(resources)
            .map_err(|e| Error::caused_by("world", e))?;
        Ok(())
    }
}
