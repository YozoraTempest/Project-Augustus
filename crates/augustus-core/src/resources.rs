// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

/// Uninitialized stocks remain unknown until an initialization rule supplies them.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CountryResources {
    pub treasury_milli: Option<i64>,
    pub manpower_milli: Option<i64>,
    pub mercantilism_milli: Option<i64>,
}
