// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::{collections::BTreeMap, path::Path};

use jomini::{
    TextToken,
    binary::TokenResolver,
    common::Date,
    text::{ObjectReader, Operator, ValueReader},
};
use serde::Serialize;

use crate::administration_validation::{SaveTokenResolver, read_eu4_save_documents};
use crate::{Error, ParsedText, TextEncoding};

type Object<'a, 'b> = ObjectReader<'a, 'b, TextEncoding>;
type Field<'a, 'b> = ValueReader<'a, 'b, TextEncoding>;

/// Current subject relations recorded in an EU4 save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaSubjectRelationsImport {
    pub date: String,
    /// `None` means the save did not contain root diplomacy data.
    pub relations: Option<Vec<VanillaSavedSubjectRelation>>,
}

/// One subject edge from a saved root diplomacy dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VanillaSavedSubjectRelation {
    pub overlord: String,
    pub subject: String,
    pub subject_type: String,
}

/// Read saved subject relations from an EU4 text or ZIP-wrapped text save.
pub fn read_vanilla_subject_relations(
    path: impl AsRef<Path>,
) -> Result<VanillaSubjectRelationsImport, Error> {
    read_vanilla_subject_relations_inner_thread(path.as_ref(), None, None)
}

/// Read saved subject relations using a caller-provided binary token map.
pub fn read_vanilla_subject_relations_with_token_map(
    path: impl AsRef<Path>,
    token_map_path: impl AsRef<Path>,
) -> Result<VanillaSubjectRelationsImport, Error> {
    read_vanilla_subject_relations_inner_thread(path.as_ref(), Some(token_map_path.as_ref()), None)
}

/// Read saved subject relations using an integration-provided token resolver.
pub fn read_vanilla_subject_relations_with_token_resolver<R>(
    path: impl AsRef<Path>,
    resolver: R,
) -> Result<VanillaSubjectRelationsImport, Error>
where
    R: TokenResolver + Send + 'static,
{
    read_vanilla_subject_relations_inner_thread(path.as_ref(), None, Some(Box::new(resolver)))
}

fn read_vanilla_subject_relations_inner_thread(
    path: &Path,
    token_map_path: Option<&Path>,
    token_resolver: Option<SaveTokenResolver>,
) -> Result<VanillaSubjectRelationsImport, Error> {
    let path = path.to_path_buf();
    let token_map_path = token_map_path.map(Path::to_path_buf);
    let parser_path = path.clone();
    let parser_token_map_path = token_map_path.clone();
    let mut parser_token_resolver = token_resolver;
    let handle = std::thread::Builder::new()
        .name("augustus-eu4-subject-relations-parser".into())
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            read_vanilla_subject_relations_inner(
                &parser_path,
                parser_token_map_path.as_deref(),
                &mut parser_token_resolver,
            )
        })
        .map_err(|error| Error::caused_by(&path, error))?;
    handle
        .join()
        .map_err(|_| Error::message(&path, "EU4 subject relations parser thread panicked"))?
}

fn read_vanilla_subject_relations_inner(
    path: &Path,
    token_map_path: Option<&Path>,
    token_resolver: &mut Option<SaveTokenResolver>,
) -> Result<VanillaSubjectRelationsImport, Error> {
    let documents = read_eu4_save_documents(path, token_map_path, token_resolver)?;
    let parsed = documents.game_state.parse()?;
    let root = parsed.reader();
    let meta = documents
        .meta
        .as_ref()
        .map(|document| document.parse())
        .transpose()?;
    let date = match optional_scalar(&parsed, &root, "date")? {
        Some(date) => date,
        None => meta
            .as_ref()
            .map(|metadata| optional_scalar(metadata, &metadata.reader(), "date"))
            .transpose()?
            .flatten()
            .ok_or_else(|| Error::message(parsed.source(), "missing save field date"))?,
    };
    Date::parse(&date).map_err(|error| Error::caused_by(parsed.source(), error))?;

    let relations = optional_object(&parsed, &root, "diplomacy")?
        .map(|diplomacy| saved_subject_relations(&parsed, &diplomacy))
        .transpose()?;

    Ok(VanillaSubjectRelationsImport { date, relations })
}

fn saved_subject_relations(
    parsed: &ParsedText<'_>,
    diplomacy: &Object<'_, '_>,
) -> Result<Vec<VanillaSavedSubjectRelation>, Error> {
    // Saved dependencies are snapshot members, unlike dated diplomacy history.
    // Optional date labels are validated without filtering or scheduling edges.
    let mut relations = Vec::new();
    for (key, operator, value) in diplomacy.fields() {
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                "invalid root diplomacy field operator",
            ));
        }
        if key.read_string() != "dependency" {
            continue;
        }
        let relation = relation_object(parsed, &value)?;
        relations.push(saved_subject_relation(parsed, &relation)?);
    }
    Ok(relations)
}

fn saved_subject_relation(
    parsed: &ParsedText<'_>,
    relation: &Object<'_, '_>,
) -> Result<VanillaSavedSubjectRelation, Error> {
    let mut fields = BTreeMap::new();
    for (key, operator, value) in relation.fields() {
        let name = key.read_string();
        if !matches!(
            name.as_str(),
            "first" | "second" | "start_date" | "end_date" | "subject_type"
        ) {
            return Err(Error::message(
                parsed.source(),
                format!("unknown saved dependency field {name}"),
            ));
        }
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid saved dependency field operator for {name}"),
            ));
        }
        let value = scalar(parsed, &value, &name)?;
        if fields.insert(name.clone(), value).is_some() {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate saved dependency field {name}"),
            ));
        }
    }

    let first = required(&fields, parsed, "first")?;
    let second = required(&fields, parsed, "second")?;
    let subject_type = required(&fields, parsed, "subject_type")?;
    if !is_country_tag(first) || !is_country_tag(second) || first == second {
        return Err(Error::message(
            parsed.source(),
            "saved dependency has invalid country endpoints",
        ));
    }
    if subject_type.trim().is_empty() || subject_type.bytes().any(|byte| byte.is_ascii_whitespace())
    {
        return Err(Error::message(
            parsed.source(),
            "saved dependency has an invalid subject type",
        ));
    }
    for date_name in ["start_date", "end_date"] {
        if let Some(date) = fields.get(date_name) {
            Date::parse(date).map_err(|error| Error::caused_by(parsed.source(), error))?;
        }
    }

    Ok(VanillaSavedSubjectRelation {
        overlord: first.clone(),
        subject: second.clone(),
        subject_type: subject_type.clone(),
    })
}

fn optional_scalar(
    parsed: &ParsedText<'_>,
    reader: &Object<'_, '_>,
    name: &str,
) -> Result<Option<String>, Error> {
    let mut result = None;
    for (_, operator, value) in reader
        .fields()
        .filter(|(key, _, _)| key.read_string() == name)
    {
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid save field {name}"),
            ));
        }
        let value = scalar(parsed, &value, name)?;
        if result.replace(value).is_some() {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate save field {name}"),
            ));
        }
    }
    Ok(result)
}

fn optional_object<'a, 'b>(
    parsed: &ParsedText<'_>,
    reader: &Object<'a, 'b>,
    name: &str,
) -> Result<Option<Object<'a, 'b>>, Error> {
    let mut result = None;
    for (_, operator, value) in reader
        .fields()
        .filter(|(key, _, _)| key.read_string() == name)
    {
        if operator.is_some_and(|operator| operator != Operator::Equal) {
            return Err(Error::message(
                parsed.source(),
                format!("invalid save field {name}"),
            ));
        }
        if result.is_some() {
            return Err(Error::message(
                parsed.source(),
                format!("duplicate save field {name}"),
            ));
        }
        result = Some(object(parsed, &value)?);
    }
    Ok(result)
}

fn object<'a, 'b>(parsed: &ParsedText<'_>, value: &Field<'a, 'b>) -> Result<Object<'a, 'b>, Error> {
    if !matches!(value.token(), TextToken::Object { mixed: false, .. })
        && !(matches!(value.token(), TextToken::Array { mixed: false, .. })
            && value.read_array().is_ok_and(|array| array.is_empty()))
    {
        return Err(Error::message(parsed.source(), "expected save object"));
    }
    value
        .read_object()
        .map_err(|error| Error::caused_by(parsed.source(), error))
}

fn relation_object<'a, 'b>(
    parsed: &ParsedText<'_>,
    value: &Field<'a, 'b>,
) -> Result<Object<'a, 'b>, Error> {
    if !matches!(value.token(), TextToken::Object { mixed: false, .. }) {
        return Err(Error::message(
            parsed.source(),
            "saved dependency must be a non-mixed object",
        ));
    }
    value
        .read_object()
        .map_err(|error| Error::caused_by(parsed.source(), error))
}

fn scalar(parsed: &ParsedText<'_>, value: &Field<'_, '_>, name: &str) -> Result<String, Error> {
    if matches!(
        value.token(),
        TextToken::Array { .. } | TextToken::Object { .. }
    ) {
        return Err(Error::message(
            parsed.source(),
            format!("saved field {name} must be a scalar"),
        ));
    }
    value
        .read_string()
        .map_err(|error| Error::caused_by(parsed.source(), error))
}

fn required<'a>(
    fields: &'a BTreeMap<String, String>,
    parsed: &ParsedText<'_>,
    name: &str,
) -> Result<&'a String, Error> {
    fields.get(name).ok_or_else(|| {
        Error::message(
            parsed.source(),
            format!("missing saved dependency field {name}"),
        )
    })
}

fn is_country_tag(value: &str) -> bool {
    value.len() == 3
        && value
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

#[cfg(test)]
mod tests {
    use super::{
        VanillaSavedSubjectRelation, read_vanilla_subject_relations,
        read_vanilla_subject_relations_with_token_map,
        read_vanilla_subject_relations_with_token_resolver,
    };
    use jomini::{
        Scalar,
        binary::{Token, TokenResolver},
        common::Date,
    };
    use std::{
        fs::{self, File},
        io::Write,
    };
    use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

    fn read_save(text: &str) -> Result<super::VanillaSubjectRelationsImport, super::Error> {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("subject-save.eu4");
        fs::write(&path, format!("EU4txt\n{text}")).unwrap();
        read_vanilla_subject_relations(path)
    }

    fn dependency(first: &str, second: &str, subject_type: &str) -> String {
        format!(
            "dependency={{ first=\"{first}\" second=\"{second}\" subject_type=\"{subject_type}\" }}"
        )
    }

    struct SubjectTokenResolver;

    impl TokenResolver for SubjectTokenResolver {
        fn resolve(&self, token: u16) -> Option<&str> {
            match token {
                0x3100 => Some("date"),
                0x3101 => Some("diplomacy"),
                0x3102 => Some("dependency"),
                0x3103 => Some("first"),
                0x3104 => Some("second"),
                0x3105 => Some("subject_type"),
                _ => None,
            }
        }

        fn lookup(&self, _index: u32) -> Option<&str> {
            None
        }
    }

    fn binary_subject_save() -> Vec<u8> {
        let date = Date::parse("1445.1.1").unwrap().to_binary();
        let mut bytes = b"EU4bin".to_vec();
        for token in [
            Token::Id(0x3100),
            Token::Equal,
            Token::I32(date),
            Token::Id(0x3101),
            Token::Equal,
            Token::Open,
            Token::Id(0x3102),
            Token::Equal,
            Token::Open,
            Token::Id(0x3103),
            Token::Equal,
            Token::Quoted(Scalar::new(b"DAN")),
            Token::Id(0x3104),
            Token::Equal,
            Token::Quoted(Scalar::new(b"SWE")),
            Token::Id(0x3105),
            Token::Equal,
            Token::Quoted(Scalar::new(b"personal_union")),
            Token::Close,
            Token::Close,
        ] {
            token.write(&mut bytes).unwrap();
        }
        bytes
    }

    #[test]
    fn root_presence_distinguishes_unknown_from_known_empty_relations() {
        let missing = read_save(
            "date=1445.1.1 countries={ DAN={ diplomacy={ dependency={ first=\"DAN\" second=\"SWE\" subject_type=\"personal_union\" } } }}",
        )
        .unwrap();
        assert_eq!(missing.relations, None);

        let empty = read_save("date=1445.1.1 diplomacy={}").unwrap();
        assert_eq!(empty.relations, Some(vec![]));

        let unrelated =
            read_save("date=1445.1.1 diplomacy={ alliance={ first=\"DAN\" second=\"SWE\" } }")
                .unwrap();
        assert_eq!(unrelated.relations, Some(vec![]));
    }

    #[test]
    fn only_root_dependencies_are_imported_and_dates_do_not_filter_snapshot_edges() {
        let save = read_save(
            "date=1445.1.1 diplomacy={ dependency={ first=\"DAN\" second=\"SWE\" start_date=1444.1.1 end_date=1448.6.20 subject_type=\"mod_only_subject\" } } countries={ DAN={ diplomacy={ dependency={ first=\"AAA\" second=\"BBB\" subject_type=\"nested_subject\" } } }}",
        )
        .unwrap();

        assert_eq!(save.date, "1445.1.1");
        assert_eq!(
            save.relations,
            Some(vec![VanillaSavedSubjectRelation {
                overlord: "DAN".into(),
                subject: "SWE".into(),
                subject_type: "mod_only_subject".into(),
            }])
        );
    }

    #[test]
    fn optional_dependency_dates_and_unregistered_subject_types_are_preserved_as_snapshot_data() {
        let text = format!(
            "date=1445.1.1 diplomacy={{ {} }}",
            dependency("DAN", "SWE", "unregistered_subject")
        );
        let save = read_save(&text).unwrap();
        let relations = save.relations.unwrap();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].subject_type, "unregistered_subject");
    }

    #[test]
    fn malformed_dependency_fields_and_shapes_are_rejected() {
        let unknown_field = read_save(
            "date=1445.1.1 diplomacy={ dependency={ first=\"DAN\" second=\"SWE\" subject_type=\"personal_union\" extra=1 } }",
        );
        assert!(unknown_field.is_err());

        let array_relation =
            read_save("date=1445.1.1 diplomacy={ dependency={ \"DAN\" \"SWE\" } }");
        assert!(array_relation.is_err());

        let array_endpoint = read_save(
            "date=1445.1.1 diplomacy={ dependency={ first={ \"DAN\" } second=\"SWE\" subject_type=\"personal_union\" } }",
        );
        assert!(array_endpoint.is_err());

        let mixed_diplomacy = read_save(
            "date=1445.1.1 diplomacy={ dependency={ first=\"DAN\" second=\"SWE\" subject_type=\"personal_union\" } 1 }",
        );
        assert!(mixed_diplomacy.is_err());
    }

    #[test]
    fn duplicate_root_diplomacy_and_relation_fields_are_rejected() {
        let duplicate_root = read_save("date=1445.1.1 diplomacy={} diplomacy={}");
        assert!(duplicate_root.is_err());

        let duplicate_relation_field = read_save(
            "date=1445.1.1 diplomacy={ dependency={ first=\"DAN\" first=\"NOR\" second=\"SWE\" subject_type=\"personal_union\" } }",
        );
        assert!(duplicate_relation_field.is_err());

        let invalid_tag = read_save(
            "date=1445.1.1 diplomacy={ dependency={ first=\"dan\" second=\"SWE\" subject_type=\"personal_union\" } }",
        );
        assert!(invalid_tag.is_err());

        let invalid_date = read_save("date=not-a-date diplomacy={}");
        assert!(invalid_date.is_err());
    }

    #[test]
    fn zip_save_uses_meta_date_when_gamestate_date_is_missing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("subject-save.eu4");
        let mut archive = ZipWriter::new(File::create(&path).unwrap());
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        archive.start_file("meta", options).unwrap();
        archive.write_all(b"EU4txt\ndate=1445.1.1\n").unwrap();
        archive.start_file("gamestate", options).unwrap();
        archive
            .write_all(
                b"EU4txt\ndiplomacy={ dependency={ first=\"DAN\" second=\"SWE\" subject_type=\"personal_union\" } }\n",
            )
            .unwrap();
        archive.finish().unwrap();

        let save = read_vanilla_subject_relations(path).unwrap();

        assert_eq!(save.date, "1445.1.1");
        assert_eq!(save.relations.unwrap().len(), 1);
    }

    #[test]
    fn binary_token_map_and_resolver_entries_decode_the_same_relation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("subject-save.eu4");
        fs::write(&path, binary_subject_save()).unwrap();
        let token_map_path = directory.path().join("tokens.txt");
        fs::write(
            &token_map_path,
            "0x3100 date\n0x3101 diplomacy\n0x3102 dependency\n0x3103 first\n0x3104 second\n0x3105 subject_type\n",
        )
        .unwrap();

        let mapped = read_vanilla_subject_relations_with_token_map(&path, &token_map_path).unwrap();
        let resolved =
            read_vanilla_subject_relations_with_token_resolver(&path, SubjectTokenResolver)
                .unwrap();

        assert_eq!(mapped, resolved);
        assert_eq!(mapped.date, "1445.1.1");
        assert_eq!(
            mapped.relations.unwrap(),
            vec![VanillaSavedSubjectRelation {
                overlord: "DAN".into(),
                subject: "SWE".into(),
                subject_type: "personal_union".into(),
            }]
        );
    }
}
