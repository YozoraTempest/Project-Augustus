// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;

use augustus_core::ProvinceKind;
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

        // 完整载入并创建纹理后再替换状态，失败的重新载入不会清除现有地图。
        self.world = Some(world);
        self.texture = Some(texture);
        self.political_texture = Some(political_texture);
        GString::new()
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
    fn get_start_date(&self) -> GString {
        START_DATE.into()
    }

    #[func]
    fn get_country_count(&self) -> i64 {
        self.world
            .as_ref()
            .map_or(0, |world| world.landed_country_count() as i64)
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
        match province.map(|province| province.kind) {
            Some(ProvinceKind::Land) => "land",
            Some(ProvinceKind::Sea) => "sea",
            Some(ProvinceKind::Lake) => "lake",
            Some(ProvinceKind::Wasteland) => "wasteland",
            None => "",
        }
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
    use super::encode_ids;

    #[test]
    fn texture_encoding_preserves_all_four_id_bytes() {
        assert_eq!(
            encode_ids(&[0, 7, 0x0100_0001, u32::MAX]),
            [0, 0, 0, 0, 7, 0, 0, 0, 1, 0, 0, 1, 255, 255, 255, 255]
        );
    }
}
