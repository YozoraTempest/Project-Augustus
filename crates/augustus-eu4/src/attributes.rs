// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    AdministrativeCategory, AffiliationEffect, AffiliationRelation, AttributeBase,
    AttributeChannel, AttributeFormula, AttributeTerm, Attributes, Condition, InitialWorld,
    ProvinceClass, Target,
};
use serde::Serialize;

use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::{PropertyDiagnostics, block, properties};
use crate::{
    ContentView, Error, FileEntry, LoadedSource, ParsedText, SetupReport, SourceLocation,
    TextEncoding,
};

const ENCODING: TextEncoding = TextEncoding::Windows1252;
const ATTRIBUTE_ONE_MICROS: i64 = 1_000_000;
const AUTONOMY_MONTHLY_SCALE_MICROS: i64 = 10_000;
// One game autonomy tick is 0.001 percentage points, or 10 ratio millionths.
const AUTONOMY_MONTHLY_QUANTUM_MICROS: i64 = 10;
pub(super) const ATTRIBUTE_NAMES: [&str; 4] = [
    "tax_basis",
    "manpower_contribution",
    "tax_after_autonomy",
    "manpower_after_autonomy",
];
pub(super) const COUNTRY_ANNUAL_TAX: &str = "country_annual_tax";

const GLOBAL_TAX_INCOME: &str = "global_tax_income";

#[derive(Debug, Default, Serialize)]
pub struct AttributeReport {
    pub static_definitions: usize,
    pub government_rank_definitions: usize,
    pub building_definitions: usize,
    pub trade_good_definitions: usize,
    pub religion_definitions: usize,
    pub religious_school_definitions: usize,
    pub province_calculations: usize,
    pub manpower_per_base_micros: i64,
    pub definition_sources: BTreeMap<String, LoadedSource>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub overridden_definition_sources: BTreeMap<String, Vec<LoadedSource>>,
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

struct SchoolProperties<'a> {
    report: &'a mut AttributeReport,
    unresolved: BTreeSet<String>,
}

impl PropertyDiagnostics for SchoolProperties<'_> {
    fn unsupported(&mut self, parsed: &ParsedText<'_>, kind: &str, name: &str, at: SourceLocation) {
        self.unresolved.insert(name.into());
        self.report.unsupported(parsed, kind, name, at);
    }
    fn ambiguous(&mut self, key: String, first: LoadedSource, next: LoadedSource) {
        self.report.ambiguous(key, first, next);
    }
}

pub(super) struct AttributeDraft {
    attributes: Attributes,
    static_unresolved: BTreeMap<String, BTreeSet<String>>,
    manpower_per_base: i64,
}

/// Reads the selected numeric result from the evaluated define session.
fn manpower_define(defines: &crate::Defines, report: &mut AttributeReport) -> Result<i64, Error> {
    let (value, at) = defines.required("NMilitary", "BASE_MP_TO_MANPOWER")?;
    let amount = value
        .checked_mul(1000)
        .filter(|v| *v > 0)
        .ok_or_else(|| Error::message(&at.path, "expected positive manpower define"))?;
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
            | GLOBAL_TAX_INCOME
            | "local_manpower_modifier"
            | "global_manpower_modifier"
            | "local_autonomy"
            | "global_autonomy"
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
        root: &impl ContentView,
        files: &[FileEntry],
        defines: &crate::Defines,
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
                    let (definition, effect_field, effect_source) = if kind == "static_modifiers" {
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
                        (definition, None, None)
                    } else {
                        let field = if kind == "buildings" {
                            "modifier"
                        } else {
                            "province"
                        };
                        let mut definition = None;
                        let mut effect_source = None;
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
                            effect_source = Some(source(&parsed, at));
                        }
                        (
                            definition.unwrap_or_default(),
                            Some(field.to_owned()),
                            effect_source,
                        )
                    };
                    for property in definition
                        .values_micros
                        .keys()
                        .chain(definition.flags.keys())
                        .filter(|p| {
                            !used_property(p)
                                && !(kind == "buildings" && p.as_str() == "fort_level")
                        })
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
                    if kind == "buildings" && catalog.contains_key(&name) {
                        for source_key in [identifier.clone(), format!("{identifier}:modifier")] {
                            if let Some(previous) = report.definition_sources.remove(&source_key) {
                                report
                                    .overridden_definition_sources
                                    .entry(source_key)
                                    .or_default()
                                    .push(previous);
                            }
                        }
                    } else if catalog.contains_key(&name) {
                        return Err(fail("duplicate attribute definition"));
                    }
                    catalog.insert(name.clone(), definition);
                    report
                        .definition_sources
                        .insert(identifier.clone(), source(&parsed, at));
                    if let (Some(field), Some(effect_source)) = (effect_field, effect_source) {
                        report
                            .definition_sources
                            .insert(format!("{identifier}:{field}"), effect_source);
                    }
                }
            }
        }
        let mut government_rank_definitions = 0_usize;
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/government_ranks"))
        {
            let doc = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let name = key.read_string();
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                let rank = name
                    .parse::<u8>()
                    .ok()
                    .filter(|rank| (1..=3).contains(rank))
                    .ok_or_else(|| fail("unsupported government rank identifier"))?;
                let rank_name = rank.to_string();
                let identifier = format!("government_ranks:{rank}");
                let reader = block(&value, &fail)?;
                let definition = properties(
                    &parsed,
                    reader.clone(),
                    "government_ranks",
                    &rank_name,
                    report,
                )?;
                let unresolved = reader
                    .fields()
                    .map(|(key, _, _)| key.read_string())
                    .filter(|property| {
                        !definition.values_micros.contains_key(property)
                            && !definition.flags.contains_key(property)
                    })
                    .collect::<BTreeSet<_>>();
                let origin = source(&parsed, at);
                if let Some(origins) = report.ambiguous_definitions.get_mut(&identifier) {
                    origins.push(origin);
                    continue;
                }
                if let Some(first) = report.definition_sources.remove(&identifier) {
                    attributes.government_rank_modifiers.remove(&rank);
                    attributes
                        .government_rank_unresolved_properties
                        .remove(&rank);
                    government_rank_definitions = government_rank_definitions.saturating_sub(1);
                    report
                        .ambiguous_definitions
                        .insert(identifier, vec![first, origin]);
                    continue;
                }
                for property in definition
                    .values_micros
                    .keys()
                    .chain(definition.flags.keys())
                    .filter(|property| {
                        property.as_str() != "global_autonomy" && !used_property(property)
                    })
                {
                    *report
                        .unmapped_properties
                        .entry(format!("government_ranks:{property}"))
                        .or_default() += 1;
                }
                attributes
                    .government_rank_modifiers
                    .insert(rank, definition);
                if !unresolved.is_empty() {
                    attributes
                        .government_rank_unresolved_properties
                        .insert(rank, unresolved);
                }
                report.definition_sources.insert(identifier, origin);
                government_rank_definitions += 1;
            }
        }
        if government_rank_definitions > 0
            && !attributes.government_rank_modifiers.contains_key(&1)
            && !report
                .ambiguous_definitions
                .contains_key("government_ranks:1")
        {
            attributes
                .government_rank_modifiers
                .insert(1, augustus_core::ModifierDefinition::default());
        }
        report.government_rank_definitions = government_rank_definitions;
        report.static_definitions = attributes.static_modifiers.len();
        report.building_definitions = attributes.building_modifiers.len();
        report.trade_good_definitions = attributes.trade_good_modifiers.len();
        Self::religions(root, files, &mut attributes, report)?;
        Self::religious_schools(root, files, &mut attributes, report)?;
        attributes.static_unresolved_properties = static_unresolved
            .iter()
            .filter(|(name, properties)| {
                attributes.static_modifiers.contains_key(*name) && !properties.is_empty()
            })
            .map(|(name, properties)| (name.clone(), properties.clone()))
            .collect();
        let manpower_per_base = manpower_define(defines, report)?;
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
                        AttributeTerm::Metrics {
                            target: Target::Current,
                            property: local_rate.into(),
                        },
                        AttributeTerm::Metrics {
                            target: Target::Owner,
                            property: global_rate.into(),
                        },
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
                channel.rates.push(AttributeTerm::AffiliationModifiers {
                    dimension: "religious_school".into(),
                    target: Target::Owner,
                    scope: "country".into(),
                    property: global_rate.into(),
                });
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
                    Some(true) => self.static_term(
                        &mut channel,
                        "city",
                        local_rate,
                        true,
                        report,
                        &mut unresolved,
                    )?,
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
                        true,
                        report,
                        &mut unresolved,
                    )?,
                    (Some(_), None) => {
                        unresolved.insert("controller".into());
                    }
                    _ => (),
                }
                Self::ambiguities(initial, entity, &channel, report, setup, &mut unresolved);
                let tax_efficiency_terms = (name == "tax_basis").then(|| channel.rates.clone());
                if let Some(terms) = &tax_efficiency_terms {
                    channel.rates = vec![AttributeTerm::ProductOfSums {
                        source: "province_tax_efficiency".into(),
                        factors: vec![
                            terms.clone(),
                            vec![AttributeTerm::Constant {
                                source: "tax_efficiency_baseline".into(),
                                micros: ATTRIBUTE_ONE_MICROS,
                            }],
                        ],
                        quantum_micros: 1000,
                    }];
                }
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
                    minimum_micros: (name == "tax_basis").then_some(0),
                    maximum_micros: None,
                    result_quantum_micros: tax_efficiency_terms.as_ref().map(|_| 1000),
                    unresolved,
                };
                let mut after_autonomy = formula.clone();
                after_autonomy.unresolved.remove("autonomy_and_state_rules");
                let derived = if name == "tax_basis" {
                    "tax_after_autonomy"
                } else {
                    "manpower_after_autonomy"
                };
                if tax_efficiency_terms.is_some() {
                    after_autonomy
                        .unresolved
                        .remove("original_runtime_comparison");
                }
                // Apply autonomy after province tax efficiency has run.
                after_autonomy.channels.push(AttributeChannel {
                    flat: Vec::new(),
                    rates: vec![AttributeTerm::Autonomy],
                });
                formulas.insert(derived.into(), after_autonomy);
                formulas.insert(name.into(), formula);
            }
            formulas.insert(
                augustus_core::MONTHLY_AUTONOMY_DELTA_ATTRIBUTE.into(),
                self.monthly_autonomy_formula(initial, entity, province, report, setup)?,
            );
            report.province_calculations += formulas.len();
            self.attributes.formulas.insert(entity, formulas);
        }

        let all_nations_ambiguous = report
            .ambiguous_definitions
            .contains_key("static_modifiers:all_nations");
        let all_nations_tax_ambiguous = report
            .ambiguous_properties
            .contains_key(&format!("static_modifiers:all_nations:{GLOBAL_TAX_INCOME}"));
        let all_nations_tax_unsupported = self
            .static_unresolved
            .get("all_nations")
            .is_some_and(|properties| properties.contains(GLOBAL_TAX_INCOME));
        let all_nations_tax_is_flag = self
            .attributes
            .static_modifiers
            .get("all_nations")
            .is_some_and(|definition| definition.flags.contains_key(GLOBAL_TAX_INCOME));
        let has_all_nations_source =
            self.attributes.static_modifiers.contains_key("all_nations") || all_nations_ambiguous;

        if has_all_nations_source {
            let free_city_tax_key =
                format!("static_modifiers:free_cities_in_hre:{GLOBAL_TAX_INCOME}");
            let free_city_definition_ambiguous = report
                .ambiguous_definitions
                .contains_key("static_modifiers:free_cities_in_hre");
            let free_city_tax_ambiguous =
                report.ambiguous_properties.contains_key(&free_city_tax_key);
            let free_city_tax_unsupported = self
                .static_unresolved
                .get("free_cities_in_hre")
                .is_some_and(|properties| properties.contains(GLOBAL_TAX_INCOME));
            let free_city_tax_is_flag = self
                .attributes
                .static_modifiers
                .get("free_cities_in_hre")
                .is_some_and(|definition| definition.flags.contains_key(GLOBAL_TAX_INCOME));
            let free_city_tax_unknown = free_city_definition_ambiguous
                || free_city_tax_ambiguous
                || free_city_tax_unsupported
                || free_city_tax_is_flag;
            let free_city_value_present = self
                .attributes
                .static_modifiers
                .get("free_cities_in_hre")
                .is_some_and(|definition| definition.values_micros.contains_key(GLOBAL_TAX_INCOME));
            let free_city_tax_term = if free_city_tax_unknown {
                Some(AttributeTerm::Unknown {
                    reason: "emperor_free_city_tax_source_unresolved".into(),
                })
            } else if free_city_value_present {
                Some(AttributeTerm::CountedStaticModifier {
                    definition: "free_cities_in_hre".into(),
                    property: GLOBAL_TAX_INCOME.into(),
                    reform: "free_city".into(),
                    affiliation_dimension: crate::imperial_membership::DIMENSION.into(),
                    affiliation_member: "hre".into(),
                })
            } else {
                None
            };

            for country in initial.countries() {
                let entity = initial
                    .country_entity(&country.id)
                    .expect("registered country");
                let mut formulas = self.attributes.formulas.remove(&entity).unwrap_or_default();
                let mut unresolved = BTreeSet::new();

                let source_unresolved = all_nations_ambiguous
                    || all_nations_tax_ambiguous
                    || all_nations_tax_unsupported
                    || all_nations_tax_is_flag;
                let (base, mut channels) = if source_unresolved {
                    unresolved.insert("all_nations_tax_source_unresolved".into());
                    (
                        AttributeBase::Unknown {
                            reason: "all_nations_tax_source_unresolved".into(),
                        },
                        Vec::new(),
                    )
                } else {
                    (
                        AttributeBase::Constant { micros: 0 },
                        vec![AttributeChannel {
                            flat: vec![AttributeTerm::Conditional {
                                condition: Box::new(Condition::HasOwnedProvince {
                                    target: Target::Current,
                                }),
                                then: vec![AttributeTerm::Static {
                                    definition: "all_nations".into(),
                                    property: GLOBAL_TAX_INCOME.into(),
                                }],
                                otherwise: vec![AttributeTerm::Unknown {
                                    reason: "all_nations_landless_country_type_unresolved".into(),
                                }],
                            }],
                            rates: Vec::new(),
                        }],
                    )
                };
                if free_city_tax_term.is_some() && channels.is_empty() {
                    channels.push(AttributeChannel {
                        flat: Vec::new(),
                        rates: Vec::new(),
                    });
                }
                if let Some(term) = &free_city_tax_term {
                    channels[0].flat.push(AttributeTerm::OfficeScoped {
                        office: "imperial_emperor".into(),
                        term: Box::new(term.clone()),
                    });
                }
                formulas.insert(
                    COUNTRY_ANNUAL_TAX.into(),
                    AttributeFormula {
                        base,
                        channels,
                        minimum_micros: None,
                        maximum_micros: None,
                        result_quantum_micros: None,
                        unresolved,
                    },
                );
                self.attributes.formulas.insert(entity, formulas);
            }
        }
        Ok(self.attributes)
    }

    fn static_term(
        &self,
        channel: &mut AttributeChannel,
        definition: &str,
        property: &str,
        is_rate: bool,
        report: &AttributeReport,
        unresolved: &mut BTreeSet<String>,
    ) -> Result<(), Error> {
        if self.attributes.static_modifiers.contains_key(definition) {
            let term = AttributeTerm::Static {
                definition: definition.into(),
                property: property.into(),
            };
            if is_rate {
                channel.rates.push(term);
            } else {
                channel.flat.push(term);
            }
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

    fn monthly_autonomy_formula(
        &self,
        initial: &InitialWorld,
        entity: augustus_core::EntityId,
        province: &augustus_core::ProvinceState,
        report: &mut AttributeReport,
        setup: &SetupReport,
    ) -> Result<AttributeFormula, Error> {
        let mut unresolved = BTreeSet::new();
        if !(1..=3).all(|rank| {
            self.attributes
                .government_rank_modifiers
                .contains_key(&rank)
        }) {
            unresolved.insert("government_ranks:incomplete".into());
        }
        let mut sources = AttributeChannel {
            flat: vec![
                AttributeTerm::Administration {
                    mapping: "province_status".into(),
                    property: "local_autonomy".into(),
                },
                AttributeTerm::Metrics {
                    target: Target::Current,
                    property: "local_autonomy".into(),
                },
                AttributeTerm::Metrics {
                    target: Target::Owner,
                    property: "global_autonomy".into(),
                },
                AttributeTerm::Buildings {
                    property: "local_autonomy".into(),
                },
                AttributeTerm::TradeGood {
                    property: "local_autonomy".into(),
                },
                AttributeTerm::Modifiers {
                    target: Target::Current,
                    property: "local_autonomy".into(),
                },
                AttributeTerm::Modifiers {
                    target: Target::Owner,
                    property: "global_autonomy".into(),
                },
                AttributeTerm::Estates {
                    target: Target::Owner,
                    property: "global_autonomy".into(),
                },
                AttributeTerm::Ideas {
                    target: Target::Owner,
                    property: "global_autonomy".into(),
                },
                AttributeTerm::Government {
                    target: Target::Owner,
                    property: "global_autonomy".into(),
                },
                AttributeTerm::AffiliationRelation {
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
                    property: "local_autonomy".into(),
                },
                AttributeTerm::AffiliationModifiers {
                    dimension: "religion".into(),
                    target: Target::Current,
                    scope: "province".into(),
                    property: "local_autonomy".into(),
                },
                AttributeTerm::AffiliationModifiers {
                    dimension: "religion".into(),
                    target: Target::Owner,
                    scope: "country".into(),
                    property: "global_autonomy".into(),
                },
                AttributeTerm::AffiliationModifiers {
                    dimension: "religious_school".into(),
                    target: Target::Owner,
                    scope: "country".into(),
                    property: "global_autonomy".into(),
                },
            ],
            rates: Vec::new(),
        };
        match province.is_city {
            Some(true) => self.static_term(
                &mut sources,
                "city",
                "local_autonomy",
                false,
                report,
                &mut unresolved,
            )?,
            None => {
                unresolved.insert("city_status".into());
            }
            _ => (),
        }
        match (&province.owner, &province.controller) {
            (Some(owner), Some(controller)) if owner != controller => self.static_term(
                &mut sources,
                "occupied",
                "local_autonomy",
                false,
                report,
                &mut unresolved,
            )?,
            (Some(_), None) => {
                unresolved.insert("controller".into());
            }
            _ => (),
        }
        Self::ambiguities(initial, entity, &sources, report, setup, &mut unresolved);

        let mut inputs = sources.flat;
        inputs.push(AttributeTerm::Conditional {
            condition: Box::new(Condition::PoliticalStatus {
                target: Target::Owner,
                name: "at_war".into(),
            }),
            then: vec![AttributeTerm::Static {
                definition: "war".into(),
                property: "global_autonomy".into(),
            }],
            otherwise: vec![AttributeTerm::Static {
                definition: "peace".into(),
                property: "global_autonomy".into(),
            }],
        });
        Ok(AttributeFormula {
            base: AttributeBase::Constant { micros: 0 },
            channels: vec![AttributeChannel {
                flat: vec![AttributeTerm::Scale {
                    factor_micros: AUTONOMY_MONTHLY_SCALE_MICROS,
                    term: Box::new(AttributeTerm::TruncatedSum {
                        source: "eu4_monthly_autonomy_modifiers".into(),
                        quantum_micros: 1,
                        terms: inputs,
                    }),
                }],
                rates: Vec::new(),
            }],
            minimum_micros: None,
            maximum_micros: None,
            result_quantum_micros: Some(AUTONOMY_MONTHLY_QUANTUM_MICROS),
            unresolved,
        })
    }

    fn religions(
        root: &impl ContentView,
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
                    if name == "religious_schools" {
                        continue;
                    }
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

    fn religious_schools(
        root: &impl ContentView,
        files: &[FileEntry],
        attributes: &mut Attributes,
        report: &mut AttributeReport,
    ) -> Result<(), Error> {
        let mut members = BTreeMap::new();
        for file in files
            .iter()
            .filter(|file| direct_txt(&file.relative_path, "common/religions"))
        {
            let document = root.read_text(&file.relative_path, ENCODING)?;
            let parsed = document.parse()?;
            for (group, operator, value) in parsed.reader().fields() {
                let fail = |message: &str| {
                    Error::message(parsed.source(), message)
                        .with_location(parsed.field_location(&group))
                };
                equal(operator, &fail)?;
                for (_, operator, value) in block(&value, &fail)?
                    .fields()
                    .filter(|(key, _, _)| key.read_string() == "religious_schools")
                {
                    equal(operator, &fail)?;
                    for (school, operator, value) in block(&value, &fail)?.fields() {
                        let at = parsed.field_location(&school);
                        let fail = |message: &str| {
                            Error::message(parsed.source(), message).with_location(at)
                        };
                        equal(operator, &fail)?;
                        let name = school.read_string();
                        let identifier = format!("religious_schools:{name}");
                        if let Some(origins) = report.ambiguous_definitions.get_mut(&identifier) {
                            origins.push(source(&parsed, at));
                            continue;
                        }
                        if let Some(first) = report.definition_sources.remove(&identifier) {
                            members.remove(&name);
                            report
                                .ambiguous_definitions
                                .insert(identifier, vec![first, source(&parsed, at)]);
                            continue;
                        }
                        let kind = "religious_schools.country";
                        let mut diagnostics = SchoolProperties {
                            report,
                            unresolved: BTreeSet::new(),
                        };
                        let modifier = properties(
                            &parsed,
                            block(&value, &fail)?,
                            kind,
                            &name,
                            &mut diagnostics,
                        )?;
                        let mut unresolved_properties = diagnostics.unresolved;
                        let prefix = format!("{kind}:{name}:");
                        unresolved_properties.extend(
                            report
                                .ambiguous_properties
                                .keys()
                                .filter_map(|key| key.strip_prefix(&prefix).map(String::from)),
                        );
                        for property in modifier
                            .values_micros
                            .keys()
                            .chain(modifier.flags.keys())
                            .filter(|property| !used_property(property))
                        {
                            *report
                                .unmapped_properties
                                .entry(format!("{kind}:{property}"))
                                .or_default() += 1;
                        }
                        members.insert(
                            name,
                            AffiliationEffect {
                                modifier,
                                unresolved_properties,
                            },
                        );
                        report
                            .definition_sources
                            .insert(identifier, source(&parsed, at));
                    }
                }
            }
        }
        report.religious_school_definitions = members.len();
        attributes.affiliation_modifiers.insert(
            "religious_school".into(),
            [("country".into(), members)].into(),
        );
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
