// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{DiplomaticKind, InitialWorld, Politics, WorldDate};
use jomini::common::Date;
use jomini::text::ObjectReader;

use crate::bootstrap::{dated_name, direct_txt, equal, source};
use crate::setup::{block, flag};
use crate::{ContentView, Error, FileEntry, IdeaReport, ParsedText, TextEncoding};

const AT_WAR_STATUS: &str = "at_war";

#[derive(Debug, Clone, Copy)]
enum WarSide {
    Attacker,
    Defender,
}

#[derive(Debug, Clone)]
struct WarChange {
    side: WarSide,
    add: bool,
    country: String,
}

#[derive(Debug, Clone)]
struct WarEvent {
    date: WorldDate,
    changes: Vec<WarChange>,
}

#[derive(Debug, Default)]
struct WarHistory {
    events: Vec<WarEvent>,
}

#[derive(Default)]
struct History {
    revolution_target: bool,
    elector: Option<bool>,
    uncertain: BTreeSet<String>,
}

#[derive(Default)]
pub(super) struct CountryStatusDraft {
    subjects: BTreeMap<String, Option<Vec<(String, String)>>>,
    histories: BTreeMap<String, History>,
    pending_origins: BTreeSet<String>,
    wars: Option<Vec<WarHistory>>,
}

impl CountryStatusDraft {
    pub(super) fn load(
        root: &impl ContentView,
        files: &[FileEntry],
        report: &mut IdeaReport,
    ) -> Result<Self, Error> {
        let mut result = Self {
            wars: load_wars(root, files)?,
            ..Default::default()
        };
        for file in files
            .iter()
            .filter(|f| direct_txt(&f.relative_path, "common/subject_types"))
        {
            let doc = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
            let parsed = doc.parse()?;
            for (key, op, value) in parsed.reader().fields() {
                let at = parsed.field_location(&key);
                let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                equal(op, &fail)?;
                let body = block(&value, &fail)?;
                // Empty forward declarations are not complete definitions.
                if body.fields().next().is_none() {
                    continue;
                }
                let mut fields = Vec::new();
                let mut seen = BTreeSet::new();
                let mut known = true;
                for (key, op, value) in body.fields().filter(|(k, _, _)| {
                    matches!(
                        k.read_string().as_str(),
                        "copy_from" | "has_colonial_parent"
                    )
                }) {
                    let name = key.read_string();
                    equal(op, &fail)?;
                    let text = value
                        .read_string()
                        .map_err(|_| fail("expected subject status scalar"))?;
                    if name == "has_colonial_parent" && flag(&text).is_none() {
                        return Err(fail("expected colonial parent flag"));
                    }
                    known &= seen.insert(name.clone());
                    fields.push((name, text));
                }
                let id = key.read_string();
                result
                    .subjects
                    .entry(id.clone())
                    .and_modify(|v| *v = None)
                    .or_insert(known.then_some(fields));
                report
                    .status_sources
                    .insert(format!("subject:{id}"), source(&parsed, at));
            }
        }
        Ok(result)
    }

    pub(super) fn country_history(
        &mut self,
        parsed: &ParsedText<'_>,
        reader: ObjectReader<'_, '_, TextEncoding>,
        tag: &str,
        report: &mut IdeaReport,
    ) -> Result<(), Error> {
        let history = self.histories.entry(tag.into()).or_default();
        for (key, op, value) in reader.fields() {
            let name = key.read_string();
            if dated_name(&name) {
                continue;
            }
            let at = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
            match name.as_str() {
                "elector" => {
                    equal(op, &fail)?;
                    history.elector = Some(
                        flag(
                            &value
                                .read_string()
                                .map_err(|_| fail("expected elector flag"))?,
                        )
                        .ok_or_else(|| fail("expected yes or no"))?,
                    );
                    history.uncertain.remove("elector");
                    report
                        .status_sources
                        .insert(format!("country:{tag}:elector"), source(parsed, at));
                }
                "revolution_target" => {
                    equal(op, &fail)?;
                    history.revolution_target = flag(
                        &value
                            .read_string()
                            .map_err(|_| fail("expected revolution target flag"))?,
                    )
                    .ok_or_else(|| fail("expected yes or no"))?;
                    history.uncertain.remove("revolution_target");
                    report.status_sources.insert(
                        format!("country:{tag}:revolution_target"),
                        source(parsed, at),
                    );
                }
                _ => {
                    if let Some(status) = creation_status(&name) {
                        self.pending_origins.insert(status.into());
                    }
                    if let Ok(children) = value.read_object() {
                        scan(children, history, &mut self.pending_origins, 0)?;
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn bind(
        self,
        initial: &InitialWorld,
        date: WorldDate,
        report: &mut IdeaReport,
    ) -> Result<Politics, Error> {
        let mut politics = initial.world().politics().clone();
        let active_wars = self.active_war_participants(initial, date);
        let baseline = date
            == (WorldDate {
                year: 1444,
                month: 11,
                day: 11,
            });
        let mut targets = Vec::new();
        for country in initial.countries() {
            let entity = initial
                .country_entity(&country.id)
                .expect("registered country");
            let history = self.histories.get(&country.id);
            let mut statuses: BTreeMap<String, Option<bool>> = BTreeMap::new();
            // The fixed bookmark constructs the registered historical nations,
            // not nations generated by client-state creation or federation union.
            // Later bookmarks and unexecuted creation effects retain unknown origins.
            for name in ["client_nation", "federation_nation"] {
                let value = (baseline && history.is_some() && !self.pending_origins.contains(name))
                    .then_some(false);
                statuses.insert(name.into(), value);
            }
            let subject = politics
                .relations
                .iter()
                .find(|r| r.kind == DiplomaticKind::Subject && r.second == entity);
            let colonial = if history.is_none() || self.pending_origins.contains("colonial_nation")
            {
                None
            } else if let Some(subject) = subject {
                subject
                    .subject_type
                    .as_ref()
                    .and_then(|name| colonial(name, &self.subjects, &mut BTreeSet::new()))
            } else {
                Some(false)
            };
            statuses.insert("colonial_nation".into(), colonial);
            let target = history
                .filter(|h| {
                    !h.uncertain.contains("revolution_target")
                        && !self.pending_origins.contains("revolution_target")
                })
                .map(|h| h.revolution_target);
            if target == Some(true) {
                targets.push(country.id.clone());
            }
            statuses.insert("revolution_target".into(), target);
            let elector = history
                .filter(|history| !history.uncertain.contains("elector"))
                .and_then(|history| history.elector);
            statuses.insert("elector".into(), elector);
            let at_war = active_wars
                .as_ref()
                .map(|participants| participants.contains(&country.id));
            statuses.insert(AT_WAR_STATUS.into(), at_war);
            for (name, value) in &statuses {
                if value.is_none() {
                    report
                        .unknown_statuses
                        .entry(country.id.clone())
                        .or_default()
                        .insert(name.clone());
                }
            }
            politics.countries.entry(entity).or_default().statuses = statuses;
        }
        if targets.len() > 1 {
            return Err(Error::message(
                "history/countries",
                format!("multiple revolution targets: {}", targets.join(", ")),
            ));
        }
        Ok(politics)
    }

    fn active_war_participants(
        &self,
        initial: &InitialWorld,
        date: WorldDate,
    ) -> Option<BTreeSet<String>> {
        let histories = self.wars.as_ref()?;
        let mut participants = BTreeSet::new();
        for history in histories {
            let mut attackers = BTreeSet::new();
            let mut defenders = BTreeSet::new();
            for event in history.events.iter().filter(|event| event.date <= date) {
                for change in &event.changes {
                    let side = match change.side {
                        WarSide::Attacker => &mut attackers,
                        WarSide::Defender => &mut defenders,
                    };
                    if change.add {
                        side.insert(change.country.clone());
                    } else {
                        side.remove(&change.country);
                    }
                }
            }
            if !attackers.is_empty() && !defenders.is_empty() {
                for country in attackers.union(&defenders) {
                    initial.country_entity(country)?;
                    participants.insert(country.clone());
                }
            }
        }
        Some(participants)
    }
}

fn load_wars(
    root: &impl ContentView,
    files: &[FileEntry],
) -> Result<Option<Vec<WarHistory>>, Error> {
    let mut histories = Vec::new();
    for file in files
        .iter()
        .filter(|file| direct_txt(&file.relative_path, "history/wars"))
    {
        let document = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = document.parse()?;
        let mut events = Vec::new();
        let root_changes = war_changes(&parsed, parsed.reader())?;
        if !root_changes.is_empty() {
            events.push(WarEvent {
                date: WorldDate {
                    year: 1,
                    month: 1,
                    day: 1,
                },
                changes: root_changes,
            });
        }
        for (key, op, value) in parsed.reader().fields() {
            let name = key.read_string();
            if !dated_name(&name) {
                continue;
            }
            let location = parsed.field_location(&key);
            let fail =
                |message: &str| Error::message(parsed.source(), message).with_location(location);
            equal(op, &fail)?;
            let date = Date::parse(&name).map_err(|_| fail("invalid war history date"))?;
            let block = value
                .read_object()
                .map_err(|_| fail("dated war history requires a block"))?;
            events.push(WarEvent {
                date: crate::politics::label(date),
                changes: war_changes(&parsed, block)?,
            });
        }
        events.sort_by_key(|event| event.date);
        histories.push(WarHistory { events });
    }
    Ok((!histories.is_empty()).then_some(histories))
}

fn war_changes(
    parsed: &ParsedText<'_>,
    reader: ObjectReader<'_, '_, TextEncoding>,
) -> Result<Vec<WarChange>, Error> {
    let mut changes = Vec::new();
    for (key, op, value) in reader.fields() {
        let name = key.read_string();
        let (side, add) = match name.as_str() {
            "add_attacker" => (WarSide::Attacker, true),
            "rem_attacker" => (WarSide::Attacker, false),
            "add_defender" => (WarSide::Defender, true),
            "rem_defender" => (WarSide::Defender, false),
            _ => continue,
        };
        let location = parsed.field_location(&key);
        let fail = |message: &str| Error::message(parsed.source(), message).with_location(location);
        equal(op, &fail)?;
        let country = value
            .read_string()
            .map_err(|_| fail("war participant must be a country tag"))?;
        if country.is_empty() || country == "---" {
            return Err(fail("war participant requires a country tag"));
        }
        changes.push(WarChange { side, add, country });
    }
    Ok(changes)
}

fn colonial(
    name: &str,
    definitions: &BTreeMap<String, Option<Vec<(String, String)>>>,
    stack: &mut BTreeSet<String>,
) -> Option<bool> {
    if stack.len() >= 64 || !stack.insert(name.into()) {
        return None;
    }
    let result = (|| {
        let fields = definitions.get(name)?.as_ref()?;
        let mut value = if name == "default" {
            Some(false)
        } else {
            colonial("default", definitions, stack)
        };
        for (key, text) in fields {
            value = if key == "copy_from" {
                colonial(text, definitions, stack)
            } else {
                flag(text)
            };
        }
        value
    })();
    stack.remove(name);
    result
}

fn creation_status(name: &str) -> Option<&'static str> {
    match name {
        "create_client_state" => Some("client_nation"),
        "create_federation" => Some("federation_nation"),
        "create_colonial_nation" => Some("colonial_nation"),
        _ => None,
    }
}

fn scan(
    reader: ObjectReader<'_, '_, TextEncoding>,
    history: &mut History,
    origins: &mut BTreeSet<String>,
    depth: usize,
) -> Result<(), Error> {
    if depth > 64 {
        return Err(Error::message(
            "history/countries",
            "country status history nesting exceeds limit",
        ));
    }
    for (key, _, value) in reader.fields() {
        let name = key.read_string();
        if let Some(status) = creation_status(&name) {
            origins.insert(status.into());
        }
        if name == "revolution_target" {
            history.uncertain.insert("revolution_target".into());
            origins.insert("revolution_target".into());
        }
        if name == "elector" {
            history.uncertain.insert("elector".into());
        }
        if let Ok(children) = value.read_object() {
            scan(children, history, origins, depth + 1)?;
        }
    }
    Ok(())
}
