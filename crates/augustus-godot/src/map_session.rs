// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, HashMap};

use augustus_eu4::world::{OpeningWorld, START_DATE};
use godot::{
    classes::{Image, ImageTexture, image::Format},
    prelude::*,
};

#[derive(GodotClass)]
#[class(init, base = RefCounted)]
struct AugustusMap {
    world: Option<OpeningWorld>,
    texture: Option<Gd<ImageTexture>>,
    political_texture: Option<Gd<ImageTexture>>,
    owner_texture: Option<Gd<ImageTexture>>,
    // 渲染编号只在本次载入内使用；核心国家身份仍然是标签。
    country_render_ids: BTreeMap<String, u32>,
    // 玩家身份只在正式开局时确定，与地图上的浏览选择分开保存。
    player_country: Option<String>,
}

#[godot_api]
impl AugustusMap {
    /// 成功时返回空字符串；失败时返回原因，并保留当前地图。
    #[func]
    fn load_map(&mut self, game_dir: GString) -> GString {
        let world = match OpeningWorld::load(game_dir.to_string()) {
            Ok(world) => world,
            Err(error) => return error.to_string().as_str().into(),
        };
        let map = world.map();
        let summary = map.summary();
        let (Ok(width), Ok(height)) = (i32::try_from(summary.width), i32::try_from(summary.height))
        else {
            return "地图尺寸超出 Godot 图像范围".into();
        };
        let bytes = PackedByteArray::from(encode_ids(map.province_ids()).as_slice());
        let Some(image) = Image::create_from_data(width, height, false, Format::RGBA8, &bytes)
        else {
            return "无法创建省份编号图像".into();
        };
        let Some(texture) = ImageTexture::create_from_image(&image) else {
            return "无法创建省份编号纹理".into();
        };

        let bytes = PackedByteArray::from(political_colors(&world).as_slice());
        let Some(image) = Image::create_from_data(width, height, false, Format::RGBA8, &bytes)
        else {
            return "无法创建政治地图图像".into();
        };
        let Some(political_texture) = ImageTexture::create_from_image(&image) else {
            return "无法创建政治地图纹理".into();
        };

        let country_render_ids = country_render_ids(&world);
        let bytes = PackedByteArray::from(owner_ids(&world, &country_render_ids).as_slice());
        let Some(image) = Image::create_from_data(width, height, false, Format::RGBA8, &bytes)
        else {
            return "无法创建省份归属图像".into();
        };
        let Some(owner_texture) = ImageTexture::create_from_image(&image) else {
            return "无法创建省份归属纹理".into();
        };

        // 完整载入并创建纹理后再替换状态，失败的重新载入不会清除现有地图。
        self.world = Some(world);
        self.texture = Some(texture);
        self.political_texture = Some(political_texture);
        self.owner_texture = Some(owner_texture);
        self.country_render_ids = country_render_ids;
        self.player_country = None;
        GString::new()
    }

    /// 只有地图上拥有领土的国家可以开局；开始后不能通过浏览或重复调用换国。
    #[func]
    fn start_game(&mut self, tag: GString) -> GString {
        if self.player_country.is_some() {
            return "游戏已经开始，玩家国家不能更改".into();
        }
        let Some(world) = &self.world else {
            return "请先载入开局".into();
        };
        let tag = tag.to_string();
        let Some(summary) = world.country_summary(&tag) else {
            return "请选择有效的国家".into();
        };
        if summary.provinces == 0 {
            return "只能选择开局时拥有领土的国家".into();
        }
        self.player_country = Some(tag);
        GString::new()
    }

    #[func]
    fn get_player_country(&self) -> GString {
        self.player_country.as_deref().unwrap_or("").into()
    }

    #[func]
    fn get_map_texture(&self) -> Option<Gd<ImageTexture>> {
        self.texture.clone()
    }

    #[func]
    fn get_political_texture(&self) -> Option<Gd<ImageTexture>> {
        self.political_texture.clone()
    }

    #[func]
    fn get_owner_texture(&self) -> Option<Gd<ImageTexture>> {
        self.owner_texture.clone()
    }

    /// 编号 0 表示无国家，与归属纹理的空白、海洋和无主区域一致。
    #[func]
    fn get_country_render_id(&self, tag: GString) -> i64 {
        self.country_render_ids
            .get(&tag.to_string())
            .copied()
            .map_or(0, i64::from)
    }

    #[func]
    fn get_start_date(&self) -> GString {
        START_DATE.into()
    }

    #[func]
    fn get_country_count(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |world| world.landed_country_count() as i64)
    }

    /// 信息字典是当前世界的只读结果；未知编号或未载入时返回空字典。
    #[func]
    fn get_province_info(&self, id: i64) -> VarDictionary {
        let (Some(world), Ok(id)) = (self.world.as_ref(), u32::try_from(id)) else {
            return VarDictionary::new();
        };
        crate::info::province_info(world, id)
    }

    #[func]
    fn get_country_info(&self, tag: GString) -> VarDictionary {
        self.world
            .as_ref()
            .map_or_else(VarDictionary::new, |world| {
                crate::info::country_info(world, &tag.to_string())
            })
    }

    /// 空字符串表示无所有者；海洋、湖泊和荒地由类型查询区分。
    #[func]
    fn get_province_owner(&self, id: i64) -> GString {
        self.world
            .as_ref()
            .and_then(|world| {
                u32::try_from(id)
                    .ok()
                    .and_then(|id| world.state().province_owner(id))
            })
            .map(|country| country.tag())
            .unwrap_or("")
            .into()
    }

    #[func]
    fn get_province_kind(&self, id: i64) -> GString {
        let province = self.world.as_ref().and_then(|world| {
            u32::try_from(id)
                .ok()
                .and_then(|id| world.state().province(id))
        });
        province
            .map_or("", |province| crate::info::kind_name(province.kind))
            .into()
    }

    #[func]
    fn get_map_size(&self) -> Vector2i {
        self.world.as_ref().map_or(Vector2i::ZERO, |world| {
            let map = world.map();
            Vector2i::new(map.summary().width as i32, map.summary().height as i32)
        })
    }

    #[func]
    fn get_province_count(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |world| world.map().summary().mapped_provinces as i64)
    }

    /// 返回省份编号；0 表示无省份，-1 表示未载入地图或坐标越界。
    #[func]
    fn province_at(&self, pixel: Vector2i) -> i64 {
        let (Some(world), Ok(x), Ok(y)) = (
            self.world.as_ref(),
            u32::try_from(pixel.x),
            u32::try_from(pixel.y),
        ) else {
            return -1;
        };
        match world.map().province_at(x, y) {
            Ok(Some(id)) => i64::from(id),
            Ok(None) => 0,
            Err(_) => -1,
        }
    }
}

fn encode_ids(ids: &[u32]) -> Vec<u8> {
    // RGBA 四通道保存完整的 32 位编号；使用最近邻采样，禁止颜色插值。
    ids.iter().flat_map(|id| id.to_le_bytes()).collect()
}

fn country_render_ids(world: &OpeningWorld) -> BTreeMap<String, u32> {
    world
        .state()
        .countries()
        .keys()
        .enumerate()
        .map(|(index, tag)| (tag.clone(), index as u32 + 1))
        .collect()
}

fn owner_ids(world: &OpeningWorld, countries: &BTreeMap<String, u32>) -> Vec<u8> {
    // 按省份缓存归属，避免逐像素查国家；使用所有者而非控制者。
    let owners: HashMap<_, _> = world
        .map()
        .defined_province_ids()
        .iter()
        .chain(std::iter::once(&0))
        .map(|&id| {
            let owner = world
                .state()
                .province_owner(id)
                .map_or(0, |country| countries[country.tag()]);
            (id, owner)
        })
        .collect();
    world
        .map()
        .province_ids()
        .iter()
        .flat_map(|id| owners[id].to_le_bytes())
        .collect()
}

fn political_colors(world: &OpeningWorld) -> Vec<u8> {
    // 同一省份只解析一次颜色；编号保持稀疏，不按最大编号分配颜色表。
    let colors: HashMap<_, _> = world
        .map()
        .defined_province_ids()
        .iter()
        .chain(std::iter::once(&0))
        .map(|&id| {
            let [r, g, b] = world.province_color(id);
            (id, [r, g, b, 255])
        })
        .collect();
    world
        .map()
        .province_ids()
        .iter()
        .flat_map(|id| colors[id])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{country_render_ids, encode_ids, owner_ids, political_colors};
    use augustus_eu4::world::OpeningWorld;

    #[test]
    fn texture_encoding_preserves_all_four_id_bytes() {
        assert_eq!(
            encode_ids(&[0, 7, 0x0100_0001, u32::MAX]),
            [0, 0, 0, 0, 7, 0, 0, 0, 1, 0, 0, 1, 255, 255, 255, 255]
        );
    }

    #[test]
    fn owner_texture_groups_provinces_by_owner_even_when_country_colors_match() {
        let world = OpeningWorld::load(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../godot/tests/data/map_modes"),
        )
        .unwrap();
        let countries = country_render_ids(&world);
        assert_ne!(countries["AAA"], countries["BBB"]);
        assert_eq!(
            owner_ids(&world, &countries),
            encode_ids(&[1, 1, 2, 2, 1, 1, 1, 1, 0, 0, 0, 0])
        );
        let colors = political_colors(&world);
        assert_eq!(colors[0..4], colors[8..12]);
        assert_eq!(
            world
                .state()
                .province(16777217)
                .unwrap()
                .controller
                .as_deref(),
            Some("AAA")
        );
    }
}
