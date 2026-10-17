// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    Administration, AdministrationRules, AreaStatus, CoreType, InitialWorld,
    ProvinceAdministration, ProvinceClass,
};
use jomini::common::Date;
use jomini::{
    TextToken,
    text::{ObjectReader, ValueReader},
};
use serde::Serialize;

use crate::bootstrap::{equal, source};
use crate::setup::micros;
use crate::{ContentView, Error, FileEntry, LoadedSource, ParsedText, TextEncoding};

const ENCODING: TextEncoding = TextEncoding::Windows1252;

#[derive(Debug, Default, Serialize)]
pub struct AdministrationReport {
    pub areas: usize,
    pub regions: usize,
    pub superregions: usize,
    pub continents: usize,
    pub unapplied_geography_fields: BTreeMap<String, LoadedSource>,
    pub mapped_provinces: usize,
    pub province_records: usize,
    pub capital_states: usize,
    pub initial_states: usize,
    pub full_core_types: usize,
    pub direct_autonomy_operations: usize,
    pub country_autonomy_operations: usize,
    pub initialized_autonomy: usize,
    pub unknown_initial_autonomy: Vec<u32>,
    pub initial_country_ranks: BTreeMap<String, u8>,
    pub applied_country_autonomy: BTreeMap<u32, Vec<LoadedSource>>,
    pub missing_land_areas: Vec<u32>,
    pub unknown_country_area_statuses: usize,
    pub unspecified_core_types: usize,
    pub area_sources: BTreeMap<String, LoadedSource>,
    pub repeated_province_memberships: BTreeMap<u32, usize>,
    pub empty_areas: BTreeSet<String>,
    pub autonomy_sources: BTreeMap<u32, Vec<LoadedSource>>,
    pub pending_country_autonomy: BTreeMap<String, Vec<LoadedSource>>,
    pub pending_province_initialization: BTreeMap<u32, Vec<LoadedSource>>,
    pub rule_sources: BTreeMap<String, LoadedSource>,
}

pub(super) struct AdministrationDraft {
    areas: BTreeSet<String>,
    ordered_areas: BTreeMap<String, Vec<u32>>,
    memberships: BTreeMap<u32, String>,
    deltas: BTreeMap<u32, i64>,
    operations: BTreeMap<u32, Vec<i64>>,
    autonomy: crate::autonomy::AutonomyDraft,
    capital_maximum: i64,
}

impl AdministrationDraft {
    pub(super) fn ordered_areas(&self) -> BTreeMap<String, Vec<u32>> {
        self.ordered_areas.clone()
    }

    pub(super) fn affiliations(&self, initial: &InitialWorld) -> augustus_core::Affiliations {
        self.autonomy.affiliations(initial)
    }

    pub(super) fn load(
        root: &impl ContentView,
        files: &[FileEntry],
        defines: &crate::Defines,
        map: &ParsedText<'_>,
        report: &mut AdministrationReport,
    ) -> Result<Self, Error> {
        let mut result = Self {
            areas: BTreeSet::new(),
            ordered_areas: BTreeMap::new(),
            memberships: BTreeMap::new(),
            deltas: BTreeMap::new(),
            operations: BTreeMap::new(),
            autonomy: crate::autonomy::AutonomyDraft::load(root, files, defines, map, report)?,
            capital_maximum: 0,
        };
        let (value, at) = defines.required("NEconomy", "CAPITAL_MAX_AUTONOMY")?;
        result.capital_maximum = value / 100;
        if value > 100_000_000 || value % 100 != 0 {
            return Err(Error::message(
                &at.path,
                "capital autonomy must fit a ratio with six decimal places",
            ));
        }
        report.rule_sources.insert("capital_maximum".into(), at);
        let area_fields: Vec<_> = map
            .reader()
            .fields()
            .filter(|(k, _, _)| k.read_string() == "area")
            .collect();
        if area_fields.len() > 1 {
            return Err(Error::message(map.source(), "duplicate map area path"));
        }
        let Some((key, op, value)) = area_fields.into_iter().next() else {
            return Ok(result);
        };
        let fail =
            |m: &str| Error::message(map.source(), m).with_location(map.field_location(&key));
        equal(op, &fail)?;
        let filename = value
            .read_string()
            .map_err(|_| fail("expected area filename"))?;
        // ContentView applies traversal checks to the combined relative path.
        let doc = root.read_text(std::path::Path::new("map").join(filename), ENCODING)?;
        let parsed = doc.parse()?;
        for (key, op, value) in parsed.reader().fields() {
            let name = key.read_string();
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            equal(op, &fail)?;
            if name.is_empty() || !result.areas.insert(name.clone()) {
                return Err(fail("empty or duplicate area"));
            }
            if !matches!(
                value.token(),
                TextToken::Array { .. } | TextToken::Object { mixed: true, .. }
            ) {
                return Err(fail(
                    "area requires a province list, optionally preceded by color",
                ));
            }
            let object = value
                .read_object()
                .map_err(|_| fail("expected area block"))?;
            for (colors, (key, op, value)) in object.fields().enumerate() {
                equal(op, &fail)?;
                if key.read_string() != "color" || colors != 0 {
                    return Err(fail("unsupported or duplicate area metadata"));
                }
                let components = value
                    .read_array()
                    .map_err(|_| fail("expected area color"))?;
                if components.len() != 3
                    || components.values().any(|v| {
                        v.read_string()
                            .ok()
                            .and_then(|s| s.parse::<u8>().ok())
                            .is_none()
                    })
                {
                    return Err(fail("expected three byte color components"));
                }
            }
            let list = value
                .read_array()
                .map_err(|_| fail("expected area province list"))?;
            if list.is_empty() {
                report.empty_areas.insert(name.clone());
            }
            let mut ordered = Vec::new();
            for id in list.values() {
                let id = id
                    .read_string()
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|v| *v > 0)
                    .ok_or_else(|| fail("expected positive area province identifier"))?;
                ordered.push(id);
                if let Some(previous) = result.memberships.get(&id) {
                    if previous != &name {
                        return Err(fail("province belongs to more than one area"));
                    }
                    *report.repeated_province_memberships.entry(id).or_insert(1) += 1;
                } else {
                    result.memberships.insert(id, name.clone());
                }
            }
            result.ordered_areas.insert(name.clone(), ordered);
            report.area_sources.insert(name, source(&parsed, at));
        }
        report.areas = result.areas.len();
        report.mapped_provinces = result.memberships.len();
        result.autonomy.set_areas(&result.areas);
        Ok(result)
    }

    pub(super) fn province_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        province: u32,
        report: &mut AdministrationReport,
    ) -> Result<BTreeSet<String>, Error> {
        self.autonomy
            .province_history(parsed, reader.clone(), province, report)?;
        let mut handled = BTreeSet::new();
        for (key, op, value) in reader
            .fields()
            .filter(|(k, _, _)| k.read_string() == "add_local_autonomy")
        {
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            equal(op, &fail)?;
            let value = autonomy_delta(&value, &fail)?;
            self.operations.entry(province).or_default().push(value);
            let delta = self.deltas.entry(province).or_default();
            *delta = delta
                .checked_add(value)
                .ok_or_else(|| fail("autonomy history delta overflow"))?;
            report.direct_autonomy_operations += 1;
            report
                .autonomy_sources
                .entry(province)
                .or_default()
                .push(source(parsed, at));
            handled.insert("add_local_autonomy".into());
        }
        Ok(handled)
    }

    pub(super) fn country_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        country: &str,
        report: &mut AdministrationReport,
    ) -> Result<BTreeSet<String>, Error> {
        let handled = reader
            .fields()
            .filter_map(|(key, _, _)| {
                let name = key.read_string();
                matches!(
                    name.as_str(),
                    "government_rank"
                        | "add_accepted_culture"
                        | "remove_accepted_culture"
                        | "add_government_reform"
                        | "remove_government_reform"
                )
                .then_some(name)
            })
            .collect();
        self.autonomy
            .country_history(parsed, reader, country, &self.areas, report)?;
        Ok(handled)
    }

    pub(super) fn initialize_country_ranks(
        &self,
        initial: &mut InitialWorld,
        report: &mut AdministrationReport,
    ) -> Result<(), Error> {
        let ranks = initial
            .countries()
            .iter()
            .filter_map(|country| {
                let entity = initial.country_entity(&country.id)?;
                self.autonomy
                    .rank(initial, &country.id)
                    .map(|rank| (country.id.clone(), entity, rank))
            })
            .collect::<Vec<_>>();
        for (country, entity, rank) in ranks {
            initial
                .set_government_rank(entity, Some(rank))
                .map_err(|error| Error::caused_by("government rank", error))?;
            report.initial_country_ranks.insert(country, rank);
        }
        Ok(())
    }

    pub(super) fn bind(
        mut self,
        initial: &InitialWorld,
        history: &BTreeSet<u32>,
        report: &mut AdministrationReport,
        attributes: &augustus_core::Attributes,
        definitions: &crate::AttributeReport,
        date: Date,
    ) -> Result<Administration, Error> {
        self.autonomy.bind_rank_autonomy(attributes);
        let baseline = date == Date::parse("1444.11.11").expect("valid baseline date");
        for &id in self.memberships.keys() {
            if initial.province(id).is_none() {
                return Err(Error::message(
                    "map/area",
                    format!("area references unknown province {id}"),
                ));
            }
        }
        let floor = |definition: &str| -> Result<Option<i64>, Error> {
            let key = format!("static_modifiers:{definition}:min_local_autonomy");
            if definitions
                .ambiguous_definitions
                .contains_key(&format!("static_modifiers:{definition}"))
                || definitions.ambiguous_properties.contains_key(&key)
            {
                return Ok(None);
            }
            let Some(data) = attributes.static_modifiers.get(definition) else {
                return Err(Error::message(
                    "administration",
                    format!("missing required static modifier {definition}"),
                ));
            };
            if data.flags.contains_key("min_local_autonomy") {
                return Err(Error::message(
                    "administration",
                    "minimum autonomy requires a numeric property",
                ));
            }
            let value = data
                .values_micros
                .get("min_local_autonomy")
                .copied()
                .unwrap_or(0);
            if value % 100 != 0 {
                return Err(Error::message(
                    "administration",
                    "minimum autonomy exceeds ratio precision",
                ));
            }
            if let Some(at) = definitions
                .definition_sources
                .get(&format!("static_modifiers:{definition}"))
            {
                report.rule_sources.insert(definition.into(), at.clone());
            }
            Ok(Some(value / 100))
        };
        let mut floor = floor;
        let rules = AdministrationRules {
            state_full_minimum_micros: floor("core")?,
            state_territorial_minimum_micros: floor("colonial_core")?,
            state_noncore_minimum_micros: floor("non_core")?,
            territory_core_minimum_micros: floor("territory_core")?,
            territory_noncore_minimum_micros: floor("territory_non_core")?,
            capital_maximum_micros: Some(self.capital_maximum),
        };
        let mut administration = Administration {
            areas: self.areas,
            regions: self.autonomy.geography.regions.clone(),
            superregions: self.autonomy.geography.superregions.clone(),
            rules,
            ..Default::default()
        };
        for (name, provinces) in &self.autonomy.geography.continents {
            let mut members = BTreeSet::new();
            for &id in provinces {
                let entity = initial.province_entity(id).ok_or_else(|| {
                    Error::message(
                        "map/continent",
                        format!("continent {name} references unknown province {id}"),
                    )
                })?;
                members.insert(entity);
            }
            administration.continents.insert(name.clone(), members);
        }
        let mut country_areas = BTreeSet::new();
        for p in initial
            .provinces()
            .iter()
            .filter(|p| p.class == ProvinceClass::Land)
        {
            let entity = initial.province_entity(p.id).expect("registered province");
            let area = self.memberships.get(&p.id).cloned();
            if area.is_none() {
                report.missing_land_areas.push(p.id);
            }
            let mut unresolved = BTreeSet::new();
            unresolved.insert("other_minimum_autonomy_effects".into());
            if self
                .autonomy
                .has_pending(area.as_deref(), p.owner.as_deref())
            {
                unresolved.insert("scoped_country_history".into());
            }
            let cores = p
                .cores
                .iter()
                .map(|c| {
                    (
                        initial.country_entity(c).expect("validated core country"),
                        if baseline {
                            CoreType::Full
                        } else {
                            CoreType::Unspecified
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>();
            if baseline {
                report.full_core_types += cores.len();
            } else {
                report.unspecified_core_types += cores.len();
            }
            let raw_autonomy = if baseline {
                self.autonomy.generate(
                    initial,
                    p,
                    history.contains(&p.id),
                    area.as_deref(),
                    self.operations.get(&p.id).map_or(&[], Vec::as_slice),
                    report,
                )
            } else {
                None
            };
            if raw_autonomy.is_some() {
                report.initialized_autonomy += 1;
            } else {
                report.unknown_initial_autonomy.push(p.id);
            }
            administration.provinces.insert(
                entity,
                ProvinceAdministration {
                    area: area.clone(),
                    cores_known: history.contains(&p.id),
                    cores,
                    raw_autonomy_micros: raw_autonomy,
                    history_autonomy_delta_micros: if raw_autonomy.is_some() {
                        0
                    } else {
                        self.deltas.get(&p.id).copied().unwrap_or(0)
                    },
                    unresolved,
                },
            );
            if let (Some(owner), Some(area)) = (p.owner.as_ref(), area) {
                country_areas.insert((
                    initial.country_entity(owner).expect("validated owner"),
                    area,
                ));
            }
        }
        // The fixed initial bookmark creates a state in every owned area.
        // Later bookmarks need their own initialization evidence.
        if baseline {
            for (country, area) in &country_areas {
                administration
                    .country_areas
                    .entry(*country)
                    .or_default()
                    .insert(area.clone(), AreaStatus::State);
            }
            report.initial_states = country_areas.len();
        }
        for c in initial.countries() {
            if let Some(capital) = initial.bound_capital(&c.id)
                && let Some(area) = self.memberships.get(&capital)
                && administration.areas.contains(area)
            {
                administration
                    .country_areas
                    .entry(initial.country_entity(&c.id).expect("registered country"))
                    .or_default()
                    .insert(area.clone(), AreaStatus::State);
                report.capital_states += 1;
            }
        }
        report.unknown_country_area_statuses = country_areas
            .iter()
            .filter(|(country, area)| {
                !administration
                    .country_areas
                    .get(country)
                    .is_some_and(|states| states.contains_key(area))
            })
            .count();
        report.province_records = administration.provinces.len();
        Ok(administration)
    }
}

pub(super) fn autonomy_delta(
    value: &ValueReader<'_, '_, TextEncoding>,
    fail: &impl Fn(&str) -> Error,
) -> Result<i64, Error> {
    value
        .read_string()
        .ok()
        .and_then(|s| micros(&s))
        .filter(|v| v % 100 == 0)
        .map(|v| v / 100)
        .ok_or_else(|| {
            fail("expected literal autonomy percentage with at most four decimal places")
        })
}
