// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::BTreeMap;

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountryTagSlotKind {
    Placeholder,
    Content,
    Generated,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CountryTagSlot {
    pub tag: String,
    pub kind: CountryTagSlotKind,
}

impl CountryTagSlot {
    fn new(tag: impl Into<String>, kind: CountryTagSlotKind) -> Self {
        Self {
            tag: tag.into(),
            kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CountryTagRegistry {
    slots: Vec<CountryTagSlot>,
    by_tag: BTreeMap<String, usize>,
}

impl CountryTagRegistry {
    fn new(slots: Vec<CountryTagSlot>) -> Result<Self, Error> {
        let mut by_tag = BTreeMap::new();
        let mut placeholder_seen = false;
        for (index, slot) in slots.iter().enumerate() {
            match slot.kind {
                CountryTagSlotKind::Placeholder => {
                    if slot.tag != "---" || placeholder_seen {
                        return Err(Error::message(
                            "common/country_tags",
                            "invalid country tag placeholder",
                        ));
                    }
                    placeholder_seen = true;
                }
                CountryTagSlotKind::Content if slot.tag.is_empty() => {
                    return Err(Error::message(
                        "common/country_tags",
                        "empty content country tag",
                    ));
                }
                CountryTagSlotKind::Generated
                    if slot.tag.len() != 3
                        || !slot
                            .tag
                            .bytes()
                            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()) =>
                {
                    return Err(Error::message(
                        "common/country_tags",
                        "invalid generated country tag",
                    ));
                }
                _ => {}
            }
            if by_tag.insert(slot.tag.clone(), index).is_some() {
                return Err(Error::message(
                    "common/country_tags",
                    "duplicate registered country tag",
                ));
            }
        }

        Ok(Self { slots, by_tag })
    }

    pub fn slots(&self) -> &[CountryTagSlot] {
        &self.slots
    }

    pub fn slot(&self, tag: &str) -> Option<&CountryTagSlot> {
        self.by_tag.get(tag).map(|&index| &self.slots[index])
    }

    pub fn index(&self, tag: &str) -> Option<usize> {
        self.by_tag.get(tag).copied()
    }

    pub fn generated_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.kind == CountryTagSlotKind::Generated)
            .count()
    }
}

// Default generated-tag pool sizes observed for EU4 1.37.5.0.
const EU4_1_37_5_GENERATED_TAG_POOLS: [(char, u8); 7] = [
    ('C', 75),
    ('D', 75),
    ('E', 50),
    ('F', 20),
    ('K', 100),
    ('O', 10),
    ('T', 75),
];

pub(super) fn build_registry(content_order: &[String]) -> Result<CountryTagRegistry, Error> {
    let generated_count: usize = EU4_1_37_5_GENERATED_TAG_POOLS
        .iter()
        .map(|(_, count)| usize::from(*count))
        .sum();
    let mut slots = Vec::with_capacity(1 + content_order.len() + generated_count);
    slots.push(CountryTagSlot::new("---", CountryTagSlotKind::Placeholder));
    slots.extend(
        content_order
            .iter()
            .cloned()
            .map(|tag| CountryTagSlot::new(tag, CountryTagSlotKind::Content)),
    );
    for (prefix, count) in EU4_1_37_5_GENERATED_TAG_POOLS {
        for number in 0..count {
            slots.push(CountryTagSlot::new(
                format!("{prefix}{number:02}"),
                CountryTagSlotKind::Generated,
            ));
        }
    }
    CountryTagRegistry::new(slots)
}

#[cfg(test)]
mod tests {
    use super::{CountryTagSlotKind, build_registry};

    #[test]
    fn registry_keeps_content_order_and_appends_all_generated_pools() {
        let registry = build_registry(&["BBB".into(), "AAA".into()]).unwrap();
        let slots = registry.slots();

        assert_eq!(slots.len(), 408);
        assert_eq!(registry.generated_count(), 405);
        assert_eq!(slots[0].tag, "---");
        assert_eq!(slots[0].kind, CountryTagSlotKind::Placeholder);
        assert_eq!(slots[1].tag, "BBB");
        assert_eq!(slots[2].tag, "AAA");
        assert_eq!(slots[3].tag, "C00");
        assert_eq!(slots[77].tag, "C74");
        assert_eq!(slots[78].tag, "D00");
        assert_eq!(slots[153].tag, "E00");
        assert_eq!(slots[203].tag, "F00");
        assert_eq!(slots[223].tag, "K00");
        assert_eq!(slots[323].tag, "O00");
        assert_eq!(slots[333].tag, "T00");
        assert_eq!(slots[407].tag, "T74");
        assert_eq!(registry.index("C00"), Some(3));
        assert_eq!(
            registry.slot("C00").unwrap().kind,
            CountryTagSlotKind::Generated
        );
    }

    #[test]
    fn generated_tag_collision_with_content_is_rejected() {
        assert!(build_registry(&["C00".into()]).is_err());
    }
}
