// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VanillaExternalStateEdict {
    pub identifier: String,
    pub enacted_on: String,
}

/// Current state edicts keyed by EU4 country tag and map area.
pub type VanillaExternalStateEdicts = BTreeMap<String, BTreeMap<String, VanillaExternalStateEdict>>;

pub(crate) fn modifier_identifier(identifier: &str) -> String {
    format!("state_edict:{identifier}")
}
