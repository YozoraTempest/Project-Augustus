// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

/// Caller-resolved inputs for one native migration AI destination score.
///
/// The compatibility layer supplies the trade factor, `a0d180` result,
/// devastation, native membership, migration steps, and already normalized
/// distance penalties. The core does not derive any of those values from map
/// geometry, random state, or EU4 scripts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationScoreInput {
    pub eligible: bool,
    pub trade_factor: i64,
    pub a0d180_result: i64,
    pub devastation: i64,
    pub destination_in_native_membership: bool,
    pub migration_steps: Option<u32>,
    pub normalized_distance_penalties: Vec<i64>,
}

/// Score terms produced for one eligible native migration destination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaNativeMigrationScoreResult {
    pub base_score: i64,
    pub migration_steps_penalty: i64,
    pub distance_penalty: i64,
    pub score: i64,
}

/// A destination paired with its caller-resolved score inputs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaNativeMigrationScoreCandidateInput {
    pub candidate_id: u32,
    pub score: VanillaNativeMigrationScoreInput,
}

/// The winning destination and its score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VanillaNativeMigrationScoreCandidateResult {
    pub candidate_id: u32,
    pub score: VanillaNativeMigrationScoreResult,
}

/// Calculates one caller-resolved native migration destination score.
///
/// The observed formula is:
///
/// ```text
/// base = (trade_factor % 100) * 1000
///      + a0d180_result * 10
///      + (100000 - devastation)
///
/// member:    score = base
/// nonmember: score = base - migration_steps * 10000
///                  - min(normalized_distance_penalties)
/// ```
///
/// `Ok(None)` means the caller has not resolved a candidate (ineligible,
/// missing non-member steps, or no non-member distance penalty). Negative
/// penalties and arithmetic overflow are invalid inputs.
pub fn calculate_vanilla_native_migration_score(
    input: VanillaNativeMigrationScoreInput,
) -> Result<Option<VanillaNativeMigrationScoreResult>, crate::Error> {
    if input.devastation < 0 {
        return Err(crate::Error::Invalid(
            "native migration score devastation must be nonnegative".into(),
        ));
    }
    if input
        .normalized_distance_penalties
        .iter()
        .any(|penalty| *penalty < 0)
    {
        return Err(crate::Error::Invalid(
            "native migration score distance penalties must be nonnegative".into(),
        ));
    }
    if !input.eligible {
        return Ok(None);
    }

    let base_score = (input.trade_factor % 100)
        .checked_mul(1_000)
        .and_then(|value| value.checked_add(input.a0d180_result.checked_mul(10)?))
        .and_then(|value| value.checked_add(100_000_i64.checked_sub(input.devastation)?))
        .ok_or(crate::Error::Overflow)?;

    let (migration_steps_penalty, distance_penalty) = if input.destination_in_native_membership {
        (0, 0)
    } else {
        let Some(migration_steps) = input.migration_steps else {
            return Ok(None);
        };
        let Some(distance_penalty) = input.normalized_distance_penalties.iter().min() else {
            return Ok(None);
        };
        (
            i64::from(migration_steps)
                .checked_mul(10_000)
                .ok_or(crate::Error::Overflow)?,
            *distance_penalty,
        )
    };
    let score = base_score
        .checked_sub(migration_steps_penalty)
        .and_then(|value| value.checked_sub(distance_penalty))
        .ok_or(crate::Error::Overflow)?;
    Ok(Some(VanillaNativeMigrationScoreResult {
        base_score,
        migration_steps_penalty,
        distance_penalty,
        score,
    }))
}

/// Selects the first highest-scoring eligible destination.
///
/// A later candidate replaces the winner only when its score is strictly
/// greater, so equal scores preserve input order.
pub fn select_vanilla_native_migration_candidate(
    candidates: &[VanillaNativeMigrationScoreCandidateInput],
) -> Result<Option<VanillaNativeMigrationScoreCandidateResult>, crate::Error> {
    let mut winner = None;
    for candidate in candidates {
        let Some(score) = calculate_vanilla_native_migration_score(candidate.score.clone())? else {
            continue;
        };
        if winner
            .as_ref()
            .is_none_or(|current: &VanillaNativeMigrationScoreCandidateResult| {
                score.score > current.score.score
            })
        {
            winner = Some(VanillaNativeMigrationScoreCandidateResult {
                candidate_id: candidate.candidate_id,
                score,
            });
        }
    }
    Ok(winner)
}
