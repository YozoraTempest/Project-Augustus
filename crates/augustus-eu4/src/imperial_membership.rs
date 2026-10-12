// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeSet;

use augustus_core::{AffiliationDimension, Affiliations, InitialWorld};
use jomini::text::ObjectReader;

use crate::{Error, ParsedText, TextEncoding};

pub(super) const DIMENSION: &str = "imperial_membership";

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

/// Unexecuted history effects may change membership or move a country's capital.
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
            || matches!(
                name.as_str(),
                "effect" | "set_in_empire" | "dismantle_hre" | "set_capital"
            )
            || (name == "hre" && !province_scope)
            || (name == "capital" && depth > 0)
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
