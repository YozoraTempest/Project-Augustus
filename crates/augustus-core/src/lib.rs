// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

//! Deterministic integer state and ordered plans, independent of input formats and UI.

#![forbid(unsafe_code)]

mod plan;
mod world;

pub use plan::{Action, Condition, Plan, Target};
pub use world::{Entity, EntityId, EntityKind, FieldId, RuleId, Schema, World};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid simulation state: {0}")]
    Invalid(String),
    #[error("integer overflow")]
    Overflow,
    #[error("save format: {0}")]
    Save(#[from] serde_json::Error),
}
