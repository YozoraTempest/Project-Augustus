// SPDX-License-Identifier: GPL-3.0-only

use augustus_eu4::map::ProvinceMap;
use godot::{
    classes::{Image, ImageTexture, image::Format},
    prelude::*,
};

#[derive(GodotClass)]
#[class(init, base = RefCounted)]
struct AugustusMap {
    map: Option<ProvinceMap>,
    texture: Option<Gd<ImageTexture>>,
}

#[godot_api]
impl AugustusMap {
    /// 成功时返回空字符串；失败时返回原因，并保留当前地图。
    #[func]
    fn load_map(&mut self, game_dir: GString) -> GString {
        let map = match ProvinceMap::load(game_dir.to_string()) {
            Ok(map) => map,
            Err(error) => return error.to_string().as_str().into(),
        };
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

        // 完整载入并创建纹理后再替换状态，失败的重新载入不会清除现有地图。
        self.map = Some(map);
        self.texture = Some(texture);
        GString::new()
    }

    #[func]
    fn get_map_texture(&self) -> Option<Gd<ImageTexture>> {
        self.texture.clone()
    }

    #[func]
    fn get_map_size(&self) -> Vector2i {
        self.map.as_ref().map_or(Vector2i::ZERO, |map| {
            Vector2i::new(map.summary().width as i32, map.summary().height as i32)
        })
    }

    #[func]
    fn get_province_count(&self) -> i64 {
        self.map
            .as_ref()
            .map_or(0, |map| map.summary().mapped_provinces as i64)
    }

    /// 返回省份编号；0 表示无省份，-1 表示未载入地图或坐标越界。
    #[func]
    fn province_at(&self, pixel: Vector2i) -> i64 {
        let (Some(map), Ok(x), Ok(y)) = (
            self.map.as_ref(),
            u32::try_from(pixel.x),
            u32::try_from(pixel.y),
        ) else {
            return -1;
        };
        match map.province_at(x, y) {
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
