// SPDX-License-Identifier: GPL-3.0-only

//! 从地形定义和位图决定每个陆地省份的地形补给。海、湖和荒地不参与。

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use augustus_core::{Province, ProvinceKind};
use jomini::{Windows1252Encoding, text::ValueReader};

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    map::{
        ProvinceMap,
        indexed::{self, IndexedBitmap, RgbBitmap},
    },
    script,
};

struct Category {
    supply_limit: u32,
    water: bool,
    /// 该类别在地形段里出现过的最小调色板序号，树木改写用它打破平票。
    lowest_index: Option<u8>,
}

struct Tables {
    categories: Vec<Category>,
    index_category: [Option<u16>; 256],
    tree_category: [Option<u16>; 256],
    /// 省份首次出现的类别优先，值是该类别的地形补给。
    overrides: BTreeMap<u32, u32>,
}

struct MapPaths {
    terrain: PathBuf,
    rivers: PathBuf,
    definition: PathBuf,
    trees: PathBuf,
}

pub(super) fn apply(
    content: &ContentRoot,
    map: &ProvinceMap,
    provinces: &mut BTreeMap<u32, Province>,
) -> Result<(), Error> {
    let paths = map_paths(&content.read("map/default.map")?)?;
    let definition = content.read(&paths.definition)?;
    let tables = parse_tables(&definition)?;
    let terrain_file = content.read(&paths.terrain)?;
    let terrain = indexed::read_indexed(&terrain_file.bytes)
        .map_err(|error| Error::invalid(&terrain_file.path, error))?;
    let river_file = content.read(&paths.rivers)?;
    let rivers = indexed::read_rgb(&river_file.bytes)
        .map_err(|error| Error::invalid(&river_file.path, error))?;
    let tree_file = content.read(&paths.trees)?;
    let trees = indexed::read_indexed(&tree_file.bytes)
        .map_err(|error| Error::invalid(&tree_file.path, error))?;
    let summary = map.summary();
    assign(
        &tables,
        &TerrainMaps {
            terrain: &terrain,
            rivers: &rivers,
            trees: &trees,
            province_pixels: map.province_ids(),
            width: summary.width,
            height: summary.height,
            mapped: map.mapped_province_ids(),
            source: &definition.path,
        },
        provinces,
    )
}

struct TerrainMaps<'a> {
    terrain: &'a IndexedBitmap,
    rivers: &'a RgbBitmap,
    trees: &'a IndexedBitmap,
    province_pixels: &'a [u32],
    width: u32,
    height: u32,
    mapped: &'a BTreeSet<u32>,
    source: &'a Path,
}

fn map_paths(file: &ContentFile) -> Result<MapPaths, Error> {
    let tape = script::parse(file)?;
    let mut terrain = None;
    let mut rivers = None;
    let mut definition = None;
    let mut trees = None;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        let name = key.read_str();
        let slot = match name.as_ref() {
            "terrain" => &mut terrain,
            "rivers" => &mut rivers,
            "terrain_definition" => &mut definition,
            "tree_definition" => &mut trees,
            _ => continue,
        };
        script::equal(file, operator, &name)?;
        if slot.is_some() {
            return Err(Error::invalid(&file.path, format!("duplicate {name}")));
        }
        let filename = script::content_path(&value, file)?;
        if filename.is_empty() {
            return Err(Error::invalid(
                &file.path,
                format!("{name}: expected a filename"),
            ));
        }
        *slot = Some(PathBuf::from("map").join(filename));
    }
    let required = |value: Option<PathBuf>, name: &str| {
        value.ok_or_else(|| Error::invalid(&file.path, format!("missing {name}")))
    };
    Ok(MapPaths {
        terrain: required(terrain, "terrain")?,
        rivers: required(rivers, "rivers")?,
        definition: required(definition, "terrain_definition")?,
        trees: required(trees, "tree_definition")?,
    })
}

fn parse_tables(file: &ContentFile) -> Result<Tables, Error> {
    let tape = script::parse(file)?;
    let mut categories_object = None;
    let mut terrain_object = None;
    let mut tree_object = None;
    for (key, operator, value) in tape.windows1252_reader().fields() {
        let name = key.read_str();
        let slot = match name.as_ref() {
            "categories" => &mut categories_object,
            "terrain" => &mut terrain_object,
            "tree" => &mut tree_object,
            _ => continue,
        };
        script::equal(file, operator, &name)?;
        if slot.is_some() {
            return Err(Error::invalid(
                &file.path,
                format!("duplicate {name} section"),
            ));
        }
        *slot = Some(
            value
                .read_object()
                .map_err(|error| Error::invalid(&file.path, error.to_string()))?,
        );
    }
    let categories_object = categories_object
        .ok_or_else(|| Error::invalid(&file.path, "missing categories section"))?;
    let terrain_object =
        terrain_object.ok_or_else(|| Error::invalid(&file.path, "missing terrain section"))?;
    let tree_object =
        tree_object.ok_or_else(|| Error::invalid(&file.path, "missing tree section"))?;

    let mut tables = Tables {
        categories: Vec::new(),
        index_category: [None; 256],
        tree_category: [None; 256],
        overrides: BTreeMap::new(),
    };
    let mut category_names = BTreeMap::new();
    for (key, operator, value) in categories_object.fields() {
        let name = key.read_str().into_owned();
        script::equal(file, operator, &name)?;
        if category_names.contains_key(&name) {
            return Err(Error::invalid(
                &file.path,
                format!("duplicate terrain category: {name}"),
            ));
        }
        let object = value
            .read_object()
            .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
        let mut supply_limit = 0;
        let mut water = false;
        let mut overrides = Vec::new();
        for (key, operator, value) in object.fields() {
            let field = key.read_str();
            match field.as_ref() {
                "supply_limit" => {
                    script::equal(file, operator, &field)?;
                    supply_limit = whole_number(&value, file, "supply_limit")?;
                }
                "is_water" | "inland_sea" => {
                    script::equal(file, operator, &field)?;
                    water |= yes(&value, file, &field)?;
                }
                "terrain_override" => {
                    script::equal(file, operator, &field)?;
                    overrides = province_ids(&value, file)?;
                }
                _ => (),
            }
        }
        let index = u16::try_from(tables.categories.len())
            .map_err(|_| Error::invalid(&file.path, "too many terrain categories"))?;
        for province in overrides {
            tables.overrides.entry(province).or_insert(supply_limit);
        }
        category_names.insert(name, index);
        tables.categories.push(Category {
            supply_limit,
            water,
            lowest_index: None,
        });
    }

    let mut graphics = BTreeMap::new();
    for (key, operator, value) in terrain_object.fields() {
        let name = key.read_str().into_owned();
        script::equal(file, operator, &name)?;
        if graphics.contains_key(&name) {
            return Err(Error::invalid(
                &file.path,
                format!("duplicate terrain graphic: {name}"),
            ));
        }
        let (category_name, colors) = typed_colors(&value, file, "type")?;
        let category = *category_names.get(&category_name).ok_or_else(|| {
            Error::invalid(
                &file.path,
                format!("unknown terrain category: {category_name}"),
            )
        })?;
        for color in colors {
            if tables.index_category[usize::from(color)].is_some() {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate terrain color index {color}"),
                ));
            }
            tables.index_category[usize::from(color)] = Some(category);
            let lowest = &mut tables.categories[usize::from(category)].lowest_index;
            *lowest = Some(lowest.map_or(color, |current| current.min(color)));
        }
        graphics.insert(name, category);
    }

    for (key, operator, value) in tree_object.fields() {
        let name = key.read_str();
        script::equal(file, operator, &name)?;
        let (graphic_name, colors) = typed_colors(&value, file, "terrain")?;
        let category = *graphics.get(&graphic_name).ok_or_else(|| {
            Error::invalid(&file.path, format!("unknown tree terrain: {graphic_name}"))
        })?;
        for color in colors {
            if tables.tree_category[usize::from(color)].is_some() {
                return Err(Error::invalid(
                    &file.path,
                    format!("duplicate tree color index {color}"),
                ));
            }
            tables.tree_category[usize::from(color)] = Some(category);
        }
    }
    Ok(tables)
}

fn typed_colors(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
    type_field: &str,
) -> Result<(String, Vec<u8>), Error> {
    let object = value
        .read_object()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    let mut kind = None;
    let mut colors = None;
    for (key, operator, value) in object.fields() {
        let field = key.read_str();
        if field.as_ref() != type_field && field.as_ref() != "color" {
            continue;
        }
        script::equal(file, operator, &field)?;
        if field.as_ref() == type_field {
            kind = Some(
                value
                    .read_string()
                    .map_err(|error| Error::invalid(&file.path, error.to_string()))?,
            );
        } else {
            colors = Some(color_indices(&value, file)?);
        }
    }
    Ok((
        kind.ok_or_else(|| Error::invalid(&file.path, format!("missing {type_field}")))?,
        colors.ok_or_else(|| Error::invalid(&file.path, "missing color"))?,
    ))
}

fn assign(
    tables: &Tables,
    maps: &TerrainMaps<'_>,
    provinces: &mut BTreeMap<u32, Province>,
) -> Result<(), Error> {
    let TerrainMaps {
        terrain,
        rivers,
        trees,
        province_pixels,
        width,
        height,
        mapped,
        source,
    } = maps;
    let width = *width;
    let height = *height;
    if terrain.width != width
        || terrain.height != height
        || terrain.indices.len() != province_pixels.len()
        || rivers.width != width
        || rivers.height != height
        || rivers.pixels.len() != province_pixels.len()
    {
        return Err(Error::invalid(
            source,
            format!("terrain bitmaps must be {width} x {height}, matching the province map"),
        ));
    }
    if trees.width == 0
        || trees.height == 0
        || trees.indices.len() != (trees.width as usize) * (trees.height as usize)
    {
        return Err(Error::invalid(source, "tree bitmap is empty"));
    }
    for &province in tables.overrides.keys() {
        if !provinces.contains_key(&province) {
            return Err(Error::invalid(
                source,
                format!("unknown terrain override province: {province}"),
            ));
        }
    }

    let mut resolved = BTreeMap::new();
    let mut voting = BTreeSet::new();
    for (&id, province) in provinces.iter() {
        if province.kind != ProvinceKind::Land {
            continue;
        }
        if let Some(&supply) = tables.overrides.get(&id) {
            resolved.insert(id, supply);
        } else if mapped.contains(&id) {
            voting.insert(id);
        }
    }

    let trees = tree_mask(tables, trees, width, height);
    let mut ballots: BTreeMap<u32, BTreeMap<u16, (u32, u8)>> = BTreeMap::new();
    for (index, &province) in province_pixels.iter().enumerate() {
        if !voting.contains(&province) || is_river(rivers.pixels[index]) {
            continue;
        }
        let (category, tie) = if let Some(category) = trees[index] {
            let tie = tables.categories[usize::from(category)]
                .lowest_index
                .unwrap_or(u8::MAX);
            (category, tie)
        } else {
            let color = terrain.indices[index];
            let Some(category) = tables.index_category[usize::from(color)] else {
                continue;
            };
            (category, color)
        };
        if tables.categories[usize::from(category)].water {
            continue;
        }
        let tally = ballots
            .entry(province)
            .or_default()
            .entry(category)
            .or_insert((0, tie));
        tally.0 = tally.0.saturating_add(1);
        tally.1 = tally.1.min(tie);
    }

    for province in voting {
        let winner = ballots.get(&province).and_then(|votes| {
            votes
                .iter()
                .max_by(|left, right| left.1.0.cmp(&right.1.0).then(right.1.1.cmp(&left.1.1)))
                .map(|(category, _)| *category)
        });
        let Some(category) = winner else {
            return Err(Error::invalid(
                source,
                format!("mapped land province {province} has no terrain"),
            ));
        };
        resolved.insert(
            province,
            tables.categories[usize::from(category)].supply_limit,
        );
    }
    for (id, supply) in resolved {
        provinces
            .get_mut(&id)
            .expect("已确认省份存在")
            .terrain_supply = supply;
    }
    Ok(())
}

/// 原版河流位图只把这四种颜色当成河流。(0, 225, 255) 是河源或边界，仍然可以计算补给。
fn is_river(pixel: [u8; 3]) -> bool {
    matches!(
        pixel,
        [0, 200, 255] | [0, 100, 255] | [0, 0, 200] | [0, 150, 255]
    )
}

/// 树木图和地形图同尺寸时逐像素对应。原版树木图更小：每格起点是列号乘比例再减 2，负坐标裁掉。
fn tree_mask(tables: &Tables, trees: &IndexedBitmap, width: u32, height: u32) -> Vec<Option<u16>> {
    let mut mask = vec![None; (width as usize) * (height as usize)];
    if trees.width == width && trees.height == height {
        for (pixel, index) in mask.iter_mut().zip(&trees.indices) {
            *pixel = tables.tree_category[usize::from(*index)];
        }
        return mask;
    }
    let step_x = f64::from(width) / f64::from(trees.width);
    let step_y = f64::from(height) / f64::from(trees.height);
    let paint_w = step_x.round() as i64;
    let paint_h = (step_y - 1.0).round() as i64;
    if paint_w <= 0 || paint_h <= 0 {
        return mask;
    }
    for row in 0..trees.height {
        for col in 0..trees.width {
            let index = trees.indices[(row * trees.width + col) as usize];
            let Some(category) = tables.tree_category[usize::from(index)] else {
                continue;
            };
            let x0 = (f64::from(col) * step_x - 2.0).floor() as i64;
            let y0 = (f64::from(row) * step_y - 2.0).floor() as i64;
            let x_start = u32::try_from(x0.max(0)).unwrap_or(0);
            let y_start = u32::try_from(y0.max(0)).unwrap_or(0);
            let x_end = (x0 + paint_w).clamp(0, i64::from(width)) as u32;
            let y_end = (y0 + paint_h).clamp(0, i64::from(height)) as u32;
            for y in y_start..y_end {
                for x in x_start..x_end {
                    mask[(y * width + x) as usize] = Some(category);
                }
            }
        }
    }
    mask
}

fn whole_number(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
    field: &str,
) -> Result<u32, Error> {
    let scalar = value
        .read_scalar()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    if let Ok(number) = scalar.to_u64() {
        return u32::try_from(number)
            .map_err(|_| Error::invalid(&file.path, format!("{field} is out of range")));
    }
    let Ok(number) = scalar.to_f64() else {
        return Err(Error::invalid(&file.path, format!("invalid {field}")));
    };
    if number >= 0.0 && number.fract() == 0.0 && number <= u32::MAX as f64 {
        Ok(number as u32)
    } else {
        Err(Error::invalid(
            &file.path,
            format!("{field} must be a non-negative whole number"),
        ))
    }
}

fn yes(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
    field: &str,
) -> Result<bool, Error> {
    value
        .read_scalar()
        .ok()
        .and_then(|scalar| scalar.to_bool().ok())
        .ok_or_else(|| Error::invalid(&file.path, format!("{field} must be yes or no")))
}

fn color_indices(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<Vec<u8>, Error> {
    let array = value
        .read_array()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))?;
    array
        .values()
        .map(|value| {
            value
                .read_scalar()
                .ok()
                .and_then(|scalar| scalar.to_u64().ok())
                .and_then(|index| u8::try_from(index).ok())
                .ok_or_else(|| Error::invalid(&file.path, "terrain color index must be 0..=255"))
        })
        .collect()
}

fn province_ids(
    value: &ValueReader<'_, '_, Windows1252Encoding>,
    file: &ContentFile,
) -> Result<Vec<u32>, Error> {
    let read_one = |value: &ValueReader<'_, '_, Windows1252Encoding>| {
        value
            .read_scalar()
            .ok()
            .and_then(|scalar| scalar.to_u64().ok())
            .and_then(|id| u32::try_from(id).ok())
            .filter(|id| *id > 0)
            .ok_or_else(|| Error::invalid(&file.path, "expected a positive province ID"))
    };
    if let Ok(array) = value.read_array() {
        return array.values().map(|value| read_one(&value)).collect();
    }
    read_one(value).map(|id| vec![id])
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        path::PathBuf,
    };

    use augustus_core::{Province, ProvinceKind};

    use super::{TerrainMaps, assign, map_paths, parse_tables};
    use crate::{
        content::ContentFile,
        map::indexed::{IndexedBitmap, RgbBitmap},
    };

    const RULES: &str = "
categories = {
    ocean = { inland_sea = yes }
    plains = { supply_limit = 1 }
    forest = { supply_limit = 9.0 }
    farmlands = { supply_limit = 3 }
    hills = { supply_limit = 8 }
}
terrain = {
    woods = { type = forest color = { 1 } }
    sea = { type = ocean color = { 2 } }
    grasslands = { type = plains color = { 4 } }
    farms = { type = farmlands color = { 5 } }
    highlands = { type = hills color = { 8 } }
}
tree = {
    forest = { terrain = woods color = { 7 } }
}
";

    fn file(path: &str, text: &str) -> ContentFile {
        ContentFile {
            path: PathBuf::from(path),
            bytes: text.as_bytes().to_vec(),
        }
    }

    fn province(kind: ProvinceKind) -> Province {
        Province::new(kind)
    }

    fn indexed(width: u32, height: u32, indices: &[u8]) -> IndexedBitmap {
        IndexedBitmap {
            width,
            height,
            indices: indices.to_vec(),
        }
    }

    fn rgb(width: u32, height: u32, pixels: &[[u8; 3]]) -> RgbBitmap {
        RgbBitmap {
            width,
            height,
            pixels: pixels.to_vec(),
        }
    }

    fn run(
        text: &str,
        provinces_pixels: &[u32],
        colors: &[u8],
        rivers: &[[u8; 3]],
        trees: &IndexedBitmap,
        provinces: &[(u32, ProvinceKind)],
    ) -> Result<BTreeMap<u32, Province>, crate::Error> {
        let width = u32::try_from(provinces_pixels.len()).expect("宽度有效");
        let definition = file("map/terrain.txt", text);
        let tables = parse_tables(&definition)?;
        let mut states = provinces
            .iter()
            .map(|(id, kind)| (*id, province(*kind)))
            .collect();
        let mapped = provinces_pixels
            .iter()
            .copied()
            .filter(|id| *id > 0)
            .collect();
        assign(
            &tables,
            &TerrainMaps {
                terrain: &indexed(width, 1, colors),
                rivers: &rgb(width, 1, rivers),
                trees,
                province_pixels: provinces_pixels,
                width,
                height: 1,
                mapped: &mapped,
                source: definition.path.as_path(),
            },
            &mut states,
        )?;
        Ok(states)
    }

    #[test]
    fn votes_skip_water_and_overrides_keep_file_order() {
        let trees = indexed(2, 1, &[0, 0]);
        let states = run(
            RULES
                .replace(
                    "farmlands = { supply_limit = 3 }",
                    "farmlands = { supply_limit = 3 terrain_override = { 1 7 } }",
                )
                .replace(
                    "hills = { supply_limit = 8 }",
                    "hills = { supply_limit = 8 terrain_override = { 1 } }",
                )
                .as_str(),
            &[1, 1],
            &[8, 8],
            &[[0, 0, 0], [0, 0, 0]],
            &trees,
            &[(1, ProvinceKind::Land), (7, ProvinceKind::Sea)],
        )
        .expect("地形有效");
        // 省份 1 同时被 farmlands 和 hills 指定，文件中 farmlands 在前。像素全是 hills。
        assert_eq!(states[&1].terrain_supply, 3);
        assert_eq!(states[&7].terrain_supply, 0);

        let tied = run(
            RULES,
            &[8, 8],
            &[1, 4],
            &[[0, 0, 0], [0, 0, 0]],
            &trees,
            &[(8, ProvinceKind::Land)],
        )
        .expect("平票可决定");
        // 序号 1 与 4 各一票，较小序号属于 forest。
        assert_eq!(tied[&8].terrain_supply, 9);

        let dry = run(
            RULES,
            &[6, 6],
            &[2, 4],
            &[[0, 0, 0], [0, 0, 0]],
            &trees,
            &[(6, ProvinceKind::Land)],
        )
        .expect("水域不投票");
        // 若水域也投票，较小序号会让 ocean 胜出。
        assert_eq!(dry[&6].terrain_supply, 1);

        let pale = run(
            RULES,
            &[6, 6],
            &[1, 4],
            &[[0, 225, 255], [0, 0, 0]],
            &trees,
            &[(6, ProvinceKind::Land)],
        )
        .expect("浅青不是河流");
        // 若把 (0, 225, 255) 当成河流，只剩 plains 一票。
        assert_eq!(pale[&6].terrain_supply, 9);
    }

    #[test]
    fn river_pixel_cannot_win_a_tie() {
        let trees = indexed(2, 1, &[0, 0]);
        let states = run(
            RULES,
            &[5, 5],
            &[1, 4],
            &[[0, 100, 255], [0, 0, 0]],
            &trees,
            &[(5, ProvinceKind::Land)],
        )
        .expect("河流被排除");
        assert_eq!(states[&5].terrain_supply, 1);
    }

    #[test]
    fn unmapped_land_and_sea_do_not_need_a_terrain() {
        let trees = indexed(1, 1, &[0]);
        let definition = file("map/terrain.txt", RULES);
        let tables = parse_tables(&definition).expect("定义有效");
        let mut states = BTreeMap::from([
            (3, province(ProvinceKind::Land)),
            (4, province(ProvinceKind::Sea)),
        ]);
        assign(
            &tables,
            &TerrainMaps {
                terrain: &indexed(1, 1, &[4]),
                rivers: &rgb(1, 1, &[[0, 0, 0]]),
                trees: &trees,
                province_pixels: &[4],
                width: 1,
                height: 1,
                mapped: &BTreeSet::from([4]),
                source: definition.path.as_path(),
            },
            &mut states,
        )
        .expect("未绘制陆地和海洋不必有地形");
        assert_eq!(states[&3].terrain_supply, 0);
        assert_eq!(states[&4].terrain_supply, 0);
    }

    #[test]
    fn water_only_land_duplicate_color_and_unknown_override_fail() {
        let trees = indexed(1, 1, &[0]);
        let water = run(
            RULES,
            &[5],
            &[2],
            &[[0, 0, 0]],
            &trees,
            &[(5, ProvinceKind::Land)],
        );
        assert!(water.unwrap_err().to_string().contains("province 5"));

        let duplicate = parse_tables(&file(
            "map/terrain.txt",
            "
categories = { plains = { supply_limit = 1 } }
terrain = {
    one = { type = plains color = { 4 } }
    two = { type = plains color = { 4 } }
}
tree = {}
",
        ));
        assert!(
            duplicate
                .map(|_| ())
                .unwrap_err()
                .to_string()
                .contains("duplicate terrain color")
        );

        let unknown = "
categories = { plains = { terrain_override = { 99 } } }
terrain = { grass = { type = plains color = { 4 } } }
tree = {}
";
        let failed = run(
            unknown,
            &[1],
            &[4],
            &[[0, 0, 0]],
            &trees,
            &[(1, ProvinceKind::Land)],
        );
        assert!(failed.unwrap_err().to_string().contains("province: 99"));
    }

    #[test]
    fn scaled_trees_paint_before_the_vote_and_matching_sizes_do_not_shift() {
        let definition = file("map/terrain.txt", RULES);
        let tables = parse_tables(&definition).expect("定义有效");
        let mut terrain_indices = vec![2u8; 48];
        terrain_indices[47] = 4;
        let mut provinces = vec![1u32; 48];
        provinces[47] = 2;
        let mut states = BTreeMap::from([
            (1, province(ProvinceKind::Land)),
            (2, province(ProvinceKind::Land)),
        ]);
        assign(
            &tables,
            &TerrainMaps {
                terrain: &indexed(8, 6, &terrain_indices),
                rivers: &rgb(8, 6, &[[0, 0, 0]; 48]),
                trees: &indexed(1, 1, &[7]),
                province_pixels: &provinces,
                width: 8,
                height: 6,
                mapped: &BTreeSet::from([1, 2]),
                source: definition.path.as_path(),
            },
            &mut states,
        )
        .expect("缩放涂色有效");
        assert_eq!(states[&1].terrain_supply, 9);
        assert_eq!(states[&2].terrain_supply, 1);

        let mut same = BTreeMap::from([(1, province(ProvinceKind::Land))]);
        assign(
            &tables,
            &TerrainMaps {
                terrain: &indexed(2, 2, &[2, 2, 2, 2]),
                rivers: &rgb(2, 2, &[[0, 0, 0]; 4]),
                trees: &indexed(2, 2, &[0, 7, 0, 0]),
                province_pixels: &[0, 1, 0, 0],
                width: 2,
                height: 2,
                mapped: &BTreeSet::from([1]),
                source: definition.path.as_path(),
            },
            &mut same,
        )
        .expect("同尺寸树木不位移");
        assert_eq!(same[&1].terrain_supply, 9);
    }

    #[test]
    fn default_map_paths_join_the_map_directory() {
        let file = file(
            "map/default.map",
            "terrain = terrain.bmp\nrivers = \"rivers.bmp\"\nterrain_definition = \"defs\\\\terrain.txt\"\ntree_definition = trees.bmp\n",
        );
        let paths = map_paths(&file).expect("路径有效");
        assert_eq!(paths.terrain, PathBuf::from("map/terrain.bmp"));
        assert_eq!(paths.rivers, PathBuf::from("map/rivers.bmp"));
        assert_eq!(paths.definition, PathBuf::from("map/defs/terrain.txt"));
        assert_eq!(paths.trees, PathBuf::from("map/trees.bmp"));
    }
}
