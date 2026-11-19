// SPDX-License-Identifier: GPL-3.0-only

//! 读取 EU4 的 `.gui`、`.gfx` 和界面本地化，生成拥有数据的中间表示。

mod assets;
mod colors;
mod gfx;
mod gui;
mod localisation;
mod messages;
mod mock;
mod model;
mod report;
mod value;

pub use assets::{InterfaceLoad, InterfaceSource, resolve_texture_path};
pub use colors::{InterfaceColors, Rgb, absorb_text_colors, parse_colors_bytes};
pub use gfx::parse_gfx_bytes;
pub use gui::parse_gui_bytes;
pub use localisation::{
    LocalisationCatalog, format_eu4_text, format_eu4_text_with, parse_localisation_bytes,
};
pub use messages::{MessageType, parse_message_types_bytes};
pub use mock::{gui_placeholder, gui_placeholders};
pub use model::{
    ContentMount, ContentSource, ContentStack, GfxCatalog, GuiDocument, GuiNode, GuiNodeKind,
    SpriteDefinition, UiField, UiOperator, UiOrientation, UiScalar, UiValue, UiVec2,
};
pub use report::{document_json, inspect_gfx, inspect_gui, inspect_window, window_json};
