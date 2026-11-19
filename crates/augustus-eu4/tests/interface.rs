// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};

use augustus_eu4::interface::{
    GuiNodeKind, InterfaceSource, LocalisationCatalog, UiOperator, UiScalar, UiValue,
    format_eu4_text, format_eu4_text_with, parse_gfx_bytes, parse_gui_bytes, parse_localisation_bytes,
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/interface")
        .join(name)
}

fn read_fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixture(name)).expect("fixture")
}

#[test]
fn nested_window_keeps_geometry_duplicates_and_unknowns() {
    let document =
        parse_gui_bytes(Path::new("interface/basic.gui"), &read_fixture("basic.gui")).expect("gui");
    assert_eq!(document.roots.len(), 1);
    let window = &document.roots[0];
    assert_eq!(window.kind, GuiNodeKind::Window);
    assert_eq!(window.name.as_deref(), Some("province_window"));
    assert_eq!(window.dont_render, Some(false));
    assert_eq!(window.moveable, Some(false));
    assert_eq!(window.always_transparent, Some(true));
    let position = window.position.as_ref().expect("position");
    assert_eq!(position.x, UiScalar::Integer(10));
    assert_eq!(position.y, UiScalar::Integer(-4));
    let size = window.size.as_ref().expect("size");
    assert_eq!(size.x, UiScalar::Integer(200));
    assert_eq!(size.y, UiScalar::Decimal("30.5".to_owned()));
    assert_eq!(
        window.orientation.as_ref().map(|value| value.as_str()),
        Some("UPPER_LEFT")
    );
    assert!(window.properties.iter().any(|field| {
        field.key == "orientation"
            && matches!(&field.value, UiValue::String(text) if text == "LOWER_RIGHT")
    }));
    assert!(window.properties.iter().any(|field| {
        field.key == "custom_flag"
            && matches!(&field.value, UiValue::Atom(text) if text == "preserved")
    }));
    assert!(
        window
            .properties
            .iter()
            .any(|field| { field.key == "limit" && field.operator == UiOperator::LessThan })
    );

    let body = window
        .children
        .iter()
        .find(|child| child.name.as_deref() == Some("body"))
        .expect("container");
    assert_eq!(body.kind, GuiNodeKind::ContainerWindow);
    let icons: Vec<_> = body
        .children
        .iter()
        .filter(|child| child.kind == GuiNodeKind::Icon)
        .collect();
    assert_eq!(icons.len(), 2);
    assert_eq!(icons[0].name.as_deref(), Some("coin"));
    assert_eq!(icons[0].sprite.as_deref(), Some("GFX_coin"));
    assert_eq!(
        icons[0].quad_texture_sprite.as_deref(),
        Some("GFX_coin_quad")
    );
    assert_eq!(icons[1].name.as_deref(), Some("coin_copy"));
    let copy_position = icons[1].position.as_ref().expect("macro position");
    assert_eq!(copy_position.x, UiScalar::Atom("@macro_x".to_owned()));
    assert_eq!(copy_position.y, UiScalar::Decimal("-1.25".to_owned()));

    let title = body.find("title").expect("title");
    assert_eq!(title.kind, GuiNodeKind::InstantTextBox);
    assert_eq!(title.text.as_deref(), Some("PROVINCE_NAME"));
    assert_eq!(title.font.as_deref(), Some("vic_18"));
    assert_eq!(title.max_width, Some(UiScalar::Integer(120)));
    assert_eq!(
        body.find("entry").expect("entry").kind,
        GuiNodeKind::TextBox
    );
    assert_eq!(body.find("grid").expect("grid").kind, GuiNodeKind::GridBox);
    assert_eq!(body.find("list").expect("list").kind, GuiNodeKind::ListBox);
    assert_eq!(
        body.find("stack").expect("stack").kind,
        GuiNodeKind::OverlappingElementsBox
    );
    let button = body.find("build_tax").expect("button");
    assert_eq!(button.kind, GuiNodeKind::Button);
    assert_eq!(button.button_text.as_deref(), Some("BUILD_TAX"));
    assert_eq!(button.click_sound.as_deref(), Some("click"));
    let unknown = body.find("mystery").expect("unknown");
    assert!(matches!(unknown.kind, GuiNodeKind::Unknown(ref key) if key == "checkboxType"));
    assert!(unknown.properties.iter().any(|field| field.key == "foo"));
    assert_eq!(document.unknown_node_count(), 1);
    assert!(document.unknown_field_count() >= 2);
}

#[test]
fn windows1252_text_is_decoded() {
    let bytes = b"guiTypes = { windowType = { name = \"caf\xe9\" } }\n";
    let document = parse_gui_bytes(Path::new("interface/windows1252.gui"), bytes).expect("gui");
    assert_eq!(document.roots[0].name.as_deref(), Some("caf\u{e9}"));
    assert!(document.source.path.ends_with("windows1252.gui"));
}

#[test]
fn gfx_keeps_texture_override_and_normalizes_separators() {
    let catalog =
        parse_gfx_bytes(Path::new("interface/basic.gfx"), &read_fixture("basic.gfx")).expect("gfx");
    assert_eq!(catalog.sprites.len(), 3);
    let coin = catalog.resolve("GFX_coin").expect("coin");
    assert_eq!(
        coin.texture_file.as_deref(),
        Some("gfx/interface/coin_override.tga")
    );
    assert_eq!(coin.frames, Some(2));
    assert!(catalog.sprites.iter().any(|sprite| {
        sprite.name == "GFX_coin"
            && sprite.texture_file.as_deref() == Some("gfx/interface/coin.dds")
            && sprite.frame_duration.as_deref() == Some("1.5")
            && sprite.always_transparent == Some(true)
            && sprite
                .properties
                .iter()
                .any(|field| field.key == "animation")
    }));
    assert_eq!(
        catalog
            .resolve("GFX_text")
            .and_then(|sprite| sprite.texture_file.as_deref()),
        Some("gfx/interface/text.dds")
    );
}

#[test]
fn localisation_reads_versions_quotes_and_colors() {
    let mut catalog = LocalisationCatalog::english();
    parse_localisation_bytes(
        Path::new("localisation/basic_l_english.yml"),
        &read_fixture("basic_l_english.yml"),
        &mut catalog,
    )
    .expect("localisation");
    assert_eq!(catalog.get("PROVINCE_NAME"), Some("Provence"));
    assert_eq!(catalog.get("BUILD_TAX"), Some("Build temple"));
    assert_eq!(catalog.get("BUILD_TIP"), Some("He said \"hi\""));
    assert_eq!(
        catalog.format_key("COLOR_SAMPLE").as_deref(),
        Some("[color=#d4af37]Gold[/color]")
    );
    assert_eq!(format_eu4_text("Keep $PLAYER$"), "Keep $PLAYER$");

    let mut eu4 = LocalisationCatalog::english();
    parse_localisation_bytes(
        Path::new("localisation/quotes_l_english.yml"),
        b"l_english:\n INNER:0 \"Embrace \"manufactories\" now\"\n EMPTY:0 \"\" #Hidden\n BAD:0 bare\n",
        &mut eu4,
    )
    .expect("quotes");
    assert_eq!(eu4.get("INNER"), Some("Embrace \"manufactories\" now"));
    assert_eq!(eu4.get("EMPTY"), Some(""));
    assert!(eu4.get("BAD").is_none());
    assert_eq!(eu4.errors.len(), 1);
}

#[test]
fn missing_file_error_includes_the_source_path() {
    let root = tempfile::tempdir().expect("temp");
    let source = InterfaceSource::open(root.path());
    let error = source
        .load_gui(Path::new("interface/missing.gui"))
        .expect_err("missing");
    let message = error.to_string();
    assert!(message.contains("missing.gui"), "{message}");
    let escaped = source.resolve_texture("gfx/../secret.dds");
    assert!(escaped.is_err());
}

#[test]
fn inspect_text_lists_window_sprite_key_and_unknown_node() {
    let path = Path::new("interface/basic.gui");
    let document = parse_gui_bytes(path, &read_fixture("basic.gui")).expect("gui");
    let sprites = parse_gfx_bytes(path, &read_fixture("basic.gfx")).expect("gfx");
    let mut localisation = LocalisationCatalog::english();
    parse_localisation_bytes(
        path,
        &read_fixture("basic_l_english.yml"),
        &mut localisation,
    )
    .expect("loc");
    let report =
        augustus_eu4::interface::inspect_gui(&document, Some(&sprites), Some(&localisation));
    for expected in [
        "province_window",
        "GFX_coin",
        "PROVINCE_NAME",
        "Provence",
        "checkboxType",
        "unknown_nodes: 1",
    ] {
        assert!(report.contains(expected), "{report}");
    }
    let json = augustus_eu4::interface::document_json(&document);
    let parsed: serde_json::Value = serde_json::from_str(&json).expect("json");
    assert_eq!(parsed["roots"][0]["name"], "province_window");
}

#[test]
fn content_root_reads_the_synthetic_fixture_layout() {
    let temp = tempfile::tempdir().expect("temp");
    let interface = temp.path().join("interface");
    let localisation = temp.path().join("localisation");
    std::fs::create_dir_all(&interface).expect("interface");
    std::fs::create_dir_all(&localisation).expect("localisation");
    std::fs::copy(fixture("basic.gui"), interface.join("basic.gui")).expect("gui");
    std::fs::copy(fixture("basic.gfx"), interface.join("basic.gfx")).expect("gfx");
    std::fs::copy(
        fixture("basic_l_english.yml"),
        localisation.join("basic_l_english.yml"),
    )
    .expect("yml");
    let source = InterfaceSource::open(temp.path());
    let document = source.load_gui("interface/basic.gui").expect("gui");
    assert!(document.source.path.ends_with("basic.gui"));
    let catalog = source.load_gfx_catalog().expect("catalog");
    assert!(catalog.resolve("GFX_coin").is_some());
    let text = source.load_localisation().expect("text");
    assert_eq!(text.get("PROVINCE_NAME"), Some("Provence"));
    assert_eq!(source.content_stack().mounts.len(), 1);
    let texture = source
        .resolve_texture("gfx/interface/coin.dds")
        .expect("texture");
    assert!(texture.ends_with(Path::new("gfx/interface/coin.dds")));
}

#[test]
fn shell_content_root_parses_topbar_province_and_coin() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../godot/ui/eu4/shell");
    let source = InterfaceSource::open(&root);
    let topbar = source.load_gui("interface/topbar.gui").expect("topbar");
    assert!(topbar.find("augustus_topbar").is_some());
    assert!(topbar.find("monthly_tax").is_some());
    assert!(topbar.find("pause").is_some());
    let province = source.load_gui("interface/province.gui").expect("province");
    assert!(province.find("province_monthly_tax").is_some());
    assert!(province.find("build_tax").is_some());
    let sprites = source.load_gfx_catalog().expect("gfx");
    let coin = sprites.resolve("GFX_augustus_coin").expect("coin");
    assert_eq!(coin.texture_file.as_deref(), Some("gfx/interface/coin.bmp"));
    let text = source.load_localisation().expect("text");
    assert!(text.errors.is_empty());
    let title = text.format_key("TOPBAR_TITLE").expect("title");
    assert!(title.contains("[color=#d4af37]"));
    assert_eq!(text.get("UNIMPLEMENTED"), Some("尚未实现"));
}

#[test]
fn nested_interface_files_are_loaded() {
    let temp = tempfile::tempdir().expect("temp");
    let nested = temp.path().join("interface").join("assets");
    std::fs::create_dir_all(&nested).expect("dir");
    std::fs::write(
        nested.join("nested.gui"),
        "guiTypes = { windowType = { name = \"nested_window\" } }\n",
    )
    .expect("gui");
    std::fs::write(
        nested.join("nested.gfx"),
        "spriteTypes = { spriteType = { name = \"GFX_nested\" texturefile = \"gfx/interface/nested.dds\" } }\n",
    )
    .expect("gfx");
    let loaded = InterfaceSource::open(temp.path())
        .load_interface()
        .expect("interface");
    assert_eq!(loaded.gui_files.len(), 1);
    assert_eq!(loaded.gfx_files.len(), 1);
    assert!(loaded.failures.is_empty(), "{:?}", loaded.failures);
    assert!(loaded.documents[0].find("nested_window").is_some());
    assert!(loaded.sprites.resolve("GFX_nested").is_some());
}

#[test]
fn installed_interface_files_parse() {
    let root = PathBuf::from(r"D:\SteamLibrary\steamapps\common\Europa Universalis IV");
    if !root.join("interface").is_dir() {
        return;
    }
    let loaded = InterfaceSource::open(&root)
        .load_interface()
        .expect("interface");
    assert!(loaded.gui_files.len() >= 129, "{}", loaded.gui_files.len());
    assert!(loaded.gfx_files.len() >= 132, "{}", loaded.gfx_files.len());
    assert!(
        loaded.failures.is_empty(),
        "{} failures\n{}",
        loaded.failures.len(),
        loaded.failures.join("\n")
    );
    assert!(loaded.sprites.resolve("GFX_topbar_bg").is_some());
    assert!(loaded.sprites.resolve("GFX_lobby_left_bg").is_some());
    let font = loaded.sprites.resolve("vic_18").expect("vic_18");
    assert_eq!(
        font.texture_file.as_deref(),
        Some("gfx/fonts/vic_18.fnt")
    );
    assert_eq!(
        font.texture_file_1.as_deref(),
        Some("gfx/fonts/vic_18.tga")
    );
    let shield = loaded.sprites.resolve("GFX_shield_thin").expect("shield");
    assert!(shield.texture_file_2.as_deref().unwrap_or("").contains("mask"));
    assert!(shield.texture_file_1.as_deref().unwrap_or("").contains("frame"));
    assert!(loaded.message_types.iter().any(|item| item.name == "SLACKEN"));
    assert!(loaded.colors.entries.len() > 100, "{}", loaded.colors.entries.len());
    assert_eq!(loaded.colors.resolve("W").map(|color| color.r), Some(255));
}

#[test]
fn window_positions_stay_separate_and_container_copies_position() {
    let bytes = br#"
        guiTypes = {
            windowType = {
                name = "panel"
                fullScreen = yes
                position = { x = 1 y = 2 }
                show_position = { x = 3 y = 4 }
                hide_position = { x = 5 y = 6 }
                editBoxType = {
                    name = "year"
                    size = { x = 80 y = 30 }
                    position = { x = 120 y = 57 }
                }
            }
            containerWindowType = {
                name = "copied"
                position = { x = 7 y = 8 }
                show_position = { x = 9 y = 10 }
            }
        }
    "#;
    let document = parse_gui_bytes(Path::new("interface/positions.gui"), bytes).expect("gui");
    let panel = document.find("panel").expect("panel");
    assert_eq!(panel.fullscreen, Some(true));
    assert_eq!(panel.position.as_ref().unwrap().x, UiScalar::Integer(1));
    assert_eq!(panel.show_position.as_ref().unwrap().x, UiScalar::Integer(3));
    assert_eq!(panel.hide_position.as_ref().unwrap().x, UiScalar::Integer(5));
    let year = document.find("year").expect("year");
    assert_eq!(year.kind, GuiNodeKind::EditBox);
    assert!(year.fullscreen.is_none());
    assert!(year.show_position.is_none());
    let copied = document.find("copied").expect("copied");
    assert_eq!(copied.position.as_ref().unwrap().x, UiScalar::Integer(7));
    assert_eq!(copied.hide_position.as_ref().unwrap().x, UiScalar::Integer(7));
    assert_eq!(copied.show_position.as_ref().unwrap().x, UiScalar::Integer(9));
}

#[test]
fn bitmap_font_path_is_not_taken_from_the_deprecated_name() {
    let bytes = br#"
        bitmapfonts = {
            bitmapfont = {
                name = "vic_18"
                fontName = "Garamond"
                path = "gfx/fonts/vic_18"
            }
        }
    "#;
    let catalog = parse_gfx_bytes(Path::new("interface/core.gfx"), bytes).expect("gfx");
    let font = catalog.resolve("vic_18").expect("font");
    assert_eq!(font.texture_file.as_deref(), Some("gfx/fonts/vic_18.fnt"));
    assert_eq!(
        font.texture_file_1.as_deref(),
        Some("gfx/fonts/vic_18.tga")
    );
    assert!(catalog.resolve("Vic_18").is_none());
    assert!(catalog.resolve("Garamond").is_none());
}

#[test]
fn loads_colors_and_message_types_but_not_credits() {
    let temp = tempfile::tempdir().expect("temp");
    let interface = temp.path().join("interface");
    std::fs::create_dir_all(&interface).expect("dir");
    std::fs::write(
        interface.join("colors.txt"),
        "color = { 4 144 178 }\ncolor = { 1 2 3 }\n",
    )
    .expect("colors");
    std::fs::write(
        interface.join("messagetypes.txt"),
        "SLACKEN={ msg_icontype=\"combat\" category=\"cat_military\" option=no to_me={ log=yes onmap=no popup=yes pausepopup=no icon=no } }\n",
    )
    .expect("messages");
    std::fs::write(interface.join("credits.txt"), "not a script {\n").expect("credits");
    std::fs::write(
        interface.join("core.gfx"),
        "bitmapfonts = { textcolors = { W = { 255 255 255 } } }\n",
    )
    .expect("gfx");
    let loaded = InterfaceSource::open(temp.path())
        .load_interface()
        .expect("interface");
    assert!(loaded.failures.is_empty(), "{:?}", loaded.failures);
    assert_eq!(loaded.colors.get(0).expect("first").r, 4);
    assert_eq!(loaded.colors.resolve("1").expect("index").b, 3);
    assert_eq!(loaded.colors.resolve("W").expect("white").r, 255);
    let message = loaded
        .message_types
        .iter()
        .find(|item| item.name == "SLACKEN")
        .expect("type");
    assert_eq!(message.category.as_deref(), Some("cat_military"));
    assert_eq!(message.option, Some(false));
    assert_eq!(message.channels[0].audience, "to_me");
    assert_eq!(message.channels[0].log, Some(true));
    let formatted = format_eu4_text_with("§WHello", |code| {
        loaded.colors.resolve(&code.to_string())
    });
    assert!(formatted.contains("#ffffff"), "{formatted}");
}
