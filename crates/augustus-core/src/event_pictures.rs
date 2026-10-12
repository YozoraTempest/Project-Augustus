// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use serde::{Deserialize, Serialize};

use crate::{Condition, EntityId, Error, World};

/// One content-defined resource key and its display condition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PictureVariant {
    pub key: String,
    pub condition: Condition,
}

/// Ordered picture selection. Content can explicitly provide a key to use
/// when no condition matches; the core does not invent a default resource.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventPictures {
    pub variants: Vec<PictureVariant>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
}

impl EventPictures {
    pub(crate) fn validate_keys(&self) -> Result<(), Error> {
        if (self.variants.is_empty() && self.fallback.is_none())
            || self
                .variants
                .iter()
                .any(|variant| variant.key.trim().is_empty())
            || self
                .fallback
                .as_ref()
                .is_some_and(|key| key.trim().is_empty())
        {
            return Err(Error::Invalid(
                "empty event picture definition or key".into(),
            ));
        }
        Ok(())
    }

    pub(crate) fn contains(&self, key: Option<&str>) -> bool {
        match key {
            Some(key) => {
                self.fallback.as_deref() == Some(key)
                    || self.variants.iter().any(|variant| variant.key == key)
            }
            None => self.fallback.is_none(),
        }
    }

    /// Resolve against current state without executing effects or consuming
    /// randomness. Event dispatch captures this result after immediate effects.
    pub fn select(&self, world: &World, current: EntityId) -> Result<Option<&str>, Error> {
        self.validate_keys()?;
        world.entity(current)?;
        for variant in &self.variants {
            if variant.condition.evaluate(world, current)? {
                return Ok(Some(&variant.key));
            }
        }
        Ok(self.fallback.as_deref())
    }
}
