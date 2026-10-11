// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    AttributeBase, AttributeChannel, AttributeFormula, AttributeTerm, Attributes, InitialWorld,
    ProvinceClass, Target,
};
use jomini::Encoding;
use serde::Serialize;

use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::{PropertyDiagnostics, block, micros, properties};
use crate::{
    ContentRoot, Error, FileEntry, LoadedSource, ParsedText, SetupReport, SourceLocation,
    TextEncoding,
};

const ENCODING: TextEncoding = TextEncoding::Windows1252;
pub(super) const ATTRIBUTE_NAMES: [&str; 2] = ["tax_basis", "manpower_contribution"];

#[derive(Debug, Default, Serialize)]
pub struct AttributeReport {
    pub static_definitions: usize,
    pub building_definitions: usize,
    pub trade_good_definitions: usize,
    pub province_calculations: usize,
    pub manpower_per_base_micros: i64,
    pub definition_sources: BTreeMap<String, LoadedSource>,
    pub unapplied_definition_fields: BTreeMap<String, usize>,
    pub unapplied_examples: BTreeMap<String, LoadedSource>,
    pub unmapped_properties: BTreeMap<String, usize>,
    pub ambiguous_properties: BTreeMap<String, Vec<LoadedSource>>,
    pub ambiguous_definitions: BTreeMap<String, Vec<LoadedSource>>,
}

impl PropertyDiagnostics for AttributeReport {
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

pub(super) struct AttributeDraft {
    attributes: Attributes,
    manpower_per_base: i64,
}

/// Reads only the selected literal in the base file's explicit NMilitary block.
/// Expressions and assignments in additional define files require Lua support.
fn manpower_define(
    root: &ContentRoot,
    files: &[FileEntry],
    report: &mut AttributeReport,
) -> Result<i64, Error> {
    const KEY: &str = "BASE_MP_TO_MANPOWER";
    let doc = root.read_text("common/defines.lua", ENCODING)?;
    let text = ENCODING.decode(doc.bytes());
    let mut military = false;
    let mut section_count = 0;
    let mut amount = None;
    for (index, line) in text.lines().enumerate() {
        let clean = line.split_once("--").map_or(line, |(code, _)| code).trim();
        if clean == "NMilitary = {" {
            military = true;
            section_count += 1;
        } else if military && clean == "}," {
            military = false;
        }
        if !clean.contains(KEY) {
            continue;
        }
        let at = SourceLocation {
            line: index + 1,
            column: line.find(KEY).unwrap_or(0) + 1,
        };
        let fail = |m: &str| Error::message(doc.source(), m).with_location(at);
        if !military || section_count != 1 || amount.is_some() {
            return Err(fail(
                "manpower define must be a unique literal in NMilitary",
            ));
        }
        let (key, value) = clean
            .split_once('=')
            .ok_or_else(|| fail("expected literal manpower define"))?;
        if key.trim() != KEY {
            return Err(fail("unsupported manpower define assignment"));
        }
        let value = value
            .trim()
            .strip_suffix(',')
            .unwrap_or(value.trim())
            .trim();
        let value = micros(value)
            .filter(|v| *v > 0)
            .and_then(|v| v.checked_mul(1000))
            .ok_or_else(|| fail("expected positive literal manpower define"))?;
        amount = Some(value);
        report.definition_sources.insert(
            "define:manpower_per_base".into(),
            LoadedSource {
                path: doc.source().into(),
                line: at.line,
                column: at.column,
            },
        );
    }
    let amount =
        amount.ok_or_else(|| Error::message(doc.source(), "missing literal manpower define"))?;
    for file in files.iter().filter(|f| {
        f.relative_path
            .parent()
            .is_some_and(|p| p == std::path::Path::new("common/defines"))
            && f.relative_path.extension().is_some_and(|e| e == "lua")
    }) {
        let doc = root.read_text(&file.relative_path, ENCODING)?;
        let text = ENCODING.decode(doc.bytes());
        for (index, line) in text.lines().enumerate() {
            let code = line.split_once("--").map_or(line, |(code, _)| code);
            if code.contains(KEY) {
                return Err(Error::message(
                    doc.source(),
                    "manpower define override requires unsupported Lua evaluation",
                )
                .with_location(SourceLocation {
                    line: index + 1,
                    column: code.find(KEY).unwrap_or(0) + 1,
                }));
            }
        }
    }
    report.manpower_per_base_micros = amount;
    Ok(amount)
}

fn used_property(name: &str) -> bool {
    matches!(
        name,
        "local_tax_modifier"
            | "global_tax_modifier"
            | "tax_income"
            | "local_manpower_modifier"
            | "global_manpower_modifier"
    )
}

impl AttributeDraft {
    pub(super) fn load(
        root: &ContentRoot,
        files: &[FileEntry],
        report: &mut AttributeReport,
    ) -> Result<Self, Error> {
        let mut attributes = Attributes::default();
        for kind in ["static_modifiers", "buildings", "tradegoods"] {
            for file in files
                .iter()
                .filter(|f| direct_txt(&f.relative_path, &format!("common/{kind}")))
            {
                let doc = root.read_text(&file.relative_path, ENCODING)?;
                let parsed = doc.parse()?;
                for (key, op, value) in parsed.reader().fields() {
                    let name = key.read_string();
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    equal(op, &fail)?;
                    if name.is_empty() || name.starts_with('@') {
                        return Err(fail("unsupported attribute definition identifier"));
                    }
                    let reader = block(&value, &fail)?;
                    let definition = if kind == "static_modifiers" {
                        properties(&parsed, reader, kind, &name, report)?
                    } else {
                        let field = if kind == "buildings" {
                            "modifier"
                        } else {
                            "province"
                        };
                        let mut definition = None;
                        for (key, op, value) in
                            reader.fields().filter(|(k, _, _)| k.read_string() == field)
                        {
                            let at = parsed.field_location(&key);
                            let fail =
                                |m: &str| Error::message(parsed.source(), m).with_location(at);
                            equal(op, &fail)?;
                            if definition.is_some() {
                                return Err(fail("duplicate attribute effect block"));
                            }
                            definition = Some(properties(
                                &parsed,
                                block(&value, &fail)?,
                                kind,
                                &name,
                                report,
                            )?);
                            report
                                .definition_sources
                                .insert(format!("{kind}:{name}:{field}"), source(&parsed, at));
                        }
                        definition.unwrap_or_default()
                    };
                    for property in definition
                        .values_micros
                        .keys()
                        .chain(definition.flags.keys())
                        .filter(|p| !used_property(p))
                    {
                        *report
                            .unmapped_properties
                            .entry(format!("{kind}:{property}"))
                            .or_default() += 1;
                    }
                    let catalog = match kind {
                        "static_modifiers" => &mut attributes.static_modifiers,
                        "buildings" => &mut attributes.building_modifiers,
                        _ => &mut attributes.trade_good_modifiers,
                    };
                    let identifier = format!("{kind}:{name}");
                    if kind == "static_modifiers" {
                        if let Some(origins) = report.ambiguous_definitions.get_mut(&identifier) {
                            origins.push(source(&parsed, at));
                            continue;
                        }
                        if let Some(first) = report.definition_sources.remove(&identifier) {
                            catalog.remove(&name);
                            report
                                .ambiguous_definitions
                                .insert(identifier, vec![first, source(&parsed, at)]);
                            continue;
                        }
                    }
                    if catalog.insert(name.clone(), definition).is_some() {
                        return Err(fail("duplicate attribute definition"));
                    }
                    report
                        .definition_sources
                        .insert(identifier, source(&parsed, at));
                }
            }
        }
        report.static_definitions = attributes.static_modifiers.len();
        report.building_definitions = attributes.building_modifiers.len();
        report.trade_good_definitions = attributes.trade_good_modifiers.len();
        let manpower_per_base = manpower_define(root, files, report)?;
        Ok(Self {
            attributes,
            manpower_per_base,
        })
    }

    pub(super) fn bind(
        mut self,
        initial: &InitialWorld,
        report: &mut AttributeReport,
        setup: &SetupReport,
    ) -> Result<Attributes, Error> {
        for province in initial
            .provinces()
            .iter()
            .filter(|p| p.class == ProvinceClass::Land)
        {
            let entity = initial
                .province_entity(province.id)
                .expect("registered province");
            let mut formulas = BTreeMap::new();
            for (name, base, field, scale, local_rate, global_rate) in [
                (
                    "tax_basis",
                    province.tax_milli,
                    "province.tax_base",
                    1000,
                    "local_tax_modifier",
                    "global_tax_modifier",
                ),
                (
                    "manpower_contribution",
                    province.manpower_milli,
                    "province.manpower_base",
                    self.manpower_per_base / 1000,
                    "local_manpower_modifier",
                    "global_manpower_modifier",
                ),
            ] {
                let mut unresolved: BTreeSet<String> = [
                    "autonomy_and_state_rules",
                    "culture_and_religion",
                    "government_ideas_estates_and_other_effects",
                    "building_retention_conditions",
                    "original_runtime_comparison",
                ]
                .into_iter()
                .map(String::from)
                .collect();
                let mut channel = AttributeChannel {
                    flat: if name == "tax_basis" {
                        vec![
                            AttributeTerm::Buildings {
                                property: "tax_income".into(),
                            },
                            AttributeTerm::TradeGood {
                                property: "tax_income".into(),
                            },
                            AttributeTerm::Modifiers {
                                target: Target::Current,
                                property: "tax_income".into(),
                            },
                        ]
                    } else {
                        Vec::new()
                    },
                    rates: vec![
                        AttributeTerm::Buildings {
                            property: local_rate.into(),
                        },
                        AttributeTerm::TradeGood {
                            property: local_rate.into(),
                        },
                        AttributeTerm::Modifiers {
                            target: Target::Current,
                            property: local_rate.into(),
                        },
                        AttributeTerm::Modifiers {
                            target: Target::Owner,
                            property: global_rate.into(),
                        },
                    ],
                };
                match province.is_city {
                    Some(true) => {
                        self.static_term(&mut channel, "city", local_rate, report, &mut unresolved)?
                    }
                    None => {
                        unresolved.insert("city_status".into());
                    }
                    _ => (),
                }
                match (&province.owner, &province.controller) {
                    (Some(owner), Some(controller)) if owner != controller => self.static_term(
                        &mut channel,
                        "occupied",
                        local_rate,
                        report,
                        &mut unresolved,
                    )?,
                    (Some(_), None) => {
                        unresolved.insert("controller".into());
                    }
                    _ => (),
                }
                let mut origins = vec![format!("province:{}:", province.id)];
                if let Some(owner) = &province.owner {
                    origins.push(format!("country:{owner}:"));
                }
                for pending in setup
                    .pending_timed_modifiers
                    .keys()
                    .filter(|key| origins.iter().any(|p| key.starts_with(p)))
                {
                    unresolved.insert(format!("pending_modifier:{pending}"));
                }
                Self::ambiguities(initial, entity, &channel, report, setup, &mut unresolved);
                let formula = AttributeFormula {
                    base: if base.is_some() {
                        AttributeBase::Field {
                            field: initial.world().schema().field(field).expect("base field"),
                            micros_per_unit: scale,
                        }
                    } else {
                        AttributeBase::Unknown {
                            reason: format!("province:{}:missing_base:{field}", province.id),
                        }
                    },
                    channels: vec![channel],
                    minimum_micros: None,
                    maximum_micros: None,
                    unresolved,
                };
                formulas.insert(name.into(), formula);
            }
            report.province_calculations += formulas.len();
            self.attributes.formulas.insert(entity, formulas);
        }
        Ok(self.attributes)
    }

    fn static_term(
        &self,
        channel: &mut AttributeChannel,
        definition: &str,
        property: &str,
        report: &AttributeReport,
        unresolved: &mut BTreeSet<String>,
    ) -> Result<(), Error> {
        if self.attributes.static_modifiers.contains_key(definition) {
            channel.rates.push(AttributeTerm::Static {
                definition: definition.into(),
                property: property.into(),
            });
        } else if report
            .ambiguous_definitions
            .contains_key(&format!("static_modifiers:{definition}"))
        {
            unresolved.insert(format!("ambiguous_static:{definition}"));
        } else {
            return Err(Error::message(
                "world",
                format!("missing required static modifier {definition}"),
            ));
        }
        Ok(())
    }

    fn ambiguities(
        initial: &InitialWorld,
        entity: augustus_core::EntityId,
        channel: &AttributeChannel,
        report: &AttributeReport,
        setup: &SetupReport,
        unresolved: &mut BTreeSet<String>,
    ) {
        let mut candidates = Vec::new();
        let economy = initial.world().economy().provinces.get(&entity);
        for term in channel.flat.iter().chain(&channel.rates) {
            match term {
                AttributeTerm::Static {
                    definition,
                    property,
                } => candidates.push(format!("static_modifiers:{definition}:{property}")),
                AttributeTerm::Buildings { property } => {
                    if let Some(state) = economy {
                        candidates.extend(
                            state
                                .buildings
                                .iter()
                                .map(|b| format!("buildings:{b}:{property}")),
                        );
                    }
                }
                AttributeTerm::TradeGood { property } => {
                    if let Some(good) = economy.and_then(|s| s.trade_good.as_ref()) {
                        candidates.push(format!("tradegoods:{good}:{property}"));
                    }
                }
                AttributeTerm::Modifiers { target, property } => {
                    let target = match target {
                        Target::Current => Some(entity),
                        Target::Owner => {
                            initial
                                .world()
                                .entity(entity)
                                .expect("known province")
                                .owner
                        }
                        Target::Capital => {
                            initial
                                .world()
                                .entity(entity)
                                .expect("known entity")
                                .capital
                        }
                    };
                    if let Some(active) =
                        target.and_then(|e| initial.world().modifiers().active.get(&e))
                    {
                        candidates.extend(
                            active
                                .keys()
                                .map(|name| format!("event_modifier:{name}:{property}")),
                        );
                    }
                }
                _ => (),
            }
        }
        for key in candidates {
            if report.ambiguous_properties.contains_key(&key)
                || setup.ambiguous_properties.contains_key(&key)
            {
                unresolved.insert(format!("ambiguous:{key}"));
            }
        }
    }
}
