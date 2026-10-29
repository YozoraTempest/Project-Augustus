// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::{
    Error, VanillaNativeMigrationCostInput, VanillaNativeMigrationCostResult,
    calculate_vanilla_native_migration_cost,
};

/// Explicit context for the native migration command gate.
///
/// The compatibility layer supplies every context flag. An omitted optional
/// flag remains unknown; this calculator never derives eligibility from a
/// missing owner, controller, or native membership value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationGateInput {
    pub source_country: Option<crate::EntityId>,
    pub current_capital: Option<u32>,
    pub destination_province: Option<u32>,
    pub cost: VanillaNativeMigrationCostInput,
    pub native_membership_known: bool,
    pub native_mode_eligible: Option<bool>,
    pub owner_eligible: Option<bool>,
    pub controller_eligible: Option<bool>,
    pub tribal_owner_eligible: Option<bool>,
    pub additional_gate_eligible: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaNativeMigrationGateResult {
    pub source_country: Option<crate::EntityId>,
    pub current_capital: Option<u32>,
    pub destination_province: Option<u32>,
    /// Absent when native membership has not been resolved.
    pub cost: Option<VanillaNativeMigrationCostResult>,
    pub native_membership_known: bool,
    pub native_mode_eligible: Option<bool>,
    pub owner_eligible: Option<bool>,
    pub controller_eligible: Option<bool>,
    pub tribal_owner_eligible: Option<bool>,
    pub additional_gate_eligible: Option<bool>,
    pub cost_eligible: Option<bool>,
    pub resolved_gate: Option<bool>,
    pub unresolved: Vec<String>,
    pub rejected: Vec<String>,
}

fn record_gate(
    value: Option<bool>,
    unknown_reason: &str,
    rejected_reason: &str,
    unresolved: &mut Vec<String>,
    rejected: &mut Vec<String>,
) {
    match value {
        Some(true) => {}
        Some(false) => rejected.push(rejected_reason.into()),
        None => unresolved.push(unknown_reason.into()),
    }
}

/// Calculates a read-only native migration command-gate report.
///
/// Unresolved membership suppresses cost calculation; the membership boolean
/// inside the cost input is ignored until it is known. Other eligibility flags
/// are caller assertions, not conditions discovered or verified here.
///
/// A known cost failure or explicit false context rejects the command. When
/// the cost passes but any required context is unknown, the result is
/// indeterminate instead of treating the missing state as false or true.
pub fn calculate_vanilla_native_migration_gate(
    input: VanillaNativeMigrationGateInput,
) -> Result<VanillaNativeMigrationGateResult, Error> {
    let cost = input
        .native_membership_known
        .then(|| calculate_vanilla_native_migration_cost(input.cost))
        .transpose()?;
    let cost_eligible = cost.map(|result| result.eligible);
    let mut unresolved = Vec::new();
    let mut rejected = Vec::new();

    if input.source_country.is_none() {
        unresolved.push("source_country_unknown".into());
    }
    if input.current_capital.is_none() {
        unresolved.push("current_capital_unknown".into());
    }
    if input.destination_province.is_none() {
        unresolved.push("destination_province_unknown".into());
    }
    if !input.native_membership_known {
        unresolved.push("native_membership_unknown".into());
    }
    record_gate(
        input.native_mode_eligible,
        "native_mode_unknown",
        "native_mode_ineligible",
        &mut unresolved,
        &mut rejected,
    );
    record_gate(
        input.owner_eligible,
        "owner_unknown",
        "owner_ineligible",
        &mut unresolved,
        &mut rejected,
    );
    record_gate(
        input.controller_eligible,
        "controller_unknown",
        "controller_ineligible",
        &mut unresolved,
        &mut rejected,
    );
    record_gate(
        input.tribal_owner_eligible,
        "tribal_owner_unknown",
        "tribal_owner_ineligible",
        &mut unresolved,
        &mut rejected,
    );
    record_gate(
        input.additional_gate_eligible,
        "additional_gate_unknown",
        "additional_gate_ineligible",
        &mut unresolved,
        &mut rejected,
    );

    if cost_eligible == Some(false) {
        rejected.insert(0, "cost_ineligible".into());
    }
    let resolved_gate = if !rejected.is_empty() {
        Some(false)
    } else if unresolved.is_empty() {
        Some(true)
    } else {
        None
    };

    Ok(VanillaNativeMigrationGateResult {
        source_country: input.source_country,
        current_capital: input.current_capital,
        destination_province: input.destination_province,
        cost,
        native_membership_known: input.native_membership_known,
        native_mode_eligible: input.native_mode_eligible,
        owner_eligible: input.owner_eligible,
        controller_eligible: input.controller_eligible,
        tribal_owner_eligible: input.tribal_owner_eligible,
        additional_gate_eligible: input.additional_gate_eligible,
        cost_eligible,
        resolved_gate,
        unresolved,
        rejected,
    })
}
