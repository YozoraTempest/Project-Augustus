// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, PendingEventId, ScopeSelector, World};

/// Fixed province names offered by an event and its individual options.
/// These are presentation metadata; reading them does not execute a choice.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventLocations {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, String>,
}

impl EventLocations {
    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.event
            .iter()
            .chain(self.options.values())
            .map(String::as_str)
    }

    pub(crate) fn resolve(world: &World, name: &str) -> Result<EntityId, Error> {
        world
            .resolve_scope(&ScopeSelector::Named {
                kind: EntityKind::Province,
                name: name.into(),
            })?
            .ok_or_else(|| Error::Invalid(format!("unknown event location province {name}")))
    }
}

impl World {
    /// Fixed event location in current immutable content. No fallback to the
    /// receiving country's capital or to an option location is implied.
    pub fn pending_event_location(&self, id: PendingEventId) -> Result<Option<EntityId>, Error> {
        let pending = self
            .pending_events()
            .iter()
            .find(|pending| pending.id == id)
            .ok_or_else(|| Error::Invalid(format!("unknown pending event {}", id.0)))?;
        self.event_catalog()
            .locations()
            .get(&pending.event)
            .and_then(|locations| locations.event.as_deref())
            .map(|name| EventLocations::resolve(self, name))
            .transpose()
    }

    /// Location explicitly declared on an option. This neither selects the
    /// option nor inherits the location declared on the event.
    pub fn pending_event_option_location(
        &self,
        id: PendingEventId,
        option: &str,
    ) -> Result<Option<EntityId>, Error> {
        let pending = self
            .pending_events()
            .iter()
            .find(|pending| pending.id == id)
            .ok_or_else(|| Error::Invalid(format!("unknown pending event {}", id.0)))?;
        let definition = self.event_catalog().get(&pending.event)?;
        if !definition.options.iter().any(|item| item.id == option) {
            return Err(Error::Invalid(format!(
                "unknown option {option} for event {}",
                pending.event
            )));
        }
        self.event_catalog()
            .locations()
            .get(&pending.event)
            .and_then(|locations| locations.options.get(option))
            .map(|name| EventLocations::resolve(self, name))
            .transpose()
    }
}
