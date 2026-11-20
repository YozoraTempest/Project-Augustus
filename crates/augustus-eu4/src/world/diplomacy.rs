// SPDX-License-Identifier: GPL-3.0-only

//! 开局当天仍有效的外交关系。只记双方和关系名，不读意见或效果。

use std::collections::BTreeMap;

use augustus_core::{Country, DiplomaticTie};
use jomini::common::Date;

use crate::{
    Error,
    content::{ContentFile, ContentRoot},
    script,
};

/// 这些是 `history/diplomacy` 里实际出现的关系块。带 `first` 和 `second` 的其他块也接受。
const NAMED_RELATIONS: &[&str] = &[
    "alliance",
    "royal_marriage",
    "guarantee",
    "vassal",
    "march",
    "union",
    "dependency",
];

pub(super) fn apply(
    content: &ContentRoot,
    countries: &mut BTreeMap<String, Country>,
) -> Result<(), Error> {
    let files = match content.text_files("history/diplomacy") {
        Ok(files) => files,
        Err(Error::Read { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let at = Date::parse(super::START_DATE).expect("固定开局日期必须有效");
    let mut from_files: BTreeMap<String, Vec<DiplomaticTie>> = BTreeMap::new();
    for path in files {
        let file = content.read(path)?;
        let tape = script::parse(&file)?;
        for (key, operator, value) in tape.windows1252_reader().fields() {
            let kind = key.read_str().into_owned();
            let Ok(object) = value.read_object() else {
                continue;
            };
            script::equal(&file, operator, &kind)?;
            let mut first = None;
            let mut second = None;
            let mut start = None;
            let mut end = None;
            let mut subject_type = None;
            for (key, operator, value) in object.fields() {
                let name = key.read_str();
                script::equal(&file, operator, &name)?;
                match name.as_ref() {
                    "first" => first = Some(text(&file, &value)?),
                    "second" => second = Some(text(&file, &value)?),
                    "start_date" => start = Some(date(&file, &value, "start_date")?),
                    "end_date" => end = Some(date(&file, &value, "end_date")?),
                    "subject_type" => subject_type = Some(text(&file, &value)?),
                    _ => {}
                }
            }
            let (Some(first), Some(second)) = (first, second) else {
                if NAMED_RELATIONS.contains(&kind.as_str()) {
                    return Err(Error::invalid(
                        &file.path,
                        format!("{kind} is missing first or second"),
                    ));
                }
                continue;
            };
            if !active(start, end, at) {
                continue;
            }
            let (first_label, second_label) = labels(&kind, subject_type.as_deref());
            let (first_icon, second_icon) = icons(&kind, subject_type.as_deref());
            push(
                &mut from_files,
                countries,
                &first,
                first_label,
                first_icon,
                &second,
            );
            push(
                &mut from_files,
                countries,
                &second,
                second_label,
                second_icon,
                &first,
            );
        }
    }
    for (tag, mut ties) in from_files {
        let Some(country) = countries.get_mut(&tag) else {
            continue;
        };
        ties.append(&mut country.diplomacy);
        country.diplomacy = ties;
    }
    Ok(())
}

fn push(
    found: &mut BTreeMap<String, Vec<DiplomaticTie>>,
    countries: &BTreeMap<String, Country>,
    tag: &str,
    label: String,
    icon: &str,
    other: &str,
) {
    if tag == other || !countries.contains_key(tag) || !countries.contains_key(other) {
        return;
    }
    found
        .entry(tag.to_owned())
        .or_default()
        .push(DiplomaticTie {
            label,
            other: other.to_owned(),
            icon: icon.to_owned(),
        });
}

fn icons(kind: &str, subject_type: Option<&str>) -> (&'static str, &'static str) {
    match kind {
        "alliance" => ("GFX_diplomacy_alliances", "GFX_diplomacy_alliances"),
        "royal_marriage" => ("GFX_diplomacy_royalmarriage", "GFX_diplomacy_royalmarriage"),
        "guarantee" => ("GFX_diplomacy_guaranting", "GFX_diplomacy_guaranteed"),
        "vassal" => ("GFX_diplomacy_leadvassal", "GFX_diplomacy_amvassal"),
        "march" => ("GFX_diplomacy_leadmarch", "GFX_diplomacy_weakmarch"),
        "union" => ("GFX_diplomacy_leadunions", "GFX_diplomacy_inunion"),
        "dependency" => match subject_type {
            Some("tributary_state") => ("GFX_diplomacy_leadtributary", "GFX_diplomacy_amtributary"),
            Some("appanage") => (
                "GFX_icon_diplomacy_lead_appanage",
                "GFX_icon_diplomacy_am_appanage",
            ),
            _ => ("GFX_diplomacy_leadvassal", "GFX_diplomacy_amvassal"),
        },
        _ => ("GFX_diplomacy_relations", "GFX_diplomacy_relations"),
    }
}

fn labels(kind: &str, subject_type: Option<&str>) -> (String, String) {
    match kind {
        "alliance" => ("alliance".to_owned(), "alliance".to_owned()),
        "royal_marriage" => ("royal_marriage".to_owned(), "royal_marriage".to_owned()),
        "guarantee" => ("GUARANTEEING".to_owned(), "guaranteed".to_owned()),
        "vassal" => ("vassal_title".to_owned(), "DEFAULT_OVERLORD".to_owned()),
        "march" => ("march_title".to_owned(), "DEFAULT_OVERLORD".to_owned()),
        "union" => (
            "PERSONAL_UNION".to_owned(),
            "personal_union_title".to_owned(),
        ),
        "dependency" => (
            subject_type
                .map(|name| format!("{name}_title"))
                .unwrap_or_else(|| "dependency".to_owned()),
            "DEFAULT_OVERLORD".to_owned(),
        ),
        other => (other.to_owned(), other.to_owned()),
    }
}

/// `start_date` 当天算在内，`end_date` 当天已经结束。没写开始日期的不算生效。
fn active(start: Option<Date>, end: Option<Date>, at: Date) -> bool {
    let Some(start) = start else {
        return false;
    };
    start <= at && end.is_none_or(|end| end > at)
}

fn text(
    file: &ContentFile,
    value: &jomini::text::ValueReader<'_, '_, jomini::Windows1252Encoding>,
) -> Result<String, Error> {
    value
        .read_string()
        .map_err(|error| Error::invalid(&file.path, error.to_string()))
}

fn date(
    file: &ContentFile,
    value: &jomini::text::ValueReader<'_, '_, jomini::Windows1252Encoding>,
    field: &str,
) -> Result<Date, Error> {
    let text = text(file, value)?;
    Date::parse(text.as_bytes())
        .map_err(|_| Error::invalid(&file.path, format!("invalid {field}: {text}")))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use augustus_core::Country;

    use crate::content::ContentRoot;

    use super::apply;

    #[test]
    fn start_date_is_included_and_end_date_has_already_ended() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "augustus-diplomacy-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("history/diplomacy")).unwrap();
        fs::write(
            root.join("history/diplomacy/sample.txt"),
            r#"
                guarantee = { first = TUR second = RAG start_date = 1444.11.11 end_date = 1500.1.1 }
                alliance = { first = FRA second = CAS start_date = 1444.1.1 end_date = 1450.1.1 }
                royal_marriage = { first = FRA second = CAS start_date = 1440.1.1 end_date = 1444.11.11 }
                vassal = { first = TUR second = WAL start_date = 1440.1.1 end_date = 1443.1.1 }
                dependency = { subject_type = tributary_state first = MNG second = DAI start_date = 1444.1.1 end_date = 1500.1.1 }
            "#,
        )
        .unwrap();
        let mut countries = BTreeMap::from([
            ("TUR".to_owned(), Country::new("TUR")),
            ("RAG".to_owned(), Country::new("RAG")),
            ("FRA".to_owned(), Country::new("FRA")),
            ("CAS".to_owned(), Country::new("CAS")),
            ("WAL".to_owned(), Country::new("WAL")),
            ("MNG".to_owned(), Country::new("MNG")),
            ("DAI".to_owned(), Country::new("DAI")),
        ]);
        apply(&ContentRoot::new(&root), &mut countries).unwrap();
        let _ = fs::remove_dir_all(&root);
        let tur: Vec<_> = countries
            .get("TUR")
            .unwrap()
            .diplomacy
            .iter()
            .map(|tie| tie.label.as_str())
            .collect();
        assert_eq!(tur, ["GUARANTEEING"]);
        assert_eq!(
            countries.get("RAG").unwrap().diplomacy[0].label,
            "guaranteed"
        );
        let fra: Vec<_> = countries
            .get("FRA")
            .unwrap()
            .diplomacy
            .iter()
            .map(|tie| tie.label.as_str())
            .collect();
        assert_eq!(fra, ["alliance"]);
        assert_eq!(
            countries.get("MNG").unwrap().diplomacy[0].label,
            "tributary_state_title"
        );
        assert_eq!(
            countries.get("DAI").unwrap().diplomacy[0].label,
            "DEFAULT_OVERLORD"
        );
        assert!(countries.get("WAL").unwrap().diplomacy.is_empty());
    }
}
