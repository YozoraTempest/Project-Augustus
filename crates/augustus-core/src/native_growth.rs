// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::Error;

const DEVASTATION_CAP_MILLI: i64 = 100_000;
const SCALE_MILLI: i64 = 1_000;

/// Explicit inputs for the verified native province-growth cache stage.
///
/// The caller resolves native eligibility, the selected native component,
/// territory contribution, and the net monthly devastation change. The core
/// only applies the observed arithmetic and does not mutate metrics or schedule
/// a monthly system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeGrowthCacheInput {
    pub base_progress_milli: i32,
    pub territory_bonus_milli: i32,
    pub current_devastation_milli: i32,
    pub net_monthly_devastation_milli: i32,
    pub devastation_burn_milli: i32,
    pub selected_native_development_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaNativeGrowthCacheResult {
    pub combined_base_progress_milli: i32,
    pub devastation_difference_milli: i64,
    pub high_branch: bool,
    pub ratio_milli: Option<i64>,
    pub progress_milli: i32,
    pub cache_milli: i32,
}

/// Calculates the verified native province-growth cache stage.
///
/// All division uses Rust's signed truncation toward zero. Current devastation
/// at or above 100000 blocks cache production. Otherwise, a difference at or
/// above 100000 enters the high-devastation branch; zero burn uses the observed
/// ratio of -1, while negative burn is invalid input. A selected native
/// component below 1000 passes the progress through unchanged; values at or
/// above 1000 normalize the progress by that component.
pub fn calculate_vanilla_native_growth_cache(
    input: VanillaNativeGrowthCacheInput,
) -> Result<VanillaNativeGrowthCacheResult, Error> {
    if input.base_progress_milli < 0 {
        return Err(Error::Invalid(
            "native growth base progress must be nonnegative".into(),
        ));
    }
    if input.territory_bonus_milli < 0 {
        return Err(Error::Invalid(
            "native growth territory bonus must be nonnegative".into(),
        ));
    }
    if input.current_devastation_milli < 0 {
        return Err(Error::Invalid(
            "native growth devastation must be nonnegative".into(),
        ));
    }
    if input.devastation_burn_milli < 0 {
        return Err(Error::Invalid(
            "native growth burn must be nonnegative".into(),
        ));
    }
    if input.selected_native_development_milli <= 0 {
        return Err(Error::Invalid(
            "native growth component must be positive".into(),
        ));
    }

    let combined_base_progress_milli = input
        .base_progress_milli
        .checked_add(input.territory_bonus_milli)
        .ok_or(Error::Overflow)?;
    let devastation_difference_milli = i64::from(input.current_devastation_milli)
        .checked_sub(i64::from(input.net_monthly_devastation_milli))
        .ok_or(Error::Overflow)?;

    if i64::from(input.current_devastation_milli) >= DEVASTATION_CAP_MILLI {
        return Ok(VanillaNativeGrowthCacheResult {
            combined_base_progress_milli,
            devastation_difference_milli,
            high_branch: false,
            ratio_milli: None,
            progress_milli: 0,
            cache_milli: 0,
        });
    }

    let high_branch = devastation_difference_milli >= DEVASTATION_CAP_MILLI;
    let (ratio_milli, progress_milli) = if high_branch {
        let ratio_milli = if input.devastation_burn_milli == 0 {
            -1
        } else {
            devastation_difference_milli
                .checked_mul(SCALE_MILLI)
                .ok_or(Error::Overflow)?
                / i64::from(input.devastation_burn_milli)
        };
        let progress_milli = ratio_milli
            .checked_mul(i64::from(combined_base_progress_milli))
            .ok_or(Error::Overflow)?
            / SCALE_MILLI;
        let progress_milli = i32::try_from(progress_milli).map_err(|_| Error::Overflow)?;
        if progress_milli < 0 {
            return Err(Error::Invalid(
                "negative native growth progress is unsupported".into(),
            ));
        }
        (Some(ratio_milli), progress_milli)
    } else {
        (None, combined_base_progress_milli)
    };

    let cache_milli = if input.selected_native_development_milli >= 1_000 {
        let cache_milli = i64::from(progress_milli)
            .checked_mul(SCALE_MILLI)
            .ok_or(Error::Overflow)?
            / i64::from(input.selected_native_development_milli);
        i32::try_from(cache_milli).map_err(|_| Error::Overflow)?
    } else {
        progress_milli
    };

    Ok(VanillaNativeGrowthCacheResult {
        combined_base_progress_milli,
        devastation_difference_milli,
        high_branch,
        ratio_milli,
        progress_milli,
        cache_milli,
    })
}

/// Explicit inputs for applying the verified native province and country
/// growth stages after the cache has been calculated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeGrowthApplicationInput {
    pub selected_native_development_milli: i32,
    pub province_cache_milli: i32,
    pub national_growth_milli: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VanillaNativeGrowthApplicationResult {
    pub province_write_milli: Option<i32>,
    pub component_after_province_milli: i32,
    pub country_write_milli: i32,
    pub final_component_milli: i32,
}

/// Applies the verified order of native province and country growth writes.
///
/// A positive province cache is added first and reported as the province
/// write. The resulting component is then used as the denominator for the
/// country growth path when it is at least 1000; a zero cache skips the
/// province write. Eligibility, selectors, sources, and monthly scheduling
/// remain explicit caller responsibilities.
pub fn calculate_vanilla_native_growth_application(
    input: VanillaNativeGrowthApplicationInput,
) -> Result<VanillaNativeGrowthApplicationResult, Error> {
    if input.selected_native_development_milli <= 0 {
        return Err(Error::Invalid(
            "native growth component must be positive".into(),
        ));
    }
    if input.province_cache_milli < 0 {
        return Err(Error::Invalid(
            "native growth province cache must be nonnegative".into(),
        ));
    }
    if input.national_growth_milli < 0 {
        return Err(Error::Invalid(
            "native growth national amount must be nonnegative".into(),
        ));
    }

    let component_after_province_milli = i32::try_from(
        i64::from(input.selected_native_development_milli)
            .checked_add(i64::from(input.province_cache_milli))
            .ok_or(Error::Overflow)?,
    )
    .map_err(|_| Error::Overflow)?;
    let province_write_milli =
        (input.province_cache_milli > 0).then_some(input.province_cache_milli);
    let country_write_milli = if component_after_province_milli >= 1_000 {
        i32::try_from(
            i64::from(input.national_growth_milli)
                .checked_mul(SCALE_MILLI)
                .ok_or(Error::Overflow)?
                / i64::from(component_after_province_milli),
        )
        .map_err(|_| Error::Overflow)?
    } else {
        input.national_growth_milli
    };
    let final_component_milli = i32::try_from(
        i64::from(component_after_province_milli)
            .checked_add(i64::from(country_write_milli))
            .ok_or(Error::Overflow)?,
    )
    .map_err(|_| Error::Overflow)?;

    Ok(VanillaNativeGrowthApplicationResult {
        province_write_milli,
        component_after_province_milli,
        country_write_milli,
        final_component_milli,
    })
}
