// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
};

use augustus_core::{
    EstateConditionalModifier, EstateConditionalScale, EstateDefinition, EstateEffect,
    EstateInfluenceBand, EstateLoyaltyDecay, EstatePrivilege, EstateRules, EstateState, Estates,
    InitialWorld, VanillaEstateInfluenceScaleInput, calculate_vanilla_estate_influence_scale,
};
use jomini::text::ObjectReader;
use serde::Serialize;

use crate::bootstrap::{
    VanillaExternalEstatePrivileges, VanillaExternalEstateStates, dated_name, direct_txt, equal,
    source,
};
use crate::country_conditions::Condition;
use crate::setup::{PropertyDiagnostics, block, micros, properties};
use crate::{
    ContentView, Error, FileEntry, LoadedSource, ParsedText, SourceLocation, TextEncoding,
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
    pub unsupported_conditional_triggers: BTreeMap<String, usize>,
    pub ambiguous_properties: BTreeMap<String, Vec<LoadedSource>>,
    pub unresolved_countries: BTreeMap<String, BTreeSet<String>>,
    pub pending_land_changes: BTreeMap<String, Vec<LoadedSource>>,
    pub pending_loyalty_changes: BTreeMap<String, Vec<LoadedSource>>,
    pub applied_startup_effects: usize,
    pub startup_effect_sources: Vec<LoadedSource>,
    pub startup_script_ignored_definitions: BTreeMap<String, Vec<LoadedSource>>,
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
    script_library: crate::ScriptLibrary,
    conditional_modifiers: BTreeMap<String, Vec<ConditionalDraft>>,
}

struct ConditionalDraft {
    trigger: Result<crate::script::ConditionSource, Error>,
    source: LoadedSource,
    scale: EstateConditionalScale,
    modifier: EstateEffect,
    unresolved_fields: bool,
}

impl EstateDraft {
    pub(super) fn load(
        root: &impl ContentView,
        files: &[FileEntry],
        defines: &crate::Defines,
        report: &mut EstateReport,
    ) -> Result<Self, Error> {
        let mut draft = Self {
            content: Estates::default(),
            conditions: BTreeMap::new(),
            histories: BTreeMap::new(),
            initialization: EstateInitialization::default(),
            script_library: crate::ScriptLibrary::new(),
            conditional_modifiers: BTreeMap::new(),
        };
        if !files
            .iter()
            .any(|f| direct_txt(&f.relative_path, "common/estates"))
        {
            return Ok(draft);
        }
        let library = library(root, files, report)?;
        draft.script_library = crate::ScriptLibrary::from_root(root, files, ENCODING)?;
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
                        "influence" => {
                            if !seen.insert(name.clone()) {
                                return Err(fail("duplicate privilege influence"));
                            }
                            let value = micros(
                                &value
                                    .read_string()
                                    .map_err(|_| fail("expected privilege influence"))?,
                            )
                            .ok_or_else(|| fail("expected numeric privilege influence"))?;
                            if value % 10 != 0 {
                                return Err(fail(
                                    "privilege influence exceeds supported precision",
                                ));
                            }
                            privilege.influence_milli = Some(value / 10);
                            report
                                .definition_sources
                                .insert(format!("privilege:{id}:influence"), source(&parsed, at));
                        }
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
                            let scale = match name.as_str() {
                                "conditional_modifier" => EstateConditionalScale::Fixed,
                                "loyalty_scaled_conditional_modifier" => {
                                    EstateConditionalScale::Loyalty
                                }
                                _ => EstateConditionalScale::Influence,
                            };
                            let mut condition = None;
                            let mut modifier = None;
                            let mut unresolved_condition = false;
                            for (conditional_key, conditional_op, conditional_value) in
                                body.fields()
                            {
                                let conditional_name = conditional_key.read_string();
                                let conditional_at = parsed.field_location(&conditional_key);
                                let conditional_fail = |message: &str| {
                                    Error::message(parsed.source(), message)
                                        .with_location(conditional_at)
                                };
                                equal(conditional_op, &conditional_fail)?;
                                match conditional_name.as_str() {
                                    "trigger" if condition.is_none() => {
                                        let trigger = block(&conditional_value, &conditional_fail)?;
                                        condition = Some((
                                            crate::script::ConditionSource::parse(
                                                &parsed,
                                                "AND",
                                                trigger,
                                                conditional_at,
                                            ),
                                            source(&parsed, conditional_at),
                                        ));
                                    }
                                    "trigger" => {
                                        return Err(conditional_fail(
                                            "duplicate conditional modifier trigger",
                                        ));
                                    }
                                    "modifier" if modifier.is_none() => {
                                        modifier = Some(effect(
                                            &parsed,
                                            block(&conditional_value, &conditional_fail)?,
                                            &format!("{id}:{name}"),
                                            report,
                                        )?);
                                    }
                                    "modifier" => {
                                        return Err(conditional_fail(
                                            "duplicate conditional modifier payload",
                                        ));
                                    }
                                    _ => {
                                        unresolved_condition = true;
                                        report.unsupported(
                                            &parsed,
                                            "privilege",
                                            "conditional_modifier_field",
                                            conditional_at,
                                        );
                                    }
                                }
                            }
                            if condition.is_none() && modifier.is_none() && !unresolved_condition {
                                continue;
                            }
                            let modifier = modifier.ok_or_else(|| {
                                fail("conditional modifier is missing its modifier block")
                            })?;
                            let (trigger, source) = condition.ok_or_else(|| {
                                fail("conditional modifier is missing its trigger block")
                            })?;
                            draft
                                .conditional_modifiers
                                .entry(id.clone())
                                .or_default()
                                .push(ConditionalDraft {
                                    trigger,
                                    source,
                                    scale,
                                    modifier,
                                    unresolved_fields: unresolved_condition,
                                });
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
            if let Some((value, at)) = defines.optional("NCountry", name)? {
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
        let influence_level_1 = defines.optional("NCountry", "ESTATE_INFLUENCE_LEVEL_1")?;
        let influence_level_2 = defines.optional("NCountry", "ESTATE_INFLUENCE_LEVEL_2")?;
        let influence_level_3 = defines.optional("NCountry", "ESTATE_INFLUENCE_LEVEL_3")?;
        match (influence_level_1, influence_level_2, influence_level_3) {
            (Some((level_1, at_1)), Some((level_2, at_2)), Some((level_3, at_3))) => {
                let levels = [
                    ("ESTATE_INFLUENCE_LEVEL_1", level_1, &at_1),
                    ("ESTATE_INFLUENCE_LEVEL_2", level_2, &at_2),
                    ("ESTATE_INFLUENCE_LEVEL_3", level_3, &at_3),
                ];
                for (name, level, at) in &levels {
                    if level % 1000 != 0 {
                        return Err(Error::message(
                            &at.path,
                            format!("{name} exceeds supported precision"),
                        ));
                    }
                    if !(0..=100_000_000).contains(level) {
                        return Err(Error::message(
                            &at.path,
                            format!("{name} is outside the supported range"),
                        ));
                    }
                    report
                        .definition_sources
                        .insert(format!("define:{name}"), (*at).clone());
                }
                if level_1 >= level_2 {
                    return Err(Error::message(
                        &at_2.path,
                        "estate influence levels must be strictly increasing",
                    ));
                }
                if level_2 >= level_3 {
                    return Err(Error::message(
                        &at_3.path,
                        "estate influence levels must be strictly increasing",
                    ));
                }
                let thresholds_milli = [level_1 / 1000, level_2 / 1000, level_3 / 1000];
                let mut band_starts_milli = vec![0, 1];
                band_starts_milli.extend(thresholds_milli);
                band_starts_milli.sort_unstable();
                band_starts_milli.dedup();
                rules.influence_bands = Some(
                    band_starts_milli
                        .into_iter()
                        .map(|minimum_milli| {
                            let scale_milli = calculate_vanilla_estate_influence_scale(
                                VanillaEstateInfluenceScaleInput {
                                    raw_influence_milli: minimum_milli as i32,
                                    additional_value_milli: 0,
                                    first_threshold_milli: thresholds_milli[0] as i32,
                                    second_threshold_milli: thresholds_milli[1] as i32,
                                    third_threshold_milli: thresholds_milli[2] as i32,
                                },
                            )
                            .scale_milli;
                            EstateInfluenceBand {
                                minimum_milli,
                                scale_micros: i64::from(scale_milli) * 1_000,
                            }
                        })
                        .collect(),
                );
            }
            (None, None, None) => {}
            (level_1, level_2, level_3) => {
                let at = level_1
                    .or(level_2)
                    .or(level_3)
                    .map(|(_, at)| at)
                    .expect("partial influence levels have a source");
                return Err(Error::message(
                    &at.path,
                    "estate influence levels must be provided together",
                ));
            }
        }
        if let Some((base, at)) = defines.optional("NCountry", "ESTATE_DEFAULT_LOYALTY")? {
            if base % 1000 != 0 {
                return Err(Error::message(
                    &at.path,
                    "estate loyalty target base define exceeds supported precision",
                ));
            }
            let base_milli = base / 1000;
            if base_milli > i64::from(i32::MAX) {
                return Err(Error::message(
                    &at.path,
                    "estate loyalty target base define exceeds supported range",
                ));
            }
            rules.loyalty_target_base_milli = Some(base_milli);
            report
                .definition_sources
                .insert("define:ESTATE_DEFAULT_LOYALTY".into(), at);
        }
        let decay_min = defines.optional("NCountry", "ESTATE_LOYALTY_DECAY_BASE_MIN")?;
        let decay_max = defines.optional("NCountry", "ESTATE_LOYALTY_DECAY_BASE_MAX")?;
        match (decay_min, decay_max) {
            (Some((minimum, minimum_at)), Some((maximum, maximum_at))) => {
                if minimum % 1000 != 0 {
                    return Err(Error::message(
                        &minimum_at.path,
                        "estate loyalty decay define exceeds supported precision",
                    ));
                }
                if maximum % 1000 != 0 {
                    return Err(Error::message(
                        &maximum_at.path,
                        "estate loyalty decay define exceeds supported precision",
                    ));
                }
                let base_min_annual_milli = minimum / 1000;
                let base_max_annual_milli = maximum / 1000;
                if base_min_annual_milli < 0
                    || base_max_annual_milli < base_min_annual_milli
                    || base_max_annual_milli > i64::from(i32::MAX)
                {
                    return Err(Error::message(
                        &maximum_at.path,
                        "invalid estate loyalty decay defines",
                    ));
                }
                rules.loyalty_decay = Some(EstateLoyaltyDecay {
                    base_min_annual_milli,
                    base_max_annual_milli,
                });
                report
                    .definition_sources
                    .insert("define:ESTATE_LOYALTY_DECAY_BASE_MIN".into(), minimum_at);
                report
                    .definition_sources
                    .insert("define:ESTATE_LOYALTY_DECAY_BASE_MAX".into(), maximum_at);
            }
            (None, None) => {}
            (Some((_, at)), None) | (None, Some((_, at))) => {
                return Err(Error::message(
                    &at.path,
                    "estate loyalty decay defines must be provided together",
                ));
            }
        }
        draft
            .initialization
            .load_rules(root, files, defines, report)?;
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
    ) -> Result<BTreeSet<String>, Error> {
        let mut handled = BTreeSet::new();
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
                    handled.insert(name);
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
                    handled.insert(name);
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
        Ok(handled)
    }

    pub(super) fn history_privilege_ids(&self) -> BTreeSet<String> {
        self.histories
            .values()
            .flat_map(|history| history.privileges.iter().cloned())
            .collect()
    }

    pub(super) fn bind(
        mut self,
        initial: &InitialWorld,
        external_privileges: Option<&VanillaExternalEstatePrivileges>,
        external_states: Option<&VanillaExternalEstateStates>,
        report: &mut EstateReport,
    ) -> Result<Estates, Error> {
        if self.content.definitions.is_empty() {
            return Ok(self.content);
        }
        for (id, drafts) in std::mem::take(&mut self.conditional_modifiers) {
            let effect = &mut self
                .content
                .privileges
                .get_mut(&id)
                .expect("parsed privilege")
                .effect;
            for draft in drafts {
                let condition = draft.trigger.and_then(|source| {
                    crate::ScriptCompiler::for_world(initial.world(), &self.script_library, false)
                        .compile_condition_source(&source, augustus_core::EntityKind::Country)
                });
                let condition = match condition {
                    Ok(condition) => Some(condition),
                    Err(error) => {
                        *report
                            .unsupported_conditional_triggers
                            .entry(error.detail().to_owned())
                            .or_default() += 1;
                        let key = "privilege:conditional_modifier_trigger".to_owned();
                        *report.unsupported_fields.entry(key.clone()).or_default() += 1;
                        report
                            .unsupported_examples
                            .entry(key)
                            .or_insert(draft.source);
                        None
                    }
                };
                if !draft.unresolved_fields
                    && let Some(condition) = condition
                {
                    effect
                        .conditional_modifiers
                        .push(EstateConditionalModifier {
                            condition,
                            scale: draft.scale,
                            modifier: draft.modifier.modifier,
                            unresolved_properties: draft.modifier.unresolved_properties,
                        });
                } else {
                    let properties: BTreeSet<_> = draft
                        .modifier
                        .modifier
                        .values_micros
                        .keys()
                        .chain(draft.modifier.modifier.flags.keys())
                        .chain(draft.modifier.unresolved_properties.iter())
                        .cloned()
                        .collect();
                    if properties.is_empty() {
                        effect
                            .unresolved_effects
                            .insert("conditional_modifier_trigger".into());
                    } else {
                        effect.unresolved_properties.extend(properties);
                    }
                }
            }
            for name in &effect.unresolved_properties {
                effect.modifier.values_micros.remove(name);
                effect.modifier.flags.remove(name);
            }
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
            let country_external_values =
                external_privileges.and_then(|values| values.get(&country.id));
            if let Some(values) = country_external_values {
                for (estate, selected) in values {
                    if let Some(selected) = selected
                        && let Some(state) = states.get_mut(estate)
                    {
                        state.privileges = Some(selected.clone());
                    }
                }
            }
            let country_external_privileges: BTreeSet<_> = country_external_values
                .into_iter()
                .flat_map(|values| values.values())
                .filter_map(|selected| selected.as_ref())
                .flatten()
                .cloned()
                .collect();
            self.initialization.bind(
                initial,
                &country.id,
                history,
                &self.content.privileges,
                &mut states,
                &lookup,
                &country_external_privileges,
                external_states,
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
    target
        .conditional_modifiers
        .extend(source.conditional_modifiers);
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
    root: &impl ContentView,
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
                    | "is_or_was_tag"
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
