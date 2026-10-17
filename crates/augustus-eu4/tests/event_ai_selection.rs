// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::convert::Infallible;

use augustus_eu4::{evaluate_ai_event_weight, select_ai_event_option};

#[test]
fn event_weight_modifiers_run_in_order_with_original_integer_semantics() {
    assert_eq!(
        evaluate_ai_event_weight(1_000, &[(500, true), (2_000, false)], 500),
        250
    );
    assert_eq!(evaluate_ai_event_weight(1_000, &[(500, true)], 1_000), 500);
    assert_eq!(evaluate_ai_event_weight(1, &[(500, true)], 1_000), 0);
    assert_eq!(
        evaluate_ai_event_weight(i32::MAX, &[(1_000, true)], 1_000),
        300_000_000
    );
}

#[test]
fn selection_matches_weight_edge_cases() {
    let cases: &[(&[i32], i32, Option<usize>)] = &[
        (&[1_000], 1, Some(0)),
        (&[1_000, 1_000], 1, Some(1)),
        (&[1_000, 1_000], 2, Some(0)),
        (&[50, 50], 1, Some(1)),
        (&[-1, 1_000], 1, Some(1)),
        (&[1, 2], 1, None),
        (&[i32::MAX, 1_000], 1, None),
        (&[300_000_000, 10], 1, Some(0)),
    ];

    for &(weights, seed, expected) in cases {
        let actual = select_ai_event_option(weights.len(), seed, |index| {
            Ok::<_, Infallible>(Some(weights[index]))
        })
        .unwrap();
        assert_eq!(actual, expected, "weights={weights:?}, seed={seed}");
    }
}

#[test]
fn ineligible_options_keep_their_declared_indices() {
    let actual = select_ai_event_option(3, 1, |index| {
        Ok::<_, Infallible>(match index {
            0 => Some(1_000),
            1 => None,
            2 => Some(1_000),
            _ => unreachable!(),
        })
    })
    .unwrap();

    assert_eq!(actual, Some(2));
}

#[test]
fn selection_pass_uses_reevaluated_conditions_and_weights() {
    let mut calls = [0; 2];
    let actual = select_ai_event_option(2, 1, |index| {
        calls[index] += 1;
        Ok::<_, Infallible>(match (index, calls[index]) {
            (0, 1) => Some(2_000),
            (0, _) => None,
            (_, _) => Some(1_000),
        })
    })
    .unwrap();

    assert_eq!(actual, None);
    assert_eq!(calls, [2, 2]);
}

#[test]
fn nonpositive_totals_return_without_a_second_pass() {
    let mut calls = 0;
    let actual = select_ai_event_option(1, 1, |_| {
        calls += 1;
        Ok::<_, Infallible>(Some(0))
    })
    .unwrap();

    assert_eq!(actual, None);
    assert_eq!(calls, 1);
}
