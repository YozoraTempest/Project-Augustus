// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::bootstrap::equal;
use crate::setup::{PropertyDiagnostics, block, flag};
use crate::{Error, ParsedText, TextEncoding};

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
    If(Box<Self>, Box<Self>),
    Capital(Vec<Self>),
    Region(String, bool),
    Unknown(#[serde(skip)] String),
}

impl Condition {
    pub(super) fn compile(
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        report: &mut impl PropertyDiagnostics,
        depth: usize,
        country: bool,
        library: Option<&BTreeMap<String, Condition>>,
        skip_limit: bool,
    ) -> Result<Vec<Self>, Error> {
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
                    | "is_territorial_core" => {
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
                    "exists" | "has_global_flag" | "is_part_of_hre" => {
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
                        let limit = Self::All(Self::compile(
                            parsed,
                            block(&limits[0].2, &fail)?,
                            report,
                            depth + 1,
                            country,
                            library,
                            false,
                        )?);
                        let children = Self::All(Self::compile(
                            parsed,
                            body,
                            report,
                            depth + 1,
                            country,
                            library,
                            true,
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
                        let children = Self::compile(
                            parsed,
                            block(&value, &fail)?,
                            report,
                            depth + 1,
                            country,
                            library,
                            false,
                        )?;
                        match name.as_str() {
                            "AND" | "and" => Self::All(children),
                            "OR" | "or" => Self::Any(children),
                            _ => Self::None(children),
                        }
                    }
                    "capital_scope" if country => Self::Capital(Self::compile(
                        parsed,
                        block(&value, &fail)?,
                        report,
                        depth + 1,
                        false,
                        library,
                        false,
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
                    "government" | "has_reform" | "has_government_attribute" if country => {
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
                    | "religion_group" | "technology_group" | "dynasty"
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
                        let kind = if matches!(self, Self::Scoped(_)) {
                            "unknown_scoped_condition"
                        } else {
                            "unknown_variable_state"
                        };
                        reasons.insert(format!("{kind}:{error}"));
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
            | Self::Region(id, _) => replace(id),
            _ => true,
        };
        if !complete {
            *self = Self::Unknown("unbound_script_parameter".into());
        }
    }
}
