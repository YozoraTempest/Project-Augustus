// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{EntityKind, InitialWorld, MetricDefinition, MetricEffect, Metrics, WorldDate};

use crate::attributes::AttributeDraft;
use crate::bootstrap::direct_txt;
use crate::{ContentView, Error, FileEntry, TextEncoding};

pub(super) fn named_history_mutations_with_scripts(
    reader: jomini::text::ObjectReader<'_, '_, crate::TextEncoding>,
    scripted_effects: &BTreeSet<String>,
    depth: usize,
    names: &[&str],
    direct_effects: &BTreeSet<String>,
    scoped_effects: &BTreeSet<String>,
) -> (bool, bool) {
    if depth > 64 {
        return (false, true);
    }
    let mut direct = false;
    let mut scoped = false;
    for (key, _, value) in reader.fields() {
        let name = key.read_string();
        if name.bytes().filter(|c| *c == b'.').count() == 2
            && jomini::common::Date::parse(&name).is_ok()
        {
            continue;
        }
        if names.contains(&name.as_str()) || direct_effects.contains(&name) {
            direct = true;
        } else if scoped_effects.contains(&name) || scripted_effects.contains(&name) {
            scoped = true;
        } else if let Ok(block) = value.read_object() {
            let (nested, deeper) = named_history_mutations_with_scripts(
                block,
                scripted_effects,
                depth + 1,
                names,
                direct_effects,
                scoped_effects,
            );
            scoped |= nested || deeper;
        }
    }
    (direct, scoped)
}

#[derive(Default)]
struct EffectMutation {
    direct: bool,
    scoped: bool,
    calls: Vec<(String, bool)>,
}

pub(super) fn history_scripted_effects(
    root: &impl ContentView,
    files: &[FileEntry],
    mutations: &[&str],
) -> Result<(BTreeSet<String>, BTreeSet<String>), Error> {
    let effect_files: Vec<_> = files
        .iter()
        .filter(|file| direct_txt(&file.relative_path, "common/scripted_effects"))
        .collect();
    let mut identifiers = BTreeSet::new();
    for file in &effect_files {
        let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = document.parse()?;
        identifiers.extend(
            parsed
                .reader()
                .fields()
                .map(|(key, _, _)| key.read_string())
                .filter(|name| !name.starts_with('@')),
        );
    }

    let mut effects = BTreeMap::<String, EffectMutation>::new();
    for file in effect_files {
        let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = document.parse()?;
        for (key, _, value) in parsed.reader().fields() {
            let name = key.read_string();
            if name.starts_with('@') || effects.contains_key(&name) {
                continue;
            }
            let Ok(body) = value.read_object() else {
                continue;
            };
            let mut mutation = EffectMutation::default();
            inspect_effect_body(body, &identifiers, mutations, false, 0, &mut mutation)?;
            effects.insert(name, mutation);
        }
    }

    let mut direct = BTreeSet::new();
    let mut scoped = BTreeSet::new();
    for (name, mutation) in &effects {
        if mutation.direct {
            direct.insert(name.clone());
        }
        if mutation.scoped {
            scoped.insert(name.clone());
        }
    }
    loop {
        let mut changed = false;
        for (name, mutation) in &effects {
            for (callee, inside_scope) in &mutation.calls {
                let callee_direct = direct.contains(callee);
                let callee_scoped = scoped.contains(callee);
                if callee_direct {
                    let destination = if *inside_scope {
                        &mut scoped
                    } else {
                        &mut direct
                    };
                    changed |= destination.insert(name.clone());
                }
                if callee_scoped {
                    changed |= scoped.insert(name.clone());
                }
            }
        }
        if !changed {
            break;
        }
    }
    Ok((direct, scoped))
}

fn inspect_effect_body(
    reader: jomini::text::ObjectReader<'_, '_, crate::TextEncoding>,
    scripted_effects: &BTreeSet<String>,
    mutations: &[&str],
    inside_scope: bool,
    depth: usize,
    result: &mut EffectMutation,
) -> Result<(), Error> {
    if depth > 64 {
        return Err(Error::message(
            "history/countries",
            "scripted history effect nesting exceeds limit",
        ));
    }
    for (key, _, value) in reader.fields() {
        let name = key.read_string();
        if mutations.contains(&name.as_str()) {
            if inside_scope {
                result.scoped = true;
            } else {
                result.direct = true;
            }
            continue;
        }
        if scripted_effects.contains(&name) {
            result.calls.push((name, inside_scope));
            continue;
        }
        if let Ok(body) = value.read_object() {
            let enters_scope = matches!(
                name.as_str(),
                "ROOT"
                    | "root"
                    | "FROM"
                    | "from"
                    | "PREV"
                    | "prev"
                    | "THIS"
                    | "this"
                    | "owner"
                    | "controller"
                    | "capital_scope"
                    | "emperor"
                    | "every_country"
                    | "random_country"
                    | "every_subject_country"
                    | "random_subject_country"
                    | "every_owned_province"
                    | "random_owned_province"
                    | "every_neighbor_country"
                    | "every_province"
            ) || name.starts_with("event_target:")
                || name.parse::<u32>().is_ok_and(|id| id > 0)
                || name.len() == 3
                    && name
                        .bytes()
                        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit());
            inspect_effect_body(
                body,
                scripted_effects,
                mutations,
                inside_scope || enters_scope,
                depth + 1,
                result,
            )?;
        }
    }
    Ok(())
}

pub(super) fn bind(
    initial: &InitialWorld,
    attributes: &AttributeDraft,
    date: WorldDate,
    histories: &BTreeSet<String>,
    unknown_history: &BTreeSet<String>,
    unknown_scoped_history: bool,
) -> Metrics {
    let effect = |name: &str| {
        let modifier = attributes.static_modifier(name).cloned();
        let mut unresolved_properties = attributes.static_unresolved_properties(name);
        if modifier.is_none() {
            unresolved_properties.extend([
                "global_tax_modifier".into(),
                "global_manpower_modifier".into(),
            ]);
        }
        MetricEffect {
            modifier,
            unresolved_properties,
        }
    };
    let positive = effect("positive_stability");
    Metrics {
        definitions: [(
            "stability".into(),
            MetricDefinition {
                kind: EntityKind::Country,
                minimum: -3,
                maximum: 3,
                baseline: 0,
                divisor: 1,
                modifier: positive.modifier,
                unresolved_properties: positive.unresolved_properties,
                below_baseline: Some(effect("negative_stability")),
                growth_ceiling: false,
            },
        )]
        .into(),
        entities: initial
            .countries()
            .iter()
            .map(|country| {
                let value = (!unknown_scoped_history
                    && date
                        == (WorldDate {
                            year: 1444,
                            month: 11,
                            day: 11,
                        })
                    && histories.contains(&country.id)
                    && !unknown_history.contains(&country.id))
                .then_some(0);
                (
                    initial
                        .country_entity(&country.id)
                        .expect("registered country"),
                    BTreeMap::from([("stability".into(), value)]),
                )
            })
            .collect(),
        ceilings: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ContentRoot, TextDocument};
    use std::path::Path;

    fn write(root: &Path, relative: &str, contents: &str) {
        let path = root.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn history_macro_analysis_tracks_only_matching_metric_writes_and_scopes() {
        let temp = tempfile::tempdir().unwrap();
        write(
            temp.path(),
            "common/scripted_effects/test.txt",
            "direct={if={limit={always=yes} add_liberty_desire=10}} unrelated={add_stability=1} cross_country={every_country={add_liberty_desire=1}} direct_call={direct=yes} scoped_call={if={limit={always=yes} cross_country=yes}}",
        );
        let root = ContentRoot::open(temp.path()).unwrap();
        let files = root.files().unwrap();
        let (direct, scoped) =
            history_scripted_effects(&root, &files, &["add_liberty_desire"]).unwrap();
        assert!(direct.contains("direct"));
        assert!(direct.contains("direct_call"));
        assert!(!direct.contains("unrelated"));
        assert!(scoped.contains("cross_country"));
        assert!(scoped.contains("scoped_call"));

        for (history, expected) in [
            ("direct_call=yes", (true, false)),
            ("scoped_call=yes", (false, true)),
            ("unrelated=yes", (false, false)),
        ] {
            let document = TextDocument::from_bytes(
                "country-history.txt",
                history.as_bytes().to_vec(),
                TextEncoding::Utf8,
            );
            let parsed = document.parse().unwrap();
            assert_eq!(
                named_history_mutations_with_scripts(
                    parsed.reader(),
                    &BTreeSet::new(),
                    0,
                    &["liberty_desire", "add_liberty_desire"],
                    &direct,
                    &scoped,
                ),
                expected,
                "{history}"
            );
        }
    }
}
