// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::bootstrap::{
    BootstrapReport, LoadedSource, direct_txt, equal, history_blocks, nonnegative_milli, source,
};
use crate::{ContentRoot, Error, FileEntry, ParsedText, SourceLocation, TextEncoding};
use augustus_core::{
    CountryPolitics, DiplomaticKind, DiplomaticRelation, EntityId, InitialWorld, PersonState,
    Politics, WorldDate,
};
use jomini::{
    common::{Date, PdsDate},
    text::ValueReader,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub(super) struct PersonDraft {
    state: PersonState,
    origin: Option<String>,
    source: Option<LoadedSource>,
}

#[derive(Debug, Default)]
pub(super) struct CountryDraft {
    pub national_focus: Option<augustus_core::NationalFocus>,
    pub ruler: Option<PersonDraft>,
    pub heir: Option<PersonDraft>,
    pub consort: Option<PersonDraft>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RelationSource {
    pub relation: DiplomaticRelation,
    pub source: LoadedSource,
    pub start_date: WorldDate,
    pub end_date: WorldDate,
}

pub(super) fn label(date: Date) -> WorldDate {
    WorldDate {
        year: date.year(),
        month: date.month(),
        day: date.day(),
    }
}

pub(super) fn ruler_skill_bounds(
    root: &ContentRoot,
    files: &[FileEntry],
    report: &mut crate::SetupReport,
) -> Result<Option<augustus_core::RulerSkillBounds>, Error> {
    if !files
        .iter()
        .any(|file| file.relative_path == std::path::Path::new("common/defines.lua"))
    {
        return Ok(None);
    }
    let mut bounds = Vec::new();
    for (key, name) in [
        ("MONARCH_MIN_SKILL", "minimum"),
        ("MONARCH_MAX_SKILL", "maximum"),
    ] {
        let Some((value, source)) = crate::defines::optional_literal(root, files, "NCountry", key)?
        else {
            continue;
        };
        let bound = u8::try_from(value / 1_000_000)
            .ok()
            .filter(|_| value % 1_000_000 == 0)
            .ok_or_else(|| {
                Error::message(&source.path, format!("{key} requires an integer in 0..255"))
            })?;
        report
            .definition_sources
            .insert(format!("ruler_skill:{name}"), source);
        bounds.push(bound);
    }
    let result = if bounds.len() == 2 {
        if bounds[0] > bounds[1] {
            return Err(Error::message(
                root.path(),
                "MONARCH_MIN_SKILL exceeds MONARCH_MAX_SKILL",
            ));
        }
        Some(augustus_core::RulerSkillBounds {
            minimum: bounds[0],
            maximum: bounds[1],
        })
    } else {
        None
    };
    report.ruler_skill_bounds = result;
    Ok(result)
}

pub(super) fn unknown(
    parsed: &ParsedText<'_>,
    name: String,
    location: SourceLocation,
    report: &mut BootstrapReport,
    diplomacy: bool,
) {
    let fields = if diplomacy {
        &mut report.unapplied_diplomacy_fields
    } else {
        &mut report.unapplied_country_fields
    };
    *fields.entry(name.clone()).or_default() += 1;
    let key = format!("{}:{name}", if diplomacy { "diplomacy" } else { "country" });
    report
        .unapplied_examples
        .entry(key)
        .or_insert_with(|| source(parsed, location));
}

pub(super) fn person(
    parsed: &ParsedText<'_>,
    value: ValueReader<'_, '_, TextEncoding>,
    kind: &str,
    location: SourceLocation,
    report: &mut BootstrapReport,
) -> Result<PersonDraft, Error> {
    let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
    let reader = value
        .read_object()
        .map_err(|_| fail("person history requires a block"))?;
    let mut draft = PersonDraft {
        source: Some(source(parsed, location)),
        ..Default::default()
    };
    let mut assigned = BTreeSet::new();
    for (key, op, value) in reader.fields() {
        let name = key.read_string();
        let location = parsed.field_location(&key);
        let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
        let normalized = match name.as_str() {
            "ADM" => "adm",
            "DIP" => "dip",
            "MIL" => "mil",
            "birth_Date" => "birth_date",
            "death_Date" => "death_date",
            _ => &name,
        };
        if !matches!(
            normalized,
            "name"
                | "monarch_name"
                | "dynasty"
                | "culture"
                | "religion"
                | "country_of_origin"
                | "birth_date"
                | "death_date"
                | "adm"
                | "dip"
                | "mil"
                | "claim"
                | "female"
                | "regent"
                | "block_disinherit"
        ) {
            unknown(parsed, format!("{kind}.{name}"), location, report, false);
            continue;
        }
        equal(op, &fail)?;
        if !assigned.insert(normalized.to_owned()) {
            return Err(fail("duplicate person field"));
        }
        let text = value
            .read_string()
            .map_err(|_| fail("expected person scalar"))?;
        match normalized {
            "name" => draft.state.name = Some(text),
            "monarch_name" => draft.state.regnal_name = Some(text),
            "dynasty" => draft.state.dynasty = Some(text),
            "culture" => draft.state.culture = Some(text),
            "religion" => draft.state.religion = Some(text),
            "country_of_origin" => draft.origin = Some(text),
            "birth_date" | "death_date" => {
                let date = label(Date::parse(&text).map_err(|_| fail("invalid person date"))?);
                if normalized == "birth_date" {
                    draft.state.birth_date = Some(date);
                } else {
                    draft.state.historical_death_date = Some(date);
                }
            }
            "adm" | "dip" | "mil" => {
                let skill: u8 = text
                    .parse()
                    .map_err(|_| fail("expected person skill integer"))?;
                if skill > 6 {
                    return Err(fail("person skill outside 0..6"));
                }
                *match normalized {
                    "adm" => &mut draft.state.administration,
                    "dip" => &mut draft.state.diplomacy,
                    _ => &mut draft.state.military,
                } = Some(skill);
            }
            "claim" => {
                let claim = nonnegative_milli(&text)
                    .filter(|v| *v <= 100000)
                    .ok_or_else(|| fail("claim outside 0..100 or unsupported precision"))?;
                draft.state.claim_milli = Some(claim);
            }
            "female" | "regent" | "block_disinherit" => {
                let flag = match text.as_str() {
                    "yes" => true,
                    "no" => false,
                    _ => return Err(fail("expected yes or no")),
                };
                *match normalized {
                    "female" => &mut draft.state.female,
                    "regent" => &mut draft.state.regent,
                    _ => &mut draft.state.block_disinherit,
                } = Some(flag);
            }
            _ => unreachable!(),
        }
    }
    if let (Some(birth), Some(death)) = (draft.state.birth_date, draft.state.historical_death_date)
        && death < birth
    {
        return Err(fail("person death precedes birth"));
    }
    Ok(draft)
}

pub(super) fn bind_people(
    initial: &InitialWorld,
    drafts: BTreeMap<String, CountryDraft>,
    date: Date,
    report: &mut BootstrapReport,
) -> Result<Politics, Error> {
    let mut politics = Politics::default();
    for (tag, draft) in drafts {
        let mut country = CountryPolitics {
            national_focus: draft.national_focus,
            ..Default::default()
        };
        for (role, person, slot) in [
            ("ruler", draft.ruler, &mut country.ruler),
            ("heir", draft.heir, &mut country.heir),
            ("consort", draft.consort, &mut country.consort),
        ] {
            let Some(mut person) = person else {
                continue;
            };
            let source = person.source.take().expect("parsed person source");
            report
                .person_sources
                .entry(tag.clone())
                .or_default()
                .insert(role.into(), source.clone());
            if person
                .state
                .historical_death_date
                .is_some_and(|death| death <= label(date))
            {
                report.expired_people.push(format!("{tag}:{role}"));
                continue;
            }
            if person
                .state
                .birth_date
                .is_some_and(|birth| birth > label(date))
            {
                report.not_yet_born_people.push(format!("{tag}:{role}"));
                continue;
            }
            if let Some(origin) = person.origin {
                person.state.country_of_origin =
                    Some(initial.country_entity(&origin).ok_or_else(|| {
                        Error::message(
                            &source.path,
                            format!("unknown person country of origin {origin}"),
                        )
                        .with_location(SourceLocation {
                            line: source.line,
                            column: source.column,
                        })
                    })?);
            }
            *slot = Some(person.state);
        }
        if country != CountryPolitics::default() {
            politics.countries.insert(
                initial.country_entity(&tag).expect("registered country"),
                country,
            );
        }
    }
    Ok(politics)
}

pub(super) fn load_diplomacy(
    root: &ContentRoot,
    files: &[FileEntry],
    initial: &InitialWorld,
    date: Date,
    politics: &mut Politics,
    report: &mut BootstrapReport,
) -> Result<BTreeSet<String>, Error> {
    let mut subject_types = BTreeSet::new();
    for file in files
        .iter()
        .filter(|f| direct_txt(&f.relative_path, "common/subject_types"))
    {
        let doc = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = doc.parse()?;
        for (key, _, _) in parsed.reader().fields() {
            subject_types.insert(key.read_string());
        }
    }
    let mut relations = BTreeSet::new();
    let mut offices = Vec::new();
    for file in files
        .iter()
        .filter(|f| direct_txt(&f.relative_path, "history/diplomacy"))
    {
        report.diplomacy_history_files += 1;
        let doc = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = doc.parse()?;
        for (timestamp, reader) in history_blocks(&parsed, date)? {
            for (key, op, value) in reader.fields() {
                let name = key.read_string();
                if crate::bootstrap::dated_name(&name) {
                    continue;
                }
                let location = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
                if matches!(name.as_str(), "emperor" | "celestial_emperor") {
                    equal(op, &fail)?;
                    let tag = value
                        .read_string()
                        .map_err(|_| fail("expected office holder"))?;
                    offices.push((timestamp, name, tag, source(&parsed, location)));
                    continue;
                }
                let (mut kind, preset) = match name.as_str() {
                    "alliance" => (DiplomaticKind::Alliance, None),
                    "royal_marriage" => (DiplomaticKind::RoyalMarriage, None),
                    "guarantee" => (DiplomaticKind::Guarantee, None),
                    "warning" => (DiplomaticKind::Warning, None),
                    "vassal" => (DiplomaticKind::Subject, Some("vassal")),
                    "march" => (DiplomaticKind::Subject, Some("march")),
                    "union" => (DiplomaticKind::Subject, Some("personal_union")),
                    "dependency" => (DiplomaticKind::Subject, None),
                    _ => {
                        unknown(&parsed, name, location, report, true);
                        continue;
                    }
                };
                equal(op, &fail)?;
                let block = value
                    .read_object()
                    .map_err(|_| fail("diplomatic relation requires a block"))?;
                let mut scalars = BTreeMap::new();
                for (key, op, value) in block.fields() {
                    let field = key.read_string();
                    let location = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
                    if !matches!(
                        field.as_str(),
                        "first"
                            | "second"
                            | "start_date"
                            | "end_date"
                            | "subject_type"
                            | "trade_league"
                    ) {
                        unknown(&parsed, format!("{name}.{field}"), location, report, true);
                        continue;
                    }
                    equal(op, &fail)?;
                    let text = value
                        .read_string()
                        .map_err(|_| fail("expected diplomacy scalar"))?;
                    if scalars.insert(field, text).is_some() {
                        return Err(fail("duplicate relation field"));
                    }
                }
                let scalar = |name: &str| {
                    scalars
                        .get(name)
                        .ok_or_else(|| fail("missing relation field"))
                };
                let start = Date::parse(scalar("start_date")?)
                    .map_err(|_| fail("invalid relation start date"))?;
                let end = Date::parse(scalar("end_date")?)
                    .map_err(|_| fail("invalid relation end date"))?;
                if end <= start {
                    return Err(fail("relation end must follow start"));
                }
                if date < start || date >= end {
                    continue;
                }
                if let Some(league) = scalars.get("trade_league") {
                    if kind != DiplomaticKind::Alliance {
                        return Err(fail("trade league marker requires an alliance record"));
                    }
                    match league.as_str() {
                        "1" => kind = DiplomaticKind::TradeLeague,
                        "0" => {}
                        _ => return Err(fail("trade league marker must be 0 or 1")),
                    }
                }
                let bind = |tag: &str| {
                    initial
                        .country_entity(tag)
                        .ok_or_else(|| fail("unknown diplomatic country"))
                };
                let first = bind(scalar("first")?)?;
                let second = bind(scalar("second")?)?;
                let subject_type = if kind == DiplomaticKind::Subject {
                    if preset.is_some() && scalars.contains_key("subject_type") {
                        return Err(fail(
                            "subject type conflicts with the history relation kind",
                        ));
                    }
                    let text = match preset {
                        Some(s) => s.to_owned(),
                        None => scalar("subject_type")?.clone(),
                    };
                    if !subject_types.contains(&text) {
                        return Err(fail("unknown subject type"));
                    }
                    Some(text)
                } else {
                    if scalars.contains_key("subject_type") {
                        return Err(fail("subject type on a non-subject relation"));
                    }
                    None
                };
                let mut relation = DiplomaticRelation {
                    kind,
                    first,
                    second,
                    subject_type,
                };
                relation.canonicalize();
                report.relation_sources.push(RelationSource {
                    relation: relation.clone(),
                    source: source(&parsed, location),
                    start_date: label(start),
                    end_date: label(end),
                });
                if !relations.insert(relation) {
                    report.duplicate_relation_records += 1;
                }
            }
        }
    }
    // Cross-file office changes also follow chronology, with stable source order for ties.
    offices.sort_by_key(|entry| entry.0);
    for (_, name, tag, source) in offices {
        let name = if name == "emperor" {
            "imperial_emperor"
        } else {
            "celestial_emperor"
        };
        if tag == "---" {
            politics.offices.remove(name);
        } else {
            let holder = initial.country_entity(&tag).ok_or_else(|| {
                Error::message(
                    &source.path,
                    format!("unknown political office holder {tag}"),
                )
                .with_location(SourceLocation {
                    line: source.line,
                    column: source.column,
                })
            })?;
            politics.offices.insert(name.into(), holder);
        }
        report.office_sources.insert(name.into(), source);
    }
    politics.relations = relations.into_iter().collect();
    report.diplomatic_relations = politics.relations.len();
    report.people = politics
        .countries
        .values()
        .map(|c| {
            [&c.ruler, &c.heir, &c.consort]
                .into_iter()
                .flatten()
                .count()
        })
        .sum();
    Ok(subject_types)
}

pub(super) fn has_subject_mutation(
    parsed: &ParsedText<'_>,
    reader: jomini::text::ObjectReader<'_, '_, TextEncoding>,
    scripted_effects: &BTreeSet<String>,
    depth: usize,
) -> Result<bool, Error> {
    if depth > 64 {
        return Err(Error::message(
            parsed.source(),
            "subject history nesting exceeds limit",
        ));
    }
    for (key, _, value) in reader.fields() {
        let name = key.read_string();
        if crate::bootstrap::dated_name(&name) {
            continue;
        }
        if scripted_effects.contains(&name)
            || matches!(
                name.as_str(),
                "create_subject"
                    | "create_vassal"
                    | "create_march"
                    | "create_union"
                    | "change_subject_type"
                    | "grant_independence"
                    | "break_union"
                    | "free_vassal"
                    | "vassalize"
                    | "inherit"
                    | "release"
                    | "release_all_subjects"
                    | "release_all_possible_countries"
                    | "effect"
            )
        {
            return Ok(true);
        }
        if let Ok(body) = value.read_object()
            && has_subject_mutation(parsed, body, scripted_effects, depth + 1)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(super) fn history_effect_names(
    root: &ContentRoot,
    files: &[FileEntry],
) -> Result<BTreeSet<String>, Error> {
    let mut names = BTreeSet::new();
    for file in files
        .iter()
        .filter(|file| direct_txt(&file.relative_path, "common/scripted_effects"))
    {
        let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = document.parse()?;
        names.extend(
            parsed
                .reader()
                .fields()
                .map(|(key, _, _)| key.read_string())
                .filter(|name| !name.starts_with('@')),
        );
    }
    Ok(names)
}

#[derive(Debug, Serialize)]
pub struct RelationView<'a> {
    pub kind: DiplomaticKind,
    pub first: &'a str,
    pub second: &'a str,
    pub subject_type: &'a Option<String>,
}

pub(super) fn relation_view<'a>(
    initial: &'a InitialWorld,
    relation: &'a DiplomaticRelation,
) -> RelationView<'a> {
    let tag = |id: EntityId| {
        initial
            .world()
            .entity(id)
            .expect("validated country")
            .name
            .as_str()
    };
    RelationView {
        kind: relation.kind,
        first: tag(relation.first),
        second: tag(relation.second),
        subject_type: &relation.subject_type,
    }
}
