// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Caller-resolved province gold-stage queries.

use augustus_core::{
    VanillaGoldBaseInput, VanillaGoldBaseResult, VanillaGoldProductionInput,
    VanillaGoldProductionResult, calculate_vanilla_gold_base, calculate_vanilla_gold_production,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaGoldBaseQuery {
    pub province: u32,
    pub input: VanillaGoldBaseInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaGoldProductionQuery {
    pub province: u32,
    pub input: VanillaGoldProductionInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VanillaGoldStageQueryMode {
    Base,
    Production,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    content = "query",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum VanillaGoldStageQuery {
    Base(VanillaGoldBaseQuery),
    Production(VanillaGoldProductionQuery),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum VanillaGoldStageEvaluation {
    Base(VanillaGoldBaseResult),
    Production(VanillaGoldProductionResult),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaGoldStageQueryReport {
    pub mode: VanillaGoldStageQueryMode,
    pub province: u32,
    pub trade_good: Option<String>,
    pub gold_type: Option<bool>,
    pub evaluation: Option<VanillaGoldStageEvaluation>,
    pub unresolved: Vec<String>,
}

impl crate::VanillaWorld {
    fn gold_stage_trade_good(
        &self,
        province: u32,
    ) -> Result<Option<(String, bool)>, augustus_core::Error> {
        let entity = self
            .initial
            .province_entity(province)
            .ok_or_else(|| augustus_core::Error::Invalid("unknown province".into()))?;
        let economy = self.initial.world().economy();
        let Some(trade_good) = economy
            .provinces
            .get(&entity)
            .and_then(|state| state.trade_good.as_deref())
        else {
            return Ok(None);
        };
        let definition = economy.trade_goods.get(trade_good).ok_or_else(|| {
            augustus_core::Error::Invalid(format!(
                "province {province} references unknown trade good {trade_good}"
            ))
        })?;
        Ok(Some((
            trade_good.into(),
            definition.gold_type == Some(true),
        )))
    }

    pub fn query_vanilla_gold_stage(
        &self,
        query: &VanillaGoldStageQuery,
    ) -> Result<VanillaGoldStageQueryReport, crate::Error> {
        let (mode, province, input) = match query {
            VanillaGoldStageQuery::Base(query) => (
                VanillaGoldStageQueryMode::Base,
                query.province,
                VanillaGoldStageInput::Base(query.input),
            ),
            VanillaGoldStageQuery::Production(query) => (
                VanillaGoldStageQueryMode::Production,
                query.province,
                VanillaGoldStageInput::Production(query.input),
            ),
        };
        let trade_good = self
            .gold_stage_trade_good(province)
            .map_err(|error| crate::Error::caused_by("gold stage query", error))?;
        let (trade_good_name, gold) = trade_good.clone().unwrap_or_else(|| (String::new(), false));
        if trade_good.is_none() {
            return Ok(gold_stage_report(
                mode,
                province,
                None,
                None,
                None,
                vec!["gold_stage:missing_trade_good".into()],
            ));
        }
        if !gold {
            return Ok(gold_stage_report(
                mode,
                province,
                Some(trade_good_name),
                Some(false),
                None,
                vec!["gold_stage:not_gold".into()],
            ));
        }
        let evaluation = match input {
            VanillaGoldStageInput::Base(input) => {
                VanillaGoldStageEvaluation::Base(calculate_vanilla_gold_base(input))
            }
            VanillaGoldStageInput::Production(input) => {
                VanillaGoldStageEvaluation::Production(calculate_vanilla_gold_production(input))
            }
        };
        Ok(gold_stage_report(
            mode,
            province,
            Some(trade_good_name),
            Some(true),
            Some(evaluation),
            Vec::new(),
        ))
    }
}

enum VanillaGoldStageInput {
    Base(VanillaGoldBaseInput),
    Production(VanillaGoldProductionInput),
}

fn gold_stage_report(
    mode: VanillaGoldStageQueryMode,
    province: u32,
    trade_good: Option<String>,
    gold_type: Option<bool>,
    evaluation: Option<VanillaGoldStageEvaluation>,
    unresolved: Vec<String>,
) -> VanillaGoldStageQueryReport {
    VanillaGoldStageQueryReport {
        mode,
        province,
        trade_good,
        gold_type,
        evaluation,
        unresolved,
    }
}
