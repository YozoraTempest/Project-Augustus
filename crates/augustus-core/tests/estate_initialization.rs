// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::EstateLandRules;

fn rules() -> EstateLandRules {
    EstateLandRules {
        crown_influence_milli: 12_000,
        minimum_crown_share_micros: 400_000,
        initial_share_quantum_micros: 1_000,
        normalized_share_quantum_micros: 10,
    }
}

#[test]
fn crown_reserve_quantization_and_zero_weights_preserve_total_land() {
    let rules = rules();
    assert_eq!(rules.distribute(&[]).unwrap(), Vec::<i64>::new());
    assert_eq!(rules.distribute(&[0, 0]).unwrap(), vec![0, 0]);
    assert_eq!(rules.distribute(&[12_000, 0]).unwrap(), vec![500_000, 0]);
    assert_eq!(
        rules.distribute(&[1_000, 2_000]).unwrap(),
        vec![66_000, 133_000]
    );
    let shares = rules.distribute(&[12_000, 24_000, 12_000]).unwrap();
    assert_eq!(shares, vec![150_000, 300_000, 150_000]);
    assert_eq!(shares.iter().sum::<i64>(), 600_000);
    assert_eq!(
        rules.distribute(&[i64::MAX, i64::MAX]).unwrap(),
        vec![300_000, 300_000]
    );
}

#[test]
fn rounding_remainder_goes_to_first_largest_in_input_order() {
    let mut rules = rules();
    rules.minimum_crown_share_micros = 400_010;
    assert_eq!(
        rules.distribute(&[12_000, 12_000, 12_000]).unwrap(),
        vec![199_990, 200_000, 200_000]
    );
    rules.minimum_crown_share_micros = 1_000_000;
    assert_eq!(rules.distribute(&[12_000, 24_000]).unwrap(), vec![0, 0]);
}

#[test]
fn invalid_inputs_fail_without_partial_state() {
    assert!(rules().distribute(&[-1]).is_err());
    for field in 0..4 {
        let mut rules = rules();
        match field {
            0 => rules.crown_influence_milli = -1,
            1 => rules.minimum_crown_share_micros = 1_000_001,
            2 => rules.initial_share_quantum_micros = 0,
            _ => rules.normalized_share_quantum_micros = 3,
        }
        assert!(rules.distribute(&[12_000]).is_err());
    }
    let mut rules = rules();
    rules.crown_influence_milli = 0;
    assert_eq!(rules.distribute(&[0]).unwrap(), vec![0]);
}
