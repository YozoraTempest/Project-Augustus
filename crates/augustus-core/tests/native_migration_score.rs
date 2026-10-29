// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::{
    VanillaNativeMigrationScoreCandidateInput, VanillaNativeMigrationScoreInput,
    calculate_vanilla_native_migration_score, select_vanilla_native_migration_candidate,
};

fn input() -> VanillaNativeMigrationScoreInput {
    VanillaNativeMigrationScoreInput {
        eligible: true,
        trade_factor: 1_519_919_863,
        a0d180_result: 6_000,
        devastation: 0,
        destination_in_native_membership: false,
        migration_steps: Some(2),
        normalized_distance_penalties: vec![1_264, 1_910],
    }
}

#[test]
fn member_destination_keeps_the_base_score() {
    let mut member = input();
    member.destination_in_native_membership = true;
    let result = calculate_vanilla_native_migration_score(member)
        .unwrap()
        .unwrap();
    assert_eq!(result.base_score, 223_000);
    assert_eq!(result.migration_steps_penalty, 0);
    assert_eq!(result.distance_penalty, 0);
    assert_eq!(result.score, 223_000);
}

#[test]
fn nonmember_destination_applies_steps_and_minimum_distance_penalties() {
    let result = calculate_vanilla_native_migration_score(input())
        .unwrap()
        .unwrap();
    assert_eq!(result.base_score, 223_000);
    assert_eq!(result.migration_steps_penalty, 20_000);
    assert_eq!(result.distance_penalty, 1_264);
    assert_eq!(result.score, 201_736);
}

#[test]
fn selection_skips_unresolved_candidates_and_keeps_first_tie() {
    let mut unresolved = input();
    unresolved.migration_steps = None;
    let mut ineligible = input();
    ineligible.eligible = false;
    let mut tie = input();
    tie.destination_in_native_membership = true;
    let winner = select_vanilla_native_migration_candidate(&[
        VanillaNativeMigrationScoreCandidateInput {
            candidate_id: 10,
            score: unresolved,
        },
        VanillaNativeMigrationScoreCandidateInput {
            candidate_id: 20,
            score: ineligible,
        },
        VanillaNativeMigrationScoreCandidateInput {
            candidate_id: 30,
            score: tie.clone(),
        },
        VanillaNativeMigrationScoreCandidateInput {
            candidate_id: 40,
            score: tie,
        },
    ])
    .unwrap()
    .unwrap();
    assert_eq!(winner.candidate_id, 30);
    assert_eq!(winner.score.score, 223_000);
}

#[test]
fn empty_distance_or_candidate_ranges_produce_no_winner() {
    let mut no_distance = input();
    no_distance.normalized_distance_penalties.clear();
    assert!(
        calculate_vanilla_native_migration_score(no_distance)
            .unwrap()
            .is_none()
    );
    assert!(
        select_vanilla_native_migration_candidate(&[])
            .unwrap()
            .is_none()
    );
}

#[test]
fn invalid_inputs_and_arithmetic_overflow_are_rejected() {
    let mut negative_devastation = input();
    negative_devastation.devastation = -1;
    assert!(calculate_vanilla_native_migration_score(negative_devastation).is_err());

    let mut negative_distance = input();
    negative_distance.normalized_distance_penalties[0] = -1;
    assert!(calculate_vanilla_native_migration_score(negative_distance).is_err());

    let mut overflow = input();
    overflow.a0d180_result = i64::MAX;
    assert!(calculate_vanilla_native_migration_score(overflow).is_err());
}
