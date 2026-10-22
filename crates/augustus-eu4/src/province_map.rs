// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{EntityId, ProvinceClass, ProvinceConnectivity, ProvinceState};

use crate::{Error, TextDocument};

#[derive(Debug, Default)]
pub(crate) struct ProvinceTopology {
    adjacent: BTreeMap<u32, BTreeSet<u32>>,
    uncertain_adjacency: BTreeSet<u32>,
}

impl ProvinceTopology {
    fn connect(&mut self, left: u32, right: u32) {
        if left == right {
            return;
        }
        self.adjacent.entry(left).or_default().insert(right);
        self.adjacent.entry(right).or_default().insert(left);
    }

    pub(crate) fn add_special_adjacencies(
        &mut self,
        document: &TextDocument,
        provinces: &BTreeMap<u32, ProvinceState>,
    ) -> Result<(), Error> {
        let mut reader = csv::ReaderBuilder::new()
            .delimiter(b';')
            .flexible(true)
            .from_reader(document.bytes());
        let headers = reader
            .byte_headers()
            .map_err(|error| Error::caused_by(document.source(), error))?
            .clone();
        let from_column = column_index(&headers, b"from").ok_or_else(|| {
            Error::message(
                document.source(),
                "adjacency CSV is missing the From column",
            )
        })?;
        let to_column = column_index(&headers, b"to").ok_or_else(|| {
            Error::message(document.source(), "adjacency CSV is missing the To column")
        })?;

        for row in reader.byte_records() {
            let row = row.map_err(|error| Error::caused_by(document.source(), error))?;
            if row
                .iter()
                .all(|field| field.iter().all(u8::is_ascii_whitespace))
            {
                continue;
            }
            let line = row.position().map_or(2, |position| position.line());
            let fail = |message: &str| {
                Error::message(
                    document.source(),
                    format!("{message} on adjacency row {line}"),
                )
            };
            let parse_endpoint = |column: usize| -> Result<i64, Error> {
                let value = row
                    .get(column)
                    .ok_or_else(|| fail("missing adjacency endpoint"))?;
                std::str::from_utf8(value)
                    .map_err(|_| fail("adjacency endpoint is not ASCII"))?
                    .trim()
                    .parse()
                    .map_err(|_| fail("invalid adjacency endpoint"))
            };
            let from = parse_endpoint(from_column)?;
            let to = parse_endpoint(to_column)?;
            if from == -1 && to == -1 {
                continue;
            }
            let from = u32::try_from(from).map_err(|_| fail("invalid From province"))?;
            let to = u32::try_from(to).map_err(|_| fail("invalid To province"))?;
            if from == 0 || to == 0 {
                return Err(fail("province zero is not a valid adjacency endpoint"));
            }
            if !provinces.contains_key(&from) || !provinces.contains_key(&to) {
                return Err(fail("adjacency references an undefined province"));
            }
            self.connect(from, to);
        }
        Ok(())
    }

    pub(crate) fn to_core(
        &self,
        province_entities: &BTreeMap<u32, EntityId>,
    ) -> Result<ProvinceConnectivity, Error> {
        let province_entity = |province: u32| {
            province_entities.get(&province).copied().ok_or_else(|| {
                Error::message(
                    "province topology",
                    format!("connectivity references unknown province {province}"),
                )
            })
        };
        let mut edges = Vec::new();
        for (&left, neighbors) in &self.adjacent {
            for &right in neighbors {
                if left < right {
                    edges.push((province_entity(left)?, province_entity(right)?));
                }
            }
        }
        let uncertain_adjacency = self
            .uncertain_adjacency
            .iter()
            .map(|&province| province_entity(province))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ProvinceConnectivity::new(edges, uncertain_adjacency))
    }
}

fn column_index(headers: &csv::ByteRecord, expected: &[u8]) -> Option<usize> {
    headers.iter().position(|header| {
        let header = header.strip_prefix(b"\xef\xbb\xbf").unwrap_or(header);
        header.trim_ascii().eq_ignore_ascii_case(expected)
    })
}

#[derive(Debug)]
pub(crate) struct DerivedProvinceMap {
    pub port_presence: BTreeMap<u32, Option<bool>>,
    pub topology: ProvinceTopology,
}

pub(crate) fn derive(
    pixels: &image::RgbImage,
    province_ids_by_color: &BTreeMap<[u8; 3], u32>,
    provinces: &BTreeMap<u32, ProvinceState>,
    force_coastal: &BTreeSet<u32>,
) -> DerivedProvinceMap {
    let width = pixels.width() as usize;
    let height = pixels.height() as usize;
    let raster: Vec<_> = pixels
        .pixels()
        .map(|pixel| province_ids_by_color.get(&pixel.0).copied())
        .collect();
    let mut coastal = BTreeSet::new();
    let mut uncertain = BTreeSet::new();
    let mut topology = ProvinceTopology::default();

    for (index, province_id) in raster.iter().enumerate() {
        let Some(province_id) = province_id else {
            continue;
        };
        let Some(province) = provinces.get(province_id) else {
            topology.uncertain_adjacency.insert(*province_id);
            continue;
        };
        let x = index % width;
        let y = index / width;
        let neighbors = [
            Some(y * width + (x + width - 1) % width),
            Some(y * width + (x + 1) % width),
            (y > 0).then(|| index - width),
            (y + 1 < height).then(|| index + width),
        ];
        for neighbor in neighbors.into_iter().flatten() {
            if let Some(neighbor_id) = raster[neighbor]
                && provinces.contains_key(&neighbor_id)
            {
                if neighbor_id != *province_id {
                    // Every undirected bitmap edge is visited twice. `connect` deduplicates it.
                    topology.connect(*province_id, neighbor_id);
                }
            } else {
                topology.uncertain_adjacency.insert(*province_id);
            }
        }

        if province.class != ProvinceClass::Land {
            continue;
        }
        for neighbor in neighbors.into_iter().flatten() {
            match raster[neighbor].and_then(|id| provinces.get(&id)) {
                Some(neighbor) if neighbor.class == ProvinceClass::Sea => {
                    coastal.insert(*province_id);
                }
                Some(neighbor) if neighbor.class == ProvinceClass::Unused => {
                    uncertain.insert(*province_id);
                }
                None => {
                    uncertain.insert(*province_id);
                }
                _ => {}
            }
        }
    }

    let port_presence = provinces
        .iter()
        .map(|(&id, province)| {
            let has_port = if province.class != ProvinceClass::Land {
                Some(false)
            } else if force_coastal.contains(&id) || coastal.contains(&id) {
                Some(true)
            } else if uncertain.contains(&id) {
                None
            } else {
                Some(false)
            };
            (id, has_port)
        })
        .collect();

    DerivedProvinceMap {
        port_presence,
        topology,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use augustus_core::{EntityKind, Schema, Target, World};

    const LAND: [u8; 3] = [1, 0, 0];
    const SEA: [u8; 3] = [2, 0, 0];
    const LAKE: [u8; 3] = [3, 0, 0];
    const UNUSED: [u8; 3] = [4, 0, 0];
    const EXTRA_LAND: [u8; 3] = [5, 0, 0];

    fn province(id: u32, color: [u8; 3], class: ProvinceClass) -> ProvinceState {
        ProvinceState {
            id,
            name: id.to_string(),
            color,
            class,
            has_port: None,
            owner: None,
            controller: None,
            cores: Vec::new(),
            culture: None,
            religion: None,
            trade_good: None,
            tax_milli: None,
            production_milli: None,
            manpower_milli: None,
            is_city: None,
            in_empire: None,
        }
    }

    fn map(
        width: u32,
        height: u32,
        colors: &[[u8; 3]],
    ) -> (
        image::RgbImage,
        BTreeMap<[u8; 3], u32>,
        BTreeMap<u32, ProvinceState>,
    ) {
        let pixels = image::RgbImage::from_fn(width, height, |x, y| {
            image::Rgb(colors[(y * width + x) as usize])
        });
        let mut ids_by_color = BTreeMap::new();
        let mut provinces = BTreeMap::new();
        for (id, color, class) in [
            (1, LAND, ProvinceClass::Land),
            (2, SEA, ProvinceClass::Sea),
            (3, LAKE, ProvinceClass::Lake),
            (4, UNUSED, ProvinceClass::Unused),
            (5, EXTRA_LAND, ProvinceClass::Land),
        ] {
            if class != ProvinceClass::Unused {
                ids_by_color.insert(color, id);
            }
            provinces.insert(id, province(id, color, class));
        }
        (pixels, ids_by_color, provinces)
    }

    fn area_port_count(
        topology: &ProvinceTopology,
        provinces: &BTreeMap<u32, ProvinceState>,
        known_unowned: &BTreeSet<u32>,
        owner: &str,
        capital: u32,
    ) -> Option<usize> {
        let capital_state = provinces.get(&capital)?;
        if capital_state.class != ProvinceClass::Land
            || capital_state.owner.as_deref() != Some(owner)
        {
            return None;
        }

        let mut world = World::new(Schema::new(["base"]).ok()?);
        let owner_tags: BTreeSet<_> = provinces
            .values()
            .filter_map(|province| province.owner.clone())
            .collect();
        let owner_entities: BTreeMap<_, _> = owner_tags
            .into_iter()
            .map(|tag| {
                let entity = world.add_entity(tag.clone(), EntityKind::Country);
                (tag, entity)
            })
            .collect();
        let province_entities: BTreeMap<_, _> = provinces
            .keys()
            .map(|&id| {
                let entity = world.add_entity(id.to_string(), EntityKind::Province);
                (id, entity)
            })
            .collect();
        for (&id, province) in provinces {
            let entity = province_entities[&id];
            if let Some(owner) = &province.owner {
                world.set_owner(entity, owner_entities[owner]).ok()?;
            } else if province.class != ProvinceClass::Land || known_unowned.contains(&id) {
                world.mark_target_absent(entity, Target::Owner).ok()?;
            }
        }
        world
            .set_province_connectivity(topology.to_core(&province_entities).ok()?)
            .ok()?;
        let owner_area = world
            .province_owner_area(province_entities[&capital])
            .ok()??;
        let province_ids: BTreeMap<_, _> = province_entities
            .iter()
            .map(|(&id, &entity)| (entity, id))
            .collect();
        let mut count = 0;
        for entity in owner_area {
            let province = provinces.get(&province_ids[&entity])?;
            if province.class != ProvinceClass::Land {
                continue;
            }
            match province.has_port {
                Some(true) => count += 1,
                Some(false) => {}
                None => return None,
            }
        }
        Some(count)
    }

    #[test]
    fn sea_border_and_force_coastal_create_port_access() {
        let (pixels, ids_by_color, provinces) = map(3, 1, &[LAND, SEA, EXTRA_LAND]);
        let result = derive(&pixels, &ids_by_color, &provinces, &[5].into());
        assert_eq!(result.port_presence[&1], Some(true));
        assert_eq!(result.port_presence[&2], Some(false));
        assert_eq!(result.port_presence[&5], Some(true));
    }

    #[test]
    fn lake_and_diagonal_water_do_not_create_port_access() {
        let (pixels, ids_by_color, provinces) = map(2, 2, &[LAND, LAKE, EXTRA_LAND, SEA]);
        let result = derive(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(result.port_presence[&1], Some(false));
    }

    #[test]
    fn horizontal_map_edges_wrap() {
        let (pixels, ids_by_color, mut provinces) = map(3, 1, &[LAND, SEA, EXTRA_LAND]);
        provinces.get_mut(&1).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&1).unwrap().has_port = Some(false);
        provinces.get_mut(&5).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&5).unwrap().has_port = Some(true);
        let result = derive(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(result.port_presence[&1], Some(true));
        assert_eq!(
            area_port_count(&result.topology, &provinces, &BTreeSet::new(), "AAA", 1),
            Some(1)
        );
    }

    #[test]
    fn same_owner_neighbors_join_the_capital_area_but_foreign_land_does_not() {
        let (pixels, ids_by_color, mut provinces) = map(2, 1, &[LAND, EXTRA_LAND]);
        provinces.get_mut(&1).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&1).unwrap().has_port = Some(true);
        provinces.get_mut(&5).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&5).unwrap().has_port = Some(false);
        let result = derive(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(
            area_port_count(&result.topology, &provinces, &BTreeSet::new(), "AAA", 1),
            Some(1)
        );

        provinces.get_mut(&5).unwrap().owner = Some("BBB".into());
        assert_eq!(
            area_port_count(&result.topology, &provinces, &BTreeSet::new(), "AAA", 1),
            Some(1)
        );
    }

    #[test]
    fn unknown_neighbor_ownership_is_not_treated_as_foreign_or_unowned() {
        let (pixels, ids_by_color, mut provinces) = map(2, 1, &[LAND, EXTRA_LAND]);
        provinces.get_mut(&1).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&1).unwrap().has_port = Some(false);
        let result = derive(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(
            area_port_count(&result.topology, &provinces, &BTreeSet::new(), "AAA", 1),
            None
        );
        assert_eq!(
            area_port_count(&result.topology, &provinces, &[5].into(), "AAA", 1),
            Some(0)
        );
    }

    #[test]
    fn special_edges_connect_owned_provinces_and_skip_the_terminator() {
        let (pixels, ids_by_color, mut provinces) =
            map(4, 2, &[LAND, SEA, SEA, SEA, SEA, SEA, SEA, EXTRA_LAND]);
        provinces.get_mut(&1).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&1).unwrap().has_port = Some(false);
        provinces.get_mut(&5).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&5).unwrap().has_port = Some(true);
        let mut result = derive(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(
            area_port_count(&result.topology, &provinces, &BTreeSet::new(), "AAA", 1),
            Some(0)
        );
        let adjacency = TextDocument::from_bytes(
            "self-authored-adjacencies.csv",
            b"From;To;Type\n1;5;sea\n-1;-1;sea\n".to_vec(),
            crate::TextEncoding::Utf8,
        );
        result
            .topology
            .add_special_adjacencies(&adjacency, &provinces)
            .unwrap();
        assert_eq!(
            area_port_count(&result.topology, &provinces, &BTreeSet::new(), "AAA", 1),
            Some(1)
        );
    }

    #[test]
    fn non_sea_special_edges_also_join_the_owned_area() {
        let (pixels, ids_by_color, mut provinces) =
            map(4, 2, &[LAND, SEA, SEA, SEA, SEA, SEA, SEA, EXTRA_LAND]);
        provinces.get_mut(&1).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&1).unwrap().has_port = Some(false);
        provinces.get_mut(&5).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&5).unwrap().has_port = Some(true);
        let mut result = derive(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(
            area_port_count(&result.topology, &provinces, &BTreeSet::new(), "AAA", 1),
            Some(0)
        );
        let adjacency = TextDocument::from_bytes(
            "self-authored-adjacencies.csv",
            b"From;To;Type\n1;5;canal\n-1;-1;sea\n".to_vec(),
            crate::TextEncoding::Utf8,
        );
        result
            .topology
            .add_special_adjacencies(&adjacency, &provinces)
            .unwrap();
        assert_eq!(
            area_port_count(&result.topology, &provinces, &BTreeSet::new(), "AAA", 1),
            Some(1)
        );
    }

    #[test]
    fn unknown_boundaries_make_the_capital_area_count_unknown() {
        let colors = [[9, 0, 0], LAND];
        let (pixels, ids_by_color, mut provinces) = map(2, 1, &colors);
        provinces.get_mut(&1).unwrap().owner = Some("AAA".into());
        provinces.get_mut(&1).unwrap().has_port = Some(false);
        let result = derive(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(
            area_port_count(&result.topology, &provinces, &BTreeSet::new(), "AAA", 1),
            None
        );
    }

    #[test]
    fn absent_special_adjacency_endpoints_are_rejected() {
        let (_, _, provinces) = map(1, 1, &[LAND]);
        let mut topology = ProvinceTopology::default();
        let adjacency = TextDocument::from_bytes(
            "self-authored-adjacencies.csv",
            b"From;To;Type\n1;99;sea\n".to_vec(),
            crate::TextEncoding::Utf8,
        );
        let error = topology
            .add_special_adjacencies(&adjacency, &provinces)
            .unwrap_err();
        assert!(error.to_string().contains("undefined province"));
    }
}
