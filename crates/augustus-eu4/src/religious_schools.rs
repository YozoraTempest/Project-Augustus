// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use augustus_core::{AffiliationDimension, InitialWorld};
use jomini::text::ObjectReader;
use serde::Serialize;

use crate::bootstrap::{direct_txt, equal, source};
use crate::setup::block;
use crate::{ContentView, Error, FileEntry, LoadedSource, ParsedText, TextEncoding};

const ENCODING: TextEncoding = TextEncoding::Windows1252;

#[derive(Debug, Default, Serialize)]
pub struct ReligiousSchoolReport {
    pub definitions: usize,
    pub selected_countries: usize,
    pub absent_countries: usize,
    pub unknown_countries: Vec<String>,
    pub definition_sources: BTreeMap<String, Vec<LoadedSource>>,
    pub history_sources: BTreeMap<String, Vec<LoadedSource>>,
}

#[derive(Default)]
struct History {
    religion: Option<String>,
    school: Option<String>,
    unknown: bool,
}

pub(super) struct ReligiousSchoolsDraft {
    groups: BTreeMap<String, Option<String>>,
    religions: BTreeMap<String, Option<String>>,
    histories: BTreeMap<String, History>,
}

impl ReligiousSchoolsDraft {
    pub(super) fn load(
        root: &impl ContentView,
        files: &[FileEntry],
        report: &mut ReligiousSchoolReport,
    ) -> Result<Self, Error> {
        let mut result = Self {
            groups: BTreeMap::new(),
            religions: crate::autonomy::groups(
                root,
                files,
                "common/religions",
                &mut Default::default(),
            )?,
            histories: BTreeMap::new(),
        };
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
                for (key, operator, value) in block(&value, &fail)?
                    .fields()
                    .filter(|(key, _, _)| key.read_string() == "religious_schools")
                {
                    let fail = |message: &str| {
                        Error::message(parsed.source(), message)
                            .with_location(parsed.field_location(&key))
                    };
                    equal(operator, &fail)?;
                    for (school, operator, value) in block(&value, &fail)?.fields() {
                        equal(operator, &fail)?;
                        block(&value, &fail)?;
                        let name = school.read_string();
                        if name.is_empty() {
                            return Err(fail("empty religious school identifier"));
                        }
                        result
                            .groups
                            .entry(name.clone())
                            .and_modify(|group| *group = None)
                            .or_insert(Some(group.read_string()));
                        report
                            .definition_sources
                            .entry(name)
                            .or_default()
                            .push(source(&parsed, parsed.field_location(&school)));
                    }
                }
            }
        }
        report.definitions = result.groups.len();
        Ok(result)
    }

    pub(super) fn country_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        country: &str,
        report: &mut ReligiousSchoolReport,
    ) -> Result<(), Error> {
        let history = self.histories.entry(country.into()).or_default();
        for (key, operator, value) in reader.fields() {
            let name = key.read_string();
            if crate::bootstrap::dated_name(&name)
                || matches!(
                    name.as_str(),
                    "monarch" | "heir" | "queen" | "monarch_consort"
                )
            {
                continue;
            }
            let fail = |message: &str| {
                Error::message(parsed.source(), message).with_location(parsed.field_location(&key))
            };
            if matches!(name.as_str(), "religion" | "religious_school") {
                equal(operator, &fail)?;
                let text = value
                    .read_string()
                    .map_err(|_| fail("expected religious school history scalar"))?;
                if text.is_empty() {
                    return Err(fail("empty religious school history identifier"));
                }
                report
                    .history_sources
                    .entry(country.into())
                    .or_default()
                    .push(source(parsed, parsed.field_location(&key)));
                if name == "religion" {
                    let old = history
                        .religion
                        .as_ref()
                        .and_then(|name| self.religions.get(name))
                        .and_then(|group| group.as_ref());
                    let new = self.religions.get(&text).and_then(|group| group.as_ref());
                    if old.is_some() && new.is_some() && old != new {
                        history.school = None;
                        history.unknown = false;
                    } else if new.is_none() && history.school.is_some() {
                        history.unknown = true;
                    }
                    history.religion = Some(text);
                } else {
                    let school_group = self.groups.get(&text).and_then(|group| group.as_ref());
                    let current_group = history
                        .religion
                        .as_ref()
                        .and_then(|name| self.religions.get(name))
                        .and_then(|group| group.as_ref());
                    if school_group.is_none() {
                        history.unknown = true;
                    } else if history.religion.is_none() {
                        // Before any religion assignment the engine has no applicable group.
                    } else if current_group.is_none() {
                        history.unknown = true;
                    } else if school_group == current_group {
                        history.school = Some(text);
                        history.unknown = false;
                    }
                }
            } else if let Ok(body) = value.read_object() {
                let mut pending = vec![(body, 0)];
                while let Some((body, depth)) = pending.pop() {
                    if depth > 64 {
                        return Err(fail("religious school history nesting exceeds 64"));
                    }
                    for (key, _, value) in body.fields() {
                        if matches!(key.read_string().as_str(), "religion" | "religious_school") {
                            history.unknown = true;
                            report
                                .history_sources
                                .entry(country.into())
                                .or_default()
                                .push(source(parsed, parsed.field_location(&key)));
                        } else if let Ok(body) = value.read_object() {
                            pending.push((body, depth + 1));
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn bind(
        &self,
        initial: &InitialWorld,
        report: &mut ReligiousSchoolReport,
    ) -> AffiliationDimension {
        let mut dimension = AffiliationDimension {
            groups: self.groups.clone(),
            ..Default::default()
        };
        for country in initial.countries() {
            let entity = initial
                .country_entity(&country.id)
                .expect("registered country");
            match self.histories.get(&country.id) {
                Some(history) if !history.unknown => {
                    if let Some(school) = &history.school {
                        dimension.members.insert(entity, Some(school.clone()));
                        report.selected_countries += 1;
                    } else {
                        dimension.absent_members.insert(entity);
                        report.absent_countries += 1;
                    }
                }
                _ => {
                    dimension.members.insert(entity, None);
                    report.unknown_countries.push(country.id.clone());
                }
            }
        }
        dimension
    }
}
