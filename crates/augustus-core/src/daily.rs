// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::{Error, World};

/// A replaceable daily simulation phase.
///
/// The provider owns any state that is specific to its ruleset and uses the
/// shared [`World`] as the only mutable game state. The selected provider id is
/// saved with the session; callers must supply the same provider again when a
/// saved world is advanced.
pub trait DailySystem {
    fn id(&self) -> &str;
    fn validate(&self, world: &World) -> Result<(), Error>;
    fn tick(&self, world: &mut World) -> Result<(), Error>;
}
