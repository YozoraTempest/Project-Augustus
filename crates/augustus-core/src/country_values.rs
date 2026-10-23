// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

/// One row in the country-value table's ascending index.
///
/// Deferred rows remain visible until the owning system performs its cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CountryValueIndexEntry {
    pub target_key: u64,
    pub value: i32,
    pub deferred_removal: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CountryValueRecord {
    target_key: u64,
    value: i32,
}

/// A scoped integer table indexed by the signed high 16 bits of a packed key.
///
/// This models the table update primitive only. The caller supplies score
/// sources and decides when deferred index rows are cleaned up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountryValueTable {
    context_index: i16,
    records: BTreeMap<i16, CountryValueRecord>,
    ordered_index: Vec<CountryValueIndexEntry>,
}

impl CountryValueTable {
    pub fn new(context_key: u64) -> Self {
        Self {
            context_index: packed_index(context_key),
            records: BTreeMap::new(),
            ordered_index: Vec::new(),
        }
    }

    /// Returns zero for a missing key. Keys match on their signed high 16 bits.
    pub fn value(&self, target_key: u64) -> i32 {
        self.records
            .get(&packed_index(target_key))
            .map_or(0, |record| record.value)
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The score-ascending index. Equal scores are inserted before older rows.
    pub fn ordered_index(&self) -> &[CountryValueIndexEntry] {
        &self.ordered_index
    }

    /// Removes rows previously marked for deferred removal.
    ///
    /// The caller controls when this operation is appropriate.
    pub fn remove_deferred_index_entries(&mut self) {
        self.ordered_index.retain(|entry| !entry.deferred_removal);
    }

    /// Adds a signed delta, preserving the first full key stored for a target.
    ///
    /// Self-targets and zero deltas are ignored. Missing targets are created
    /// only for positive deltas. Existing values use signed 32-bit wrapping
    /// addition and are removed when the result is below one. When requested,
    /// the old index row is marked and retained for caller-owned cleanup.
    pub fn add_delta(&mut self, target_key: u64, delta: i32, defer_index_removal: bool) {
        let target_index = packed_index(target_key);
        if target_index == self.context_index || delta == 0 {
            return;
        }

        let Some(existing) = self.records.get(&target_index).copied() else {
            if delta > 0 {
                self.records.insert(
                    target_index,
                    CountryValueRecord {
                        target_key,
                        value: delta,
                    },
                );
                self.insert_index(CountryValueIndexEntry {
                    target_key,
                    value: delta,
                    deferred_removal: false,
                });
            }
            return;
        };

        let next_value = existing.value.wrapping_add(delta);
        if let Some(position) = self
            .ordered_index
            .iter()
            .position(|entry| packed_index(entry.target_key) == target_index)
        {
            if defer_index_removal {
                self.ordered_index[position].deferred_removal = true;
            } else {
                self.ordered_index.remove(position);
            }
        }

        for entry in &mut self.ordered_index {
            if packed_index(entry.target_key) == target_index {
                entry.value = next_value;
            }
        }

        if next_value < 1 {
            self.records.remove(&target_index);
            return;
        }

        self.records
            .get_mut(&target_index)
            .expect("record was found above")
            .value = next_value;
        self.insert_index(CountryValueIndexEntry {
            target_key: existing.target_key,
            value: next_value,
            deferred_removal: false,
        });
    }

    fn insert_index(&mut self, entry: CountryValueIndexEntry) {
        let position = self
            .ordered_index
            .iter()
            .position(|existing| existing.value >= entry.value)
            .unwrap_or(self.ordered_index.len());
        self.ordered_index.insert(position, entry);
    }
}

fn packed_index(key: u64) -> i16 {
    ((key >> 32) as u16) as i16
}
