// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    GovernmentForm, GovernmentMetric, GovernmentReform, GovernmentState, Governments, InitialWorld,
};
use jomini::text::ObjectReader;
use serde::Serialize;

use crate::attributes::AttributeDraft;
use crate::autonomy::constant_condition;
use crate::bootstrap::{dated_name, direct_txt, equal, nonnegative_milli, source};
use crate::setup::{PropertyDiagnostics, block, properties};
use crate::{
    ContentView, Error, FileEntry, LoadedSource, ParsedText, SourceLocation, TextEncoding,
};

type Reader<'a, 'b> = ObjectReader<'a, 'b, TextEncoding>;
const ENCODING: TextEncoding = TextEncoding::Windows1252;
const FLAGS: [&str; 8] = [
    "monarchy",
    "republic",
    "nomad",
    "has_devotion",
    "has_meritocracy",
    "religion",
    "tribal",
    "native_mechanic",
];

#[derive(Debug, Default, Serialize)]
pub struct GovernmentReport {
    pub transition_forms: usize,
    pub unavailable_transitions: BTreeMap<String, String>,
    pub government_definitions: usize,
    pub reform_definitions: usize,
    pub countries_with_selected_reforms: usize,
    pub countries_with_legitimacy: usize,
    pub default_legitimacy_countries: Vec<String>,
    pub unknown_selection_countries: Vec<String>,
    pub unknown_metric_countries: Vec<String>,
    pub definition_sources: BTreeMap<String, LoadedSource>,
    pub history_sources: BTreeMap<String, LoadedSource>,
    pub unapplied_definition_fields: BTreeMap<String, usize>,
    pub unapplied_examples: BTreeMap<String, LoadedSource>,
    pub ambiguous_properties: BTreeMap<String, Vec<LoadedSource>>,
    pub ambiguous_definitions: BTreeMap<String, Vec<LoadedSource>>,
    pub pending_history: BTreeMap<String, Vec<LoadedSource>>,
}

impl PropertyDiagnostics for GovernmentReport {
    fn unsupported(&mut self, parsed: &ParsedText<'_>, kind: &str, name: &str, at: SourceLocation) {
        let key = format!("{kind}:{name}");
        *self
            .unapplied_definition_fields
            .entry(key.clone())
            .or_default() += 1;
        self.unapplied_examples
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
struct Seed {
    kind: Option<String>,
    reforms: Vec<(bool, String)>,
    legitimacy: Option<i64>,
    uncertain: BTreeSet<String>,
}

#[derive(Default)]
struct Reform {
    effect: GovernmentReform,
    feature_sources: BTreeMap<String, LoadedSource>,
    flags: BTreeMap<String, bool>,
    uncertain_flags: BTreeSet<String>,
}

struct Form {
    base: Option<String>,
    levels: BTreeMap<String, Option<String>>,
}

#[derive(Default)]
pub(super) struct GovernmentDraft {
    forms: BTreeMap<String, Form>,
    reforms: BTreeMap<String, Reform>,
    countries: BTreeMap<String, Seed>,
}

impl GovernmentDraft {
    pub(super) fn load(
        root: &impl ContentView,
        files: &[FileEntry],
        report: &mut GovernmentReport,
    ) -> Result<Self, Error> {
        let mut draft = Self::default();
        for file in files.iter().filter(|f| {
            direct_txt(&f.relative_path, "common/governments")
                || direct_txt(&f.relative_path, "common/government_reforms")
        }) {
            let forms = direct_txt(&file.relative_path, "common/governments");
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let name = key.read_string();
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                let id = format!("{}:{name}", if forms { "government" } else { "reform" });
                if let Some(origins) = report.ambiguous_definitions.get_mut(&id) {
                    origins.push(source(&parsed, at));
                    continue;
                }
                if let Some(first) = report.definition_sources.remove(&id) {
                    if forms {
                        draft.forms.remove(&name);
                    } else {
                        draft.reforms.remove(&name);
                    }
                    report
                        .ambiguous_definitions
                        .insert(id, vec![first, source(&parsed, at)]);
                    continue;
                }
                if forms {
                    let fields: Vec<_> = body
                        .fields()
                        .filter(|(k, _, _)| k.read_string() == "basic_reform")
                        .collect();
                    if fields.len() > 1 {
                        report.ambiguous_properties.insert(
                            format!("government:{name}:basic_reform"),
                            fields
                                .iter()
                                .map(|(k, _, _)| source(&parsed, parsed.field_location(k)))
                                .collect(),
                        );
                    }
                    let base = if fields.len() == 1 {
                        let (key, op, value) = &fields[0];
                        let fail = |m: &str| {
                            Error::message(parsed.source(), m)
                                .with_location(parsed.field_location(key))
                        };
                        equal(*op, &fail)?;
                        Some(
                            value
                                .read_string()
                                .map_err(|_| fail("expected basic reform identifier"))?,
                        )
                    } else {
                        None
                    };
                    let mut levels = BTreeMap::new();
                    let level_blocks: Vec<_> = body
                        .fields()
                        .filter(|(k, _, _)| k.read_string() == "reform_levels")
                        .collect();
                    if level_blocks.len() > 1 {
                        return Err(fail("duplicate reform levels block"));
                    }
                    if let Some((_, op, value)) = level_blocks.first() {
                        equal(*op, &fail)?;
                        for (level, op, value) in block(value, &fail)?.fields() {
                            equal(op, &fail)?;
                            let lists: Vec<_> = block(&value, &fail)?
                                .fields()
                                .filter(|(k, _, _)| k.read_string() == "reforms")
                                .collect();
                            if lists.len() != 1 {
                                return Err(fail("reform level requires one reform list"));
                            }
                            let (_, op, value) = &lists[0];
                            equal(*op, &fail)?;
                            for value in value
                                .read_array()
                                .map_err(|_| fail("expected reform level list"))?
                                .values()
                            {
                                let reform = value
                                    .read_string()
                                    .map_err(|_| fail("expected reform identifier"))?;
                                levels
                                    .entry(reform)
                                    .and_modify(|v| *v = None)
                                    .or_insert(Some(level.read_string()));
                            }
                        }
                    }
                    draft.forms.insert(name, Form { base, levels });
                } else {
                    let mut reform = Reform::default();
                    Self::reform_body(&parsed, body, &name, &mut reform, report, 0)?;
                    draft.reforms.insert(name, reform);
                }
                report.definition_sources.insert(id, source(&parsed, at));
            }
        }
        report.government_definitions = draft.forms.len();
        report.reform_definitions = draft.reforms.len();
        Ok(draft)
    }

    fn reform_body(
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        name: &str,
        reform: &mut Reform,
        report: &mut GovernmentReport,
        depth: usize,
    ) -> Result<(), Error> {
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "government definition nesting exceeds 64",
            ));
        }
        let mut seen_flags = BTreeSet::new();
        let mut modifiers_seen = false;
        for (key, op, value) in reader.fields() {
            let field = key.read_string();
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            if FLAGS.contains(&field.as_str()) {
                equal(op, &fail)?;
                let flag = match value.read_string().ok().as_deref() {
                    Some("yes") => true,
                    Some("no") => false,
                    _ => return Err(fail("expected government flag")),
                };
                if !seen_flags.insert(field.clone()) {
                    reform.uncertain_flags.insert(field.clone());
                }
                if field == "nomad" {
                    Self::feature(parsed, &key, op, &value, name, reform, report)?;
                }
                reform.flags.insert(field, flag);
            } else if field == "revolutionary" {
                Self::feature(parsed, &key, op, &value, name, reform, report)?;
            } else if field == "custom_attributes" {
                equal(op, &fail)?;
                for (key, op, value) in block(&value, &fail)?.fields() {
                    Self::feature(parsed, &key, op, &value, name, reform, report)?;
                }
            } else if field == "modifiers" {
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                let definition = properties(parsed, body.clone(), "reform", name, report)?;
                reform.effect.unresolved_properties.extend(
                    body.fields()
                        .map(|(key, _, _)| key.read_string())
                        .filter(|p| {
                            !definition.values_micros.contains_key(p)
                                && !definition.flags.contains_key(p)
                        }),
                );
                if depth > 0 || modifiers_seen {
                    reform.effect.unresolved_properties.extend(
                        definition
                            .values_micros
                            .keys()
                            .chain(definition.flags.keys())
                            .cloned(),
                    );
                    if modifiers_seen {
                        reform.effect.unresolved_properties.extend(
                            reform
                                .effect
                                .modifier
                                .values_micros
                                .keys()
                                .chain(reform.effect.modifier.flags.keys())
                                .cloned(),
                        );
                        reform.effect.modifier = Default::default();
                    }
                    report.unsupported(parsed, "reform", "conditional_or_duplicate_modifiers", at);
                } else {
                    reform.effect.modifier = definition;
                }
                modifiers_seen = true;
            } else if field == "conditional" {
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                match constant_condition(parsed, body.clone(), "allow")? {
                    Some(false) => {}
                    Some(true) => Self::reform_body(parsed, body, name, reform, report, depth + 1)?,
                    None => Self::pending_definition(parsed, body, reform, report, depth + 1)?,
                }
            } else if !matches!(field.as_str(), "basic_reform" | "allow" | "icon") {
                report.unsupported(parsed, "reform", &field, at);
            }
        }
        for key in report
            .ambiguous_properties
            .keys()
            .filter_map(|k| k.strip_prefix(&format!("reform:{name}:")))
        {
            reform.effect.unresolved_properties.insert(key.into());
        }
        Ok(())
    }

    fn feature(
        parsed: &ParsedText<'_>,
        key: &jomini::text::ScalarReader<'_, TextEncoding>,
        op: Option<jomini::text::Operator>,
        value: &jomini::text::ValueReader<'_, '_, TextEncoding>,
        id: &str,
        reform: &mut Reform,
        report: &mut GovernmentReport,
    ) -> Result<(), Error> {
        let at = parsed.field_location(key);
        let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
        equal(op, &fail)?;
        let name = key.read_string();
        let flag = crate::setup::flag(
            &value
                .read_string()
                .map_err(|_| fail("expected government feature flag"))?,
        )
        .ok_or_else(|| fail("expected yes or no"))?;
        if let Some(first) = reform.feature_sources.get(&name) {
            reform.effect.features.insert(name.clone(), None);
            report.ambiguous(
                format!("reform_feature:{id}:{name}"),
                first.clone(),
                source(parsed, at),
            );
        } else {
            reform
                .feature_sources
                .insert(name.clone(), source(parsed, at));
            reform.effect.features.entry(name).or_insert(Some(flag));
        }
        Ok(())
    }

    fn pending_definition(
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        reform: &mut Reform,
        report: &mut GovernmentReport,
        depth: usize,
    ) -> Result<(), Error> {
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "government definition nesting exceeds 64",
            ));
        }
        for (key, _, value) in reader.fields() {
            let name = key.read_string();
            if name == "allow" {
                continue;
            }
            if FLAGS.contains(&name.as_str()) {
                if name == "nomad" {
                    reform.effect.features.insert(name.clone(), None);
                }
                reform.uncertain_flags.insert(name);
            } else if name == "revolutionary" {
                reform.effect.features.insert(name, None);
            } else if name == "custom_attributes" {
                let fail = |m: &str| {
                    Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
                };
                for (key, _, _) in block(&value, &fail)?.fields() {
                    reform.effect.features.insert(key.read_string(), None);
                }
            } else if name == "modifiers" {
                let fail = |m: &str| {
                    Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
                };
                for (key, _, _) in block(&value, &fail)?.fields() {
                    reform
                        .effect
                        .unresolved_properties
                        .insert(key.read_string());
                }
            } else if name == "conditional" {
                Self::pending_definition(
                    parsed,
                    value.read_object().map_err(|_| {
                        Error::message(parsed.source(), "expected conditional block")
                    })?,
                    reform,
                    report,
                    depth + 1,
                )?;
            }
            report.unsupported(
                parsed,
                "reform",
                "pending_conditional",
                parsed.field_location(&key),
            );
        }
        Ok(())
    }

    pub(super) fn country_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        country: &str,
        report: &mut GovernmentReport,
    ) -> Result<(), Error> {
        let seed = self.countries.entry(country.into()).or_default();
        Self::history(parsed, reader, country, seed, report, 0)
    }

    fn history(
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        country: &str,
        seed: &mut Seed,
        report: &mut GovernmentReport,
        depth: usize,
    ) -> Result<(), Error> {
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "government history nesting exceeds 64",
            ));
        }
        for (key, op, value) in reader.fields() {
            let name = key.read_string();
            if dated_name(&name) {
                continue;
            }
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            if matches!(
                name.as_str(),
                "government" | "add_government_reform" | "remove_government_reform" | "legitimacy"
            ) {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected government history scalar"))?;
                match name.as_str() {
                    "government" => {
                        if seed.kind.as_ref().is_some_and(|k| k != &text) {
                            seed.uncertain.insert("reforms".into());
                        }
                        seed.kind = Some(text);
                    }
                    "add_government_reform" => {
                        seed.reforms.push((true, text));
                    }
                    "remove_government_reform" => {
                        seed.reforms.push((false, text));
                    }
                    _ => {
                        seed.legitimacy = Some(nonnegative_milli(&text).filter(|v| *v <= 100_000).ok_or_else(|| fail("legitimacy must be between 0 and 100 with at most three decimals"))?);
                    }
                }
                report
                    .history_sources
                    .insert(format!("{country}:{name}"), source(parsed, at));
            } else if name == "if" {
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                match constant_condition(parsed, body.clone(), "limit")? {
                    Some(true) => Self::history(parsed, body, country, seed, report, depth + 1)?,
                    Some(false) => {}
                    None => Self::pending_history(parsed, body, country, seed, report, depth + 1)?,
                }
            } else if name == "add_legitimacy" {
                seed.uncertain.insert("legitimacy".into());
                report
                    .pending_history
                    .entry(country.into())
                    .or_default()
                    .push(source(parsed, at));
            } else if !matches!(
                name.as_str(),
                "limit" | "monarch" | "heir" | "queen" | "monarch_consort"
            ) && let Ok(body) = value.read_object()
            {
                Self::pending_history(parsed, body, country, seed, report, depth + 1)?;
            }
        }
        Ok(())
    }

    fn pending_history(
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        country: &str,
        seed: &mut Seed,
        report: &mut GovernmentReport,
        depth: usize,
    ) -> Result<(), Error> {
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "government history nesting exceeds 64",
            ));
        }
        for (key, _, value) in reader.fields() {
            let name = key.read_string();
            if dated_name(&name)
                || matches!(
                    name.as_str(),
                    "limit" | "monarch" | "heir" | "queen" | "monarch_consort"
                )
            {
                continue;
            }
            let field = match name.as_str() {
                "government" => Some("kind"),
                "add_government_reform" | "remove_government_reform" => Some("reforms"),
                "legitimacy" | "add_legitimacy" => Some("legitimacy"),
                _ => None,
            };
            if let Some(field) = field {
                seed.uncertain.insert(field.into());
                report
                    .pending_history
                    .entry(country.into())
                    .or_default()
                    .push(source(parsed, parsed.field_location(&key)));
            } else if let Ok(body) = value.read_object() {
                Self::pending_history(parsed, body, country, seed, report, depth + 1)?;
            }
        }
        Ok(())
    }

    pub(super) fn bind(
        self,
        initial: &InitialWorld,
        attributes: &AttributeDraft,
        date: &augustus_core::WorldDate,
        report: &mut GovernmentReport,
    ) -> Governments {
        let mut result = Governments {
            reform_levels: self
                .forms
                .iter()
                .map(|(kind, form)| (kind.clone(), form.levels.clone()))
                .collect(),
            reforms: self
                .reforms
                .iter()
                .map(|(name, reform)| (name.clone(), reform.effect.clone()))
                .collect(),
            ..Default::default()
        };
        for (name, reform) in &self.reforms {
            if !reform.flags.contains_key("native_mechanic") {
                continue;
            }
            result
                .reforms
                .get_mut(name)
                .expect("copied reform")
                .features
                .insert(
                    "eu4/native_mechanic".into(),
                    (!reform.uncertain_flags.contains("native_mechanic"))
                        .then_some(reform.flags["native_mechanic"]),
                );
        }
        result.metrics.insert(
            "legitimacy".into(),
            GovernmentMetric {
                minimum_milli: 0,
                maximum_milli: 100_000,
                baseline_milli: 50_000,
                divisor_milli: 100_000,
                modifier: attributes.static_modifier("legitimacy").cloned(),
                unresolved_properties: attributes.static_unresolved_properties("legitimacy"),
            },
        );
        // Initial values and scaling of these two metrics are separate from
        // transition initialization. Their attribute effects remain unresolved.
        for name in ["republican_tradition", "devotion"] {
            result.metrics.insert(
                name.into(),
                GovernmentMetric {
                    minimum_milli: 0,
                    maximum_milli: 100_000,
                    baseline_milli: 0,
                    divisor_milli: 100_000,
                    modifier: None,
                    unresolved_properties: BTreeSet::new(),
                },
            );
        }
        for (kind, form) in &self.forms {
            let profile = form
                .base
                .as_ref()
                .and_then(|base| self.reforms.get(base).map(|reform| (base, reform)))
                .filter(|(_, reform)| reform.uncertain_flags.is_empty())
                .and_then(|(base, reform)| {
                    let yes = |name: &str| reform.flags.get(name) == Some(&true);
                    if yes("nomad") || yes("has_meritocracy") {
                        return None;
                    }
                    let metric = match kind.as_str() {
                        "monarchy"
                            if yes("monarchy")
                                && !yes("republic")
                                && !yes("has_devotion")
                                && !yes("native_mechanic") =>
                        {
                            Some("legitimacy")
                        }
                        "tribal"
                            if yes("tribal")
                                && yes("monarchy")
                                && !yes("republic")
                                && !yes("has_devotion")
                                && !yes("native_mechanic") =>
                        {
                            Some("legitimacy")
                        }
                        "republic"
                            if yes("republic")
                                && !yes("monarchy")
                                && !yes("has_devotion")
                                && !yes("native_mechanic") =>
                        {
                            Some("republican_tradition")
                        }
                        "theocracy"
                            if yes("religion")
                                && yes("has_devotion")
                                && !yes("monarchy")
                                && !yes("republic")
                                && !yes("native_mechanic") =>
                        {
                            Some("devotion")
                        }
                        "native"
                            if yes("native_mechanic")
                                && !yes("monarchy")
                                && !yes("republic")
                                && !yes("has_devotion") =>
                        {
                            None
                        }
                        _ => return None,
                    };
                    Some(GovernmentForm {
                        reforms: [base.clone()].into(),
                        metrics: metric
                            .map(|name| (name.into(), Some(50_000)))
                            .into_iter()
                            .collect(),
                        retain_higher_metrics: if metric == Some("republican_tradition") {
                            ["republican_tradition".into()].into()
                        } else {
                            BTreeSet::new()
                        },
                        inactive_metrics: if metric == Some("legitimacy") {
                            BTreeMap::new()
                        } else {
                            [("legitimacy".into(), Some(100_000))].into()
                        },
                    })
                });
            if let Some(profile) = profile {
                result.forms.insert(kind.clone(), profile);
            } else {
                report.unavailable_transitions.insert(
                    kind.clone(),
                    "unverified or ambiguous basic reform classification".into(),
                );
            }
        }
        report.transition_forms = result.forms.len();
        for country in initial.countries() {
            let entity = initial
                .country_entity(&country.id)
                .expect("registered country");
            let mut state = GovernmentState::default();
            if let Some(seed) = self.countries.get(&country.id) {
                state.kind = (!seed.uncertain.contains("kind"))
                    .then(|| seed.kind.clone())
                    .flatten();
                if !seed.uncertain.contains("reforms")
                    && let Some(form) = state.kind.as_ref().and_then(|k| self.forms.get(k))
                    && let Some(base) = &form.base
                {
                    let mut selected = BTreeSet::new();
                    let mut known = true;
                    for (add, name) in &seed.reforms {
                        if *add {
                            match form.levels.get(name) {
                                Some(Some(level)) => selected.retain(|old| {
                                    form.levels.get(old).and_then(Option::as_ref) != Some(level)
                                }),
                                Some(None) => known = false,
                                None => (),
                            }
                            selected.insert(name.clone());
                        } else {
                            selected.remove(name);
                        }
                    }
                    selected.insert(base.clone());
                    state.reforms = known.then_some(selected);
                }
                if let Some(selected) = &state.reforms {
                    let mut flags = BTreeMap::new();
                    let mut uncertain = false;
                    // The basic reform is applied first, then selected reforms.
                    let base = state
                        .kind
                        .as_ref()
                        .and_then(|k| self.forms.get(k))
                        .and_then(|form| form.base.as_ref());
                    let ordered = base
                        .into_iter()
                        .chain(selected.iter().filter(|r| Some(*r) != base));
                    for name in ordered {
                        if let Some(reform) = self.reforms.get(name) {
                            uncertain |= !reform.uncertain_flags.is_empty();
                            for (name, &value) in &reform.flags {
                                if flags.get(name).is_some_and(|v| *v != value) {
                                    uncertain = true;
                                }
                                flags.insert(name.clone(), value);
                            }
                        } else {
                            uncertain = true;
                        }
                    }
                    if !uncertain {
                        let mut metrics = BTreeMap::new();
                        if flags.get("monarchy") == Some(&true)
                            && !["republic", "nomad", "has_devotion", "has_meritocracy"]
                                .iter()
                                .any(|f| flags.get(*f) == Some(&true))
                        {
                            let value = if seed.uncertain.contains("legitimacy") {
                                None
                            } else if let Some(value) = seed.legitimacy {
                                Some(value)
                            } else if (date.year, date.month, date.day) == (1444, 11, 11) {
                                report.default_legitimacy_countries.push(country.id.clone());
                                Some(100_000)
                            } else {
                                None
                            };
                            if value.is_some() {
                                report.countries_with_legitimacy += 1;
                            }
                            metrics.insert("legitimacy".into(), value);
                        } else if flags.get("republic") == Some(&true) {
                            metrics.insert("republican_tradition".into(), None);
                        } else if flags.get("has_devotion") == Some(&true) {
                            metrics.insert("devotion".into(), None);
                        }
                        state.metrics = Some(metrics);
                    }
                }
            }
            if state.reforms.is_some() {
                report.countries_with_selected_reforms += 1;
            } else {
                report.unknown_selection_countries.push(country.id.clone());
            }
            if state.metrics.is_none()
                || state
                    .metrics
                    .as_ref()
                    .is_some_and(|m| m.values().any(Option::is_none))
            {
                report.unknown_metric_countries.push(country.id.clone());
            }
            result.countries.insert(entity, state);
        }
        result
    }
}
