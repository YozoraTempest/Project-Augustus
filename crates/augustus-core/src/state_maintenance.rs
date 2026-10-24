// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use std::collections::{BTreeMap, BTreeSet};

use crate::{EntityId, EntityKind, Error, World};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaStateMaintenanceProvinceInputState {
    pub input: Option<VanillaStateMaintenanceInput>,
    #[serde(default)]
    pub unresolved: BTreeSet<String>,
}

/// Resolved input for the verified province contribution to state maintenance.
///
/// The compatibility layer supplies content, geography, relation, and distance
/// values. The core owns the fixed-point arithmetic and truncation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaStateMaintenanceInput {
    pub base_tax_milli: i32,
    pub base_production_milli: i32,
    pub base_manpower_milli: i32,
    pub development_scale_milli: i32,
    /// Present only when the source's special development multiplier applies.
    pub special_development_scale_milli: Option<i32>,
    pub distance: Option<VanillaStateMaintenanceDistanceInput>,
    /// Resolved modifiers from the province's region and owner-relation context.
    pub region_difference_modifier_milli: i32,
    pub owner_relation_modifier_milli: i32,
    pub country_key61_modifier_milli: i32,
    pub province_key62_modifier_milli: i32,
    pub caller_extra_modifier_milli: i32,
}

/// Resolved output of the province-distance service.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaStateMaintenanceDistanceInput {
    pub value: i32,
    /// The native zero mode applies square root before fixed-point scaling.
    pub square_root_mode: bool,
    pub modifier_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaStateMaintenanceResult {
    pub base_sum_milli: i64,
    pub scaled_base_milli: i32,
    pub distance_term_milli: i32,
    pub final_factor_milli: i32,
    pub maintenance_milli: i32,
}

/// Reproduces one province's verified contribution to state maintenance.
///
/// Each division truncates toward zero. The source stores the factor and final
/// value as signed 32-bit integers; the final contribution is clamped at zero.
pub fn calculate_vanilla_state_maintenance(
    input: VanillaStateMaintenanceInput,
) -> Result<VanillaStateMaintenanceResult, Error> {
    let base_sum = i64::from(input.base_tax_milli)
        + i64::from(input.base_production_milli)
        + i64::from(input.base_manpower_milli);
    let scaled_base_numerator = base_sum
        .checked_mul(i64::from(input.development_scale_milli))
        .ok_or(Error::Overflow)?;
    let mut scaled_base = (scaled_base_numerator / 1_000) as i32;
    if let Some(special_scale) = input.special_development_scale_milli {
        scaled_base =
            ((i64::from(scaled_base) * (i64::from(special_scale) + 1_000)) / 1_000) as i32;
    }

    let distance_term = input
        .distance
        .map(|distance| {
            let value_milli = if distance.square_root_mode {
                if distance.value < 0 {
                    return Err(Error::Invalid(
                        "square-root state-maintenance distance must be nonnegative".into(),
                    ));
                }
                ((distance.value as f32).sqrt() * 1_000.0_f32 + 0.5_f32) as i32
            } else {
                distance.value.wrapping_mul(1_000)
            };
            Ok(((i64::from(value_milli) * i64::from(distance.modifier_milli)) / 1_000) as i32)
        })
        .transpose()?
        .unwrap_or(0);

    let final_factor = input
        .country_key61_modifier_milli
        .wrapping_add(input.province_key62_modifier_milli)
        .wrapping_add(input.caller_extra_modifier_milli)
        .wrapping_add(1_000)
        .wrapping_add(input.region_difference_modifier_milli)
        .wrapping_add(distance_term)
        .wrapping_add(input.owner_relation_modifier_milli);
    let maintenance = ((i64::from(scaled_base) * i64::from(final_factor)) / 1_000) as i32;

    Ok(VanillaStateMaintenanceResult {
        base_sum_milli: base_sum,
        scaled_base_milli: scaled_base,
        distance_term_milli: distance_term,
        final_factor_milli: final_factor,
        maintenance_milli: maintenance.max(0),
    })
}

/// Calculates the complete state-maintenance expense for one country from
/// its currently owned state provinces. A missing state-membership or
/// province input keeps the country total unknown.
pub fn calculate_vanilla_state_maintenance_for_country(
    world: &World,
    country: EntityId,
    inputs: &BTreeMap<EntityId, VanillaStateMaintenanceProvinceInputState>,
) -> Result<Option<i64>, Error> {
    if world.entity(country)?.kind != EntityKind::Country {
        return Err(Error::Invalid(
            "state-maintenance calculation requires a country".into(),
        ));
    }

    let mut total = 0_i64;
    for province in world.owned_provinces(country)? {
        let Some(province_state) = world.administration().provinces.get(province) else {
            return Ok(None);
        };
        let Some(area) = province_state.area.as_ref() else {
            return Ok(None);
        };
        let Some(status) = world
            .administration()
            .country_areas
            .get(&country)
            .and_then(|areas| areas.get(area))
        else {
            return Ok(None);
        };
        if *status != crate::AreaStatus::State {
            continue;
        }

        let Some(state) = inputs.get(province) else {
            return Ok(None);
        };
        if !state.unresolved.is_empty() {
            return Ok(None);
        }
        let Some(input) = state.input else {
            return Ok(None);
        };
        let contribution = i64::from(calculate_vanilla_state_maintenance(input)?.maintenance_milli);
        total = total.checked_add(contribution).ok_or(Error::Overflow)?;
    }
    Ok(Some(total))
}
