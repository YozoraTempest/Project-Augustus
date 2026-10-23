// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::CountryValueTable;

fn key(index: i16, payload: u32) -> u64 {
    (u64::from(index as u16) << 32) | u64::from(payload)
}

#[test]
fn skips_self_targets_zero_deltas_and_absent_negative_deltas() {
    let mut values = CountryValueTable::new(key(145, 1));
    values.add_delta(key(145, 2), 5, false);
    values.add_delta(key(200, 1), 0, false);
    values.add_delta(key(300, 1), -3, false);

    assert!(values.is_empty());
    assert!(values.ordered_index().is_empty());
}

#[test]
fn inserts_positive_values_and_preserves_the_first_full_target_key() {
    let first_key = key(200, 0xa5a5_0001);
    let mut values = CountryValueTable::new(key(145, 1));
    values.add_delta(first_key, 5, false);
    values.add_delta(key(200, 0x1111_2222), 4, false);

    assert_eq!(values.value(key(200, 9)), 9);
    assert_eq!(values.len(), 1);
    assert_eq!(values.ordered_index().len(), 1);
    assert_eq!(values.ordered_index()[0].target_key, first_key);
    assert_eq!(values.ordered_index()[0].value, 9);
}

#[test]
fn maintains_ascending_values_and_places_equal_values_before_older_rows() {
    let mut values = CountryValueTable::new(key(145, 1));
    values.add_delta(key(200, 1), 5, false);
    values.add_delta(key(300, 1), 5, false);
    values.add_delta(key(400, 1), 5, false);

    assert_eq!(
        values
            .ordered_index()
            .iter()
            .map(|entry| (entry.target_key, entry.value))
            .collect::<Vec<_>>(),
        [(key(400, 1), 5), (key(300, 1), 5), (key(200, 1), 5)]
    );

    values.add_delta(key(500, 1), 3, false);
    assert_eq!(
        values
            .ordered_index()
            .iter()
            .map(|entry| entry.value)
            .collect::<Vec<_>>(),
        [3, 5, 5, 5]
    );
}

#[test]
fn removes_values_below_one_and_keeps_exactly_one() {
    let mut values = CountryValueTable::new(key(145, 1));
    values.add_delta(key(200, 1), 2, false);
    values.add_delta(key(300, 1), 2, false);
    values.add_delta(key(200, 1), -1, false);

    assert_eq!(values.value(key(200, 1)), 1);
    assert_eq!(values.value(key(300, 1)), 2);

    values.add_delta(key(200, 1), -2, false);
    assert_eq!(values.value(key(200, 1)), 0);
    assert_eq!(values.len(), 1);
    assert_eq!(values.ordered_index().len(), 1);
}

#[test]
fn wraps_signed_addition_before_applying_the_minimum_threshold() {
    let mut values = CountryValueTable::new(key(145, 1));
    values.add_delta(key(200, 1), i32::MAX, false);
    values.add_delta(key(200, 1), 1, false);

    assert!(values.is_empty());
    assert!(values.ordered_index().is_empty());
}

#[test]
fn deferred_removal_marks_the_old_row_and_inserts_a_new_row() {
    let mut values = CountryValueTable::new(key(145, 1));
    values.add_delta(key(200, 1), 3, false);
    values.add_delta(key(200, 1), 2, true);

    assert_eq!(values.value(key(200, 1)), 5);
    assert_eq!(values.ordered_index().len(), 2);
    assert!(!values.ordered_index()[0].deferred_removal);
    assert!(values.ordered_index()[1].deferred_removal);
    assert_eq!(values.ordered_index()[0].value, 5);
    assert_eq!(values.ordered_index()[1].value, 5);
}

#[test]
fn explicit_cleanup_removes_marked_index_rows_but_keeps_the_score() {
    let mut values = CountryValueTable::new(key(145, 1));
    values.add_delta(key(200, 1), 3, false);
    values.add_delta(key(200, 1), 2, true);
    values.remove_deferred_index_entries();

    assert_eq!(values.value(key(200, 1)), 5);
    assert_eq!(values.ordered_index().len(), 1);
    assert!(!values.ordered_index()[0].deferred_removal);
}

#[test]
fn signed_high_16_bit_keys_are_used_for_lookup() {
    let mut values = CountryValueTable::new(key(145, 1));
    values.add_delta(key(-1, 1), 2, false);
    values.add_delta(key(-1, 0xffff_ffff), 3, false);

    assert_eq!(values.value(key(-1, 7)), 5);
    assert_eq!(values.len(), 1);
    assert_eq!(values.ordered_index()[0].target_key, key(-1, 1));
}
