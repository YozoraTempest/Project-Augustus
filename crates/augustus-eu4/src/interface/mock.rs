// SPDX-License-Identifier: GPL-3.0-only

//! 界面上还没有模拟数据的占位值。每条以后要换的真实来源写在
//! `godot/ui/eu4/gui_mock_list.md`。

/// 控件名对应的占位文本。名字和 `topbar.gui` 里的控件一致。
const PLACEHOLDERS: &[(&str, &str)] = &[
    ("text_ADM", "0"),
    ("text_DIP", "0"),
    ("text_MIL", "0"),
    ("text_merchants", "0"),
    ("text_settlers", "0"),
    ("text_diplomats", "0"),
    ("text_missionaries", "0"),
    ("text_sailors", "0"),
    ("text_stability", "0"),
    ("text_prestige", "0"),
    ("text_dev", "0"),
    ("text_meritocracy", "0"),
    ("text_leg", "0"),
    ("text_rep", "0"),
    ("text_unity", "0"),
    ("text_powerprojection", "0"),
    ("text_corruption", "0"),
];

pub fn gui_placeholders() -> &'static [(&'static str, &'static str)] {
    PLACEHOLDERS
}

pub fn gui_placeholder(name: &str) -> Option<&'static str> {
    PLACEHOLDERS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, value)| *value)
}

#[cfg(test)]
mod tests {
    use super::gui_placeholder;

    #[test]
    fn known_counters_are_zero_and_real_fields_are_absent() {
        assert_eq!(gui_placeholder("text_ADM"), Some("0"));
        assert_eq!(gui_placeholder("text_corruption"), Some("0"));
        assert_eq!(gui_placeholder("text_gold"), None);
        assert_eq!(gui_placeholder("text_manpower"), None);
    }
}
