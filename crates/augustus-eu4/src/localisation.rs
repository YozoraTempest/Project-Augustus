// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::OnceLock,
};

use jomini::{Encoding, Windows1252Encoding};

use crate::{ContentView, FileEntry, TextDocument, TextEncoding};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct EventTextKeys {
    pub title: Option<String>,
    pub description: Option<String>,
    pub option_labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventText {
    pub title: String,
    pub description: Option<String>,
    pub option_labels: BTreeMap<String, String>,
}

#[derive(Debug, Default)]
struct EnglishLocalisation {
    values: BTreeMap<String, String>,
    conflicts: BTreeSet<String>,
}

impl EnglishLocalisation {
    fn insert(&mut self, key: String, value: String) {
        if self.conflicts.contains(&key) {
            return;
        }
        if let Some(existing) = self.values.get(&key) {
            if existing != &value {
                self.values.remove(&key);
                self.conflicts.insert(key);
            }
        } else {
            self.values.insert(key, value);
        }
    }

    fn resolve(&self, key: &str) -> String {
        self.values
            .get(key)
            .cloned()
            .unwrap_or_else(|| key.to_owned())
    }
}

pub(crate) fn load_event_texts(
    root: &impl ContentView,
    files: &[FileEntry],
    event_keys: &BTreeMap<String, EventTextKeys>,
) -> BTreeMap<String, EventText> {
    let wanted = event_keys
        .values()
        .flat_map(|text| {
            text.title
                .iter()
                .chain(text.description.iter())
                .chain(text.option_labels.values())
        })
        .cloned()
        .collect::<BTreeSet<_>>();

    let mut localisation = EnglishLocalisation::default();
    if !wanted.is_empty() {
        let mut english_files = files
            .iter()
            .filter(|file| is_english_localisation(&file.relative_path))
            .collect::<Vec<_>>();
        english_files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        for file in english_files {
            let Ok(document) = root.read_text(&file.relative_path, TextEncoding::Windows1252)
            else {
                continue;
            };
            let text = decode_localisation(document.bytes());
            for line in text.lines() {
                let Some((key, value)) = parse_entry(line, document.source(), &wanted) else {
                    continue;
                };
                localisation.insert(key, value);
            }
        }
    }

    event_keys
        .iter()
        .map(|(event, keys)| {
            let title = keys
                .title
                .as_deref()
                .map(|key| localisation.resolve(key))
                .unwrap_or_else(|| event.clone());
            let description = keys
                .description
                .as_deref()
                .map(|key| localisation.resolve(key));
            let option_labels = keys
                .option_labels
                .iter()
                .map(|(option, key)| (option.clone(), localisation.resolve(key)))
                .collect();
            (
                event.clone(),
                EventText {
                    title,
                    description,
                    option_labels,
                },
            )
        })
        .collect()
}

fn is_english_localisation(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('\\', "/");
    let lower = normalized.to_ascii_lowercase();
    lower
        .split('/')
        .any(|component| component == "localisation")
        && lower.ends_with("_l_english.yml")
}

fn decode_localisation(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    if let Ok(text) = std::str::from_utf8(bytes) {
        text.to_owned()
    } else {
        // Jomini's string decoder also removes backslashes and trims whitespace.
        // Cache its single-byte mapping so file decoding preserves syntax and
        // whitespace without allocating one temporary string per high byte.
        static HIGH_BYTES: OnceLock<[char; 128]> = OnceLock::new();
        let high_bytes = HIGH_BYTES.get_or_init(|| {
            std::array::from_fn(|index| {
                Windows1252Encoding::new()
                    .decode(&[(index + 128) as u8])
                    .chars()
                    .next()
                    .expect("a Windows-1252 byte decodes to one character")
            })
        });
        bytes
            .iter()
            .map(|&byte| {
                if byte.is_ascii() {
                    char::from(byte)
                } else {
                    high_bytes[usize::from(byte) - 128]
                }
            })
            .collect()
    }
}

fn parse_entry(line: &str, source: &Path, wanted: &BTreeSet<String>) -> Option<(String, String)> {
    let line = line.trim_start();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let (key, rest) = line.split_once(':')?;
    let key = key.trim();
    if key.is_empty() || key.chars().any(char::is_whitespace) || !wanted.contains(key) {
        return None;
    }
    let version_len = rest.bytes().take_while(u8::is_ascii_digit).count();
    if version_len == 0 {
        return None;
    }
    let value = rest[version_len..].trim_start();
    if value.is_empty() {
        return None;
    }

    let normalized = format!("{key} = {value}");
    let document = TextDocument::from_bytes(source, normalized.into_bytes(), TextEncoding::Utf8);
    let parsed = document.parse().ok()?;
    let mut fields = parsed.reader().fields();
    let (_, _, value) = fields.next()?;
    if fields.next().is_some() {
        return None;
    }
    let scalar = value.read_scalar().ok()?;
    let text = std::str::from_utf8(scalar.as_bytes()).ok()?;
    Some((key.to_owned(), decode_escapes(text)))
}

fn decode_escapes(text: &str) -> String {
    let mut decoded = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        match chars.next() {
            Some('"') => decoded.push('"'),
            Some('\\') => decoded.push('\\'),
            Some('n') => decoded.push('\n'),
            Some('r') => decoded.push('\r'),
            Some('t') => decoded.push('\t'),
            Some(other) => {
                decoded.push('\\');
                decoded.push(other);
            }
            None => decoded.push('\\'),
        }
    }
    decoded
}

#[cfg(test)]
mod tests {
    use super::{EventTextKeys, decode_localisation, load_event_texts, parse_entry};
    use crate::ContentRoot;
    use std::{
        collections::{BTreeMap, BTreeSet},
        fs,
    };
    use tempfile::tempdir;

    #[test]
    fn parses_bom_utf8_localisation_and_decodes_escaped_quotes() {
        let bytes = b"\xef\xbb\xbfl_english:\n popup.title:0 \"The \\\"Imperial\\\" Diet\"\n";
        let text = decode_localisation(bytes);
        let wanted = BTreeSet::from(["popup.title".to_owned()]);
        let parsed = parse_entry(
            text.lines().nth(1).unwrap(),
            std::path::Path::new("popup_l_english.yml"),
            &wanted,
        )
        .unwrap();
        assert_eq!(
            parsed,
            ("popup.title".into(), "The \"Imperial\" Diet".into())
        );
    }

    #[test]
    fn decodes_utf8_localisation_escapes_once_from_the_raw_scalar() {
        let wanted = BTreeSet::from(["popup.title".to_owned()]);
        let parsed = parse_entry(
            r#" popup.title:0 "第一行\nSecond\r\t\"quote\"\\path\\n\q" # comment"#,
            std::path::Path::new("popup_l_english.yml"),
            &wanted,
        )
        .unwrap();
        assert_eq!(parsed.1, "第一行\nSecond\r\t\"quote\"\\path\\n\\q");
    }

    #[test]
    fn windows1252_decoding_preserves_escapes_and_whitespace_for_localisation() {
        let bytes =
            b"l_english:\n popup.title:0 \"Caf\xe9 \\n\\\"\x93Euro \x80\x94\\\" \\\\port  \"\n\t ";
        let text = decode_localisation(bytes);
        assert!(text.ends_with("\n\t "));
        assert!(text.contains("Café \\n"));
        let wanted = BTreeSet::from(["popup.title".to_owned()]);
        let parsed = parse_entry(
            text.lines().nth(1).unwrap(),
            std::path::Path::new("popup_l_english.yml"),
            &wanted,
        )
        .unwrap();
        assert_eq!(parsed.1, "Café \n\"“Euro €”\" \\port  ");
    }

    #[test]
    fn raw_localisation_values_keep_existing_grammar_validation() {
        let wanted = BTreeSet::from(["popup.title".to_owned()]);
        let source = std::path::Path::new("popup_l_english.yml");
        for line in [
            r#"popup.title:0 { nested = yes }"#,
            r#"popup.title:0 "First" second = "Second""#,
            r#"popup.title:0 "unterminated"#,
            r#"popup.title: "no version""#,
            r#"other.key:0 "not requested""#,
        ] {
            assert!(parse_entry(line, source, &wanted).is_none(), "{line}");
        }
        assert_eq!(
            parse_entry("popup.title:0 bare_value", source, &wanted)
                .unwrap()
                .1,
            "bare_value"
        );
        assert_eq!(
            parse_entry(
                r#"popup.title:0 "  text # inside  " # outside"#,
                source,
                &wanted
            )
            .unwrap()
            .1,
            "  text # inside  "
        );
    }

    #[test]
    fn conflicting_repeated_keys_fall_back_to_the_key() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("localisation");
        fs::create_dir(&path).unwrap();
        let file_path = path.join("events_l_english.yml");
        fs::write(
            &file_path,
            "l_english:\n popup.title:0 \"First\"\n popup.title:0 \"Second\"\n",
        )
        .unwrap();
        let root = ContentRoot::open(directory.path()).unwrap();
        let files = root.files().unwrap();
        let keys = BTreeMap::from([(
            "popup.event".into(),
            EventTextKeys {
                title: Some("popup.title".into()),
                ..Default::default()
            },
        )]);
        let texts = load_event_texts(&root, &files, &keys);
        assert_eq!(texts["popup.event"].title, "popup.title");
    }

    #[test]
    fn missing_keys_and_absent_metadata_keep_their_fallbacks() {
        let directory = tempdir().unwrap();
        let root = ContentRoot::open(directory.path()).unwrap();
        let keys = BTreeMap::from([
            (
                "popup.event".into(),
                EventTextKeys {
                    title: Some("missing.title".into()),
                    description: Some("missing.description".into()),
                    option_labels: [("0".into(), "missing.option".into())].into(),
                },
            ),
            ("plain.event".into(), EventTextKeys::default()),
        ]);
        let texts = load_event_texts(&root, &[], &keys);
        assert_eq!(texts["popup.event"].title, "missing.title");
        assert_eq!(
            texts["popup.event"].description.as_deref(),
            Some("missing.description")
        );
        assert_eq!(texts["popup.event"].option_labels["0"], "missing.option");
        assert_eq!(texts["plain.event"].title, "plain.event");
        assert!(texts["plain.event"].description.is_none());
        assert!(texts["plain.event"].option_labels.is_empty());
    }

    #[test]
    fn identical_repeated_keys_keep_the_shared_translation() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("localisation");
        fs::create_dir(&path).unwrap();
        fs::write(
            path.join("events_l_english.yml"),
            "l_english:\n popup.title:0 \"Shared\"\n popup.title:1 \"Shared\"\n",
        )
        .unwrap();
        let root = ContentRoot::open(directory.path()).unwrap();
        let files = root.files().unwrap();
        let keys = BTreeMap::from([(
            "popup.event".into(),
            EventTextKeys {
                title: Some("popup.title".into()),
                ..Default::default()
            },
        )]);
        let texts = load_event_texts(&root, &files, &keys);
        assert_eq!(texts["popup.event"].title, "Shared");
    }
}
