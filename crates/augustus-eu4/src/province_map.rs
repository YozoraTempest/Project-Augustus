// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::collections::{BTreeMap, BTreeSet};

use augustus_core::{ProvinceClass, ProvinceState};

pub(crate) fn port_presence(
    pixels: &image::RgbImage,
    province_ids_by_color: &BTreeMap<[u8; 3], u32>,
    provinces: &BTreeMap<u32, ProvinceState>,
    force_coastal: &BTreeSet<u32>,
) -> BTreeMap<u32, Option<bool>> {
    let width = pixels.width() as usize;
    let height = pixels.height() as usize;
    let raster: Vec<_> = pixels
        .pixels()
        .map(|pixel| province_ids_by_color.get(&pixel.0).copied())
        .collect();
    let mut coastal = BTreeSet::new();
    let mut uncertain = BTreeSet::new();

    for (index, province_id) in raster.iter().enumerate() {
        let Some(province_id) = province_id else {
            continue;
        };
        if provinces
            .get(province_id)
            .is_none_or(|province| province.class != ProvinceClass::Land)
        {
            continue;
        }

        let x = index % width;
        let y = index / width;
        let neighbors = [
            Some(y * width + (x + width - 1) % width),
            Some(y * width + (x + 1) % width),
            (y > 0).then(|| index - width),
            (y + 1 < height).then(|| index + width),
        ];
        for neighbor in neighbors.into_iter().flatten() {
            match raster[neighbor].and_then(|id| provinces.get(&id)) {
                Some(province) if province.class == ProvinceClass::Sea => {
                    coastal.insert(*province_id);
                }
                Some(province) if province.class == ProvinceClass::Unused => {
                    uncertain.insert(*province_id);
                }
                None => {
                    uncertain.insert(*province_id);
                }
                _ => {}
            }
        }
    }

    provinces
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
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            ids_by_color.insert(color, id);
            provinces.insert(id, province(id, color, class));
        }
        (pixels, ids_by_color, provinces)
    }

    #[test]
    fn sea_border_and_force_coastal_create_port_access() {
        let (pixels, ids_by_color, provinces) = map(3, 1, &[LAND, SEA, EXTRA_LAND]);
        let result = port_presence(&pixels, &ids_by_color, &provinces, &[5].into());
        assert_eq!(result[&1], Some(true));
        assert_eq!(result[&2], Some(false));
        assert_eq!(result[&5], Some(true));
    }

    #[test]
    fn lake_and_diagonal_water_do_not_create_port_access() {
        let (pixels, ids_by_color, provinces) = map(2, 2, &[LAND, LAKE, EXTRA_LAND, SEA]);
        let result = port_presence(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(result[&1], Some(false));
    }

    #[test]
    fn horizontal_map_edges_wrap() {
        let (pixels, ids_by_color, provinces) = map(2, 1, &[LAND, SEA]);
        let result = port_presence(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(result[&1], Some(true));
    }

    #[test]
    fn unknown_border_keeps_port_presence_unknown_unless_sea_is_also_adjacent() {
        let colors = [[9, 0, 0], SEA, LAND];
        let (pixels, mut ids_by_color, mut provinces) = map(3, 1, &colors);
        ids_by_color.remove(&UNUSED);
        ids_by_color.insert(LAND, 1);
        provinces.remove(&4);
        let result = port_presence(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(result[&1], Some(true));

        let (pixels, ids_by_color, provinces) = map(2, 1, &[[9, 0, 0], LAND]);
        let result = port_presence(&pixels, &ids_by_color, &provinces, &BTreeSet::new());
        assert_eq!(result[&1], None);
    }
}
