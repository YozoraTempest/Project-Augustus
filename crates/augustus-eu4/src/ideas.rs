// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    IdeaDefinition, IdeaEffect, IdeaGroup, IdeaState, Ideas, InitialWorld, WorldDate,
};
use jomini::text::ObjectReader;
use serde::Serialize;

use crate::bootstrap::{dated_name, direct_txt, equal, source};
use crate::setup::{PropertyDiagnostics, block, flag, properties};
use crate::{
    ContentRoot, Error, FileEntry, LoadedSource, ParsedText, SourceLocation, TextEncoding,
};

const ENCODING: TextEncoding = TextEncoding::Windows1252;

#[derive(Debug, Default, Serialize)]
pub struct IdeaReport {
    pub free_groups: usize,
    pub status_sources: BTreeMap<String, LoadedSource>,
    pub unknown_statuses: BTreeMap<String, BTreeSet<String>>,
    pub skipped_paid_groups: usize,
    pub selected_countries: usize,
    pub definition_sources: BTreeMap<String, LoadedSource>,
    pub unsupported_fields: BTreeMap<String, usize>,
    pub unsupported_examples: BTreeMap<String, LoadedSource>,
    pub ambiguous_properties: BTreeMap<String, Vec<LoadedSource>>,
    pub unresolved_countries: BTreeMap<String, BTreeSet<String>>,
}

impl PropertyDiagnostics for IdeaReport {
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

enum Condition {
    All(Vec<Self>),
    Any(Vec<Self>),
    None(Vec<Self>),
    Literal(bool),
    Match(String, String),
    Status(String, bool),
    Feature(String),
    Capital(Vec<Self>),
    Region(String, bool),
    Unknown(String),
}

impl Condition {
    fn compile(
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        report: &mut IdeaReport,
        depth: usize,
        country: bool,
    ) -> Result<Vec<Self>, Error> {
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "idea trigger nesting exceeds limit",
            ));
        }
        reader
            .fields()
            .map(|(key, op, value)| {
                let name = key.read_string();
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                Ok(match name.as_str() {
                    "AND" | "OR" | "NOT" => {
                        let children = Self::compile(
                            parsed,
                            block(&value, &fail)?,
                            report,
                            depth + 1,
                            country,
                        )?;
                        match name.as_str() {
                            "AND" => Self::All(children),
                            "OR" => Self::Any(children),
                            _ => Self::None(children),
                        }
                    }
                    "capital_scope" if country => Self::Capital(Self::compile(
                        parsed,
                        block(&value, &fail)?,
                        report,
                        depth + 1,
                        false,
                    )?),
                    "region" | "superregion" if !country => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected geographic identifier"))?;
                        if id.is_empty() {
                            return Err(fail("empty geographic identifier"));
                        }
                        Self::Region(id, name == "superregion")
                    }
                    "is_client_nation"
                    | "is_colonial_nation"
                    | "is_federation_nation"
                    | "is_revolution_target"
                    | "is_revolutionary"
                        if country =>
                    {
                        let expected = flag(
                            &value
                                .read_string()
                                .map_err(|_| fail("expected country status flag"))?,
                        )
                        .ok_or_else(|| fail("expected yes or no"))?;
                        let status = if name == "is_revolution_target" {
                            "revolution_target"
                        } else {
                            name.trim_start_matches("is_")
                        };
                        Self::Status(status.into(), expected)
                    }
                    "has_government_attribute" if country => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected government feature identifier"))?;
                        if id.is_empty() {
                            return Err(fail("empty government feature identifier"));
                        }
                        Self::Feature(id)
                    }
                    "always" => Self::Literal(
                        flag(
                            &value
                                .read_string()
                                .map_err(|_| fail("expected idea trigger flag"))?,
                        )
                        .ok_or_else(|| fail("expected yes or no"))?,
                    ),
                    "tag" | "TAG" | "primary_culture" | "culture_group" | "religion"
                    | "religion_group" | "government" | "technology_group" | "dynasty"
                    | "has_reform" | "has_country_flag"
                        if country =>
                    {
                        let text = value
                            .read_string()
                            .map_err(|_| fail("expected scalar idea trigger"))?;
                        if text.is_empty() {
                            return Err(fail("empty idea trigger value"));
                        }
                        Self::Match(if name == "TAG" { "tag".into() } else { name }, text)
                    }
                    _ => {
                        report.unsupported(parsed, "trigger", &name, at);
                        Self::Unknown(name)
                    }
                })
            })
            .collect()
    }

    fn evaluate(
        &self,
        initial: &InitialWorld,
        tag: &str,
        history: &History,
        reasons: &mut BTreeSet<String>,
        province: Option<augustus_core::EntityId>,
    ) -> Option<bool> {
        match self {
            Self::Literal(value) => Some(*value),
            Self::Unknown(name) => {
                reasons.insert(format!("unsupported_trigger:{name}"));
                None
            }
            Self::All(children) | Self::Any(children) | Self::None(children) => {
                let all = matches!(self, Self::All(_));
                let mut unknown = false;
                let mut pending = BTreeSet::new();
                for child in children {
                    match child.evaluate(initial, tag, history, &mut pending, province) {
                        Some(value) if value != all => {
                            return Some(if matches!(self, Self::None(_)) {
                                false
                            } else {
                                !all
                            });
                        }
                        None => unknown = true,
                        _ => (),
                    }
                }
                if unknown {
                    reasons.extend(pending);
                    None
                } else {
                    Some(all || matches!(self, Self::None(_)))
                }
            }
            Self::Capital(children) => {
                let entity = initial.country_entity(tag).expect("registered country");
                let capital = initial.world().entity(entity).ok()?.capital;
                if capital.is_none() {
                    reasons.insert("unknown_capital".into());
                    return None;
                }
                let mut pending = BTreeSet::new();
                let mut unknown = false;
                for child in children {
                    match child.evaluate(initial, tag, history, &mut pending, capital) {
                        Some(false) => return Some(false),
                        None => unknown = true,
                        _ => (),
                    }
                }
                if unknown {
                    reasons.extend(pending);
                    None
                } else {
                    Some(true)
                }
            }
            Self::Region(name, superregion) => {
                let result = province.and_then(|p| {
                    if *superregion {
                        initial.world().administration().in_superregion(p, name)
                    } else {
                        initial.world().administration().in_region(p, name)
                    }
                });
                if result.is_none() {
                    reasons.insert(format!("unknown_geography:{name}"));
                }
                result
            }
            Self::Status(name, expected) => {
                let entity = initial.country_entity(tag).expect("registered country");
                let result = if name == "revolutionary" {
                    initial.world().governments().feature(entity, name)
                } else {
                    initial.world().politics().status(entity, name)
                };
                if result.is_none() {
                    reasons.insert(format!("unknown_status:{name}"));
                }
                result.map(|v| v == *expected)
            }
            Self::Feature(name) => {
                let entity = initial.country_entity(tag).expect("registered country");
                let result = initial.world().governments().feature(entity, name);
                if result.is_none() {
                    reasons.insert(format!("unknown_government_feature:{name}"));
                }
                result
            }
            Self::Match(name, expected) => {
                let country = initial.country(tag).expect("registered country");
                let entity = initial.country_entity(tag).expect("registered country");
                let world = initial.world();
                let actual = match name.as_str() {
                    "tag" => Some(tag),
                    "primary_culture" => country.primary_culture.as_deref(),
                    "religion" => country.religion.as_deref(),
                    "government" => world
                        .governments()
                        .countries
                        .get(&entity)
                        .and_then(|g| g.kind.as_deref()),
                    "technology_group" => country.technology_group.as_deref(),
                    "dynasty" => world
                        .politics()
                        .countries
                        .get(&entity)
                        .and_then(|c| c.ruler.as_ref())
                        .and_then(|p| p.dynasty.as_deref()),
                    "culture_group" | "religion_group" => {
                        let dimension = if name == "culture_group" {
                            "culture"
                        } else {
                            "religion"
                        };
                        world
                            .affiliations()
                            .0
                            .get(dimension)
                            .and_then(|d| d.members.get(&entity))
                            .and_then(|m| m.as_ref())
                            .and_then(|m| world.affiliations().0[dimension].groups.get(m))
                            .and_then(|g| g.as_deref())
                    }
                    "has_reform" => {
                        return if let Some(reforms) = world
                            .governments()
                            .countries
                            .get(&entity)
                            .and_then(|g| g.reforms.as_ref())
                        {
                            Some(reforms.contains(expected))
                        } else {
                            reasons.insert("unknown_reforms".into());
                            None
                        };
                    }
                    "has_country_flag" => {
                        return if history.unknown_flags {
                            reasons.insert("unknown_country_flags".into());
                            None
                        } else {
                            Some(history.flags.contains(expected))
                        };
                    }
                    _ => unreachable!(),
                };
                if actual.is_none() {
                    reasons.insert(format!("unknown_input:{name}"));
                }
                actual.map(|a| a == expected)
            }
        }
    }
}

#[derive(Default)]
struct History {
    flags: BTreeSet<String>,
    unknown_flags: bool,
    unknown_selection: bool,
}

pub(super) struct IdeaDraft {
    groups: BTreeMap<String, IdeaGroup>,
    candidates: Vec<(String, Condition)>,
    histories: BTreeMap<String, History>,
}

fn effect(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    id: &str,
    report: &mut IdeaReport,
) -> Result<IdeaEffect, Error> {
    let modifier = properties(parsed, reader.clone(), "ideas", id, report)?;
    let mut unresolved_properties: BTreeSet<String> = reader
        .fields()
        .map(|(k, _, _)| k.read_string())
        .filter(|p| !modifier.values_micros.contains_key(p) && !modifier.flags.contains_key(p))
        .collect();
    let unresolved_effects = if unresolved_properties.remove("effect") {
        ["unlock_script".into()].into()
    } else {
        BTreeSet::new()
    };
    Ok(IdeaEffect {
        modifier,
        unresolved_properties,
        unresolved_effects,
    })
}

impl IdeaDraft {
    pub(super) fn load(
        root: &ContentRoot,
        files: &[FileEntry],
        report: &mut IdeaReport,
    ) -> Result<Self, Error> {
        let mut draft = Self {
            groups: BTreeMap::new(),
            candidates: Vec::new(),
            histories: BTreeMap::new(),
        };
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/ideas"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let id = key.read_string();
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                let mut free = None;
                for (k, op, v) in body.fields().filter(|(k, _, _)| k.read_string() == "free") {
                    equal(op, &fail)?;
                    if free.is_some() {
                        return Err(fail("duplicate free idea flag"));
                    }
                    free = Some(
                        flag(
                            &v.read_string()
                                .map_err(|_| fail("expected free idea flag"))?,
                        )
                        .ok_or_else(|| {
                            Error::message(parsed.source(), "expected yes or no")
                                .with_location(parsed.field_location(&k))
                        })?,
                    );
                }
                if free != Some(true) {
                    report.skipped_paid_groups += 1;
                    continue;
                }
                let mut group = IdeaGroup {
                    traditions: IdeaEffect::default(),
                    ideas: Vec::new(),
                    completion: IdeaEffect::default(),
                };
                let mut trigger = Condition::Literal(true);
                let mut seen = BTreeSet::new();
                for (key, op, value) in body.fields() {
                    let name = key.read_string();
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    equal(op, &fail)?;
                    if !seen.insert(name.clone()) {
                        return Err(fail("duplicate idea group field"));
                    }
                    match name.as_str() {
                        "free" => (),
                        "trigger" => {
                            trigger = Condition::All(Condition::compile(
                                &parsed,
                                block(&value, &fail)?,
                                report,
                                0,
                                true,
                            )?)
                        }
                        "start" => {
                            group.traditions = effect(
                                &parsed,
                                block(&value, &fail)?,
                                &format!("{id}:traditions"),
                                report,
                            )?
                        }
                        "bonus" => {
                            group.completion = effect(
                                &parsed,
                                block(&value, &fail)?,
                                &format!("{id}:completion"),
                                report,
                            )?
                        }
                        "ai_will_do" | "category" => {
                            report.unsupported(&parsed, "metadata", &name, at)
                        }
                        _ => {
                            group.ideas.push(IdeaDefinition {
                                id: name.clone(),
                                effect: effect(
                                    &parsed,
                                    block(&value, &fail)?,
                                    &format!("{id}:{name}"),
                                    report,
                                )?,
                            });
                        }
                    }
                    report
                        .definition_sources
                        .insert(format!("{id}:{name}"), source(&parsed, at));
                }
                if id.is_empty() || group.ideas.is_empty() {
                    return Err(fail("expected nonempty idea group"));
                }
                if draft.groups.insert(id.clone(), group).is_some() {
                    return Err(fail("ambiguous idea group definition"));
                }
                report
                    .definition_sources
                    .insert(id.clone(), source(&parsed, at));
                draft.candidates.push((id, trigger));
            }
        }
        report.free_groups = draft.groups.len();
        Ok(draft)
    }

    pub(super) fn country_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        tag: &str,
        report: &mut IdeaReport,
    ) -> Result<(), Error> {
        let history = self.histories.entry(tag.into()).or_default();
        for (key, op, value) in reader.fields() {
            let name = key.read_string();
            if dated_name(&name) {
                continue;
            }
            if matches!(name.as_str(), "set_country_flag" | "clr_country_flag") {
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                let flag = value
                    .read_string()
                    .map_err(|_| fail("expected country flag identifier"))?;
                if flag.is_empty() {
                    return Err(fail("empty country flag"));
                }
                if name == "set_country_flag" {
                    history.flags.insert(flag);
                } else {
                    history.flags.remove(&flag);
                }
            } else if matches!(name.as_str(), "add_idea_group" | "swap_free_idea_group") {
                history.unknown_selection = true;
                report.unsupported(parsed, "history", &name, parsed.field_location(&key));
            } else if let Ok(children) = value.read_object() {
                scoped_history(children, history, 0)?;
            }
        }
        Ok(())
    }

    pub(super) fn bind(
        self,
        initial: &InitialWorld,
        date: WorldDate,
        report: &mut IdeaReport,
    ) -> Ideas {
        let mut countries = BTreeMap::new();
        for country in initial.countries() {
            let mut reasons = BTreeSet::new();
            let mut groups = None;
            if let Some(history) = self.histories.get(&country.id) {
                if history.unknown_selection {
                    reasons.insert("unapplied_history_selection".into());
                } else {
                    for (id, condition) in &self.candidates {
                        let mut pending = BTreeSet::new();
                        match condition.evaluate(initial, &country.id, history, &mut pending, None)
                        {
                            Some(false) => (),
                            Some(true) => {
                                let progress = (date
                                    == WorldDate {
                                        year: 1444,
                                        month: 11,
                                        day: 11,
                                    })
                                .then_some(0);
                                groups = Some([(id.clone(), progress)].into());
                                if progress.is_none() {
                                    reasons.insert("unknown_historical_progress".into());
                                }
                                break;
                            }
                            None => {
                                reasons.extend(
                                    pending.into_iter().map(|p| format!("candidate:{id}:{p}")),
                                );
                                break;
                            }
                        }
                    }
                    if groups.is_none() && reasons.is_empty() {
                        reasons.insert("no_matching_free_group".into());
                    }
                }
            } else {
                reasons.insert("missing_or_ambiguous_country_history".into());
            }
            if groups.is_some() {
                report.selected_countries += 1;
            }
            if !reasons.is_empty() {
                report
                    .unresolved_countries
                    .insert(country.id.clone(), reasons);
            }
            countries.insert(
                initial
                    .country_entity(&country.id)
                    .expect("registered country"),
                IdeaState { groups },
            );
        }
        Ideas {
            groups: self.groups,
            countries,
        }
    }
}

fn scoped_history(
    reader: ObjectReader<'_, '_, TextEncoding>,
    history: &mut History,
    depth: usize,
) -> Result<(), Error> {
    if depth > 64 {
        return Err(Error::message(
            "history/countries",
            "scoped idea history nesting exceeds limit",
        ));
    }
    for (key, _, value) in reader.fields() {
        match key.read_string().as_str() {
            "set_country_flag" | "clr_country_flag" => history.unknown_flags = true,
            "add_idea_group" | "swap_free_idea_group" => history.unknown_selection = true,
            _ => {
                if let Ok(children) = value.read_object() {
                    scoped_history(children, history, depth + 1)?;
                }
            }
        }
    }
    Ok(())
}
