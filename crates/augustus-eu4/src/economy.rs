// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{
    BuildingDefinition, BuildingRole, Economy, InitialWorld, ProvinceEconomy, TradeGoodDefinition,
    TradeNode,
};
use jomini::text::{ObjectReader, ValueReader};
use serde::Serialize;

use crate::bootstrap::{direct_txt, equal, nonnegative_milli, source};
use crate::{
    ContentView, Error, FileEntry, LoadedSource, ParsedText, SourceLocation, TextEncoding,
};

#[derive(Debug, Default, Serialize)]
pub struct EconomyReport {
    pub building_definitions: usize,
    pub trade_good_definitions: usize,
    pub trade_nodes: usize,
    pub trade_routes: usize,
    pub provinces_with_buildings: usize,
    pub building_instances: usize,
    pub provinces_with_trade_goods: usize,
    pub definition_sources: BTreeMap<String, LoadedSource>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub overridden_definition_sources: BTreeMap<String, Vec<LoadedSource>>,
    pub building_history_sources: BTreeMap<String, LoadedSource>,
    pub unapplied_definition_fields: BTreeMap<String, usize>,
    pub unapplied_examples: BTreeMap<String, LoadedSource>,
}

#[derive(Debug)]
struct NodeDraft {
    location: u32,
    members: BTreeSet<u32>,
    outgoing: BTreeSet<String>,
    outgoing_order: Vec<String>,
    inland: Option<bool>,
    end: Option<bool>,
}

#[derive(Default)]
pub(super) struct EconomyDraft {
    buildings: BTreeMap<String, BuildingDefinition>,
    goods: BTreeMap<String, TradeGoodDefinition>,
    nodes: BTreeMap<String, NodeDraft>,
    province_buildings: BTreeMap<u32, BTreeSet<String>>,
}

fn scalar(
    value: ValueReader<'_, '_, TextEncoding>,
    fail: &impl Fn(&str) -> Error,
) -> Result<String, Error> {
    value
        .read_string()
        .map_err(|_| fail("expected economic scalar"))
}

fn flag(
    value: ValueReader<'_, '_, TextEncoding>,
    fail: &impl Fn(&str) -> Error,
) -> Result<bool, Error> {
    match scalar(value, fail)?.as_str() {
        "yes" => Ok(true),
        "no" => Ok(false),
        _ => Err(fail("expected yes or no")),
    }
}

fn amount(
    value: ValueReader<'_, '_, TextEncoding>,
    fail: &impl Fn(&str) -> Error,
) -> Result<i64, Error> {
    nonnegative_milli(&scalar(value, fail)?)
        .ok_or_else(|| fail("expected nonnegative value with at most three decimal digits"))
}

fn ids(
    value: ValueReader<'_, '_, TextEncoding>,
    fail: &impl Fn(&str) -> Error,
) -> Result<BTreeSet<u32>, Error> {
    let mut ids = BTreeSet::new();
    for value in value
        .read_array()
        .map_err(|_| fail("expected province array"))?
        .values()
    {
        let id = scalar(value, fail)?
            .parse()
            .map_err(|_| fail("invalid province identifier"))?;
        if !ids.insert(id) {
            return Err(fail("duplicate trade node member"));
        }
    }
    Ok(ids)
}

fn unapplied(
    parsed: &ParsedText<'_>,
    kind: &str,
    name: &str,
    location: SourceLocation,
    report: &mut EconomyReport,
) {
    let key = format!("{kind}:{name}");
    *report
        .unapplied_definition_fields
        .entry(key.clone())
        .or_default() += 1;
    report
        .unapplied_examples
        .entry(key)
        .or_insert_with(|| source(parsed, location));
}

fn definitions(
    root: &impl ContentView,
    files: &[FileEntry],
    kind: &str,
    report: &mut EconomyReport,
    mut apply: impl FnMut(
        &str,
        &ParsedText<'_>,
        ObjectReader<'_, '_, TextEncoding>,
        &mut EconomyReport,
    ) -> Result<(), Error>,
) -> Result<(), Error> {
    let directory = format!("common/{kind}");
    for file in files
        .iter()
        .filter(|f| direct_txt(&f.relative_path, &directory))
    {
        let doc = root.read_text(&file.relative_path, TextEncoding::Windows1252)?;
        let parsed = doc.parse()?;
        for (key, op, value) in parsed.reader().fields() {
            let name = key.read_string();
            let location = parsed.field_location(&key);
            let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
            equal(op, &fail)?;
            if name.is_empty() || name.starts_with('@') {
                return Err(fail("unsupported economic definition identifier"));
            }
            let key = format!("{kind}:{name}");
            if report.definition_sources.contains_key(&key) && kind != "buildings" {
                return Err(fail(
                    "duplicate economic definition; selection is not validated",
                ));
            }
            if let Some(previous) = report
                .definition_sources
                .insert(key.clone(), source(&parsed, location))
            {
                report
                    .overridden_definition_sources
                    .entry(key)
                    .or_default()
                    .push(previous);
            }
            let reader = value
                .read_object()
                .map_err(|_| fail("economic definition requires a block"))?;
            apply(&name, &parsed, reader, report)?;
        }
    }
    Ok(())
}

impl EconomyDraft {
    pub(super) fn is_building(&self, name: &str) -> bool {
        self.buildings.contains_key(name)
    }

    pub(super) fn has_trade_good(&self, name: &str) -> bool {
        self.goods.contains_key(name)
    }

    pub(super) fn trade_node_route_orders(&self) -> BTreeMap<String, Vec<String>> {
        self.nodes
            .iter()
            .map(|(name, node)| (name.clone(), node.outgoing_order.clone()))
            .collect()
    }

    pub(super) fn load(
        root: &impl ContentView,
        files: &[FileEntry],
        report: &mut EconomyReport,
    ) -> Result<Self, Error> {
        let mut draft = Self::default();
        definitions(
            root,
            files,
            "buildings",
            report,
            |name, parsed, reader, report| {
                let mut building = BuildingDefinition::default();
                let mut seen = BTreeSet::new();
                for (key, op, value) in reader.fields() {
                    let field = key.read_string();
                    let location = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
                    if matches!(field.as_str(), "cost" | "time" | "influencing_fort") {
                        equal(op, &fail)?;
                        if !seen.insert(field.clone()) {
                            return Err(fail("duplicate building scalar"));
                        }
                        if field == "cost" {
                            building.cost_milli = Some(amount(value, &fail)?);
                        } else if field == "time" {
                            let months = scalar(value, &fail)?
                                .parse::<u32>()
                                .map_err(|_| fail("invalid construction months"))?;
                            if months == 0 {
                                return Err(fail("construction months must be positive"));
                            }
                            building.construction_months = Some(months);
                        } else if field == "influencing_fort" && flag(value, &fail)? {
                            building.role = BuildingRole::Fortification;
                        }
                    } else if field == "modifier" {
                        equal(op, &fail)?;
                        unapplied(parsed, "buildings", &field, location, report);
                        let modifier = value
                            .read_object()
                            .map_err(|_| fail("building modifier requires a block"))?;
                        for (modifier_key, modifier_op, modifier_value) in modifier
                            .fields()
                            .filter(|(key, _, _)| key.read_string() == "fort_level")
                        {
                            let modifier_at = parsed.field_location(&modifier_key);
                            let modifier_fail = |message: &str| {
                                Error::message(parsed.source(), message).with_location(modifier_at)
                            };
                            equal(modifier_op, &modifier_fail)?;
                            if !seen.insert("fort_level".into()) {
                                return Err(modifier_fail("duplicate fort level"));
                            }
                            building.fort_level_milli =
                                Some(amount(modifier_value, &modifier_fail)?);
                        }
                    } else {
                        unapplied(parsed, "buildings", &field, location, report);
                    }
                }
                draft.buildings.insert(name.into(), building);
                Ok(())
            },
        )?;
        definitions(
            root,
            files,
            "tradegoods",
            report,
            |name, parsed, reader, report| {
                let runtime_index = u32::try_from(draft.goods.len() + 1).map_err(|_| {
                    Error::message(
                        parsed.source(),
                        "trade-good runtime index exceeds the supported range",
                    )
                })?;
                draft.goods.insert(
                    name.into(),
                    TradeGoodDefinition {
                        runtime_index: Some(runtime_index),
                        ..Default::default()
                    },
                );
                for (key, _, _) in reader.fields() {
                    unapplied(
                        parsed,
                        "tradegoods",
                        &key.read_string(),
                        parsed.field_location(&key),
                        report,
                    );
                }
                Ok(())
            },
        )?;
        definitions(
            root,
            files,
            "prices",
            report,
            |name, parsed, reader, report| {
                let good = draft.goods.get_mut(name).ok_or_else(|| {
                    Error::message(
                        parsed.source(),
                        format!("price references unknown trade good {name}"),
                    )
                })?;
                let mut seen = BTreeSet::new();
                for (key, op, value) in reader.fields() {
                    let field = key.read_string();
                    let location = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
                    match field.as_str() {
                        "base_price" | "goldtype" => {
                            equal(op, &fail)?;
                            if !seen.insert(field.clone()) {
                                return Err(fail("duplicate price scalar"));
                            }
                            if field == "base_price" {
                                good.base_price_milli = Some(amount(value, &fail)?);
                            } else {
                                good.gold_type = Some(flag(value, &fail)?);
                            }
                        }
                        _ => unapplied(parsed, "prices", &field, location, report),
                    }
                }
                Ok(())
            },
        )?;
        definitions(
            root,
            files,
            "tradenodes",
            report,
            |name, parsed, reader, report| {
                let mut location = None;
                let mut members = None;
                let mut outgoing = BTreeSet::new();
                let mut outgoing_order = Vec::new();
                let mut inland = None;
                let mut end = None;
                let mut seen = BTreeSet::new();
                for (key, op, value) in reader.fields() {
                    let field = key.read_string();
                    let at = parsed.field_location(&key);
                    let fail = |m: &str| Error::message(parsed.source(), m).with_location(at);
                    if matches!(
                        field.as_str(),
                        "location" | "members" | "inland" | "end" | "outgoing"
                    ) {
                        equal(op, &fail)?;
                        if field != "outgoing" && !seen.insert(field.clone()) {
                            return Err(fail("duplicate trade node field"));
                        }
                        match field.as_str() {
                            "location" => {
                                location = Some(
                                    scalar(value, &fail)?
                                        .parse::<u32>()
                                        .map_err(|_| fail("invalid trade node location"))?,
                                )
                            }
                            "members" => members = Some(ids(value, &fail)?),
                            "inland" => inland = Some(flag(value, &fail)?),
                            "end" => end = Some(flag(value, &fail)?),
                            _ => {
                                let route = value
                                    .read_object()
                                    .map_err(|_| fail("trade route requires a block"))?;
                                let mut target = None;
                                for (key, op, value) in route.fields() {
                                    let field = key.read_string();
                                    let at = parsed.field_location(&key);
                                    let fail = |m: &str| {
                                        Error::message(parsed.source(), m).with_location(at)
                                    };
                                    if field == "name" {
                                        equal(op, &fail)?;
                                        if target.is_some() {
                                            return Err(fail("duplicate route destination"));
                                        }
                                        target = Some(scalar(value, &fail)?);
                                    } else {
                                        unapplied(
                                            parsed,
                                            "tradenodes.outgoing",
                                            &field,
                                            at,
                                            report,
                                        );
                                    }
                                }
                                let target = target
                                    .ok_or_else(|| fail("missing trade route destination"))?;
                                if !outgoing.insert(target.clone()) {
                                    return Err(fail("duplicate trade route"));
                                }
                                outgoing_order.push(target);
                            }
                        }
                    } else {
                        unapplied(parsed, "tradenodes", &field, at, report);
                    }
                }
                draft.nodes.insert(
                    name.into(),
                    NodeDraft {
                        location: location.ok_or_else(|| {
                            Error::message(
                                parsed.source(),
                                format!("missing location for trade node {name}"),
                            )
                        })?,
                        members: members.ok_or_else(|| {
                            Error::message(
                                parsed.source(),
                                format!("missing members for trade node {name}"),
                            )
                        })?,
                        outgoing,
                        outgoing_order,
                        inland,
                        end,
                    },
                );
                Ok(())
            },
        )?;
        report.building_definitions = draft.buildings.len();
        report.trade_good_definitions = draft.goods.len();
        report.trade_nodes = draft.nodes.len();
        report.trade_routes = draft.nodes.values().map(|n| n.outgoing.len()).sum();
        Ok(draft)
    }

    pub(super) fn apply_building(
        &mut self,
        parsed: &ParsedText<'_>,
        name: &str,
        value: ValueReader<'_, '_, TextEncoding>,
        province: u32,
        location: SourceLocation,
        report: &mut EconomyReport,
    ) -> Result<(), Error> {
        let fail = |m: &str| Error::message(parsed.source(), m).with_location(location);
        let set = self.province_buildings.entry(province).or_default();
        if flag(value, &fail)? {
            set.insert(name.into());
        } else {
            set.remove(name);
        }
        report
            .building_history_sources
            .insert(format!("{province}:{name}"), source(parsed, location));
        Ok(())
    }

    pub(super) fn bind(
        self,
        initial: &InitialWorld,
        known_history: &BTreeSet<u32>,
        report: &mut EconomyReport,
    ) -> Result<Economy, Error> {
        let province = |id| {
            initial.province_entity(id).ok_or_else(|| {
                Error::message(
                    "world",
                    format!("trade node references unknown province {id}"),
                )
            })
        };
        let mut economy = Economy {
            buildings: self.buildings,
            trade_goods: self.goods,
            ..Default::default()
        };
        for (name, draft) in self.nodes {
            let origin = &report.definition_sources[&format!("tradenodes:{name}")];
            let fail = |message: String| {
                Error::message(&origin.path, message).with_location(SourceLocation {
                    line: origin.line,
                    column: origin.column,
                })
            };
            let member = |id| {
                initial.province_entity(id).ok_or_else(|| {
                    fail(format!(
                        "trade node {name} references unknown province {id}"
                    ))
                })
            };
            for target in &draft.outgoing {
                if !report
                    .definition_sources
                    .contains_key(&format!("tradenodes:{target}"))
                {
                    return Err(fail(format!(
                        "unknown trade route destination {target} from {name}"
                    )));
                }
            }
            let node = TradeNode {
                location: member(draft.location)?,
                members: draft
                    .members
                    .into_iter()
                    .map(&member)
                    .collect::<Result<_, _>>()?,
                outgoing: draft.outgoing,
                inland: draft.inland,
                end: draft.end,
            };
            economy.trade_nodes.insert(name, node);
        }
        for &id in known_history {
            let state = ProvinceEconomy {
                buildings: self
                    .province_buildings
                    .get(&id)
                    .cloned()
                    .unwrap_or_default(),
                trade_good: initial
                    .province(id)
                    .expect("registered province")
                    .trade_good
                    .clone(),
            };
            if let Some(good) = &state.trade_good
                && !economy.trade_goods.contains_key(good)
            {
                return Err(Error::message(
                    "world",
                    format!("province {id} references unknown trade good {good}"),
                ));
            }
            report.building_instances += state.buildings.len();
            report.provinces_with_buildings += usize::from(!state.buildings.is_empty());
            report.provinces_with_trade_goods += usize::from(state.trade_good.is_some());
            economy.provinces.insert(province(id)?, state);
        }
        Ok(economy)
    }
}
