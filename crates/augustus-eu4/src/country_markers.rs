// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{InitialWorld, Markers};
use jomini::text::ObjectReader;

use crate::bootstrap::{dated_name, equal};
use crate::{Error, ParsedText, TextEncoding};

/// Only literal names are translated; scope suffixes need runtime name resolution.
pub(super) fn literal_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['$', '@'])
}

pub(super) fn global_flag_mutations(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    depth: usize,
) -> Result<Option<BTreeSet<String>>, Error> {
    let mut names = BTreeSet::new();
    if !collect_global_flag_mutations(parsed, reader, depth, &mut names)? {
        return Ok(None);
    }
    Ok(Some(names))
}

fn collect_global_flag_mutations(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
    depth: usize,
    names: &mut BTreeSet<String>,
) -> Result<bool, Error> {
    if depth > 64 {
        return Err(Error::message(
            parsed.source(),
            "global flag history nesting exceeds limit",
        ));
    }
    for (key, _, value) in reader.fields() {
        let name = key.read_string();
        if dated_name(&name) {
            continue;
        }
        if matches!(name.as_str(), "set_global_flag" | "clr_global_flag") {
            let Ok(flag) = value.read_string() else {
                return Ok(false);
            };
            if !literal_name(&flag) {
                return Ok(false);
            }
            names.insert(flag);
            continue;
        }
        if let Ok(body) = value.read_object()
            && !collect_global_flag_mutations(parsed, body, depth + 1, names)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Default)]
struct History {
    values: BTreeMap<String, bool>,
    unknown: bool,
}

#[derive(Default)]
pub(super) struct CountryMarkers {
    histories: BTreeMap<String, History>,
}

impl CountryMarkers {
    pub(super) fn history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        tag: &str,
    ) -> Result<BTreeSet<String>, Error> {
        let mut handled = BTreeSet::new();
        let history = self.histories.entry(tag.into()).or_default();
        for (key, op, value) in reader.fields() {
            let name = key.read_string();
            if dated_name(&name) {
                continue;
            }
            if matches!(name.as_str(), "set_country_flag" | "clr_country_flag") {
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                let id = value
                    .read_string()
                    .map_err(|_| fail("expected country flag name"))?;
                if !literal_name(&id) {
                    return Err(fail("country flag requires a literal name"));
                }
                let present = name == "set_country_flag";
                if present || history.unknown {
                    history.values.insert(id, present);
                } else {
                    history.values.remove(&id);
                }
                handled.insert(name);
            } else if let Ok(body) = value.read_object()
                && contains_operation(body, 0)?
            {
                history.values.clear();
                history.unknown = true;
            }
        }
        Ok(handled)
    }

    pub(super) fn bind(self, initial: &InitialWorld) -> Markers {
        let mut entities = BTreeMap::new();
        let mut unknown_entities = BTreeSet::new();
        for country in initial.countries() {
            let entity = initial
                .country_entity(&country.id)
                .expect("registered country");
            if let Some(history) = self.histories.get(&country.id) {
                entities.insert(entity, history.values.clone());
                if history.unknown {
                    unknown_entities.insert(entity);
                }
            } else {
                unknown_entities.insert(entity);
            }
        }
        Markers {
            entities,
            unknown_entities,
        }
    }
}

fn contains_operation(
    reader: ObjectReader<'_, '_, TextEncoding>,
    depth: usize,
) -> Result<bool, Error> {
    if depth > 64 {
        return Err(Error::message(
            "history/countries",
            "flag history nesting exceeds limit",
        ));
    }
    let mut found = false;
    for (key, _, value) in reader.fields() {
        found |= matches!(
            key.read_string().as_str(),
            "set_country_flag" | "clr_country_flag"
        );
        if let Ok(body) = value.read_object() {
            found |= contains_operation(body, depth + 1)?;
        }
    }
    Ok(found)
}
