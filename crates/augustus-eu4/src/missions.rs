// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{Completions, Condition, InitialWorld, Objectives, World};
use serde::Serialize;

use crate::bootstrap::{direct_txt, equal};
use crate::{ContentView, Error, FileEntry, ScriptCompiler, ScriptLibrary, TextEncoding};

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
        root: &impl ContentView,
        files: &[FileEntry],
        world: &World,
        geographic_scopes: &BTreeMap<String, Vec<u32>>,
    ) -> Result<Self, Error> {
        let ids = identifiers(root, files)?;
        let library = ScriptLibrary::from_root(root, files, TextEncoding::Windows1252)?;
        let documents = files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "missions"))
            .map(|file| root.read_text(&file.relative_path, TextEncoding::Windows1252))
            .collect::<Result<Vec<_>, _>>()?;
        let mut objectives = ScriptCompiler::for_world(world, &library, true)
            .with_mission_ids(ids.clone())
            .with_geographic_scopes(geographic_scopes.clone())
            .compile_missions(&documents)?;
        // Persistent conditions use content paths, independent of the player's installation.
        for source_root in root.source_roots() {
            let prefix = format!("{}{}", source_root.display(), std::path::MAIN_SEPARATOR);
            for group in &mut objectives.groups {
                relative_reasons(&mut group.condition, &prefix);
            }
        }
        Ok(Self { ids, objectives })
    }

    pub(crate) fn register_identifiers(&self, initial: &mut InitialWorld) -> Result<(), Error> {
        initial
            .set_completions(Completions {
                identifiers: self.ids.clone(),
                ..Default::default()
            })
            .map_err(|e| Error::caused_by("missions", e))
    }

    pub(crate) fn bind(
        self,
        initial: &mut InitialWorld,
        saved_completions: Option<&BTreeMap<String, BTreeSet<String>>>,
    ) -> Result<MissionReport, Error> {
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
        let mut entities = BTreeMap::new();
        if let Some(saved_completions) = saved_completions {
            for (tag, missions) in saved_completions {
                let entity = initial.country_entity(tag).ok_or_else(|| {
                    Error::message(
                        "saved mission completions",
                        format!("unknown country {tag}"),
                    )
                })?;
                entities.insert(entity, missions.clone());
            }
        }
        if fixed_start {
            for country in initial.countries() {
                let entity = initial
                    .country_entity(&country.id)
                    .expect("registered country");
                entities.entry(entity).or_default();
            }
        }
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
        Condition::If {
            condition,
            then,
            otherwise,
        } => {
            for child in [condition.as_mut(), then.as_mut(), otherwise.as_mut()] {
                relative_reasons(child, prefix);
            }
        }
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
    root: &impl ContentView,
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
