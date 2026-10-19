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
    ContentView, Error, FileEntry, LoadedSource, ParsedText, SourceLocation, TextEncoding,
};

use crate::country_conditions::Condition;

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

#[derive(Default)]
struct History {
    unknown_selection: bool,
}

pub(super) struct IdeaDraft {
    groups: BTreeMap<String, IdeaGroup>,
    known_ideas: BTreeMap<String, BTreeMap<String, u16>>,
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
        root: &impl ContentView,
        files: &[FileEntry],
        report: &mut IdeaReport,
    ) -> Result<Self, Error> {
        let mut draft = Self {
            groups: BTreeMap::new(),
            known_ideas: BTreeMap::new(),
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
                let mut idea_index = 0u16;
                for (key, op, value) in body.fields() {
                    let name = key.read_string();
                    if matches!(
                        name.as_str(),
                        "free"
                            | "trigger"
                            | "start"
                            | "bonus"
                            | "ai_will_do"
                            | "category"
                            | "important"
                    ) {
                        continue;
                    }
                    equal(op, &fail)?;
                    if value.read_object().is_err() {
                        return Err(fail("idea declaration requires a block"));
                    }
                    let index = idea_index;
                    idea_index = idea_index
                        .checked_add(1)
                        .ok_or_else(|| fail("idea group exceeds supported length"))?;
                    if draft
                        .known_ideas
                        .entry(name)
                        .or_default()
                        .insert(id.clone(), index)
                        .is_some()
                    {
                        return Err(fail("duplicate idea declaration in group"));
                    }
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
                                None,
                                false,
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
        for (key, _, value) in reader.fields() {
            let name = key.read_string();
            if dated_name(&name) {
                continue;
            }
            if matches!(name.as_str(), "add_idea_group" | "swap_free_idea_group") {
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
        saved_states: Option<&BTreeMap<String, IdeaState>>,
    ) -> Ideas {
        let mut countries = BTreeMap::new();
        for country in initial.countries() {
            let mut reasons = BTreeSet::new();
            let mut groups = None;
            let mut saved_selection = false;
            if let Some(saved) = saved_states.and_then(|states| states.get(&country.id)) {
                saved_selection = true;
                groups = saved.groups.clone();
                if groups.is_some() {
                    report.selected_countries += 1;
                } else {
                    reasons.insert("saved_idea_selection_unknown".into());
                }
            } else if let Some(history) = self.histories.get(&country.id) {
                if history.unknown_selection {
                    reasons.insert("unapplied_history_selection".into());
                } else {
                    for (id, condition) in &self.candidates {
                        let mut pending = BTreeSet::new();
                        match condition.evaluate(initial, &country.id, &mut pending, None, None) {
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
            } else if saved_selection {
                report.unresolved_countries.remove(&country.id);
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
            known_ideas: self.known_ideas,
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
