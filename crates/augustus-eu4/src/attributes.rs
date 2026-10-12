// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    AdministrativeCategory, AffiliationEffect, AffiliationRelation, AttributeBase,
    AttributeChannel, AttributeFormula, AttributeTerm, Attributes, InitialWorld, ProvinceClass,
    Target,
};
use serde::Serialize;

use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::{PropertyDiagnostics, block, properties};
use crate::{
    ContentRoot, Error, FileEntry, LoadedSource, ParsedText, SetupReport, SourceLocation,
    TextEncoding,
};

const ENCODING: TextEncoding = TextEncoding::Windows1252;
pub(super) const ATTRIBUTE_NAMES: [&str; 4] = [
    "tax_basis",
    "manpower_contribution",
    "tax_after_autonomy",
    "manpower_after_autonomy",
];

#[derive(Debug, Default, Serialize)]
pub struct AttributeReport {
    pub static_definitions: usize,
    pub building_definitions: usize,
    pub trade_good_definitions: usize,
    pub religion_definitions: usize,
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
    static_unresolved: BTreeMap<String, BTreeSet<String>>,
    manpower_per_base: i64,
}

/// Reads only the selected literal in the base file's explicit NMilitary block.
/// Expressions and assignments in additional define files require Lua support.
fn manpower_define(
    root: &ContentRoot,
    files: &[FileEntry],
    report: &mut AttributeReport,
) -> Result<i64, Error> {
    let (value, at) = crate::defines::literal(root, files, "NMilitary", "BASE_MP_TO_MANPOWER")?;
    let amount = value
        .checked_mul(1000)
        .filter(|v| *v > 0)
        .ok_or_else(|| Error::message(&at.path, "expected positive literal manpower define"))?;
    report
        .definition_sources
        .insert("define:manpower_per_base".into(), at);
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
    pub(super) fn static_modifier(&self, name: &str) -> Option<&augustus_core::ModifierDefinition> {
        self.attributes.static_modifiers.get(name)
    }
    pub(super) fn static_unresolved_properties(&self, name: &str) -> BTreeSet<String> {
        self.static_unresolved
            .get(name)
            .cloned()
            .unwrap_or_default()
    }
    pub(super) fn load(
        root: &ContentRoot,
        files: &[FileEntry],
        report: &mut AttributeReport,
    ) -> Result<Self, Error> {
        let mut attributes = Attributes::default();
        let mut static_unresolved: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
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
                        let definition = properties(&parsed, reader.clone(), kind, &name, report)?;
                        static_unresolved.insert(
                            name.clone(),
                            reader
                                .fields()
                                .map(|(key, _, _)| key.read_string())
                                .filter(|p| {
                                    !definition.values_micros.contains_key(p)
                                        && !definition.flags.contains_key(p)
                                })
                                .collect(),
                        );
                        definition
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
        Self::religions(root, files, &mut attributes, report)?;
        attributes.static_unresolved_properties = static_unresolved
            .iter()
            .filter(|(name, properties)| {
                attributes.static_modifiers.contains_key(*name) && !properties.is_empty()
            })
            .map(|(name, properties)| (name.clone(), properties.clone()))
            .collect();
        let manpower_per_base = manpower_define(root, files, report)?;
        Ok(Self {
            attributes,
            static_unresolved,
            manpower_per_base,
        })
    }

    pub(super) fn bind(
        mut self,
        initial: &InitialWorld,
        report: &mut AttributeReport,
        setup: &SetupReport,
    ) -> Result<Attributes, Error> {
        self.attributes.administration_mappings.insert(
            "province_status".into(),
            [
                (AdministrativeCategory::StateFullCore, "core".into()),
                (
                    AdministrativeCategory::StateTerritorialCore,
                    "colonial_core".into(),
                ),
                (AdministrativeCategory::StateNonCore, "non_core".into()),
                (
                    AdministrativeCategory::TerritoryCore,
                    "territory_core".into(),
                ),
                (
                    AdministrativeCategory::TerritoryNonCore,
                    "territory_non_core".into(),
                ),
            ]
            .into_iter()
            .collect(),
        );
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
                    "culture_government_effects",
                    "tolerance_and_religious_mechanics",
                    "other_government_estate_and_country_effects",
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
                        AttributeTerm::Estates {
                            target: Target::Owner,
                            property: global_rate.into(),
                        },
                        AttributeTerm::Ideas {
                            target: Target::Owner,
                            property: global_rate.into(),
                        },
                        AttributeTerm::Government {
                            target: Target::Owner,
                            property: global_rate.into(),
                        },
                    ],
                };
                channel.rates.push(AttributeTerm::AffiliationRelation {
                    dimension: "culture".into(),
                    target: Target::Owner,
                    definitions: [
                        (AffiliationRelation::SameGroup, "same_culture_group".into()),
                        (
                            AffiliationRelation::OtherGroup,
                            "non_accepted_culture".into(),
                        ),
                    ]
                    .into_iter()
                    .collect(),
                    property: local_rate.into(),
                });
                for (target, scope, property) in [
                    (Target::Current, "province", local_rate),
                    (Target::Owner, "country", global_rate),
                ] {
                    channel.rates.push(AttributeTerm::AffiliationModifiers {
                        dimension: "religion".into(),
                        target,
                        scope: scope.into(),
                        property: property.into(),
                    });
                }
                if name == "tax_basis" {
                    channel.flat.push(AttributeTerm::AffiliationModifiers {
                        dimension: "religion".into(),
                        target: Target::Current,
                        scope: "province".into(),
                        property: "tax_income".into(),
                    });
                }
                channel.rates.insert(
                    0,
                    AttributeTerm::Administration {
                        mapping: "province_status".into(),
                        property: local_rate.into(),
                    },
                );
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
                let mut after_autonomy = formula.clone();
                after_autonomy.channels.push(AttributeChannel {
                    flat: Vec::new(),
                    rates: vec![AttributeTerm::Autonomy],
                });
                after_autonomy.unresolved.remove("autonomy_and_state_rules");
                let derived = if name == "tax_basis" {
                    "tax_after_autonomy"
                } else {
                    "manpower_after_autonomy"
                };
                formulas.insert(derived.into(), after_autonomy);
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

    fn religions(
        root: &ContentRoot,
        files: &[FileEntry],
        attributes: &mut Attributes,
        report: &mut AttributeReport,
    ) -> Result<(), Error> {
        let mut scopes = BTreeMap::<String, BTreeMap<String, AffiliationEffect>>::new();
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/religions"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (group, op, value) in parsed.reader().fields() {
                let fail = |m: &str| {
                    Error::message(parsed.source(), m).with_location(parsed.field_location(&group))
                };
                equal(op, &fail)?;
                for (key, op, value) in block(&value, &fail)?.fields() {
                    if !matches!(
                        value.token(),
                        jomini::TextToken::Object { mixed: false, .. }
                    ) && !value.read_array().is_ok_and(|a| a.is_empty())
                    {
                        continue;
                    }
                    let name = key.read_string();
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    equal(op, &fail)?;
                    let identifier = format!("religions:{name}");
                    if let Some(origins) = report.ambiguous_definitions.get_mut(&identifier) {
                        origins.push(source(&parsed, at));
                        continue;
                    }
                    if let Some(first) = report.definition_sources.remove(&identifier) {
                        for members in scopes.values_mut() {
                            members.remove(&name);
                        }
                        for scope in ["country", "province"] {
                            report
                                .definition_sources
                                .remove(&format!("{identifier}:{scope}"));
                        }
                        report
                            .ambiguous_definitions
                            .insert(identifier, vec![first, source(&parsed, at)]);
                        continue;
                    }
                    let body = block(&value, &fail)?;
                    for scope in ["country", "province"] {
                        let selected: Vec<_> = body
                            .fields()
                            .filter(|(k, _, _)| k.read_string() == scope)
                            .collect();
                        if selected.len() > 1 {
                            return Err(fail("duplicate religion effect block"));
                        }
                        let kind = format!("religions.{scope}");
                        let mut effect = AffiliationEffect::default();
                        if let Some((key, op, value)) = selected.first() {
                            equal(*op, &fail)?;
                            effect.modifier =
                                properties(&parsed, block(value, &fail)?, &kind, &name, report)?;
                            report.definition_sources.insert(
                                format!("{identifier}:{scope}"),
                                source(&parsed, parsed.field_location(key)),
                            );
                        }
                        let prefix = format!("{kind}:{name}:");
                        effect.unresolved_properties = report
                            .ambiguous_properties
                            .keys()
                            .filter_map(|p| p.strip_prefix(&prefix).map(String::from))
                            .collect();
                        for property in effect
                            .modifier
                            .values_micros
                            .keys()
                            .chain(effect.modifier.flags.keys())
                            .filter(|p| !used_property(p))
                        {
                            *report
                                .unmapped_properties
                                .entry(format!("{kind}:{property}"))
                                .or_default() += 1;
                        }
                        scopes
                            .entry(scope.into())
                            .or_default()
                            .insert(name.clone(), effect);
                    }
                    report
                        .definition_sources
                        .insert(identifier, source(&parsed, at));
                }
            }
        }
        report.religion_definitions = scopes.get("country").map_or(0, BTreeMap::len);
        attributes
            .affiliation_modifiers
            .insert("religion".into(), scopes);
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
                AttributeTerm::AffiliationRelation {
                    definitions,
                    property,
                    ..
                } => {
                    candidates.extend(
                        definitions
                            .values()
                            .map(|d| format!("static_modifiers:{d}:{property}")),
                    );
                }
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
                AttributeTerm::Autonomy | AttributeTerm::Government { .. } => (),
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
