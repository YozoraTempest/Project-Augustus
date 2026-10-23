// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use augustus_core::OwnerAreaAdjacencyCounts;

#[test]
fn ignores_self_edges_and_missing_negative_deltas() {
    let mut counts = OwnerAreaAdjacencyCounts::new(1);
    counts.add_delta(1, 5, false);
    counts.add_delta(2, -1, false);

    assert!(counts.entries().is_empty());
}

#[test]
fn appends_new_edges_and_updates_existing_counts_in_place() {
    let mut counts = OwnerAreaAdjacencyCounts::new(1);
    counts.add_delta(10, 2, false);
    counts.add_delta(20, 4, false);
    counts.add_delta(10, 3, false);

    assert_eq!(counts.count_to(10), 5);
    assert_eq!(counts.count_to(20), 4);
    assert_eq!(
        counts
            .entries()
            .iter()
            .map(|entry| (entry.target_key, entry.count))
            .collect::<Vec<_>>(),
        [(10, 5), (20, 4)]
    );
}

#[test]
fn decrements_keep_edges_only_while_the_count_is_positive() {
    let mut counts = OwnerAreaAdjacencyCounts::new(1);
    counts.add_delta(10, 5, false);
    counts.add_delta(10, -2, false);

    assert_eq!(counts.count_to(10), 3);
    counts.add_delta(10, -3, false);
    assert_eq!(counts.count_to(10), 0);
    assert!(counts.entries().is_empty());
}

#[test]
fn negative_results_are_removed_immediately() {
    let mut counts = OwnerAreaAdjacencyCounts::new(1);
    counts.add_delta(10, 1, false);
    counts.add_delta(10, -2, false);

    assert_eq!(counts.count_to(10), 0);
    assert!(counts.entries().is_empty());
}

#[test]
fn deferred_removal_keeps_the_nonpositive_count_and_marks_the_row() {
    let mut counts = OwnerAreaAdjacencyCounts::new(1);
    counts.add_delta(10, 1, false);
    counts.add_delta(10, -1, true);

    assert_eq!(counts.count_to(10), 0);
    assert_eq!(counts.entries().len(), 1);
    assert!(counts.entries()[0].deferred_removal);
}

#[test]
fn positive_updates_do_not_clear_an_existing_deferred_mark() {
    let mut counts = OwnerAreaAdjacencyCounts::new(1);
    counts.add_delta(10, 1, false);
    counts.add_delta(10, -1, true);
    counts.add_delta(10, 3, false);

    assert_eq!(counts.count_to(10), 3);
    assert!(counts.entries()[0].deferred_removal);
}

#[test]
fn explicit_cleanup_removes_only_marked_rows() {
    let mut counts = OwnerAreaAdjacencyCounts::new(1);
    counts.add_delta(10, 1, false);
    counts.add_delta(10, -1, true);
    counts.add_delta(20, 3, false);
    counts.remove_deferred_entries();

    assert_eq!(counts.entries().len(), 1);
    assert_eq!(counts.entries()[0].target_key, 20);
    assert_eq!(counts.count_to(20), 3);
}
