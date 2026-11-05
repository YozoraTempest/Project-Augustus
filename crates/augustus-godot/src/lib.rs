// SPDX-License-Identifier: GPL-3.0-only

//! 将已加载的地图转换为 Godot 资源，游戏内容解析仍由 augustus-eu4 完成。

mod construction;
mod info;
mod map_session;

use godot::prelude::*;

struct AugustusExtension;

#[gdextension]
// SAFETY: 扩展注册与 Godot 对象生命周期由 godot-rust 管理。
unsafe impl ExtensionLibrary for AugustusExtension {}
