// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::bootstrap::{direct_txt, equal};
use crate::setup::{PropertyDiagnostics, block, flag};
use crate::{ContentView, Error, FileEntry, ParsedText, TextEncoding};

type EstateLookup<'a> = Option<&'a dyn Fn(&str) -> Option<bool>>;
use augustus_core::InitialWorld;
use jomini::text::ObjectReader;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, serde::Serialize)]
pub(super) enum Condition {
    All(Vec<Self>),
    Any(Vec<Self>),
    None(Vec<Self>),
    Literal(bool),
    Match(String, String),
    Status(String, bool),
    Office(String, bool),
    Completion(String),
    Modifier(String),
    Marker(String),
    Numeric(augustus_core::Condition),
    Scoped(augustus_core::Condition),
    Subject(augustus_core::Condition),
    ProvinceCore(augustus_core::Condition),
    Control(augustus_core::CountryController),
    Institution(String, String),
    Estate(String),
    EstatePrivilege(String),
    OwnsProvince(u32),
    If(Box<Self>, Box<Self>),
    Capital(Vec<Self>),
    CapitalProvince(u32),
    Region(String, bool),
    CurrentAge(CurrentAgePredicate),
    MapSetup(String),
    Unknown(#[serde(skip)] String),
}

#[derive(Clone, Copy, serde::Serialize)]
enum AgeCanStart {
    Always,
    Never,
    Conditional,
}

#[derive(Clone, Copy, serde::Serialize)]
struct AgeRule {
    start_year: i16,
    can_start: AgeCanStart,
}

#[derive(Clone, Default)]
pub(crate) struct AgeRules {
    ages: BTreeMap<String, Option<AgeRule>>,
}

#[derive(Clone, Copy)]
pub(super) struct ConditionCompileOptions<'a> {
    library: Option<&'a BTreeMap<String, Condition>>,
    age_rules: Option<&'a AgeRules>,
    skip_limit: bool,
}

impl<'a> ConditionCompileOptions<'a> {
    pub(super) fn new(
        library: Option<&'a BTreeMap<String, Condition>>,
        age_rules: Option<&'a AgeRules>,
        skip_limit: bool,
    ) -> Self {
        Self {
            library,
            age_rules,
            skip_limit,
        }
    }

    fn with_skip_limit(self, skip_limit: bool) -> Self {
        Self { skip_limit, ..self }
    }
}

#[derive(Clone, serde::Serialize)]
pub(super) struct CurrentAgePredicate {
    name: String,
    ages: BTreeMap<String, Option<AgeRule>>,
}

impl AgeRules {
    pub(crate) fn load(root: &impl ContentView, files: &[FileEntry]) -> Result<Self, Error> {
        let mut rules = BTreeMap::new();
        for file in files
            .iter()
            .filter(|file| direct_txt(&file.relative_path, "common/ages"))
        {
            let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
            let parsed = document.parse()?;
            for (key, operator, value) in parsed.reader().fields() {
                let name = key.read_string();
                if name.starts_with('@') {
                    continue;
                }
                let at = parsed.field_location(&key);
                let fail =
                    |message: &str| Error::message(parsed.source(), message).with_location(at);
                equal(operator, &fail)?;
                let mut start_year = None;
                let mut can_start = AgeCanStart::Conditional;
                let mut has_can_start = false;
                let mut invalid = false;
                for (field, operator, value) in block(&value, &fail)?.fields() {
                    let field_name = field.read_string();
                    match field_name.as_str() {
                        "start" if start_year.is_none() => {
                            equal(operator, &fail)?;
                            start_year = value
                                .read_string()
                                .ok()
                                .and_then(|text| text.parse::<i16>().ok())
                                .filter(|year| *year > 0);
                            invalid |= start_year.is_none();
                        }
                        "start" => invalid = true,
                        "can_start" if !has_can_start => {
                            equal(operator, &fail)?;
                            has_can_start = true;
                            let conditions: Vec<_> = block(&value, &fail)?.fields().collect();
                            can_start = match conditions.as_slice() {
                                [(condition, operator, value)]
                                    if condition.read_string() == "always"
                                        && operator.is_none() =>
                                {
                                    match value.read_string().ok().as_deref() {
                                        Some("yes") => AgeCanStart::Always,
                                        Some("no") => AgeCanStart::Never,
                                        _ => AgeCanStart::Conditional,
                                    }
                                }
                                _ => AgeCanStart::Conditional,
                            };
                        }
                        "can_start" => invalid = true,
                        _ => (),
                    }
                }
                let rule = if invalid {
                    None
                } else {
                    start_year.map(|start_year| AgeRule {
                        start_year,
                        can_start,
                    })
                };
                match rules.entry(name) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(rule);
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.insert(None);
                    }
                }
            }
        }
        Ok(Self { ages: rules })
    }

    fn predicate(&self, name: &str) -> Option<CurrentAgePredicate> {
        self.ages.get(name)?.as_ref()?;
        Some(CurrentAgePredicate {
            name: name.into(),
            ages: self.ages.clone(),
        })
    }
}

impl CurrentAgePredicate {
    fn evaluate(&self, year: i16) -> Option<bool> {
        let target = self.ages.get(&self.name)?.as_ref()?;
        if year < target.start_year {
            return Some(false);
        }

        let later_ages: Vec<_> = self
            .ages
            .iter()
            .filter_map(|(name, rule)| {
                rule.as_ref()
                    .filter(|rule| rule.start_year >= target.start_year && name != &self.name)
                    .map(|rule| (name, rule))
            })
            .collect();
        if later_ages
            .iter()
            .any(|(_, rule)| rule.start_year == target.start_year)
        {
            return None;
        }
        if later_ages.iter().any(|(_, rule)| {
            rule.start_year <= year && matches!(rule.can_start, AgeCanStart::Always)
        }) {
            return Some(false);
        }
        match target.can_start {
            AgeCanStart::Never => return Some(false),
            AgeCanStart::Conditional => return None,
            AgeCanStart::Always => (),
        }
        if self.ages.values().any(Option::is_none) {
            return None;
        }
        if later_ages.iter().any(|(_, rule)| {
            rule.start_year <= year && matches!(rule.can_start, AgeCanStart::Conditional)
        }) {
            return None;
        }
        Some(true)
    }
}

impl Condition {
    pub(super) fn collect_global_marker_references(&self, names: &mut BTreeSet<String>) {
        match self {
            Self::All(conditions) | Self::Any(conditions) | Self::None(conditions) => {
                for condition in conditions {
                    condition.collect_global_marker_references(names);
                }
            }
            Self::Numeric(condition)
            | Self::Scoped(condition)
            | Self::Subject(condition)
            | Self::ProvinceCore(condition) => {
                names.extend(condition.global_marker_references());
            }
            Self::If(condition, body) => {
                condition.collect_global_marker_references(names);
                body.collect_global_marker_references(names);
            }
            Self::Capital(conditions) => {
                for condition in conditions {
                    condition.collect_global_marker_references(names);
                }
            }
            _ => (),
        }
    }

    pub(super) fn compile(
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        report: &mut impl PropertyDiagnostics,
        depth: usize,
        country: bool,
        library: Option<&BTreeMap<String, Condition>>,
        skip_limit: bool,
    ) -> Result<Vec<Self>, Error> {
        Self::compile_with_age_rules(
            parsed,
            reader,
            report,
            depth,
            country,
            ConditionCompileOptions::new(library, None, skip_limit),
        )
    }

    pub(super) fn compile_with_age_rules(
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        report: &mut impl PropertyDiagnostics,
        depth: usize,
        country: bool,
        options: ConditionCompileOptions<'_>,
    ) -> Result<Vec<Self>, Error> {
        let ConditionCompileOptions {
            library,
            age_rules,
            skip_limit,
        } = options;
        if depth > 64 {
            return Err(Error::message(
                parsed.source(),
                "country condition nesting exceeds limit",
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
                    "is_subject_of"
                    | "is_subject"
                    | "truce_with"
                    | "has_truce"
                    | "manpower"
                    | "prestige"
                    | "treasury"
                    | "is_subject_of_type"
                    | "any_subject_country"
                    | "is_core"
                    | "is_territorial_core"
                    | "has_port"
                    | "has_terrain" => {
                        let schema =
                            augustus_core::Schema::new(Vec::<String>::new()).expect("empty schema");
                        let library = crate::ScriptLibrary::new();
                        let compiler = crate::ScriptCompiler::new(&schema, &library, false)
                            .with_treasury_slot(Some(augustus_core::NumericSlot::Resource(
                                augustus_core::CountryResource::Treasury,
                            )))
                            .with_root_scope(augustus_core::EntityKind::Country)
                            .with_metric_ids(
                                [
                                    "stability",
                                    "prestige",
                                    "adm_power",
                                    "dip_power",
                                    "mil_power",
                                ]
                                .map(String::from)
                                .into(),
                            )
                            .with_unbound_subject_types();
                        let scope = if country {
                            augustus_core::EntityKind::Country
                        } else {
                            augustus_core::EntityKind::Province
                        };
                        let compiled = if name == "any_subject_country" {
                            compiler.compile_block_condition_reader(
                                parsed,
                                &name,
                                block(&value, &fail)?,
                                at,
                                scope,
                            )
                        } else {
                            compiler.compile_scalar_condition_reader(
                                parsed,
                                &name,
                                value
                                    .read_string()
                                    .map_err(|_| fail("expected subject type identifier"))?,
                                at,
                                scope,
                            )
                        };
                        match compiled {
                            Ok(condition) => {
                                if matches!(name.as_str(), "is_core" | "is_territorial_core") {
                                    Self::ProvinceCore(condition)
                                } else if matches!(name.as_str(), "has_port" | "has_terrain") {
                                    Self::Scoped(condition)
                                } else {
                                    Self::Subject(condition)
                                }
                            }
                            Err(error) => {
                                report.unsupported(parsed, "trigger", &name, at);
                                Self::Unknown(error.to_string())
                            }
                        }
                    }
                    "exists" | "has_global_flag" | "is_part_of_hre" | "is_year" => {
                        let schema =
                            augustus_core::Schema::new(Vec::<String>::new()).expect("empty schema");
                        let library = crate::ScriptLibrary::new();
                        match crate::ScriptCompiler::new(&schema, &library, true)
                            .compile_scalar_condition_reader(
                                parsed,
                                &name,
                                value
                                    .read_string()
                                    .map_err(|_| fail("expected scalar condition operand"))?,
                                at,
                                if country {
                                    augustus_core::EntityKind::Country
                                } else {
                                    augustus_core::EntityKind::Province
                                },
                            ) {
                            Ok(condition) => Self::Scoped(condition),
                            Err(error) => {
                                report.unsupported(parsed, "trigger", &name, at);
                                Self::Unknown(error.to_string())
                            }
                        }
                    }
                    "ai" if country => {
                        let expected =
                            flag(&value.read_string().map_err(|_| fail("expected ai flag"))?)
                                .ok_or_else(|| fail("expected yes or no"))?;
                        Self::Control(if expected {
                            augustus_core::CountryController::Ai
                        } else {
                            augustus_core::CountryController::Human
                        })
                    }
                    "if" if country => {
                        let body = block(&value, &fail)?;
                        let limits: Vec<_> = body
                            .fields()
                            .filter(|(k, _, _)| k.read_string() == "limit")
                            .collect();
                        if limits.len() != 1 {
                            return Err(fail("conditional trigger requires one limit"));
                        }
                        equal(limits[0].1, &fail)?;
                        let limit = Self::All(Self::compile_with_age_rules(
                            parsed,
                            block(&limits[0].2, &fail)?,
                            report,
                            depth + 1,
                            country,
                            options.with_skip_limit(false),
                        )?);
                        let children = Self::All(Self::compile_with_age_rules(
                            parsed,
                            body,
                            report,
                            depth + 1,
                            country,
                            options.with_skip_limit(true),
                        )?);
                        Self::If(Box::new(limit), Box::new(children))
                    }
                    "limit" if skip_limit => Self::Literal(true),
                    "has_dlc" => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected expansion name"))?;
                        if crate::is_known_dlc(&id) {
                            Self::Literal(true)
                        } else {
                            report.unsupported(parsed, "trigger", &name, at);
                            Self::Unknown(name)
                        }
                    }
                    "current_age" if country => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected age identifier"))?;
                        if id.is_empty() {
                            return Err(fail("empty age identifier"));
                        }
                        match age_rules.and_then(|rules| rules.predicate(&id)) {
                            Some(predicate) => Self::CurrentAge(predicate),
                            None => {
                                report.unsupported(parsed, "trigger", &name, at);
                                Self::Unknown(format!("current_age:{id}"))
                            }
                        }
                    }
                    "map_setup" => {
                        let setup = value
                            .read_string()
                            .map_err(|_| fail("expected map setup identifier"))?;
                        if setup.is_empty() {
                            return Err(fail("empty map setup identifier"));
                        }
                        if setup == "map_setup_random" {
                            Self::MapSetup(setup)
                        } else {
                            report.unsupported(parsed, "trigger", &name, at);
                            Self::Unknown(format!("map_setup:{setup}"))
                        }
                    }
                    "not_in_mission_preview_mode" if country => {
                        let arguments = block(&value, &fail)?.fields().collect::<Vec<_>>();
                        if arguments.len() != 1 || arguments[0].0.read_string() != "key" {
                            return Err(fail("mission preview trigger requires exactly one key"));
                        }
                        equal(arguments[0].1, &fail)?;
                        let key = arguments[0]
                            .2
                            .read_string()
                            .map_err(|_| fail("expected mission preview key"))?;
                        if key.is_empty()
                            || key.chars().any(|character| matches!(character, '$' | '@'))
                        {
                            return Err(fail("mission preview key must be a literal identifier"));
                        }
                        // Augustus evaluates live game state, where mission preview mode is inactive.
                        Self::Literal(true)
                    }
                    "has_mission" if country => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected mission identifier"))?;
                        if id.is_empty() {
                            return Err(fail("empty mission identifier"));
                        }
                        Self::Scoped(augustus_core::Condition::ObjectiveAvailable {
                            target: augustus_core::Target::Current,
                            id,
                        })
                    }
                    "mission_completed" if country => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected mission identifier"))?;
                        if id.is_empty() {
                            return Err(fail("empty mission identifier"));
                        }
                        Self::Completion(id)
                    }
                    "has_country_modifier" if country => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected modifier name"))?;
                        if id.is_empty() {
                            return Err(fail("empty modifier name"));
                        }
                        Self::Modifier(id)
                    }
                    "has_country_flag" if country => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected country flag name"))?;
                        if id.is_empty() {
                            return Err(fail("empty country flag name"));
                        }
                        Self::Marker(id)
                    }
                    "legitimacy"
                    | "republican_tradition"
                    | "devotion"
                    | "government_rank"
                    | "stability"
                    | "liberty_desire"
                    | "adm_power"
                    | "dip_power"
                    | "mil_power"
                    | "adm"
                    | "dip"
                    | "mil"
                    | "monthly_adm"
                    | "monthly_dip"
                    | "monthly_mil"
                    | "national_focus"
                        if country =>
                    {
                        let schema =
                            augustus_core::Schema::new(Vec::<String>::new()).expect("empty schema");
                        let library = crate::ScriptLibrary::new();
                        let compiler = crate::ScriptCompiler::new(&schema, &library, true);
                        let compiler = if matches!(
                            name.as_str(),
                            "stability" | "prestige" | "adm_power" | "dip_power" | "mil_power"
                        ) {
                            compiler.with_metric_ids([name.clone()].into())
                        } else if matches!(
                            name.as_str(),
                            "legitimacy" | "republican_tradition" | "devotion"
                        ) {
                            compiler.with_government_metric_ids([name.clone()].into())
                        } else {
                            compiler
                        };
                        let compiled = compiler.compile_scalar_condition_reader(
                            parsed,
                            &name,
                            value
                                .read_string()
                                .map_err(|_| fail("expected scalar country metric threshold"))?,
                            at,
                            augustus_core::EntityKind::Country,
                        );
                        match compiled {
                            Ok(condition) => Self::Numeric(condition),
                            Err(error) => {
                                report.unsupported(parsed, "trigger", &name, at);
                                Self::Unknown(error.to_string())
                            }
                        }
                    }
                    "check_variable" | "is_variable_equal" => {
                        let schema =
                            augustus_core::Schema::new(Vec::<String>::new()).expect("empty schema");
                        let library = crate::ScriptLibrary::new();
                        match crate::ScriptCompiler::new(&schema, &library, true)
                            .compile_block_condition_reader(
                                parsed,
                                &name,
                                block(&value, &fail)?,
                                at,
                                if country {
                                    augustus_core::EntityKind::Country
                                } else {
                                    augustus_core::EntityKind::Province
                                },
                            ) {
                            Ok(condition) => Self::Numeric(condition),
                            Err(error) => {
                                report.unsupported(parsed, "trigger", &name, at);
                                Self::Unknown(error.to_string())
                            }
                        }
                    }
                    "has_estate" if country => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected estate identifier"))?;
                        if id.is_empty() {
                            return Err(fail("empty estate identifier"));
                        }
                        Self::Estate(id)
                    }
                    "has_estate_privilege" if country => {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected estate privilege identifier"))?;
                        if id.is_empty() {
                            return Err(fail("empty estate privilege identifier"));
                        }
                        Self::EstatePrivilege(id)
                    }
                    "accepted_culture" if country => {
                        let culture = value
                            .read_string()
                            .map_err(|_| fail("expected accepted culture identifier"))?;
                        if culture.is_empty() {
                            return Err(fail("empty accepted culture identifier"));
                        }
                        Self::Scoped(augustus_core::Condition::AffiliationAcceptsMember {
                            target: augustus_core::Target::Current,
                            dimension: "culture".into(),
                            member: culture,
                        })
                    }
                    "owns" if country => {
                        let province = value
                            .read_string()
                            .ok()
                            .and_then(|value| value.parse::<u32>().ok())
                            .filter(|province| *province > 0)
                            .ok_or_else(|| fail("owns requires a positive province id"))?;
                        Self::OwnsProvince(province)
                    }
                    "any_owned_province" if country => {
                        let schema =
                            augustus_core::Schema::new(Vec::<String>::new()).expect("empty schema");
                        let library = crate::ScriptLibrary::new();
                        let compiler = crate::ScriptCompiler::new(&schema, &library, true)
                            .with_root_scope(augustus_core::EntityKind::Country)
                            .with_metric_ids(
                                [
                                    "stability",
                                    "prestige",
                                    "adm_power",
                                    "dip_power",
                                    "mil_power",
                                    "devastation",
                                ]
                                .map(String::from)
                                .into(),
                            )
                            .with_government_metric_ids(
                                ["legitimacy", "republican_tradition", "devotion"]
                                    .map(String::from)
                                    .into(),
                            )
                            .with_unbound_subject_types();
                        match compiler.compile_block_condition_reader(
                            parsed,
                            &name,
                            block(&value, &fail)?,
                            at,
                            augustus_core::EntityKind::Country,
                        ) {
                            Ok(condition) => Self::Scoped(condition),
                            Err(error) => {
                                report.unsupported(parsed, "trigger", &name, at);
                                Self::Unknown(error.to_string())
                            }
                        }
                    }
                    "capital" if country => {
                        let province = value
                            .read_string()
                            .ok()
                            .and_then(|value| value.parse::<u32>().ok())
                            .filter(|province| *province > 0)
                            .ok_or_else(|| fail("capital requires a positive province id"))?;
                        Self::CapitalProvince(province)
                    }
                    "is_emperor" | "is_emperor_of_china" if country => {
                        let expected = flag(
                            &value
                                .read_string()
                                .map_err(|_| fail("expected office holder flag"))?,
                        )
                        .ok_or_else(|| fail("expected yes or no"))?;
                        let office = if name == "is_emperor" {
                            "imperial_emperor"
                        } else {
                            "celestial_emperor"
                        };
                        Self::Office(office.into(), expected)
                    }
                    "is_nomad" if country => {
                        let expected = flag(
                            &value
                                .read_string()
                                .map_err(|_| fail("expected nomad flag"))?,
                        )
                        .ok_or_else(|| fail("expected yes or no"))?;
                        let feature =
                            Self::Institution("has_government_attribute".into(), "nomad".into());
                        if expected {
                            feature
                        } else {
                            Self::None(vec![feature])
                        }
                    }
                    "AND" | "and" | "OR" | "or" | "NOT" | "not" => {
                        let children = Self::compile_with_age_rules(
                            parsed,
                            block(&value, &fail)?,
                            report,
                            depth + 1,
                            country,
                            options.with_skip_limit(false),
                        )?;
                        match name.as_str() {
                            "AND" | "and" => Self::All(children),
                            "OR" | "or" => Self::Any(children),
                            _ => Self::None(children),
                        }
                    }
                    "capital_scope" if country => Self::Capital(Self::compile_with_age_rules(
                        parsed,
                        block(&value, &fail)?,
                        report,
                        depth + 1,
                        false,
                        options.with_skip_limit(false),
                    )?),
                    "owner" if !country => {
                        let schema =
                            augustus_core::Schema::new(Vec::<String>::new()).expect("empty schema");
                        let library = crate::ScriptLibrary::new();
                        match crate::ScriptCompiler::new(&schema, &library, true)
                            .with_root_scope(augustus_core::EntityKind::Country)
                            .with_metric_ids(
                                ["stability", "adm_power", "dip_power", "mil_power"]
                                    .map(String::from)
                                    .into(),
                            )
                            .compile_block_condition_reader(
                                parsed,
                                &name,
                                block(&value, &fail)?,
                                at,
                                augustus_core::EntityKind::Province,
                            ) {
                            Ok(condition) => Self::Scoped(condition),
                            Err(error) => {
                                report.unsupported(parsed, "trigger", &name, at);
                                Self::Unknown(error.to_string())
                            }
                        }
                    }
                    _ if (crate::script::country_identifier(&name)
                        || matches!(name.as_str(), "ROOT" | "root"))
                        && value.read_object().is_ok() =>
                    {
                        let schema =
                            augustus_core::Schema::new(Vec::<String>::new()).expect("empty schema");
                        let library = crate::ScriptLibrary::new();
                        match crate::ScriptCompiler::new(&schema, &library, true)
                            .with_root_scope(augustus_core::EntityKind::Country)
                            .with_metric_ids(
                                ["stability", "adm_power", "dip_power", "mil_power"]
                                    .map(String::from)
                                    .into(),
                            )
                            .with_unbound_subject_types()
                            .compile_block_condition_reader(
                                parsed,
                                &name,
                                block(&value, &fail)?,
                                at,
                                if country {
                                    augustus_core::EntityKind::Country
                                } else {
                                    augustus_core::EntityKind::Province
                                },
                            ) {
                            Ok(condition) => Self::Scoped(condition),
                            Err(error) => {
                                report.unsupported(parsed, "trigger", &name, at);
                                Self::Unknown(error.to_string())
                            }
                        }
                    }
                    "area" | "continent" if !country => {
                        let schema =
                            augustus_core::Schema::new(Vec::<String>::new()).expect("empty schema");
                        let library = crate::ScriptLibrary::new();
                        match crate::ScriptCompiler::new(&schema, &library, true)
                            .compile_scalar_condition_reader(
                                parsed,
                                &name,
                                value
                                    .read_string()
                                    .map_err(|_| fail("expected geographic identifier"))?,
                                at,
                                augustus_core::EntityKind::Province,
                            ) {
                            Ok(condition) => Self::Scoped(condition),
                            Err(error) => {
                                report.unsupported(parsed, "trigger", &name, at);
                                Self::Unknown(error.to_string())
                            }
                        }
                    }
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
                    "government"
                    | "has_reform"
                    | "has_government_attribute"
                    | "has_government_mechanic"
                        if country =>
                    {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected government identifier"))?;
                        if id.is_empty() {
                            return Err(fail("empty government identifier"));
                        }
                        if !id.contains('$')
                            && let Err(message) = crate::script::government_condition(&name, &id)
                        {
                            report.unsupported(parsed, "trigger", &name, at);
                            Self::Unknown(message.into())
                        } else {
                            Self::Institution(name, id)
                        }
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
                    | "religion_group" | "technology_group" | "unit_type" | "dynasty"
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
                    "was_tag" if country => {
                        let tag = value
                            .read_string()
                            .map_err(|_| fail("expected historical country identifier"))?;
                        let parameter = tag
                            .strip_prefix('$')
                            .and_then(|value| value.strip_suffix('$'))
                            .is_some_and(|name| {
                                !name.is_empty()
                                    && name
                                        .bytes()
                                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
                            });
                        if !crate::script::country_identifier(&tag) && !parameter {
                            return Err(fail(
                                "was_tag requires a literal three-character country identifier",
                            ));
                        }
                        Self::Literal(false)
                    }
                    _ => {
                        if country && let Some(template) = library.and_then(|l| l.get(&name)) {
                            let mut expanded = template.clone();
                            if let Ok(body) = value.read_object() {
                                let mut params = BTreeMap::new();
                                for (k, op, v) in body.fields() {
                                    equal(op, &fail)?;
                                    if params
                                        .insert(
                                            k.read_string(),
                                            v.read_string().map_err(|_| {
                                                fail("expected scripted condition parameter")
                                            })?,
                                        )
                                        .is_some()
                                    {
                                        return Err(fail("duplicate scripted condition parameter"));
                                    }
                                }
                                expanded.substitute(&params);
                                return Ok(expanded);
                            }
                            let expected = flag(
                                &value
                                    .read_string()
                                    .map_err(|_| fail("expected scripted condition flag"))?,
                            )
                            .ok_or_else(|| fail("expected yes or no"))?;
                            expanded.substitute(&BTreeMap::new());
                            return Ok(if expected {
                                expanded
                            } else {
                                Self::None(vec![expanded])
                            });
                        }
                        report.unsupported(parsed, "trigger", &name, at);
                        Self::Unknown(name)
                    }
                })
            })
            .collect()
    }

    pub(super) fn evaluate(
        &self,
        initial: &InitialWorld,
        tag: &str,
        reasons: &mut BTreeSet<String>,
        province: Option<augustus_core::EntityId>,
        estates: EstateLookup<'_>,
    ) -> Option<bool> {
        self.evaluate_with_root(
            initial,
            tag,
            reasons,
            province,
            estates,
            initial.country_entity(tag).expect("registered country"),
        )
    }

    pub(super) fn evaluate_with_root(
        &self,
        initial: &InitialWorld,
        tag: &str,
        reasons: &mut BTreeSet<String>,
        province: Option<augustus_core::EntityId>,
        estates: EstateLookup<'_>,
        root: augustus_core::EntityId,
    ) -> Option<bool> {
        match self {
            Self::Subject(condition) | Self::ProvinceCore(condition) => {
                let entity = province
                    .unwrap_or_else(|| initial.country_entity(tag).expect("registered country"));
                match initial
                    .world()
                    .validate_condition(condition)
                    .and_then(|()| {
                        condition.evaluate_in_context(
                            initial.world(),
                            augustus_core::ScopeContext {
                                current: entity,
                                root,
                            },
                        )
                    }) {
                    Ok(value) => Some(value),
                    Err(error) => {
                        let kind = if matches!(self, Self::ProvinceCore(_)) {
                            "core"
                        } else {
                            "subject"
                        };
                        reasons.insert(format!("unknown_{kind}_condition:{error}"));
                        None
                    }
                }
            }
            Self::Control(expected) => {
                let entity = province
                    .unwrap_or_else(|| initial.country_entity(tag).expect("registered country"));
                match initial.world().country_controller(entity) {
                    Ok(Some(actual)) => Some(actual == *expected),
                    Ok(None) => {
                        reasons.insert(format!("unknown_country_controller:{tag}"));
                        None
                    }
                    Err(error) => {
                        reasons.insert(format!("invalid_country_controller:{error}"));
                        None
                    }
                }
            }
            Self::Marker(name) => {
                if !crate::country_markers::literal_name(name) {
                    reasons.insert("unsupported_country_flag_name".into());
                    return None;
                }
                let entity = initial.country_entity(tag).expect("registered country");
                match initial.world().has_marker(entity, name) {
                    Ok(value) => Some(value),
                    Err(_) => {
                        reasons.insert("unknown_country_flags".into());
                        None
                    }
                }
            }
            Self::Modifier(name) => {
                let entity = initial.country_entity(tag).expect("registered country");
                if !initial.world().modifiers().definitions.contains_key(name) {
                    return Some(false);
                }
                match initial.world().has_modifier(entity, name) {
                    Ok(value) => Some(value),
                    Err(error) => {
                        reasons.insert(format!("unknown_modifier_state:{error}"));
                        None
                    }
                }
            }
            Self::Numeric(condition) | Self::Scoped(condition) => {
                let entity = province
                    .unwrap_or_else(|| initial.country_entity(tag).expect("registered country"));
                match condition.evaluate_in_context(
                    initial.world(),
                    augustus_core::ScopeContext {
                        current: entity,
                        root,
                    },
                ) {
                    Ok(value) => Some(value),
                    Err(error) => {
                        let diagnostic = match self {
                            Self::Scoped(augustus_core::Condition::AffiliationAcceptsMember {
                                dimension,
                                member,
                                ..
                            }) if dimension == "culture" => {
                                format!("unknown_culture_acceptance:{member}")
                            }
                            _ => {
                                let kind = if matches!(self, Self::Scoped(_)) {
                                    "unknown_scoped_condition"
                                } else {
                                    "unknown_variable_state"
                                };
                                format!("{kind}:{error}")
                            }
                        };
                        reasons.insert(diagnostic);
                        None
                    }
                }
            }
            Self::Completion(id) => {
                let entity = initial.country_entity(tag).expect("registered country");
                let result = initial.world().completion_recorded(entity, id).ok();
                if result.is_none() {
                    reasons.insert(format!("unknown_mission_state:{id}"));
                }
                result
            }
            Self::CapitalProvince(province_id) => {
                let Some(expected) = initial.province_entity(*province_id) else {
                    reasons.insert(format!("unknown_capital_province:{province_id}"));
                    return None;
                };
                let country = initial.country_entity(tag).expect("registered country");
                match initial.world().entity(country) {
                    Ok(country) => match country.capital {
                        Some(capital) => Some(capital == expected),
                        None => {
                            reasons.insert("unknown_capital".into());
                            None
                        }
                    },
                    Err(error) => {
                        reasons.insert(format!("invalid_capital_state:{error}"));
                        None
                    }
                }
            }
            Self::OwnsProvince(province_id) => {
                let Some(province) = initial.province_entity(*province_id) else {
                    reasons.insert(format!("unknown_owned_province:{province_id}"));
                    return None;
                };
                let country = initial.country_entity(tag).expect("registered country");
                match initial.world().entity(province) {
                    Ok(province) => match province.owner {
                        Some(owner) => Some(owner == country),
                        None => match initial.world().owned_provinces(country) {
                            Ok(_) => Some(false),
                            Err(error) => {
                                reasons.insert(format!("unknown_province_ownership:{error}"));
                                None
                            }
                        },
                    },
                    Err(error) => {
                        reasons.insert(format!("invalid_owned_province_state:{error}"));
                        None
                    }
                }
            }
            Self::Office(name, expected) => {
                let entity = initial.country_entity(tag).expect("registered country");
                let predicate = augustus_core::Condition::ScopeIsCurrent(
                    augustus_core::ScopeSelector::Office(name.clone()),
                );
                predicate
                    .evaluate(initial.world(), entity)
                    .ok()
                    .map(|value| value == *expected)
            }
            Self::EstatePrivilege(id) => {
                let entity = initial.country_entity(tag).expect("registered country");
                let result = initial
                    .world()
                    .estates()
                    .countries
                    .get(&entity)
                    .and_then(|states| {
                        let mut unknown = false;
                        for state in states.values().filter(|s| s.active != Some(false)) {
                            match &state.privileges {
                                Some(ids) if state.active == Some(true) && ids.contains(id) => {
                                    return Some(true);
                                }
                                Some(_) => (),
                                None => unknown = true,
                            }
                            unknown |= state.active.is_none();
                        }
                        (!unknown).then_some(false)
                    });
                if result.is_none() {
                    reasons.insert(format!("unknown_estate_privilege:{id}"));
                }
                result
            }
            Self::If(limit, body) => {
                let mut pending = BTreeSet::new();
                let limit =
                    limit.evaluate_with_root(initial, tag, &mut pending, province, estates, root);
                if limit == Some(false) {
                    return Some(true);
                }
                let value =
                    body.evaluate_with_root(initial, tag, &mut pending, province, estates, root);
                if value == Some(true) {
                    return Some(true);
                }
                if limit.is_none() || value.is_none() {
                    reasons.extend(pending);
                    None
                } else {
                    value
                }
            }
            Self::Estate(id) => {
                let value = estates.and_then(|lookup| lookup(id));
                if value.is_none() {
                    reasons.insert(format!("unknown_estate:{id}"));
                }
                value
            }
            Self::CurrentAge(predicate) => {
                let Some(date) = initial.world().date().ok().flatten() else {
                    reasons.insert(format!("unknown_current_age:{}", predicate.name));
                    return None;
                };
                let value = predicate.evaluate(date.year);
                if value.is_none() {
                    reasons.insert(format!("unknown_current_age:{}", predicate.name));
                }
                value
            }
            Self::MapSetup(setup) => Some(setup != "map_setup_random"),
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
                    match child.evaluate_with_root(
                        initial,
                        tag,
                        &mut pending,
                        province,
                        estates,
                        root,
                    ) {
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
                    match child.evaluate_with_root(
                        initial,
                        tag,
                        &mut pending,
                        capital,
                        estates,
                        root,
                    ) {
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
            Self::Institution(name, value) => {
                let entity = initial.country_entity(tag).expect("registered country");
                match crate::script::government_condition(name, value) {
                    Ok(condition) => match condition.evaluate(initial.world(), entity) {
                        Ok(value) => Some(value),
                        Err(error) => {
                            reasons.insert(format!("unknown_government_condition:{name}:{error}"));
                            None
                        }
                    },
                    Err(message) => {
                        reasons.insert(format!("unknown_government_condition:{name}:{message}"));
                        None
                    }
                }
            }
            Self::Match(name, expected) => {
                let country = initial.country(tag).expect("registered country");
                let entity = initial.country_entity(tag).expect("registered country");
                let world = initial.world();
                let actual = match name.as_str() {
                    "tag" => Some(tag),
                    "primary_culture" => country.primary_culture.as_deref(),
                    "religion" => country.religion.as_deref(),
                    "technology_group" => country.technology_group.as_deref(),
                    "unit_type" => country.unit_type.as_deref(),
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
                    _ => unreachable!(),
                };
                if actual.is_none() {
                    reasons.insert(format!("unknown_input:{name}"));
                }
                actual.map(|a| a == expected)
            }
        }
    }

    fn substitute(&mut self, parameters: &BTreeMap<String, String>) {
        let replace = |text: &mut String| {
            for (name, value) in parameters {
                *text = text.replace(&format!("${name}$"), value);
            }
            !text.contains('$')
        };
        let complete = match self {
            Self::All(children)
            | Self::Any(children)
            | Self::None(children)
            | Self::Capital(children) => {
                for child in children {
                    child.substitute(parameters);
                }
                true
            }
            Self::If(limit, body) => {
                limit.substitute(parameters);
                body.substitute(parameters);
                true
            }
            Self::Numeric(augustus_core::Condition::NumericCompare {
                slot: augustus_core::NumericSlot::Named(name),
                ..
            }) => replace(name),
            Self::Numeric(_) => true,
            Self::Scoped(_) => true,
            Self::Subject(_) => true,
            Self::ProvinceCore(_) => true,
            Self::Match(name, value) | Self::Institution(name, value) => {
                replace(name) && replace(value)
            }
            Self::Completion(id)
            | Self::Modifier(id)
            | Self::Marker(id)
            | Self::Estate(id)
            | Self::EstatePrivilege(id)
            | Self::Region(id, _)
            | Self::MapSetup(id) => replace(id),
            _ => true,
        };
        if !complete {
            *self = Self::Unknown("unbound_script_parameter".into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TextDocument, TextEncoding};

    #[test]
    fn province_conditions_share_the_core_port_query() {
        let document = TextDocument::from_bytes(
            "self-authored-port-condition.txt",
            b"has_port = yes".to_vec(),
            TextEncoding::Utf8,
        );
        let parsed = document.parse().unwrap();
        let mut report = crate::estates::EstateReport::default();
        let conditions =
            Condition::compile(&parsed, parsed.reader(), &mut report, 0, false, None, false)
                .unwrap();

        assert!(matches!(
            conditions.as_slice(),
            [Condition::Scoped(augustus_core::Condition::All(conditions))]
                if matches!(conditions.as_slice(), [augustus_core::Condition::ProvinceHasPort])
        ));
        assert!(!report.unsupported_fields.contains_key("trigger:has_port"));
    }

    #[test]
    fn province_terrain_conditions_share_the_core_terrain_query() {
        let document = TextDocument::from_bytes(
            "self-authored-terrain-condition.txt",
            b"has_terrain = steppe".to_vec(),
            TextEncoding::Utf8,
        );
        let parsed = document.parse().unwrap();
        let mut report = crate::estates::EstateReport::default();
        let conditions =
            Condition::compile(&parsed, parsed.reader(), &mut report, 0, false, None, false)
                .unwrap();

        assert!(matches!(
            conditions.as_slice(),
            [Condition::Scoped(augustus_core::Condition::All(conditions))]
                if matches!(conditions.as_slice(), [augustus_core::Condition::ProvinceTerrain {
                    target: augustus_core::Target::Current,
                    terrain,
                }] if terrain == "steppe")
        ));
        assert!(
            !report
                .unsupported_fields
                .contains_key("trigger:has_terrain")
        );
    }

    #[test]
    fn unknown_accepted_culture_keeps_its_estate_diagnostic() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/world");
        let mut initial = crate::VanillaWorld::load(root, "1444.11.11")
            .unwrap()
            .initial;
        let country = initial.country_entity("AAA").unwrap();
        let mut affiliations = initial.world().affiliations().clone();
        affiliations
            .0
            .entry("culture".into())
            .or_default()
            .acceptance
            .insert(
                country,
                augustus_core::AffiliationAcceptance {
                    members: None,
                    groups: None,
                },
            );
        initial.set_affiliations(affiliations).unwrap();

        let condition = Condition::Scoped(augustus_core::Condition::AffiliationAcceptsMember {
            target: augustus_core::Target::Current,
            dimension: "culture".into(),
            member: "beta".into(),
        });
        let mut reasons = BTreeSet::new();
        assert_eq!(
            condition.evaluate(&initial, "AAA", &mut reasons, None, None),
            None
        );
        assert!(reasons.contains("unknown_culture_acceptance:beta"));
    }
}
