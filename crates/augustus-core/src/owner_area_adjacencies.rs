// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

/// One directed edge from an owner-area key to another area key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerAreaAdjacencyEntry {
    pub target_key: u64,
    pub count: i32,
    pub deferred_removal: bool,
}

/// Ordered directed adjacency counts for one owner area.
///
/// This stores the verified edge-count update behavior only. It does not
/// derive owner areas from provinces or schedule deferred-row cleanup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnerAreaAdjacencyCounts {
    context_key: u64,
    entries: Vec<OwnerAreaAdjacencyEntry>,
}

impl OwnerAreaAdjacencyCounts {
    pub fn new(context_key: u64) -> Self {
        Self {
            context_key,
            entries: Vec::new(),
        }
    }

    pub fn count_to(&self, target_key: u64) -> i32 {
        self.entries
            .iter()
            .find(|entry| entry.target_key == target_key)
            .map_or(0, |entry| entry.count)
    }

    pub fn entries(&self) -> &[OwnerAreaAdjacencyEntry] {
        &self.entries
    }

    /// Changes an edge count, preserving append order for newly created edges.
    ///
    /// Self edges are ignored. Existing counts use signed 32-bit wrapping
    /// addition; results above zero stay linked, while results at or below zero
    /// are removed or marked according to `defer_removal`. Missing edges are
    /// created only for positive deltas.
    pub fn add_delta(&mut self, target_key: u64, delta: i32, defer_removal: bool) {
        if target_key == self.context_key {
            return;
        }

        if let Some(position) = self
            .entries
            .iter()
            .position(|entry| entry.target_key == target_key)
        {
            let entry = &mut self.entries[position];
            entry.count = entry.count.wrapping_add(delta);
            if entry.count > 0 {
                return;
            }
            if defer_removal {
                entry.deferred_removal = true;
            } else {
                self.entries.remove(position);
            }
            return;
        }

        if delta > 0 {
            self.entries.push(OwnerAreaAdjacencyEntry {
                target_key,
                count: delta,
                deferred_removal: false,
            });
        }
    }

    /// Removes rows marked for deferred removal when the caller chooses.
    pub fn remove_deferred_entries(&mut self) {
        self.entries.retain(|entry| !entry.deferred_removal);
    }
}
