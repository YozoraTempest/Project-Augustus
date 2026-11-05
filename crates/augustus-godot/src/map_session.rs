// SPDX-License-Identifier: GPL-3.0-only

use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
    time::Duration,
};

use augustus_core::{GameSpeed, SavedGame, Simulation};
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
    simulation: Option<Simulation>,
    game_directory: String,
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
        self.install_world(world, game_dir.to_string())
    }

    #[func]
    fn get_game_directory(&self) -> GString {
        self.game_directory.as_str().into()
    }

    #[func]
    fn save_game(&self, path: GString) -> GString {
        let (Some(world), Some(simulation), Some(player)) =
            (&self.world, &self.simulation, &self.player_country)
        else {
            return "请先开始游戏".into();
        };
        let save = crate::save::SaveFile::new(
            self.game_directory.clone(),
            world.start_date(),
            SavedGame::capture(world.state(), simulation, player),
        );
        match save.write(Path::new(&path.to_string())) {
            Ok(()) => GString::new(),
            Err(error) => error.as_str().into(),
        }
    }

    #[func]
    fn load_saved_game(&mut self, path: GString) -> GString {
        match self.restore_saved_game(Path::new(&path.to_string())) {
            Ok(()) => GString::new(),
            Err(error) => error.as_str().into(),
        }
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
        self.simulation = Some(Simulation::new(world.start_date()));
        GString::new()
    }

    /// 日期、暂停与倍速都来自同一个核心时钟，界面只读取结果。
    #[func]
    fn get_clock_state(&self) -> VarDictionary {
        let Some(simulation) = &self.simulation else {
            return VarDictionary::new();
        };
        let clock = simulation.clock();
        let mut state = VarDictionary::new();
        state.set("date", clock.date().to_string().as_str());
        state.set("paused", clock.is_paused());
        state.set("speed", i64::from(clock.speed().multiplier()));
        state
    }

    #[func]
    fn set_game_paused(&mut self, paused: bool) {
        if let Some(simulation) = &mut self.simulation {
            simulation.set_paused(paused);
        }
    }

    #[func]
    fn set_game_speed(&mut self, multiplier: i64) -> GString {
        let Some(speed) = u32::try_from(multiplier)
            .ok()
            .and_then(GameSpeed::from_multiplier)
        else {
            return "只支持 1×、2×、4×、8× 倍速".into();
        };
        let Some(simulation) = &mut self.simulation else {
            return "请先开始游戏".into();
        };
        simulation.set_speed(speed);
        GString::new()
    }

    /// 只传入单调时钟的实际经过时间；返回是否跨日，避免每帧刷新界面。
    #[func]
    fn advance_time(&mut self, elapsed_usec: i64) -> bool {
        let (Some(simulation), Some(world), Ok(elapsed)) = (
            &mut self.simulation,
            &mut self.world,
            u64::try_from(elapsed_usec),
        ) else {
            return false;
        };
        simulation.advance(world.state_mut(), Duration::from_micros(elapsed)) > 0
    }

    #[func]
    fn get_country_manpower(&self, tag: GString) -> VarDictionary {
        self.world
            .as_ref()
            .map_or_else(VarDictionary::new, |world| {
                crate::manpower::info(world, &tag.to_string())
            })
    }

    #[func]
    fn get_country_finances(&self, tag: GString) -> VarDictionary {
        self.world
            .as_ref()
            .map_or_else(VarDictionary::new, |world| {
                crate::info::country_finances(world, &tag.to_string())
            })
    }

    #[func]
    fn get_construction_info(&self, id: i64) -> VarDictionary {
        let (Some(world), Ok(id)) = (&self.world, u32::try_from(id)) else {
            return VarDictionary::new();
        };
        let date = self.simulation.as_ref().map_or_else(
            || world.start_date(),
            |simulation| simulation.clock().date(),
        );
        crate::construction::construction_info(world, id, self.player_country.as_deref(), date)
    }

    #[func]
    fn start_construction(&mut self, id: i64, building: GString) -> GString {
        let (Some(world), Some(simulation), Some(country)) =
            (&mut self.world, &self.simulation, &self.player_country)
        else {
            return "请先开始游戏".into();
        };
        let Ok(id) = u32::try_from(id) else {
            return "请选择有效的省份".into();
        };
        match world.state_mut().start_construction(
            country,
            id,
            &building.to_string(),
            simulation.clock().date(),
        ) {
            Ok(()) => GString::new(),
            Err(error) => error.to_string().as_str().into(),
        }
    }

    #[func]
    fn get_player_country(&self) -> GString {
        self.player_country.as_deref().unwrap_or("").into()
    }

    #[func]
    fn start_recruitment(&mut self, province: i64) -> GString {
        let (Some(world), Some(simulation), Some(country)) =
            (&mut self.world, &self.simulation, &self.player_country)
        else {
            return "请先开始游戏".into();
        };
        let Ok(province) = u32::try_from(province) else {
            return "请选择有效的省份".into();
        };
        match world
            .state_mut()
            .start_recruitment(country, province, simulation.clock().date())
        {
            Ok(()) => GString::new(),
            Err(error) => error.to_string().as_str().into(),
        }
    }

    #[func]
    fn get_recruitment_info(&self, province: i64) -> VarDictionary {
        let (Some(world), Ok(province)) = (&self.world, u32::try_from(province)) else {
            return VarDictionary::new();
        };
        let date = self.simulation.as_ref().map_or_else(
            || world.start_date(),
            |simulation| simulation.clock().date(),
        );
        crate::military::recruitment_info(world, province, self.player_country.as_deref(), date)
    }

    #[func]
    fn get_army_info(&self, id: i64) -> VarDictionary {
        self.world
            .as_ref()
            .and_then(|world| {
                let army = world.state().armies().get(&u32::try_from(id).ok()?)?;
                let date = self.simulation.as_ref().map_or_else(
                    || world.start_date(),
                    |simulation| simulation.clock().date(),
                );
                Some(crate::military::army_info(
                    world,
                    army,
                    date,
                    self.player_country.as_deref(),
                ))
            })
            .unwrap_or_default()
    }

    #[func]
    fn cancel_recruitment(&mut self, province: i64) -> GString {
        let (Some(world), Some(simulation), Some(country)) =
            (&mut self.world, &self.simulation, &self.player_country)
        else {
            return "请先开始游戏".into();
        };
        let Ok(province) = u32::try_from(province) else {
            return "请选择有效的省份".into();
        };
        match world
            .state_mut()
            .cancel_recruitment(country, province, simulation.clock().date())
        {
            Ok(_) => GString::new(),
            Err(error) => error.to_string().as_str().into(),
        }
    }

    #[func]
    fn get_army_markers(&self) -> Array<VarDictionary> {
        self.world.as_ref().map_or_else(Array::new, |world| {
            let date = self.simulation.as_ref().map_or_else(
                || world.start_date(),
                |simulation| simulation.clock().date(),
            );
            crate::military::markers(world, date, self.player_country.as_deref())
        })
    }

    #[func]
    fn move_army(&mut self, id: i64, destination: i64) -> GString {
        let (Some(world), Some(simulation), Some(country)) =
            (&mut self.world, &self.simulation, &self.player_country)
        else {
            return "请先开始游戏".into();
        };
        let (Ok(id), Ok(destination)) = (u32::try_from(id), u32::try_from(destination)) else {
            return "请选择有效的军队和目标省份".into();
        };
        match world
            .state_mut()
            .move_army(country, id, destination, simulation.clock().date())
        {
            Ok(()) => GString::new(),
            Err(error) => error.to_string().as_str().into(),
        }
    }

    #[func]
    fn stop_army(&mut self, id: i64) -> GString {
        let (Some(world), Some(country)) = (&mut self.world, &self.player_country) else {
            return "请先开始游戏".into();
        };
        let Ok(id) = u32::try_from(id) else {
            return "请选择有效的军队".into();
        };
        match world.state_mut().stop_army(country, id) {
            Ok(()) => GString::new(),
            Err(error) => error.to_string().as_str().into(),
        }
    }

    #[func]
    fn get_army_disband_info(&self, id: i64) -> VarDictionary {
        let error = |message: &str| {
            let mut info = VarDictionary::new();
            info.set("error", message);
            info
        };
        let (Some(world), Some(country)) = (&self.world, &self.player_country) else {
            return error("请先开始游戏");
        };
        let Ok(id) = u32::try_from(id) else {
            return error("请选择有效的军队");
        };
        crate::organization::disband_info(world, id, country)
    }

    #[func]
    fn disband_army(&mut self, id: i64) -> GString {
        let (Some(world), Some(country)) = (&mut self.world, &self.player_country) else {
            return "请先开始游戏".into();
        };
        let Ok(id) = u32::try_from(id) else {
            return "请选择有效的军队".into();
        };
        match world.state_mut().disband_army(country, id) {
            Ok(_) => GString::new(),
            Err(error) => error.to_string().as_str().into(),
        }
    }

    #[func]
    fn get_army_organization_info(&self, id: i64) -> VarDictionary {
        let (Some(world), Ok(id)) = (&self.world, u32::try_from(id)) else {
            return VarDictionary::new();
        };
        crate::organization::info(world, id, self.player_country.as_deref())
    }

    #[func]
    fn merge_armies(&mut self, target: i64, sources: PackedInt64Array) -> GString {
        let (Some(world), Some(country)) = (&mut self.world, &self.player_country) else {
            return "请先开始游戏".into();
        };
        let sources: Result<Vec<_>, _> = sources
            .as_slice()
            .iter()
            .copied()
            .map(u32::try_from)
            .collect();
        let (Ok(target), Ok(sources)) = (u32::try_from(target), sources) else {
            return "请选择有效的军队".into();
        };
        match world.state_mut().merge_armies(country, target, &sources) {
            Ok(()) => GString::new(),
            Err(error) => error.to_string().as_str().into(),
        }
    }

    #[func]
    fn split_army(&mut self, id: i64, count: i64) -> VarDictionary {
        let result = (|| {
            let (Some(world), Some(country)) = (&mut self.world, &self.player_country) else {
                return Err("请先开始游戏".to_owned());
            };
            let id = u32::try_from(id).map_err(|_| "请选择有效的军队".to_owned())?;
            let count = usize::try_from(count).map_err(|_| "请选择有效的团数".to_owned())?;
            world
                .state_mut()
                .split_army(country, id, count)
                .map_err(|error| error.to_string())
        })();
        crate::organization::split_result(result)
    }

    #[func]
    fn get_land_neighbors(&self, province: i64) -> Array<VarDictionary> {
        let (Some(world), Ok(province)) = (&self.world, u32::try_from(province)) else {
            return Array::new();
        };
        crate::movement::neighbors(world, province)
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

impl AugustusMap {
    fn restore_saved_game(&mut self, path: &Path) -> Result<(), String> {
        let save = crate::save::SaveFile::read(path)?;
        let mut world = OpeningWorld::load(&save.game_directory).map_err(|error| {
            format!(
                "无法载入存档使用的游戏目录 {}：{error}",
                save.game_directory
            )
        })?;
        if save.opening_date != world.start_date() {
            return Err("存档使用的开局日期与当前内容不一致".to_owned());
        }
        let restored = save
            .game
            .restore(world.state())
            .map_err(|error| error.to_string())?;
        if restored.simulation.clock().date() < world.start_date() {
            return Err("存档日期早于开局日期".to_owned());
        }
        *world.state_mut() = restored.world;
        // 使用恢复后的省份归属生成政治地图和选中国家纹理。
        let error = self.install_world(world, save.game_directory);
        if !error.is_empty() {
            return Err(error.to_string());
        }
        self.simulation = Some(restored.simulation);
        self.player_country = Some(restored.player_country);
        Ok(())
    }

    fn install_world(&mut self, world: OpeningWorld, game_directory: String) -> GString {
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
        // 全部完成后再替换，读取失败时保留当前会话。
        self.world = Some(world);
        self.texture = Some(texture);
        self.political_texture = Some(political_texture);
        self.owner_texture = Some(owner_texture);
        self.country_render_ids = country_render_ids;
        self.game_directory = game_directory;
        self.player_country = None;
        self.simulation = None;
        GString::new()
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
