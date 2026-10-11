// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use crate::bootstrap::{equal, source};
use crate::setup::block;
use crate::{AdministrationReport, ContentRoot, Error, ParsedText, TextEncoding};

type Groups = BTreeMap<String, BTreeSet<String>>;

#[derive(Default)]
pub(super) struct GeographicGroups {
    pub regions: Groups,
    pub superregions: Groups,
}

impl GeographicGroups {
    pub(super) fn load(
        root: &ContentRoot,
        map: &ParsedText<'_>,
        report: &mut AdministrationReport,
    ) -> Result<Self, Error> {
        let mut result = Self::default();
        for kind in ["region", "superregion"] {
            let paths: Vec<_> = map
                .reader()
                .fields()
                .filter(|(k, _, _)| k.read_string() == kind)
                .collect();
            if paths.len() > 1 {
                return Err(Error::message(
                    map.source(),
                    format!("duplicate map {kind} path"),
                ));
            }
            let Some((key, op, value)) = paths.first() else {
                continue;
            };
            let fail =
                |m: &str| Error::message(map.source(), m).with_location(map.field_location(key));
            equal(*op, &fail)?;
            let filename = value
                .read_string()
                .map_err(|_| fail("expected geographic group path"))?;
            let doc = root.read_text(
                std::path::Path::new("map").join(filename),
                TextEncoding::Windows1252,
            )?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let name = key.read_string();
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                if name.is_empty() {
                    return Err(fail("empty geographic group identifier"));
                }
                let mut members = BTreeSet::new();
                if kind == "region" {
                    let body = block(&value, &fail)?;
                    let mut fields = body.fields();
                    let mut seen_areas = false;
                    for (key, op, value) in fields.by_ref() {
                        if key.read_string() != "areas" {
                            report.unapplied_geography_fields.insert(
                                format!(
                                    "{name}:{}:{}:{}",
                                    key.read_string(),
                                    parsed.field_location(&key).line,
                                    parsed.field_location(&key).column
                                ),
                                source(&parsed, parsed.field_location(&key)),
                            );
                            continue;
                        }
                        equal(op, &fail)?;
                        if seen_areas {
                            return Err(fail("region requires one area list"));
                        }
                        seen_areas = true;
                        for value in value
                            .read_array()
                            .map_err(|_| fail("expected region area list"))?
                            .values()
                        {
                            let id = value
                                .read_string()
                                .map_err(|_| fail("expected area identifier"))?;
                            if id.is_empty() {
                                return Err(fail("empty area identifier"));
                            }
                            members.insert(id);
                        }
                    }
                    if !seen_areas && body.fields().next().is_some() {
                        return Err(fail("region requires an area block"));
                    }
                    if fields.remainder().values().next().is_some() {
                        return Err(fail("region requires an area block"));
                    }
                } else {
                    if value
                        .read_object()
                        .is_ok_and(|body| body.fields().next().is_some())
                    {
                        return Err(fail("expected superregion region list"));
                    }
                    for value in value
                        .read_array()
                        .map_err(|_| fail("expected superregion region list"))?
                        .values()
                    {
                        let id = value
                            .read_string()
                            .map_err(|_| fail("expected region identifier"))?;
                        if id == "restrict_charter" {
                            report
                                .unapplied_geography_fields
                                .insert(format!("{name}:{id}"), source(&parsed, at));
                        } else if id.is_empty() {
                            return Err(fail("empty region identifier"));
                        } else {
                            members.insert(id);
                        }
                    }
                }
                let groups = if kind == "region" {
                    &mut result.regions
                } else {
                    &mut result.superregions
                };
                if groups.insert(name.clone(), members).is_some() {
                    return Err(fail("duplicate geographic group"));
                }
                report
                    .rule_sources
                    .insert(format!("map/{kind}:{name}"), source(&parsed, at));
            }
        }
        report.regions = result.regions.len();
        report.superregions = result.superregions.len();
        Ok(result)
    }
}
