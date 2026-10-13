// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    AffiliationAcceptance, AffiliationDimension, Affiliations, DiplomaticKind, InitialWorld,
    ProvinceState,
};
use jomini::{TextToken, text::ObjectReader};

use crate::administration::AdministrationReport;
use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::block;
use crate::{ContentView, Error, FileEntry, LoadedSource, ParsedText, TextEncoding};

const ENCODING: TextEncoding = TextEncoding::Windows1252;
type Reader<'a, 'b> = ObjectReader<'a, 'b, TextEncoding>;

#[derive(Default)]
struct CountrySeed {
    rank: Option<u8>,
    accepted: BTreeSet<String>,
    reforms: BTreeSet<String>,
    uncertain: bool,
}

#[derive(Clone)]
enum Predicate {
    Constant(bool),
    Area(String),
    Region(String),
    Owner(String),
    And(Vec<Self>),
    Or(Vec<Self>),
    Not(Box<Self>),
}

struct Change {
    predicate: Predicate,
    delta: i64,
    source: LoadedSource,
}

/// Input-only initialization data. The resulting values belong to the shared world.
pub(super) struct AutonomyDraft {
    cultures: BTreeMap<String, Option<String>>,
    religions: BTreeMap<String, Option<String>>,
    pub(super) geography: crate::geography::GeographicGroups,
    reform_ranks: BTreeMap<String, Option<u8>>,
    subject_ranks: BTreeMap<String, Option<u8>>,
    countries: BTreeMap<String, CountrySeed>,
    changes: Vec<Change>,
    pending_changes: Vec<Predicate>,
    areas: BTreeSet<String>,
    uncertain_provinces: BTreeSet<u32>,
    union_rank: u8,
    emperor_rank: u8,
}

fn integer(text: &str, maximum: u8, fail: &impl Fn(&str) -> Error) -> Result<u8, Error> {
    text.parse::<u8>()
        .ok()
        .filter(|v| *v <= maximum)
        .ok_or_else(|| fail("invalid government rank"))
}

pub(super) fn groups(
    root: &impl ContentView,
    files: &[FileEntry],
    directory: &str,
    report: &mut AdministrationReport,
) -> Result<BTreeMap<String, Option<String>>, Error> {
    let mut result = BTreeMap::new();
    for file in files
        .iter()
        .filter(|f| direct_txt(&f.relative_path, directory))
    {
        let doc = root.read_text(&file.relative_path, ENCODING)?;
        let parsed = doc.parse()?;
        for (group, op, value) in parsed.reader().fields() {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&group))
            };
            equal(op, &fail)?;
            for (key, _, value) in block(&value, &fail)?.fields() {
                // Group metadata consists of scalars and name lists, not definitions.
                if !matches!(value.token(), TextToken::Object { mixed: false, .. })
                    && !value.read_array().is_ok_and(|v| v.is_empty())
                {
                    continue;
                }
                let name = key.read_string();
                if directory == "common/religions" && name == "religious_schools" {
                    continue;
                }
                let parent = group.read_string();
                result
                    .entry(name.clone())
                    .and_modify(|v| *v = None)
                    .or_insert(Some(parent));
                report.rule_sources.insert(
                    format!("{directory}:{name}"),
                    source(&parsed, parsed.field_location(&key)),
                );
            }
        }
    }
    Ok(result)
}

impl AutonomyDraft {
    pub(super) fn affiliations(&self, initial: &InitialWorld) -> Affiliations {
        let mut culture = AffiliationDimension {
            groups: self.cultures.clone(),
            ..Default::default()
        };
        let mut religion = AffiliationDimension {
            groups: self.religions.clone(),
            ..Default::default()
        };
        for country in initial.countries() {
            let entity = initial
                .country_entity(&country.id)
                .expect("registered country");
            let seed = self.countries.get(&country.id).filter(|s| !s.uncertain);
            culture
                .members
                .insert(entity, seed.and(country.primary_culture.clone()));
            religion
                .members
                .insert(entity, seed.and(country.religion.clone()));
            let groups = self.rank(initial, &country.id).and_then(|rank| {
                if self.union_rank == 0 {
                    (rank < 3).then(BTreeSet::new)
                } else if rank < self.union_rank {
                    Some(BTreeSet::new())
                } else {
                    self.cultures
                        .get(country.primary_culture.as_ref()?)?
                        .as_ref()
                        .map(|group| [group.clone()].into_iter().collect())
                }
            });
            culture.acceptance.insert(
                entity,
                AffiliationAcceptance {
                    members: seed.map(|s| s.accepted.clone()),
                    groups,
                },
            );
        }
        for province in initial.provinces() {
            let entity = initial
                .province_entity(province.id)
                .expect("registered province");
            let known = !self.uncertain_provinces.contains(&province.id);
            culture
                .members
                .insert(entity, known.then(|| province.culture.clone()).flatten());
            religion
                .members
                .insert(entity, known.then(|| province.religion.clone()).flatten());
        }
        Affiliations(
            [("culture".into(), culture), ("religion".into(), religion)]
                .into_iter()
                .collect(),
        )
    }

    pub(super) fn load(
        root: &impl ContentView,
        files: &[FileEntry],
        map: &ParsedText<'_>,
        report: &mut AdministrationReport,
    ) -> Result<Self, Error> {
        let mut result = Self {
            cultures: groups(root, files, "common/cultures", report)?,
            religions: groups(root, files, "common/religions", report)?,
            geography: crate::geography::GeographicGroups::load(root, map, report)?,
            reform_ranks: BTreeMap::new(),
            subject_ranks: BTreeMap::new(),
            countries: BTreeMap::new(),
            changes: Vec::new(),
            pending_changes: Vec::new(),
            areas: BTreeSet::new(),
            uncertain_provinces: BTreeSet::new(),
            union_rank: 0,
            emperor_rank: 0,
        };
        for (field, destination) in [
            ("CULTURAL_UNION_MIN_RANK", &mut result.union_rank),
            ("HRE_RANK", &mut result.emperor_rank),
        ] {
            // Older self-authored fixtures do not declare rank rules. Missing input
            // prevents empire inference; the fixed-edition defaults are not substituted.
            if let Some((value, at)) =
                crate::defines::optional_literal(root, files, "NCountry", field)?
            {
                if value % 1_000_000 != 0 || !(1_000_000..=3_000_000).contains(&value) {
                    return Err(Error::message(
                        &at.path,
                        "unsupported cultural union or emperor rank",
                    ));
                }
                *destination = (value / 1_000_000) as u8;
                report.rule_sources.insert(field.into(), at);
            } else {
                *destination = 0;
            }
        }
        let mut subject_definitions = BTreeMap::<String, Option<Vec<(String, String)>>>::new();
        for (directory, property) in [
            ("common/government_reforms", "fixed_rank"),
            ("common/subject_types", "max_government_rank"),
        ] {
            for file in files
                .iter()
                .filter(|f| direct_txt(&f.relative_path, directory))
            {
                let doc = root.read_text(&file.relative_path, ENCODING)?;
                let parsed = doc.parse()?;
                for (key, op, value) in parsed.reader().fields() {
                    let fail = |m: &str| {
                        Error::message(parsed.source(), m)
                            .with_location(parsed.field_location(&key))
                    };
                    equal(op, &fail)?;
                    let reader = block(&value, &fail)?;
                    // Subject files use empty forward declarations before definitions.
                    if directory == "common/subject_types" && reader.fields().next().is_none() {
                        continue;
                    }
                    let mut selected = Vec::new();
                    for (key, op, value) in reader.fields().filter(|(k, _, _)| {
                        k.read_string() == property
                            || directory == "common/subject_types" && k.read_string() == "copy_from"
                    }) {
                        equal(op, &fail)?;
                        let text = value
                            .read_string()
                            .map_err(|_| fail("expected rank property scalar"))?;
                        if key.read_string() == property {
                            integer(&text, 3, &fail)?;
                        }
                        selected.push((key.read_string(), text));
                    }
                    let name = key.read_string();
                    report.rule_sources.insert(
                        format!("{directory}:{name}"),
                        source(&parsed, parsed.field_location(&key)),
                    );
                    if directory == "common/government_reforms" {
                        let rank = match selected.as_slice() {
                            [] => Some(0),
                            [(_, v)] => Some(integer(v, 3, &fail)?),
                            _ => None,
                        };
                        result
                            .reform_ranks
                            .entry(name)
                            .and_modify(|v| *v = None)
                            .or_insert(rank);
                    } else {
                        subject_definitions
                            .entry(name)
                            .and_modify(|v| *v = None)
                            .or_insert(Some(selected));
                    }
                }
            }
        }
        fn subject(
            name: &str,
            definitions: &BTreeMap<String, Option<Vec<(String, String)>>>,
            stack: &mut BTreeSet<String>,
        ) -> Option<u8> {
            if stack.len() >= 64 || !stack.insert(name.into()) {
                return None;
            }
            let mut rank = if name == "default" {
                0
            } else {
                subject("default", definitions, stack)?
            };
            for (key, value) in definitions.get(name)?.as_ref()? {
                rank = if key == "copy_from" {
                    subject(value, definitions, stack)?
                } else {
                    value.parse().ok()?
                };
            }
            stack.remove(name);
            Some(rank)
        }
        for name in subject_definitions.keys() {
            result.subject_ranks.insert(
                name.clone(),
                subject(name, &subject_definitions, &mut BTreeSet::new()),
            );
        }
        Ok(result)
    }

    pub(super) fn country_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        country: &str,
        areas: &BTreeSet<String>,
        report: &mut AdministrationReport,
    ) -> Result<(), Error> {
        self.countries
            .entry(country.into())
            .or_insert_with(|| CountrySeed {
                rank: Some(2),
                ..Default::default()
            });
        self.read_country(parsed, reader, country, areas, report, 0)
    }

    pub(super) fn set_areas(&mut self, areas: &BTreeSet<String>) {
        self.areas.clone_from(areas);
    }

    pub(super) fn province_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        province: u32,
        report: &mut AdministrationReport,
    ) -> Result<(), Error> {
        let mut stack = vec![(reader, 0)];
        while let Some((reader, depth)) = stack.pop() {
            if depth > 64 {
                return Err(Error::message(
                    parsed.source(),
                    "autonomy history nesting exceeds 64",
                ));
            }
            for (key, _, value) in reader.fields() {
                let name = key.read_string();
                if crate::bootstrap::dated_name(&name) {
                    continue;
                }
                if depth > 0
                    && matches!(
                        name.as_str(),
                        "add_local_autonomy"
                            | "owner"
                            | "culture"
                            | "religion"
                            | "add_core"
                            | "remove_core"
                    )
                {
                    self.uncertain_provinces.insert(province);
                    report
                        .pending_province_initialization
                        .entry(province)
                        .or_default()
                        .push(source(parsed, parsed.field_location(&key)));
                } else if let Ok(body) = value.read_object() {
                    stack.push((body, depth + 1));
                }
            }
        }
        Ok(())
    }

    fn read_country(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        country: &str,
        areas: &BTreeSet<String>,
        report: &mut AdministrationReport,
        depth: usize,
    ) -> Result<(), Error> {
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "autonomy history nesting exceeds 64",
            ));
        }
        for (key, op, value) in reader.fields() {
            let name = key.read_string();
            if crate::bootstrap::dated_name(&name) {
                continue;
            }
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            if matches!(
                name.as_str(),
                "government_rank"
                    | "add_accepted_culture"
                    | "remove_accepted_culture"
                    | "add_government_reform"
                    | "remove_government_reform"
            ) {
                equal(op, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected autonomy country scalar"))?;
                let seed = self
                    .countries
                    .get_mut(country)
                    .expect("registered country history");
                match name.as_str() {
                    "government_rank" => seed.rank = Some(integer(&text, 3, &fail)?),
                    "add_accepted_culture" => {
                        seed.accepted.insert(text);
                    }
                    "remove_accepted_culture" => {
                        seed.accepted.remove(&text);
                    }
                    "add_government_reform" => {
                        seed.reforms.insert(text);
                    }
                    "remove_government_reform" => {
                        seed.reforms.remove(&text);
                    }
                    _ => unreachable!(),
                }
                report.rule_sources.insert(
                    format!("history:{country}:{name}"),
                    source(parsed, parsed.field_location(&key)),
                );
            } else if name == "if" {
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                let condition = condition(parsed, body.clone(), true)?;
                match condition.and_then(|v| self.test(&v, None, None)) {
                    Some(true) => {
                        self.read_country(parsed, body, country, areas, report, depth + 1)?
                    }
                    Some(false) => {}
                    None => {
                        self.pending(parsed, body, country, Predicate::Constant(true), report)?
                    }
                }
            } else if name == "every_owned_province" || areas.contains(&name) {
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                let selector = if name == "every_owned_province" {
                    Predicate::Owner(country.into())
                } else {
                    Predicate::Area(name)
                };
                self.read_changes(parsed, body, country, selector, report, depth + 1)?;
            } else if !matches!(
                name.as_str(),
                "limit" | "monarch" | "heir" | "queen" | "monarch_consort"
            ) && value.read_object().is_ok()
            {
                self.pending(
                    parsed,
                    value.read_object().expect("checked object"),
                    country,
                    Predicate::Constant(true),
                    report,
                )?;
            } else if depth > 0 && matches!(name.as_str(), "primary_culture" | "religion") {
                self.countries
                    .get_mut(country)
                    .expect("registered history")
                    .uncertain = true;
            } else if name == "add_local_autonomy" {
                self.pending_changes.push(Predicate::Constant(true));
                report
                    .pending_country_autonomy
                    .entry(country.into())
                    .or_default()
                    .push(source(parsed, parsed.field_location(&key)));
            }
        }
        Ok(())
    }

    fn pending(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        country: &str,
        selector: Predicate,
        report: &mut AdministrationReport,
    ) -> Result<(), Error> {
        let mut pending = vec![(reader, 0, selector)];
        while let Some((reader, depth, selector)) = pending.pop() {
            if depth > 64 {
                return Err(Error::message(
                    parsed.source(),
                    "autonomy history nesting exceeds 64",
                ));
            }
            for (key, _, value) in reader.fields() {
                let name = key.read_string();
                if crate::bootstrap::dated_name(&name)
                    || matches!(
                        name.as_str(),
                        "limit" | "monarch" | "heir" | "queen" | "monarch_consort"
                    )
                {
                    continue;
                }
                if name == "add_local_autonomy" {
                    self.pending_changes.push(selector.clone());
                    report
                        .pending_country_autonomy
                        .entry(country.into())
                        .or_default()
                        .push(source(parsed, parsed.field_location(&key)));
                } else if matches!(
                    name.as_str(),
                    "government_rank"
                        | "add_accepted_culture"
                        | "remove_accepted_culture"
                        | "add_government_reform"
                        | "remove_government_reform"
                        | "primary_culture"
                        | "religion"
                ) {
                    self.countries
                        .get_mut(country)
                        .expect("registered history")
                        .uncertain = true;
                } else if let Ok(body) = value.read_object() {
                    let narrowed = if name == "every_owned_province" {
                        Predicate::And(vec![selector.clone(), Predicate::Owner(country.into())])
                    } else if self.areas.contains(&name) {
                        Predicate::And(vec![selector.clone(), Predicate::Area(name)])
                    } else {
                        selector.clone()
                    };
                    pending.push((body, depth + 1, narrowed));
                }
            }
        }
        Ok(())
    }

    fn read_changes(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        country: &str,
        mut selector: Predicate,
        report: &mut AdministrationReport,
        depth: usize,
    ) -> Result<(), Error> {
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "autonomy history nesting exceeds 64",
            ));
        }
        if reader.fields().any(|(k, _, _)| k.read_string() == "limit") {
            let Some(predicate) = condition(parsed, reader.clone(), false)? else {
                self.pending(parsed, reader, country, selector, report)?;
                return Ok(());
            };
            selector = Predicate::And(vec![selector, predicate]);
        }
        for (key, op, value) in reader.fields() {
            let name = key.read_string();
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            if name == "add_local_autonomy" {
                equal(op, &fail)?;
                let delta = crate::administration::autonomy_delta(&value, &fail)?;
                self.changes.push(Change {
                    predicate: selector.clone(),
                    delta,
                    source: source(parsed, parsed.field_location(&key)),
                });
                report.country_autonomy_operations += 1;
            } else if name == "if" {
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                if body.fields().any(|(k, _, _)| k.read_string() == "limit") {
                    self.read_changes(parsed, body, country, selector.clone(), report, depth + 1)?;
                } else {
                    self.pending(parsed, body, country, selector.clone(), report)?;
                }
            } else if name != "limit"
                && let Ok(body) = value.read_object()
            {
                self.pending(parsed, body, country, selector.clone(), report)?;
            }
        }
        Ok(())
    }

    fn test(&self, predicate: &Predicate, area: Option<&str>, owner: Option<&str>) -> Option<bool> {
        match predicate {
            Predicate::Constant(v) => Some(*v),
            Predicate::Area(v) => area.map(|a| a == v),
            Predicate::Region(v) => Some(self.geography.regions.get(v)?.contains(area?)),
            Predicate::Owner(v) => Some(owner == Some(v)),
            Predicate::Not(v) => self.test(v, area, owner).map(|v| !v),
            Predicate::And(v) | Predicate::Or(v) => {
                let and = matches!(predicate, Predicate::And(_));
                let mut unknown = false;
                for value in v {
                    match self.test(value, area, owner) {
                        Some(v) if v != and => return Some(!and),
                        None => unknown = true,
                        _ => {}
                    }
                }
                if unknown { None } else { Some(and) }
            }
        }
    }

    pub(super) fn rank(&self, initial: &InitialWorld, country: &str) -> Option<u8> {
        let seed = self.countries.get(country)?;
        if seed.uncertain {
            return None;
        }
        let mut rank = seed.rank?;
        let mut fixed_rank = None;
        let entity = initial.country_entity(country)?;
        // Partial histories may declare ranks without a government. When a
        // government is declared, use its resolved selection, including tier replacements.
        let reforms = if initial.country(country)?.government.is_some() {
            initial
                .world()
                .governments()
                .countries
                .get(&entity)?
                .reforms
                .as_ref()?
        } else {
            &seed.reforms
        };
        for reform in reforms {
            let fixed = self.reform_ranks.get(reform).copied().flatten()?;
            if fixed > 0 {
                if fixed_rank.is_some_and(|v| v != fixed) {
                    return None;
                }
                fixed_rank = Some(fixed);
            }
        }
        if let Some(fixed) = fixed_rank {
            rank = fixed;
        }
        for relation in &initial.world().politics().relations {
            if relation.kind == DiplomaticKind::Subject && relation.second == entity {
                let maximum = self
                    .subject_ranks
                    .get(relation.subject_type.as_ref()?)
                    .copied()
                    .flatten()?;
                if maximum > 0 {
                    rank = rank.min(maximum);
                }
            }
        }
        if initial.world().politics().offices.get("imperial_emperor") == Some(&entity) {
            if self.emperor_rank == 0 {
                return None;
            }
            rank = self.emperor_rank;
        }
        (rank > 0).then_some(rank)
    }

    pub(super) fn has_pending(&self, area: Option<&str>, owner: Option<&str>) -> bool {
        self.pending_changes
            .iter()
            .any(|p| self.test(p, area, owner) != Some(false))
    }

    pub(super) fn generate(
        &self,
        initial: &InitialWorld,
        province: &ProvinceState,
        history_known: bool,
        area: Option<&str>,
        direct: &[i64],
        report: &mut AdministrationReport,
    ) -> Option<i64> {
        if !history_known || self.uncertain_provinces.contains(&province.id) {
            return None;
        }
        let mut raw = if let Some(owner) = province.owner.as_deref() {
            let country = initial.country(owner)?;
            let seed = self.countries.get(owner)?;
            let rank = self.rank(initial, owner)?;
            report.initial_country_ranks.insert(owner.into(), rank);
            let culture = province.culture.as_ref()?;
            let primary = country.primary_culture.as_ref()?;
            if culture != primary
                && !seed.accepted.contains(culture)
                && rank >= 3
                && self.union_rank == 0
            {
                return None;
            }
            let same_group = if culture != primary
                && !seed.accepted.contains(culture)
                && self.union_rank != 0
                && rank >= self.union_rank
            {
                self.cultures.get(culture)?.as_ref()? == self.cultures.get(primary)?.as_ref()?
            } else {
                false
            };
            let cultural = if culture == primary {
                0
            } else if seed.accepted.contains(culture) || same_group {
                50_000
            } else {
                100_000
            };
            let religion = province.religion.as_ref()?;
            let state_religion = country.religion.as_ref()?;
            let religious = if religion == state_religion
                || self.religions.get(religion)?.as_ref()?
                    == self.religions.get(state_religion)?.as_ref()?
            {
                0
            } else {
                100_000
            };
            let foreign = province
                .cores
                .iter()
                .filter(|core| core.as_str() != owner)
                .count() as i64
                * 20_000;
            let value = cultural + religious + foreign;
            if rank >= 2 { value / 2 } else { value }
        } else {
            0
        };
        raw = raw.clamp(0, 1_000_000);
        for &delta in direct {
            raw = adjust(raw, delta);
        }
        if self.has_pending(area, province.owner.as_deref()) {
            return None;
        }
        let mut applied = Vec::new();
        for change in &self.changes {
            if self.test(&change.predicate, area, province.owner.as_deref())? {
                raw = adjust(raw, change.delta);
                applied.push(change.source.clone());
            }
        }
        if !applied.is_empty() {
            report.applied_country_autonomy.insert(province.id, applied);
        }
        Some(raw)
    }
}

fn adjust(raw: i64, delta: i64) -> i64 {
    (i128::from(raw) + i128::from(delta)).clamp(0, 1_000_000) as i64
}

fn condition(
    parsed: &ParsedText<'_>,
    reader: Reader<'_, '_>,
    country: bool,
) -> Result<Option<Predicate>, Error> {
    condition_named(parsed, reader, country, "limit")
}

fn condition_named(
    parsed: &ParsedText<'_>,
    reader: Reader<'_, '_>,
    country: bool,
    field: &str,
) -> Result<Option<Predicate>, Error> {
    let fields: Vec<_> = reader
        .fields()
        .filter(|(k, _, _)| k.read_string() == field)
        .collect();
    if fields.len() != 1 {
        return Ok(None);
    }
    let (key, op, value) = &fields[0];
    let fail =
        |m: &str| Error::message(parsed.source(), m).with_location(parsed.field_location(key));
    equal(*op, &fail)?;
    fn parse(
        parsed: &ParsedText<'_>,
        reader: Reader<'_, '_>,
        country: bool,
        depth: usize,
    ) -> Result<Option<Predicate>, Error> {
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "autonomy condition nesting exceeds 64",
            ));
        }
        let mut terms = Vec::new();
        for (key, op, value) in reader.fields() {
            let fail = |m: &str| {
                Error::message(parsed.source(), m).with_location(parsed.field_location(&key))
            };
            equal(op, &fail)?;
            let name = key.read_string();
            let term = match name.as_str() {
                "always" => match value.read_string().ok().as_deref() {
                    Some("yes") => Predicate::Constant(true),
                    Some("no") => Predicate::Constant(false),
                    _ => return Err(fail("expected yes or no condition")),
                },
                "has_dlc" => {
                    let name = value
                        .read_string()
                        .map_err(|_| fail("expected expansion name"))?;
                    if !crate::is_known_dlc(&name) {
                        return Ok(None);
                    }
                    Predicate::Constant(true)
                }
                "area" | "region" if !country => {
                    let text = value
                        .read_string()
                        .map_err(|_| fail("expected geographic identifier"))?;
                    if name == "area" {
                        Predicate::Area(text)
                    } else {
                        Predicate::Region(text)
                    }
                }
                "AND" | "and" | "OR" | "or" | "NOT" | "not" => {
                    let Some(Predicate::And(terms)) =
                        parse(parsed, block(&value, &fail)?, country, depth + 1)?
                    else {
                        return Ok(None);
                    };
                    match name.as_str() {
                        "AND" | "and" => Predicate::And(terms),
                        "OR" | "or" => Predicate::Or(terms),
                        _ if terms.len() == 1 => Predicate::Not(Box::new(Predicate::And(terms))),
                        _ => return Ok(None),
                    }
                }
                _ => return Ok(None),
            };
            terms.push(term);
        }
        Ok(Some(Predicate::And(terms)))
    }
    parse(parsed, block(value, &fail)?, country, 0)
}

pub(super) fn constant_condition(
    parsed: &ParsedText<'_>,
    reader: Reader<'_, '_>,
    field: &str,
) -> Result<Option<bool>, Error> {
    fn test(predicate: &Predicate) -> Option<bool> {
        match predicate {
            Predicate::Constant(value) => Some(*value),
            Predicate::Not(value) => test(value).map(|v| !v),
            Predicate::And(values) | Predicate::Or(values) => {
                let and = matches!(predicate, Predicate::And(_));
                let values = values.iter().map(test).collect::<Option<Vec<_>>>()?;
                Some(if and {
                    values.into_iter().all(|v| v)
                } else {
                    values.into_iter().any(|v| v)
                })
            }
            _ => None,
        }
    }
    Ok(condition_named(parsed, reader, true, field)?
        .as_ref()
        .and_then(test))
}
