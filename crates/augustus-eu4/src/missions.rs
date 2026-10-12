// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{Completions, Condition, InitialWorld, Objectives, World};
use serde::Serialize;

use crate::bootstrap::{direct_txt, equal};
use crate::{ContentRoot, Error, FileEntry, ScriptCompiler, ScriptLibrary, TextEncoding};

#[derive(Debug, Default, Serialize)]
pub struct MissionReport {
    pub series: usize,
    pub objectives: usize,
    pub known_countries: usize,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub unresolved: BTreeMap<String, String>,
}

pub(crate) struct MissionDraft {
    ids: BTreeSet<String>,
    objectives: Objectives,
}

impl MissionDraft {
    pub(crate) fn load(
        root: &ContentRoot,
        files: &[FileEntry],
        world: &World,
        areas: &BTreeMap<String, Vec<u32>>,
    ) -> Result<Self, Error> {
        let ids = identifiers(root, files)?;
        let library = ScriptLibrary::from_root(root, files, TextEncoding::Windows1252)?;
        let documents = files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "missions"))
            .map(|file| root.read_text(&file.relative_path, TextEncoding::Windows1252))
            .collect::<Result<Vec<_>, _>>()?;
        let mut objectives = ScriptCompiler::new(world.schema(), &library, true)
            .with_subject_type_ids(world.subjects().types.clone())
            .with_mission_ids(ids.clone())
            .with_modifier_ids(world.modifiers().definitions.keys().cloned().collect())
            .with_metric_ids(world.metrics().definitions.keys().cloned().collect())
            .with_government_metric_ids(world.governments().metrics.keys().cloned().collect())
            .with_area_scopes(areas.clone())
            .with_religious_school_groups(world.affiliations().0["religious_school"].groups.clone())
            .compile_missions(&documents)?;
        // Persistent conditions use content paths, independent of the player's installation.
        let prefix = format!("{}{}", root.path().display(), std::path::MAIN_SEPARATOR);
        for group in &mut objectives.groups {
            relative_reasons(&mut group.condition, &prefix);
        }
        Ok(Self { ids, objectives })
    }

    pub(crate) fn bind(self, initial: &mut InitialWorld) -> Result<MissionReport, Error> {
        let mut report = MissionReport {
            series: self.objectives.groups.len(),
            objectives: self.ids.len(),
            ..Default::default()
        };
        if self.ids.is_empty() && self.objectives.groups.is_empty() {
            return Ok(report);
        }
        let fixed_start = initial
            .world()
            .date()
            .map_err(|e| Error::caused_by("missions", e))?
            == Some(augustus_core::WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            });
        let entities = if fixed_start {
            initial
                .countries()
                .iter()
                .map(|country| {
                    (
                        initial
                            .country_entity(&country.id)
                            .expect("registered country"),
                        BTreeSet::new(),
                    )
                })
                .collect()
        } else {
            BTreeMap::new()
        };
        initial
            .set_completions(Completions {
                identifiers: self.ids,
                entities,
            })
            .map_err(|e| Error::caused_by("missions", e))?;
        initial
            .set_objectives(self.objectives)
            .map_err(|e| Error::caused_by("missions", e))?;
        let countries: Vec<_> = initial.countries().iter().map(|c| c.id.clone()).collect();
        for tag in countries {
            let entity = initial.country_entity(&tag).expect("registered country");
            match fixed_start.then(|| initial.refresh_objectives(entity)) {
                Some(Ok(())) => report.known_countries += 1,
                Some(Err(error)) => {
                    report.unresolved.insert(tag, error.to_string());
                }
                None => {
                    report.unresolved.insert(
                        tag,
                        "mission selection at this date is not initialized".into(),
                    );
                }
            }
        }
        Ok(report)
    }
}

fn relative_reasons(condition: &mut Condition, prefix: &str) {
    match condition {
        Condition::Unavailable(reason) => *reason = reason.replace(prefix, ""),
        Condition::All(children) | Condition::Any(children) => {
            for child in children {
                relative_reasons(child, prefix);
            }
        }
        Condition::Not(child) => relative_reasons(child, prefix),
        _ => (),
    }
}

/// Index names only. Tree eligibility, rewards and series selection are separate.
pub(crate) fn identifiers(
    root: &ContentRoot,
    files: &[FileEntry],
) -> Result<BTreeSet<String>, Error> {
    let mut identifiers = BTreeSet::new();
    for file in files
        .iter()
        .filter(|file| direct_txt(&file.relative_path, "missions"))
    {
        let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = document.parse()?;
        for (key, operator, value) in parsed.reader().fields() {
            if key.read_string().starts_with('@') {
                continue;
            }
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            equal(operator, &fail)?;
            let series = value
                .read_object()
                .map_err(|_| fail("expected mission series block"))?;
            for (key, operator, value) in series.fields() {
                let id = key.read_string();
                if id.starts_with('@') || matches!(id.as_str(), "potential" | "potential_on_load") {
                    continue;
                }
                if value.read_object().is_ok() {
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    equal(operator, &fail)?;
                    if id.is_empty() {
                        return Err(fail("mission declaration requires identifier"));
                    }
                    identifiers.insert(id);
                }
            }
        }
    }
    Ok(identifiers)
}
