// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use super::{Condition, ENCODING, EstateReport, History, Reader};
use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::{block, micros};
use crate::{ContentRoot, Error, FileEntry, LoadedSource, ParsedText, SourceLocation};
use augustus_core::{EstateLandRules, EstatePrivilege, EstateState, InitialWorld};
use std::collections::{BTreeMap, BTreeSet};

struct Weight {
    base: Option<i64>,
    conditions: Vec<(Condition, i64)>,
}

pub(super) struct HistoryMetric {
    estate: String,
    amount: i64,
    land: bool,
    source: LoadedSource,
}

impl HistoryMetric {
    pub(super) fn parse(
        parsed: &ParsedText<'_>,
        name: &str,
        body: Reader<'_, '_>,
        at: SourceLocation,
    ) -> Result<Self, Error> {
        let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
        let land = name == "change_estate_land_share";
        let mut estate = None;
        let mut amount = None;
        for (key, op, value) in body.fields() {
            equal(op, &fail)?;
            match key.read_string().as_str() {
                "estate" if estate.is_none() => {
                    estate = Some(
                        value
                            .read_string()
                            .map_err(|_| fail("expected estate identifier"))?,
                    )
                }
                key if key == if land { "share" } else { "loyalty" } && amount.is_none() => {
                    let value = number(
                        parsed,
                        &value
                            .read_string()
                            .map_err(|_| fail("expected estate history amount"))?,
                        at,
                    )?;
                    let divisor = if land { 100 } else { 1000 };
                    if value % divisor != 0 {
                        return Err(fail("estate history amount exceeds supported precision"));
                    }
                    amount = Some(value / divisor);
                }
                _ => return Err(fail("unknown or duplicate estate history field")),
            }
        }
        Ok(Self {
            estate: estate
                .filter(|v| !v.is_empty())
                .ok_or_else(|| fail("missing estate identifier"))?,
            amount: amount.ok_or_else(|| fail("missing estate history amount"))?,
            land,
            source: source(parsed, at),
        })
    }

    fn pending(&self, tag: &str, report: &mut EstateReport) {
        let pending = if self.land {
            &mut report.pending_land_changes
        } else {
            &mut report.pending_loyalty_changes
        };
        pending
            .entry(tag.into())
            .or_default()
            .push(self.source.clone());
    }
}

#[derive(Default)]
pub(super) struct EstateInitialization {
    order: Vec<String>,
    weights: BTreeMap<String, Weight>,
    land_rules: Option<EstateLandRules>,
    startup_empty: bool,
    privilege_land: BTreeSet<String>,
}

impl EstateInitialization {
    pub(super) fn definition(
        &mut self,
        parsed: &ParsedText<'_>,
        body: Reader<'_, '_>,
        id: &str,
        library: &BTreeMap<String, Condition>,
        report: &mut EstateReport,
    ) -> Result<(), Error> {
        let mut weight = Weight {
            base: None,
            conditions: Vec::new(),
        };
        for (key, op, value) in body.fields() {
            let name = key.read_string();
            if !matches!(name.as_str(), "base_influence" | "influence_modifier") {
                continue;
            }
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            equal(op, &fail)?;
            if name == "base_influence" {
                if weight.base.is_some() {
                    return Err(fail("duplicate estate base influence"));
                }
                let value = number(
                    parsed,
                    &value
                        .read_string()
                        .map_err(|_| fail("expected estate base influence"))?,
                    at,
                )?;
                if value % 1000 != 0 {
                    return Err(fail("estate influence exceeds supported precision"));
                }
                weight.base = Some(value / 1000);
            } else {
                let mut condition = None;
                let mut amount = None;
                for (key, op, value) in block(&value, &fail)?.fields() {
                    equal(op, &fail)?;
                    match key.read_string().as_str() {
                        "trigger" if condition.is_none() => {
                            condition = Some(Condition::All(Condition::compile(
                                parsed,
                                block(&value, &fail)?,
                                report,
                                0,
                                true,
                                Some(library),
                                false,
                            )?))
                        }
                        "influence" if amount.is_none() => {
                            let value = number(
                                parsed,
                                &value
                                    .read_string()
                                    .map_err(|_| fail("expected conditional influence"))?,
                                at,
                            )?;
                            if value % 1000 != 0 {
                                return Err(fail("estate influence exceeds supported precision"));
                            }
                            amount = Some(value / 1000);
                        }
                        "desc" => (),
                        _ => return Err(fail("unknown or duplicate conditional influence field")),
                    }
                }
                weight.conditions.push((
                    condition.ok_or_else(|| fail("missing influence condition"))?,
                    amount.ok_or_else(|| fail("missing conditional influence"))?,
                ));
            }
            report.definition_sources.insert(
                format!("estate:{id}:{name}:{}", weight.conditions.len()),
                source(parsed, at),
            );
        }
        self.order.push(id.into());
        self.weights.insert(id.into(), weight);
        Ok(())
    }

    pub(super) fn load_rules(
        &mut self,
        root: &ContentRoot,
        files: &[FileEntry],
        report: &mut EstateReport,
    ) -> Result<(), Error> {
        let mut values = BTreeMap::new();
        for key in [
            "ESTATE_START_CROWNLAND_INFLUENCE",
            "ESTATE_MIN_DISTRIBUTED_CROWNLAND",
        ] {
            if let Some((value, at)) =
                crate::defines::optional_literal(root, files, "NCountry", key)?
            {
                values.insert(key, value);
                report
                    .definition_sources
                    .insert(format!("define:{key}"), at);
            }
        }
        if let (Some(&influence), Some(&reserve)) = (
            values.get("ESTATE_START_CROWNLAND_INFLUENCE"),
            values.get("ESTATE_MIN_DISTRIBUTED_CROWNLAND"),
        ) {
            if influence < 0
                || influence % 1000 != 0
                || !(0..=100_000_000).contains(&reserve)
                || reserve % 100 != 0
            {
                return Err(Error::message(
                    root.path(),
                    "invalid estate land distribution defines",
                ));
            }
            self.land_rules = Some(EstateLandRules {
                crown_influence_milli: influence / 1000,
                minimum_crown_share_micros: reserve / 100,
                initial_share_quantum_micros: 1000,
                normalized_share_quantum_micros: 10,
            });
        }
        self.startup_empty = true;
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/on_actions"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed
                .reader()
                .fields()
                .filter(|(k, _, _)| k.read_string() == "on_startup")
            {
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                if block(&value, &fail)?.fields().next().is_some() {
                    self.startup_empty = false;
                    report.definition_sources.insert(
                        format!("startup:{}", report.definition_sources.len()),
                        source(&parsed, at),
                    );
                }
            }
        }
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/estate_privileges"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, _, value) in parsed.reader().fields() {
                let id = key.read_string();
                let fail = |m: &str| Error::message(parsed.source(), m);
                for (key, _, value) in block(&value, &fail)?
                    .fields()
                    .filter(|(k, _, _)| k.read_string() == "land_share")
                {
                    if value.read_string().ok().and_then(|v| micros(&v)) != Some(0) {
                        self.privilege_land.insert(id.clone());
                        report.definition_sources.insert(
                            format!("privilege:{id}:land_share"),
                            source(&parsed, parsed.field_location(&key)),
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn bind(
        &self,
        initial: &InitialWorld,
        tag: &str,
        history: Option<&History>,
        privileges: &BTreeMap<String, EstatePrivilege>,
        states: &mut BTreeMap<String, EstateState>,
        lookup: &dyn Fn(&str) -> Option<bool>,
        report: &mut EstateReport,
    ) -> Result<(), Error> {
        let entity = initial.country_entity(tag).expect("registered country");
        let mut reasons = BTreeSet::new();
        let active: Vec<_> = self
            .order
            .iter()
            .filter(|id| states[*id].active == Some(true))
            .collect();
        let mut weights = Vec::new();
        for id in &active {
            let weight = &self.weights[*id];
            let mut value = weight.base;
            if value.is_none() {
                reasons.insert(format!("{id}:missing_base_influence"));
            }
            for (condition, amount) in &weight.conditions {
                let mut pending = BTreeSet::new();
                match condition.evaluate(
                    initial,
                    tag,
                    history.filter(|h| !h.unknown_flags).map(|h| &h.flags),
                    &mut pending,
                    None,
                    Some(lookup),
                ) {
                    Some(false) => (),
                    Some(true) => value = value.and_then(|v| v.checked_add(*amount)),
                    None => {
                        value = None;
                        reasons.extend(pending.into_iter().map(|r| format!("{id}:influence:{r}")));
                    }
                }
            }
            let short = id.strip_prefix("estate_").unwrap_or(id);
            for property in [
                format!("{short}_influence_modifier"),
                format!("{short}_exclusive_influence_modifier"),
                "all_estate_influence_modifier".into(),
            ] {
                let effect = initial
                    .world()
                    .governments()
                    .property_effects(entity, &property)
                    .map_err(|e| Error::caused_by("common/estates", e))?;
                if !effect.unresolved.is_empty() {
                    value = None;
                    reasons.extend(
                        effect
                            .unresolved
                            .into_iter()
                            .map(|r| format!("{id}:influence:{r}")),
                    );
                }
                let attached = initial
                    .world()
                    .modifiers()
                    .declared_totals(entity)
                    .map_err(|e| Error::caused_by("common/estates", e))?;
                let extra = effect
                    .known_totals_micros
                    .get(&property)
                    .copied()
                    .unwrap_or(0)
                    .checked_add(attached.get(&property).copied().unwrap_or(0))
                    .ok_or_else(|| Error::message("common/estates", "estate influence overflow"))?;
                if extra % 10 != 0 {
                    value = None;
                    reasons.insert(format!("{id}:influence_precision:{property}"));
                }
                value = value.and_then(|v| v.checked_add(extra / 10));
            }
            if value.is_some_and(|v| v < 0) {
                value = None;
                reasons.insert(format!("{id}:unsupported_negative_initial_influence"));
            }
            if value.is_none() {
                reasons.insert(format!("{id}:unknown_initial_influence"));
            }
            weights.push(value);
        }
        let uncertain_presence = states.values().any(|s| s.active.is_none());
        if uncertain_presence {
            reasons.insert("unknown_estate_presence".into());
        }
        let selected_unknown = active.iter().any(|id| states[*id].privileges.is_none())
            || history.is_some_and(|h| {
                h.unknown_privileges
                    || h.privileges.iter().any(|p| {
                        states
                            .values()
                            .filter(|s| s.privileges.as_ref().is_some_and(|ids| ids.contains(p)))
                            .count()
                            != 1
                    })
            });
        let mut affected_privileges = active
            .iter()
            .filter_map(|id| states[*id].privileges.as_ref())
            .flatten()
            .chain(history.into_iter().flat_map(|h| &h.privilege_operations));
        let unexecuted_privilege = affected_privileges.any(|id| {
            self.privilege_land.contains(id)
                || privileges
                    .get(id)
                    .is_none_or(|p| !p.effect.unresolved_effects.is_empty())
        });
        if selected_unknown || unexecuted_privilege {
            reasons.insert("unresolved_privilege_initialization".into());
        }
        if self.land_rules.is_none() {
            reasons.insert("missing_land_distribution_defines".into());
        }
        let land_known = self.startup_empty
            && !uncertain_presence
            && !selected_unknown
            && !unexecuted_privilege
            && history.is_some_and(|h| !h.unknown_land)
            && weights.iter().all(Option::is_some);
        if land_known && let Some(rules) = &self.land_rules {
            let shares = rules
                .distribute(&weights.into_iter().map(Option::unwrap).collect::<Vec<_>>())
                .map_err(|e| Error::caused_by("common/estates", e))?;
            for (id, share) in active.iter().zip(shares) {
                states.get_mut(*id).unwrap().land_share_micros = Some(share);
            }
        }
        let loyalty_known = self.startup_empty
            && !selected_unknown
            && !unexecuted_privilege
            && history.is_some_and(|h| !h.unknown_loyalty);
        if !self.startup_empty {
            reasons.insert("unexecuted_startup_estate_effects".into());
        }
        if loyalty_known {
            for id in &active {
                states.get_mut(*id).unwrap().loyalty_milli = Some(50_000);
            }
        }
        if let Some(history) = history {
            for metric in &history.metrics {
                let Some(state) = states
                    .get_mut(&metric.estate)
                    .filter(|s| s.active == Some(true))
                else {
                    reasons.insert(format!("unresolved_history_estate:{}", metric.estate));
                    metric.pending(tag, report);
                    for state in states.values_mut() {
                        if metric.land {
                            state.land_share_micros = None;
                        } else {
                            state.loyalty_milli = None;
                        }
                    }
                    continue;
                };
                let slot = if metric.land {
                    &mut state.land_share_micros
                } else {
                    &mut state.loyalty_milli
                };
                *slot = slot.and_then(|v| v.checked_add(metric.amount));
                if !metric.land {
                    *slot = slot.map(|v| v.max(0));
                }
                let limit = if metric.land { 1_000_000 } else { 100_000 };
                let unresolved = slot.is_none();
                let invalid = slot.is_some_and(|v| !(0..=limit).contains(&v))
                    || (metric.land
                        && states
                            .values()
                            .filter_map(|s| s.land_share_micros)
                            .sum::<i64>()
                            > 1_000_000);
                if invalid {
                    reasons.insert(format!(
                        "unsupported_history_metric_boundary:{}",
                        metric.estate
                    ));
                    for state in states.values_mut() {
                        if metric.land {
                            state.land_share_micros = None;
                        } else {
                            state.loyalty_milli = None;
                        }
                    }
                }
                if unresolved || invalid {
                    metric.pending(tag, report);
                }
            }
            if history.unknown_land {
                reasons.insert("scoped_land_history".into());
            }
            if history.unknown_loyalty {
                reasons.insert("scoped_loyalty_history".into());
            }
        }
        if !active.is_empty()
            && active
                .iter()
                .all(|id| states[*id].land_share_micros.is_some())
        {
            report.initial_land_countries += 1;
        }
        if !active.is_empty() && active.iter().all(|id| states[*id].loyalty_milli.is_some()) {
            report.initial_loyalty_countries += 1;
        }
        if !reasons.is_empty() {
            report.unresolved_initialization.insert(tag.into(), reasons);
        }
        Ok(())
    }
}

fn number(parsed: &ParsedText<'_>, text: &str, at: SourceLocation) -> Result<i64, Error> {
    micros(text).ok_or_else(|| {
        Error::message(parsed.source(), "expected numeric estate value").with_location(at)
    })
}
