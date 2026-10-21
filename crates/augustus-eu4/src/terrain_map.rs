// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::Path,
};

use augustus_core::{ProvinceClass, ProvinceState};
use image::{ImageDecoder, RgbImage, codecs::bmp::BmpDecoder};
use jomini::text::Operator;

use crate::{Error, ParsedText, TextDocument};

// Vanilla EU4 uses these two palette entries for non-river pixels.
const RIVER_BACKGROUND_INDEXES: [u8; 2] = [254, 255];

#[derive(Debug, Default)]
pub(crate) struct TerrainDefinitions {
    terrain_by_index: BTreeMap<u8, String>,
    tree_by_index: BTreeMap<u8, String>,
    overrides: BTreeMap<u32, String>,
    water_types: BTreeSet<String>,
    priorities: BTreeMap<String, usize>,
}

#[derive(Debug)]
pub(crate) struct IndexedBitmap {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
    palette_len: usize,
}

pub(crate) struct TerrainMap<'a> {
    pub(crate) terrain: &'a IndexedBitmap,
    pub(crate) rivers: Option<&'a IndexedBitmap>,
    pub(crate) trees: Option<&'a IndexedBitmap>,
    pub(crate) automatic_tree_indexes: &'a BTreeSet<u8>,
    pub(crate) definitions: &'a TerrainDefinitions,
}

impl IndexedBitmap {
    pub(crate) fn decode(document: &TextDocument) -> Result<Self, Error> {
        let mut decoder = BmpDecoder::new(Cursor::new(document.bytes()))
            .map_err(|error| Error::caused_by(document.source(), error))?;
        let palette_len = decoder
            .get_palette()
            .map(<[[u8; 3]]>::len)
            .ok_or_else(|| Error::message(document.source(), "expected indexed bitmap"))?;
        decoder.set_indexed_color(true);
        let (width, height) = decoder.dimensions();
        let pixel_count = usize::try_from(u64::from(width) * u64::from(height))
            .map_err(|_| Error::message(document.source(), "bitmap dimensions overflow"))?;
        if usize::try_from(decoder.total_bytes()).ok() != Some(pixel_count) {
            return Err(Error::message(
                document.source(),
                "indexed bitmap has an unexpected pixel layout",
            ));
        }
        let mut pixels = vec![0; pixel_count];
        decoder
            .read_image(&mut pixels)
            .map_err(|error| Error::caused_by(document.source(), error))?;
        if pixels
            .iter()
            .any(|&index| usize::from(index) >= palette_len)
        {
            return Err(Error::message(
                document.source(),
                "bitmap references a missing palette entry",
            ));
        }
        Ok(Self {
            width,
            height,
            pixels,
            palette_len,
        })
    }

    fn pixel(&self, x: u32, y: u32) -> u8 {
        self.pixels[(y * self.width + x) as usize]
    }
}

impl TerrainDefinitions {
    pub(crate) fn parse(parsed: &ParsedText<'_>) -> Result<Self, Error> {
        let mut definitions = Self::default();
        let mut next_priority = 0;

        for (key, operator, value) in parsed.reader().fields() {
            match key.read_string().as_str() {
                "categories" => {
                    require_equal(parsed, &key, operator)?;
                    for (category_key, category_operator, category_value) in value
                        .read_object()
                        .map_err(|_| located(parsed, &key, "expected terrain categories"))?
                        .fields()
                    {
                        require_equal(parsed, &category_key, category_operator)?;
                        let category = category_key.read_string();
                        definitions
                            .priorities
                            .entry(category.clone())
                            .or_insert_with(|| {
                                let priority = next_priority;
                                next_priority += 1;
                                priority
                            });
                        for (field_key, field_operator, field_value) in category_value
                            .read_object()
                            .map_err(|_| {
                                located(parsed, &category_key, "expected terrain category")
                            })?
                            .fields()
                        {
                            match field_key.read_string().as_str() {
                                "is_water" => {
                                    require_equal(parsed, &field_key, field_operator)?;
                                    if field_value.read_string().map_err(|_| {
                                        located(parsed, &field_key, "expected water flag")
                                    })? == "yes"
                                    {
                                        definitions.water_types.insert(category.clone());
                                    }
                                }
                                "terrain_override" => {
                                    require_equal(parsed, &field_key, field_operator)?;
                                    let values = field_value.read_array().map_err(|_| {
                                        located(
                                            parsed,
                                            &field_key,
                                            "expected terrain override list",
                                        )
                                    })?;
                                    for value in values.values() {
                                        let id = value
                                            .read_string()
                                            .map_err(|_| {
                                                located(
                                                    parsed,
                                                    &field_key,
                                                    "expected province identifier",
                                                )
                                            })?
                                            .parse::<u32>()
                                            .map_err(|_| {
                                                located(
                                                    parsed,
                                                    &field_key,
                                                    "invalid terrain override province",
                                                )
                                            })?;
                                        if id != 0 {
                                            definitions
                                                .overrides
                                                .entry(id)
                                                .or_insert_with(|| category.clone());
                                        }
                                    }
                                }
                                _ => (),
                            }
                        }
                    }
                }
                "terrain" => {
                    require_equal(parsed, &key, operator)?;
                    for (terrain_key, terrain_operator, terrain_value) in value
                        .read_object()
                        .map_err(|_| located(parsed, &key, "expected terrain palette"))?
                        .fields()
                    {
                        require_equal(parsed, &terrain_key, terrain_operator)?;
                        let mut terrain_type = None;
                        let mut colors = Vec::new();
                        for (field_key, field_operator, field_value) in terrain_value
                            .read_object()
                            .map_err(|_| {
                                located(parsed, &terrain_key, "expected terrain palette entry")
                            })?
                            .fields()
                        {
                            match field_key.read_string().as_str() {
                                "type" => {
                                    require_equal(parsed, &field_key, field_operator)?;
                                    terrain_type =
                                        Some(field_value.read_string().map_err(|_| {
                                            located(parsed, &field_key, "expected terrain type")
                                        })?);
                                }
                                "color" => {
                                    require_equal(parsed, &field_key, field_operator)?;
                                    let values = field_value.read_array().map_err(|_| {
                                        located(
                                            parsed,
                                            &field_key,
                                            "expected terrain palette indices",
                                        )
                                    })?;
                                    for value in values.values() {
                                        colors.push(parse_index(parsed, &field_key, value)?);
                                    }
                                }
                                _ => (),
                            }
                        }
                        if let Some(terrain_type) = terrain_type {
                            for index in colors {
                                insert_index_type(
                                    parsed,
                                    &terrain_key,
                                    &mut definitions.terrain_by_index,
                                    index,
                                    &terrain_type,
                                )?;
                                definitions
                                    .priorities
                                    .entry(terrain_type.clone())
                                    .or_insert_with(|| {
                                        let priority = next_priority;
                                        next_priority += 1;
                                        priority
                                    });
                            }
                        }
                    }
                }
                "tree" => {
                    require_equal(parsed, &key, operator)?;
                    for (tree_key, tree_operator, tree_value) in value
                        .read_object()
                        .map_err(|_| located(parsed, &key, "expected tree palette"))?
                        .fields()
                    {
                        require_equal(parsed, &tree_key, tree_operator)?;
                        let mut terrain_type = None;
                        let mut colors = Vec::new();
                        for (field_key, field_operator, field_value) in tree_value
                            .read_object()
                            .map_err(|_| located(parsed, &tree_key, "expected tree palette entry"))?
                            .fields()
                        {
                            match field_key.read_string().as_str() {
                                "terrain" => {
                                    require_equal(parsed, &field_key, field_operator)?;
                                    terrain_type =
                                        Some(field_value.read_string().map_err(|_| {
                                            located(parsed, &field_key, "expected tree terrain")
                                        })?);
                                }
                                "color" => {
                                    require_equal(parsed, &field_key, field_operator)?;
                                    let values = field_value.read_array().map_err(|_| {
                                        located(parsed, &field_key, "expected tree palette indices")
                                    })?;
                                    for value in values.values() {
                                        colors.push(parse_index(parsed, &field_key, value)?);
                                    }
                                }
                                _ => (),
                            }
                        }
                        if let Some(terrain_type) = terrain_type {
                            for index in colors {
                                insert_index_type(
                                    parsed,
                                    &tree_key,
                                    &mut definitions.tree_by_index,
                                    index,
                                    &terrain_type,
                                )?;
                            }
                        }
                    }
                }
                _ => (),
            }
        }

        Ok(definitions)
    }

    pub(crate) fn validate_palette_indexes(
        &self,
        terrain: &IndexedBitmap,
        trees: Option<&IndexedBitmap>,
        automatic_tree_indexes: &BTreeSet<u8>,
        source: &Path,
    ) -> Result<(), Error> {
        if self
            .terrain_by_index
            .keys()
            .any(|&index| usize::from(index) >= terrain.palette_len)
            || trees.is_some_and(|bitmap| {
                self.tree_by_index
                    .keys()
                    .any(|&index| usize::from(index) >= bitmap.palette_len)
                    || automatic_tree_indexes
                        .iter()
                        .any(|&index| usize::from(index) >= bitmap.palette_len)
            })
        {
            return Err(Error::message(
                source,
                "terrain definition references a missing bitmap palette index",
            ));
        }
        Ok(())
    }

    fn is_water(&self, terrain: &str) -> bool {
        self.water_types.contains(terrain)
    }

    fn priority(&self, terrain: &str) -> usize {
        self.priorities.get(terrain).copied().unwrap_or(usize::MAX)
    }
}

pub(crate) fn derive_province_terrains(
    province_pixels: &RgbImage,
    province_ids_by_color: &BTreeMap<[u8; 3], u32>,
    provinces: &BTreeMap<u32, ProvinceState>,
    map: TerrainMap<'_>,
) -> Result<BTreeMap<u32, Option<String>>, Error> {
    let TerrainMap {
        terrain,
        rivers,
        trees,
        automatic_tree_indexes,
        definitions,
    } = map;
    if terrain.width != province_pixels.width() || terrain.height != province_pixels.height() {
        return Err(Error::message(
            "map/terrain.bmp",
            "terrain bitmap dimensions do not match the province bitmap",
        ));
    }
    if let Some(rivers) = rivers
        && (rivers.width != terrain.width || rivers.height != terrain.height)
    {
        return Err(Error::message(
            "map/rivers.bmp",
            "river bitmap dimensions do not match the terrain bitmap",
        ));
    }
    if let Some(trees) = trees
        && (trees.width == 0 || trees.height == 0)
    {
        return Err(Error::message("map/trees.bmp", "tree bitmap is empty"));
    }

    let mut result = provinces
        .keys()
        .map(|&id| (id, None))
        .collect::<BTreeMap<_, _>>();
    for (&id, terrain) in &definitions.overrides {
        if let Some(result) = result.get_mut(&id) {
            *result = Some(terrain.clone());
        }
    }

    let mut counts = BTreeMap::<u32, BTreeMap<String, u64>>::new();
    let mut unknown = BTreeMap::<u32, u64>::new();
    let width = province_pixels.width();
    for (index, province_pixel) in province_pixels.pixels().enumerate() {
        let Some(&province_id) = province_ids_by_color.get(&province_pixel.0) else {
            continue;
        };
        let Some(province) = provinces.get(&province_id) else {
            continue;
        };
        if province.class == ProvinceClass::Unused
            || definitions.overrides.contains_key(&province_id)
        {
            continue;
        }

        let x = index as u32 % width;
        let y = index as u32 / width;
        if rivers.is_some_and(|bitmap| !RIVER_BACKGROUND_INDEXES.contains(&bitmap.pixels[index])) {
            continue;
        }
        let tree_type = trees.and_then(|bitmap| {
            if automatic_tree_indexes.is_empty() {
                return None;
            }
            let tree_x = (u64::from(x) * u64::from(bitmap.width) / u64::from(terrain.width)) as u32;
            let tree_y =
                (u64::from(y) * u64::from(bitmap.height) / u64::from(terrain.height)) as u32;
            let index = bitmap.pixel(tree_x, tree_y);
            automatic_tree_indexes
                .contains(&index)
                .then(|| definitions.tree_by_index.get(&index))
                .flatten()
        });
        let terrain_type =
            tree_type.or_else(|| definitions.terrain_by_index.get(&terrain.pixels[index]));

        let Some(terrain_type) = terrain_type else {
            *unknown.entry(province_id).or_default() += 1;
            continue;
        };
        let water = definitions.is_water(terrain_type);
        if matches!(
            province.class,
            ProvinceClass::Land | ProvinceClass::Wasteland
        ) && water
        {
            continue;
        }
        if matches!(province.class, ProvinceClass::Sea | ProvinceClass::Lake) && !water {
            continue;
        }
        *counts
            .entry(province_id)
            .or_default()
            .entry(terrain_type.clone())
            .or_default() += 1;
    }

    for (&province_id, terrain_counts) in &counts {
        let mut ranked = terrain_counts.iter().collect::<Vec<_>>();
        ranked.sort_by_key(|(terrain, count)| {
            (
                std::cmp::Reverse(**count),
                definitions.priority(terrain),
                terrain.as_str(),
            )
        });
        let Some((winner, &winner_count)) = ranked.first().copied() else {
            continue;
        };
        let runner_up = ranked.get(1).map_or(0, |(_, count)| **count);
        let unknown_count = unknown.get(&province_id).copied().unwrap_or_default();
        if winner_count > runner_up + unknown_count {
            result.insert(province_id, Some(winner.clone()));
        }
    }
    Ok(result)
}

fn insert_index_type(
    parsed: &ParsedText<'_>,
    key: &jomini::text::ScalarReader<'_, crate::TextEncoding>,
    values: &mut BTreeMap<u8, String>,
    index: u8,
    terrain: &str,
) -> Result<(), Error> {
    if let Some(previous) = values.get(&index) {
        if previous != terrain {
            return Err(located(parsed, key, "conflicting terrain bitmap index"));
        }
    } else {
        values.insert(index, terrain.into());
    }
    Ok(())
}

fn parse_index(
    parsed: &ParsedText<'_>,
    key: &jomini::text::ScalarReader<'_, crate::TextEncoding>,
    value: jomini::text::ValueReader<'_, '_, crate::TextEncoding>,
) -> Result<u8, Error> {
    value
        .read_string()
        .map_err(|_| located(parsed, key, "expected bitmap palette index"))?
        .parse::<u8>()
        .map_err(|_| located(parsed, key, "invalid bitmap palette index"))
}

fn require_equal(
    parsed: &ParsedText<'_>,
    key: &jomini::text::ScalarReader<'_, crate::TextEncoding>,
    operator: Option<Operator>,
) -> Result<(), Error> {
    if operator.is_some_and(|actual| actual != Operator::Equal) {
        return Err(located(
            parsed,
            key,
            &format!(
                "terrain definition field {} requires '='",
                key.read_string()
            ),
        ));
    }
    Ok(())
}

fn located(
    parsed: &ParsedText<'_>,
    key: &jomini::text::ScalarReader<'_, crate::TextEncoding>,
    message: &str,
) -> Error {
    Error::message(parsed.source(), message).with_location(parsed.field_location(key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{TextDocument, TextEncoding};

    fn province(id: u32, color: [u8; 3], class: ProvinceClass) -> ProvinceState {
        ProvinceState {
            id,
            name: format!("province-{id}"),
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

    fn definitions() -> TerrainDefinitions {
        TerrainDefinitions {
            terrain_by_index: BTreeMap::from([
                (0, "grasslands".into()),
                (1, "steppe".into()),
                (2, "ocean".into()),
            ]),
            tree_by_index: BTreeMap::from([(3, "forest".into())]),
            overrides: BTreeMap::from([(4, "steppe".into())]),
            water_types: BTreeSet::from(["ocean".into()]),
            priorities: BTreeMap::from([
                ("grasslands".into(), 0),
                ("steppe".into(), 1),
                ("ocean".into(), 2),
                ("forest".into(), 3),
            ]),
        }
    }

    fn indexed_bitmap(width: u32, height: u32, pixels: Vec<u8>) -> IndexedBitmap {
        IndexedBitmap {
            width,
            height,
            pixels,
            palette_len: 256,
        }
    }

    fn bmp_u16(bytes: &mut [u8], offset: usize, value: u16) {
        bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
    }

    fn bmp_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }

    fn indexed_bmp(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
        let row_stride = (width as usize).div_ceil(4) * 4;
        let palette_offset = 14 + 40;
        let pixel_offset = palette_offset + 4 * 4;
        let file_size = pixel_offset + row_stride * height as usize;
        let mut bytes = vec![0; file_size];
        bytes[0..2].copy_from_slice(b"BM");
        bmp_u32(&mut bytes, 2, file_size as u32);
        bmp_u32(&mut bytes, 10, pixel_offset as u32);
        bmp_u32(&mut bytes, 14, 40);
        bmp_u32(&mut bytes, 18, width);
        bmp_u32(&mut bytes, 22, height);
        bmp_u16(&mut bytes, 26, 1);
        bmp_u16(&mut bytes, 28, 8);
        bmp_u32(&mut bytes, 34, (row_stride * height as usize) as u32);
        bmp_u32(&mut bytes, 46, 4);
        for index in 0..4 {
            let palette = palette_offset + index * 4;
            bytes[palette..palette + 4].copy_from_slice(&[index as u8, 0, 0, 0]);
        }
        for file_y in 0..height as usize {
            let source_y = height as usize - file_y - 1;
            let row = pixel_offset + file_y * row_stride;
            let source = source_y * width as usize;
            bytes[row..row + width as usize]
                .copy_from_slice(&pixels[source..source + width as usize]);
        }
        bytes
    }

    #[test]
    fn indexed_bmp_decoder_preserves_palette_indexes_and_image_orientation() {
        let document = TextDocument::from_bytes(
            "self-authored-terrain.bmp",
            indexed_bmp(2, 2, &[0, 1, 2, 3]),
            TextEncoding::Utf8,
        );
        let bitmap = IndexedBitmap::decode(&document).unwrap();

        assert_eq!((bitmap.width, bitmap.height), (2, 2));
        assert_eq!(bitmap.pixels, [0, 1, 2, 3]);
    }

    #[test]
    fn terrain_definitions_keep_palette_types_and_first_province_override() {
        let document = TextDocument::from_bytes(
            "self-authored-terrain.txt",
            b"categories = { steppe = { type = plains terrain_override = { 2 3 } } grasslands = { type = plains terrain_override = { 2 } } ocean = { is_water = yes } } terrain = { grasslands = { type = grasslands color = { 0 } } ocean = { type = ocean color = { 2 } } } tree = { forest = { terrain = forest color = { 3 } } }".to_vec(),
            TextEncoding::Utf8,
        );
        let parsed = document.parse().unwrap();
        let definitions = TerrainDefinitions::parse(&parsed).unwrap();

        assert_eq!(definitions.terrain_by_index[&0], "grasslands");
        assert_eq!(definitions.terrain_by_index[&2], "ocean");
        assert_eq!(definitions.tree_by_index[&3], "forest");
        assert_eq!(definitions.overrides[&2], "steppe");
        assert_eq!(definitions.overrides[&3], "steppe");
        assert!(definitions.is_water("ocean"));
    }

    #[test]
    fn province_terrain_uses_water_filter_tree_overlay_and_explicit_overrides() {
        let colors = [[1, 0, 0], [2, 0, 0], [3, 0, 0], [4, 0, 0], [1, 0, 0]];
        let province_pixels = RgbImage::from_fn(5, 1, |x, _| image::Rgb(colors[x as usize]));
        let ids_by_color = BTreeMap::from([
            (colors[0], 1),
            (colors[1], 2),
            (colors[2], 3),
            (colors[3], 4),
        ]);
        let provinces = BTreeMap::from([
            (1, province(1, colors[0], ProvinceClass::Land)),
            (2, province(2, colors[1], ProvinceClass::Land)),
            (3, province(3, colors[2], ProvinceClass::Sea)),
            (4, province(4, colors[3], ProvinceClass::Land)),
        ]);
        let terrain = indexed_bitmap(5, 1, vec![2, 0, 2, 2, 0]);
        let trees = indexed_bitmap(1, 1, vec![0]);
        let automatic_tree_indexes = BTreeSet::from([3]);
        let definitions = definitions();
        let result = derive_province_terrains(
            &province_pixels,
            &ids_by_color,
            &provinces,
            TerrainMap {
                terrain: &terrain,
                rivers: None,
                trees: Some(&trees),
                automatic_tree_indexes: &automatic_tree_indexes,
                definitions: &definitions,
            },
        )
        .unwrap();

        assert_eq!(result[&1].as_deref(), Some("grasslands"));
        assert_eq!(result[&2].as_deref(), Some("grasslands"));
        assert_eq!(result[&3].as_deref(), Some("ocean"));
        assert_eq!(result[&4].as_deref(), Some("steppe"));
    }

    #[test]
    fn tree_palette_overlays_terrain_pixels() {
        let color = [1, 0, 0];
        let province_pixels = RgbImage::from_fn(8, 1, |_, _| image::Rgb(color));
        let provinces = BTreeMap::from([(1, province(1, color, ProvinceClass::Land))]);
        let terrain = indexed_bitmap(8, 1, vec![0; 8]);
        let trees = indexed_bitmap(1, 1, vec![3]);
        let automatic_tree_indexes = BTreeSet::from([3]);
        let definitions = definitions();
        let result = derive_province_terrains(
            &province_pixels,
            &BTreeMap::from([(color, 1)]),
            &provinces,
            TerrainMap {
                terrain: &terrain,
                rivers: None,
                trees: Some(&trees),
                automatic_tree_indexes: &automatic_tree_indexes,
                definitions: &definitions,
            },
        )
        .unwrap();

        assert_eq!(result[&1].as_deref(), Some("forest"));
    }

    #[test]
    fn tree_overlay_uses_scaled_map_coordinates_and_one_vote_per_pixel() {
        let color = [1, 0, 0];
        let province_pixels = RgbImage::from_fn(8, 1, |_, _| image::Rgb(color));
        let provinces = BTreeMap::from([(1, province(1, color, ProvinceClass::Land))]);
        let terrain = indexed_bitmap(8, 1, vec![0, 0, 0, 0, 1, 1, 1, 1]);
        let trees = indexed_bitmap(2, 1, vec![0, 3]);
        let automatic_tree_indexes = BTreeSet::from([3]);
        let definitions = definitions();
        let result = derive_province_terrains(
            &province_pixels,
            &BTreeMap::from([(color, 1)]),
            &provinces,
            TerrainMap {
                terrain: &terrain,
                rivers: None,
                trees: Some(&trees),
                automatic_tree_indexes: &automatic_tree_indexes,
                definitions: &definitions,
            },
        )
        .unwrap();

        assert!(result[&1].is_none());
    }

    #[test]
    fn river_pixels_do_not_vote_for_province_terrain() {
        let color = [1, 0, 0];
        let province_pixels = RgbImage::from_fn(10, 1, |_, _| image::Rgb(color));
        let provinces = BTreeMap::from([(1, province(1, color, ProvinceClass::Land))]);
        let terrain = indexed_bitmap(10, 1, vec![0, 0, 0, 0, 1, 1, 1, 1, 1, 1]);
        let rivers = indexed_bitmap(10, 1, vec![254, 254, 254, 254, 2, 2, 2, 2, 2, 255]);
        let automatic_tree_indexes = BTreeSet::new();
        let definitions = definitions();
        let result = derive_province_terrains(
            &province_pixels,
            &BTreeMap::from([(color, 1)]),
            &provinces,
            TerrainMap {
                terrain: &terrain,
                rivers: Some(&rivers),
                trees: None,
                automatic_tree_indexes: &automatic_tree_indexes,
                definitions: &definitions,
            },
        )
        .unwrap();

        assert_eq!(result[&1].as_deref(), Some("grasslands"));
    }

    #[test]
    fn ambiguous_pixels_leave_province_terrain_unknown() {
        let colors = [[1, 0, 0], [1, 0, 0]];
        let province_pixels = RgbImage::from_fn(2, 1, |x, _| image::Rgb(colors[x as usize]));
        let provinces = BTreeMap::from([(1, province(1, colors[0], ProvinceClass::Land))]);
        let terrain = indexed_bitmap(2, 1, vec![0, 9]);
        let automatic_tree_indexes = BTreeSet::new();
        let definitions = definitions();
        let result = derive_province_terrains(
            &province_pixels,
            &BTreeMap::from([(colors[0], 1)]),
            &provinces,
            TerrainMap {
                terrain: &terrain,
                rivers: None,
                trees: None,
                automatic_tree_indexes: &automatic_tree_indexes,
                definitions: &definitions,
            },
        )
        .unwrap();

        assert!(result[&1].is_none());
    }
}
