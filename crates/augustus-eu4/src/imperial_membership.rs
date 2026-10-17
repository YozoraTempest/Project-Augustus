// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use augustus_core::{AffiliationDimension, Affiliations, InitialWorld};
use jomini::text::ObjectReader;

use crate::{ContentView, Error, FileEntry, ParsedText, TextEncoding};

pub(super) const DIMENSION: &str = "imperial_membership";

/// Scripted history effects matter only when they can reach a membership write.
pub(super) fn membership_effect_names(
    root: &impl ContentView,
    files: &[FileEntry],
) -> Result<BTreeSet<String>, Error> {
    let mut dependencies = std::collections::BTreeMap::<String, BTreeSet<String>>::new();
    for file in files
        .iter()
        .filter(|file| crate::bootstrap::direct_txt(&file.relative_path, "common/scripted_effects"))
    {
        let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = document.parse()?;
        for (key, _, value) in parsed.reader().fields() {
            let name = key.read_string();
            if name.starts_with('@') {
                continue;
            }
            let mut referenced = BTreeSet::new();
            if let Ok(body) = value.read_object() {
                collect_script_names(body, &mut referenced);
            }
            dependencies.entry(name).or_default().extend(referenced);
        }
    }

    let direct_writes = BTreeSet::from(["set_in_empire", "dismantle_hre"]);
    let mut affected = BTreeSet::new();
    loop {
        let newly_affected = dependencies
            .iter()
            .filter(|(name, references)| {
                !affected.contains(*name)
                    && references.iter().any(|reference| {
                        direct_writes.contains(reference.as_str()) || affected.contains(reference)
                    })
            })
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if newly_affected.is_empty() {
            break;
        }
        affected.extend(newly_affected);
    }
    Ok(affected)
}

fn collect_script_names(reader: ObjectReader<'_, '_, TextEncoding>, names: &mut BTreeSet<String>) {
    for (key, _, value) in reader.fields() {
        names.insert(key.read_string());
        if let Ok(body) = value.read_object() {
            collect_script_names(body, names);
        }
    }
}

/// Only a read history file establishes the default absence of membership.
pub(super) fn bind(
    initial: &InitialWorld,
    history_files: &BTreeSet<u32>,
    history_unknown: bool,
) -> AffiliationDimension {
    let mut data = AffiliationDimension::default();
    data.groups.insert("hre".into(), None);
    for province in initial.provinces() {
        let entity = initial
            .province_entity(province.id)
            .expect("registered province");
        let membership = if history_unknown {
            None
        } else {
            province
                .in_empire
                .or_else(|| history_files.contains(&province.id).then_some(false))
        };
        match membership {
            Some(true) => {
                data.members.insert(entity, Some("hre".into()));
            }
            Some(false) => {
                data.absent_members.insert(entity);
            }
            None => {
                data.members.insert(entity, None);
            }
        }
    }
    data
}

pub(super) fn mark_unknown(affiliations: &mut Affiliations) {
    if let Some(data) = affiliations.0.get_mut(DIMENSION) {
        for member in data.members.values_mut() {
            *member = None;
        }
        for entity in std::mem::take(&mut data.absent_members) {
            data.members.insert(entity, None);
        }
    }
}

/// Unexecuted history effects may change imperial membership.
/// Date blocks are visited by the history loader at the selected date.
pub(super) fn has_unapplied_mutation(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    scripted_effects: &BTreeSet<String>,
    province_scope: bool,
    depth: usize,
) -> Result<bool, Error> {
    if depth > 64 {
        return Err(Error::message(
            parsed.source(),
            "imperial membership history nesting exceeds limit",
        ));
    }
    for (key, _, value) in reader.fields() {
        let name = key.read_string();
        if crate::bootstrap::dated_name(&name) {
            continue;
        }
        if scripted_effects.contains(&name)
            || matches!(name.as_str(), "set_in_empire" | "dismantle_hre")
            || (name == "hre" && !province_scope)
        {
            return Ok(true);
        }
        if let Ok(body) = value.read_object()
            && has_unapplied_mutation(parsed, body, scripted_effects, false, depth + 1)?
        {
            return Ok(true);
        }
    }
    Ok(false)
}
