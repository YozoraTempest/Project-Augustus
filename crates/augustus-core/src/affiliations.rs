// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{EntityId, EntityKind, Error, World};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AffiliationRelation {
    Same,
    Accepted,
    SameGroup,
    OtherGroup,
}

/// None records an unknown acceptance policy, rather than an empty set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AffiliationAcceptance {
    pub members: Option<BTreeSet<String>>,
    pub groups: Option<BTreeSet<String>>,
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AffiliationDimension {
    /// A member with an unknown group can still match an identical member.
    pub groups: BTreeMap<String, Option<String>>,
    pub members: BTreeMap<EntityId, Option<String>>,
    pub acceptance: BTreeMap<EntityId, AffiliationAcceptance>,
}

/// Live entity memberships and acceptance policies, independent of input syntax.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Affiliations(pub BTreeMap<String, AffiliationDimension>);

impl Affiliations {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn member(&self, dimension: &str, entity: EntityId) -> Option<&str> {
        self.0.get(dimension)?.members.get(&entity)?.as_deref()
    }

    pub fn relation(
        &self,
        dimension: &str,
        current: EntityId,
        target: EntityId,
    ) -> Option<AffiliationRelation> {
        let data = self.0.get(dimension)?;
        let member = data.members.get(&current)?.as_deref()?;
        let other = data.members.get(&target)?.as_deref()?;
        if member == other {
            return Some(AffiliationRelation::Same);
        }
        if let Some(policy) = data.acceptance.get(&target) {
            let explicit = policy.members.as_ref().map(|set| set.contains(member));
            if explicit == Some(true) {
                return Some(AffiliationRelation::Accepted);
            }
            let group_acceptance = policy.groups.as_ref().and_then(|set| {
                if set.is_empty() {
                    Some(false)
                } else {
                    data.groups
                        .get(member)?
                        .as_ref()
                        .map(|group| set.contains(group))
                }
            });
            if group_acceptance == Some(true) {
                return Some(AffiliationRelation::Accepted);
            }
            if explicit.is_none() || group_acceptance.is_none() {
                return None;
            }
        }
        let group = data.groups.get(member)?.as_ref()?;
        let other_group = data.groups.get(other)?.as_ref()?;
        Some(if group == other_group {
            AffiliationRelation::SameGroup
        } else {
            AffiliationRelation::OtherGroup
        })
    }

    pub(crate) fn validate(&self, world: &World) -> Result<(), Error> {
        for (name, data) in &self.0 {
            if name.is_empty()
                || data.groups.iter().any(|(member, group)| {
                    member.is_empty() || group.as_ref().is_some_and(String::is_empty)
                })
            {
                return Err(Error::Invalid("invalid affiliation dimension".into()));
            }
            for (&entity, member) in &data.members {
                world.entity(entity)?;
                if member.as_ref().is_some_and(String::is_empty) {
                    return Err(Error::Invalid("empty affiliation member".into()));
                }
            }
            for (&entity, policy) in &data.acceptance {
                if world.entity(entity)?.kind != EntityKind::Country
                    || [policy.members.as_ref(), policy.groups.as_ref()]
                        .into_iter()
                        .flatten()
                        .any(|set| set.iter().any(String::is_empty))
                {
                    return Err(Error::Invalid("invalid affiliation acceptance".into()));
                }
            }
        }
        Ok(())
    }
}
