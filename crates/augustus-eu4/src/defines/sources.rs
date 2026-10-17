// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use crate::LoadedSource;
use full_moon::{
    ast::{Expression, Field, Index, Prefix, Stmt, Suffix, Var},
    node::Node,
    tokenizer::{TokenReference, TokenType},
};
use std::{collections::BTreeMap, path::Path};

/// Only straight assignments receive field locations. Dynamic execution keeps chunk provenance.
pub(super) fn assignments(text: &str, path: &Path) -> BTreeMap<String, LoadedSource> {
    let Ok(ast) = full_moon::parse(text) else {
        return BTreeMap::new();
    };
    let mut result = BTreeMap::new();
    for statement in ast.nodes().stmts() {
        let Stmt::Assignment(assignment) = statement else {
            return BTreeMap::new();
        };
        for (variable, expression) in assignment
            .variables()
            .iter()
            .zip(assignment.expressions().iter())
        {
            let Some(name) = variable_name(variable) else {
                return BTreeMap::new();
            };
            if !collect(
                &name,
                expression,
                variable.start_position(),
                path,
                &mut result,
            ) {
                return BTreeMap::new();
            }
        }
    }
    result
}

fn identifier(token: &TokenReference) -> Option<String> {
    match token.token().token_type() {
        TokenType::Identifier { identifier } => Some(identifier.to_string()),
        _ => None,
    }
}

fn variable_name(variable: &Var) -> Option<String> {
    match variable {
        Var::Name(name) => identifier(name),
        Var::Expression(expression) => {
            let Prefix::Name(name) = expression.prefix() else {
                return None;
            };
            let mut name = identifier(name)?;
            for suffix in expression.suffixes() {
                let Suffix::Index(Index::Dot { name: part, .. }) = suffix else {
                    return None;
                };
                name.push('.');
                name.push_str(&identifier(part)?);
            }
            Some(name)
        }
        _ => None,
    }
}

fn collect(
    name: &str,
    expression: &Expression,
    at: Option<full_moon::tokenizer::Position>,
    path: &Path,
    result: &mut BTreeMap<String, LoadedSource>,
) -> bool {
    if let Expression::TableConstructor(table) = expression {
        for field in table.fields().iter() {
            let Field::NameKey { key, value, .. } = field else {
                return false;
            };
            let Some(key_name) = identifier(key) else {
                return false;
            };
            if !collect(
                &format!("{name}.{key_name}"),
                value,
                key.start_position(),
                path,
                result,
            ) {
                return false;
            }
        }
    } else if pure(expression) {
        if let Some(at) = at {
            result.insert(
                name.into(),
                LoadedSource {
                    path: path.into(),
                    line: at.line(),
                    column: at.character(),
                },
            );
        }
    } else {
        return false;
    }
    true
}

fn pure(expression: &Expression) -> bool {
    match expression {
        Expression::Number(_) | Expression::String(_) | Expression::Symbol(_) => true,
        Expression::Parentheses { expression, .. }
        | Expression::UnaryOperator { expression, .. } => pure(expression),
        Expression::BinaryOperator { lhs, rhs, .. } => pure(lhs) && pure(rhs),
        _ => false,
    }
}
