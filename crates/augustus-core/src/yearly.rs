// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{Error, World};

/// A replaceable yearly simulation phase.
///
/// The provider owns any ruleset-specific state in [`World`]. The selected
/// provider identifier is saved with the session, so a resumed campaign must
/// supply the same provider before the next year boundary.
pub trait YearlySystem {
    fn id(&self) -> &str;
    fn validate(&self, world: &World) -> Result<(), Error>;
    fn settle(&self, world: &mut World) -> Result<(), Error>;
}
