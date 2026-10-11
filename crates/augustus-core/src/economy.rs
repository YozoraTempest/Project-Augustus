// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildingDefinition {
    pub cost_milli: Option<i64>,
    pub construction_months: Option<u32>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeGoodDefinition {
    pub base_price_milli: Option<i64>,
    pub gold_type: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TradeNode {
    pub location: EntityId,
    pub members: BTreeSet<EntityId>,
    pub outgoing: BTreeSet<String>,
    pub inland: Option<bool>,
    pub end: Option<bool>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvinceEconomy {
    pub buildings: BTreeSet<String>,
    pub trade_good: Option<String>,
}

/// Shared economic inputs and current province state, without input syntax or paths.
/// Prices are definition values; no production or trade settlement is implied.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Economy {
    pub buildings: BTreeMap<String, BuildingDefinition>,
    pub trade_goods: BTreeMap<String, TradeGoodDefinition>,
    pub trade_nodes: BTreeMap<String, TradeNode>,
    pub provinces: BTreeMap<EntityId, ProvinceEconomy>,
}

impl Economy {
    pub fn is_empty(&self) -> bool {
        self.buildings.is_empty()
            && self.trade_goods.is_empty()
            && self.trade_nodes.is_empty()
            && self.provinces.is_empty()
    }

    pub fn province_node(&self, province: EntityId) -> Option<&str> {
        self.trade_nodes
            .iter()
            .find_map(|(name, node)| node.members.contains(&province).then_some(name.as_str()))
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        let fail = |message: &str| Error::Invalid(message.into());
        let province = |id| -> Result<(), Error> {
            if world.entity(id)?.kind != EntityKind::Province {
                return Err(fail("economy requires a province entity"));
            }
            Ok(())
        };
        if self
            .buildings
            .keys()
            .chain(self.trade_goods.keys())
            .chain(self.trade_nodes.keys())
            .any(|name| name.is_empty())
        {
            return Err(fail("empty economic definition identifier"));
        }
        if self
            .buildings
            .values()
            .any(|b| b.cost_milli.is_some_and(|v| v < 0) || b.construction_months == Some(0))
            || self
                .trade_goods
                .values()
                .any(|g| g.base_price_milli.is_some_and(|v| v < 0))
        {
            return Err(fail("invalid economic definition value"));
        }
        for (&id, state) in &self.provinces {
            province(id)?;
            if state
                .buildings
                .iter()
                .any(|b| !self.buildings.contains_key(b))
                || state
                    .trade_good
                    .as_ref()
                    .is_some_and(|g| !self.trade_goods.contains_key(g))
            {
                return Err(fail("unknown province economic definition"));
            }
        }
        let mut members = BTreeSet::new();
        let mut indegree: BTreeMap<&str, usize> = self
            .trade_nodes
            .keys()
            .map(|name| (name.as_str(), 0))
            .collect();
        for (name, node) in &self.trade_nodes {
            province(node.location)?;
            if node.end == Some(true) && !node.outgoing.is_empty() {
                return Err(fail("terminal trade node has outgoing routes"));
            }
            for &id in &node.members {
                province(id)?;
                if !members.insert(id) {
                    return Err(fail("province belongs to multiple trade nodes"));
                }
            }
            for target in &node.outgoing {
                if target == name {
                    return Err(fail("trade node routes to itself"));
                }
                *indegree
                    .get_mut(target.as_str())
                    .ok_or_else(|| fail("unknown trade route destination"))? += 1;
            }
        }
        // Validate the network in linear graph traversal, without recursive depth limits.
        let mut ready: Vec<&str> = indegree
            .iter()
            .filter_map(|(&name, &degree)| (degree == 0).then_some(name))
            .collect();
        let mut visited = 0;
        while let Some(name) = ready.pop() {
            visited += 1;
            for target in &self.trade_nodes[name].outgoing {
                let degree = indegree.get_mut(target.as_str()).expect("validated route");
                *degree -= 1;
                if *degree == 0 {
                    ready.push(target);
                }
            }
        }
        if visited != self.trade_nodes.len() {
            return Err(fail("cyclic trade network"));
        }
        Ok(())
    }
}
